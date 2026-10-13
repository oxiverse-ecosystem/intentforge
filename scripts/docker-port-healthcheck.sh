#!/usr/bin/env bash
# docker-port-healthcheck.sh - Detect and recover stale Docker port bindings
#
# Problem: After a gluetun recreate or Docker Desktop restart, host->container
# port forwarding can die while containers remain healthy.  The symptom is
# curl http://localhost:4000/health returning rc=52 (empty reply from server)
# even though docker ps shows every container as (healthy).
#
# Root cause: Docker Desktop port-proxy (com.docker.backend.exe on Windows)
# loses its binding when the container that owns the port mapping is recreated.
# The gateway and all dependents share the gluetun network namespace, so a stale
# gluetun port binding breaks every published port at once.
#
# This script:
#   1. Tests the host->container path (curl localhost:4000/health)
#   2. If that fails, tests the container->container path (docker exec gateway)
#   3. If the container is healthy but the host port is dead, it recreates
#      gluetun + all network_mode:dependents to restore port bindings
#   4. Re-tests and reports the result
#
# Usage:
#   bash scripts/docker-port-healthcheck.sh              # check + auto-recover
#   bash scripts/docker-port-healthcheck.sh --check     # check only, no recovery
#   bash scripts/docker-port-healthcheck.sh --recover   # force recovery
#
# Exit codes: 0 = healthy, 1 = recovered, 2 = unhealthy (recovery failed)
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="$REPO_ROOT/services"
COMPOSE_FILE="docker-compose.dev.yml"

# Services that share the gluetun network namespace and must be recreated
# when gluetun is recreated (their port bindings live on gluetun).
GLUETUN_DEPENDENTS="gluetun gateway indexer crawler intent-engine searxng tor vpn-rotator"

# Services on the tor2 network namespace (independent path)
TOR2_DEPENDENCIES="tor2 searxng2"

LOG_PREFIX="[port-healthcheck]"
VERBOSE="${VERBOSE:-0}"

log() {
  echo "${LOG_PREFIX} $(date -u '+%Y-%m-%dT%H:%M:%SZ') $*"
}

if [ "${VERBOSE}" = "1" ]; then
  vlog() { log "$@"; }
else
  vlog() { :; }
fi

# --- Check host->container port -------------------------------------------
check_host_port() {
  local port="${1:-4000}"
  local http_code
  http_code=$(curl -s -m 5 -o /dev/null -w "%{http_code}" "http://localhost:${port}/health" 2>/dev/null)
  if [ "$http_code" = "200" ]; then
    vlog "Host port $port: OK (HTTP $http_code)"
    return 0
  else
    vlog "Host port $port: FAIL (HTTP $http_code)"
    return 1
  fi
}

# --- Check container-internal health --------------------------------------
check_container_health() {
  local container="${1:-if-dev-gateway}"
  local http_code
  http_code=$(docker exec "$container" curl -s -m 5 -o /dev/null -w "%{http_code}" "http://localhost:4000/health" 2>/dev/null)
  if [ "$http_code" = "200" ]; then
    vlog "Container $container internal: OK (HTTP $http_code)"
    return 0
  else
    vlog "Container $container internal: FAIL (HTTP $http_code)"
    return 1
  fi
}

# --- Check Docker container status ----------------------------------------
check_containers_running() {
  local missing=0
  for svc in $GLUETUN_DEPENDENTS $TOR2_DEPENDENCIES; do
    local name="if-dev-${svc}"
    if ! docker inspect "$name" >/dev/null 2>&1; then
      log "Container $name: NOT FOUND"
      missing=$((missing + 1))
    else
      local state
      state=$(docker inspect "$name" --format '{{.State.Status}}' 2>/dev/null)
      if [ "$state" != "running" ]; then
        log "Container $name: $state (not running)"
        missing=$((missing + 1))
      else
        vlog "Container $name: running"
      fi
    fi
  done
  return $missing
}

# --- Recreate gluetun + dependents ----------------------------------------
recover_ports() {
  log "Starting port recovery: recreating gluetun + dependents"
  log "This will recreate: $GLUETUN_DEPENDENTS"
  log "Index data survives (host bind mount). Crawler queue restores in ~15s."

  cd "$COMPOSE_DIR" || return 2

  # Recreate gluetun first (it owns the network namespace)
  log "Recreating gluetun..."
  if ! docker compose -f "$COMPOSE_FILE" up -d --force-recreate gluetun 2>&1; then
    log "ERROR: Failed to recreate gluetun"
    return 2
  fi

  # Wait for gluetun to be healthy
  log "Waiting for gluetun to become healthy..."
  local attempts=0
  local max_attempts=30
  while [ $attempts -lt $max_attempts ]; do
    local health
    health=$(docker inspect if-dev-gluetun --format '{{.State.Health.Status}}' 2>/dev/null)
    if [ "$health" = "healthy" ]; then
      log "gluetun is healthy"
      break
    fi
    attempts=$((attempts + 1))
    sleep 2
  done

  if [ $attempts -ge $max_attempts ]; then
    log "WARNING: gluetun did not become healthy in time, continuing anyway"
  fi

  # Recreate all dependents
  log "Recreating dependents..."
  local recreate_list=""
  for svc in $GLUETUN_DEPENDENTS; do
    if [ "$svc" != "gluetun" ]; then
      recreate_list="$recreate_list $svc"
    fi
  done

  if ! docker compose -f "$COMPOSE_FILE" up -d --force-recreate $recreate_list 2>&1; then
    log "ERROR: Failed to recreate dependents"
    return 2
  fi

  # Also recreate tor2 + searxng2 (independent path, but same stale-port risk)
  log "Recreating tor2 path..."
  docker compose -f "$COMPOSE_FILE" up -d --force-recreate tor2 searxng2 2>&1 || true

  log "Recovery complete. Waiting 5s for services to settle..."
  sleep 5

  return 0
}

# --- Main -----------------------------------------------------------------
main() {
  local mode="${1:-auto}"

  log "=== Docker Port Healthcheck ==="
  log "Mode: $mode"

  # Quick check: are containers running at all?
  if ! check_containers_running; then
    log "WARNING: Some containers are not running"
    if [ "$mode" = "check" ]; then
      log "Containers not running. Run: cd $COMPOSE_DIR && docker compose -f $COMPOSE_FILE up -d"
      return 2
    fi
  fi

  # Test host port
  if check_host_port 4000; then
    log "Host port 4000 is healthy -- no action needed"
    return 0
  fi

  log "Host port 4000 is NOT reachable"

  if [ "$mode" = "check" ]; then
    log "Host port 4000 is unreachable. Run: bash scripts/docker-port-healthcheck.sh --recover"
    return 2
  fi

  # Test container-internal health
  if check_container_health if-dev-gateway; then
    log "Container is healthy but host port is dead -- stale port binding confirmed"
  else
    log "Container is also unhealthy -- this may be a real service failure, not just port forwarding"
    log "Check: docker logs if-dev-gateway"
  fi

  if [ "$mode" = "check" ]; then
    return 2
  fi

  # Recover
  if ! recover_ports; then
    log "ERROR: Recovery failed"
    return 2
  fi

  # Verify recovery
  sleep 3
  if check_host_port 4000; then
    log "SUCCESS: Port 4000 is now reachable"
    return 1
  else
    log "ERROR: Port 4000 still unreachable after recovery"
    log "Manual steps:"
    log "  1. cd $COMPOSE_DIR && docker compose -f $COMPOSE_FILE down"
    log "  2. docker compose -f $COMPOSE_FILE up -d"
    log "  3. If still broken: restart Docker Desktop"
    return 2
  fi
}

main "$@"
