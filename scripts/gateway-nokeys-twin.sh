#!/usr/bin/env bash
# Live order-invariance harness: three gateway processes whose ONLY difference is
# the affiliate key environment.
#
#   :4003  keys PRESENT   (dummy key, same value docker-compose.dev.yml uses)
#   :4001  keys ABSENT    — the variable under test
#   :4002  keys ABSENT    — the upstream-churn CONTROL
#
# All three are started from ONE image by this script, so the comparison never
# depends on whatever the shared `services-gateway:latest` tag currently points
# at. (A concurrent rebuild of the dev gateway once silently changed the image
# under the probe and made the comparison meaningless.)
#
# WHY A THIRD PROCESS: affiliate keys are read from the PROCESS environment, so
# "keys present" vs "keys absent" can only be compared by running two processes.
# Why a second no-key process: all three query the SAME live upstreams
# independently, so result membership legitimately changes between calls.
# Comparing only keys-vs-no-keys would blame monetization for ordinary web churn.
# The control measures that churn with the affiliate variable removed.
#
# NETWORK NOTE: the gateway addresses its upstreams by LOOPBACK (127.0.0.1:6000
# indexer, :3005 intent-engine, :8080 searxng) because in compose it shares
# gluetun's network namespace. The twins must share that same namespace too,
# otherwise every upstream call is refused and they return 0 results, which
# would make an order comparison meaningless.
#
# The twins bind GATEWAY_PORT (4003/4001/4002); unset means the production 4000.
# gluetun publishes only 4000 to the host, so probes run through a short-lived
# curl container attached to the same namespace.
#
# Start:  bash scripts/gateway-nokeys-twin.sh start
# Probe:  bash scripts/gateway-nokeys-twin.sh probe <path-with-query>
# Stop:   bash scripts/gateway-nokeys-twin.sh stop
set -uo pipefail

# The dummy key matches docker-compose.dev.yml. It is a placeholder, never a real
# credential: the test only needs the env var to be RESOLVABLE, not valid.
DUMMY_KEY="${DUMMY_KEY:-dummy-test-key-do-not-use}"
KEYS_NAME=if-orderinv-keys
KEYS_PORT=4003
NAME=if-nokeys-twin
PORT=4001
CONTROL_NAME=if-nokeys-control
CONTROL_PORT=4002
IMAGE="${IMAGE:-services-gateway:latest}"
NETNS_CONTAINER=if-dev-gluetun
COMPOSE_DIR="$(cd "$(dirname "$0")/../services" && pwd)"
COMMERCE_DIR="$COMPOSE_DIR/gateway/data/commerce"
SIGNALS_DIR="$COMPOSE_DIR/shared-signals"

# MSYS/MINGW PATH MANGLE (verified defect, 2026-09-26): this script runs under
# git-bash, where `pwd` yields a POSIX path like /tmp/wt-commerce-fbu2/services.
# MSYS then auto-converts that argument when it is handed to the NATIVE docker.exe
# — but only for SOME argument forms — and here it produced a bind source of
# `/tmp/wt-commerce-fbu2/...`, a path that does not exist inside the Linux VM.
# Docker created the missing directory instead of failing, so the container came
# up HEALTHY with an EMPTY /app/data/commerce: `AffiliateCtx::load()` logged
# "loaded 0 network(s)", every result degraded to `affiliate: null`, and the
# keys-present vs keys-absent comparison silently became two no-op runs —
# a VACUOUS order-invariance "pass".
#
# Fix: hand docker.exe a native `C:/...` path. cygpath -m yields the mixed
# form docker.exe understands; if it is unavailable we refuse to start rather
# than run a vacuous probe.
to_native_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$1"
  else
    case "$1" in
      [A-Za-z]:/*) printf '%s\n' "$1" ;;
      /*) printf 'C:/%s\n' "${1#/}" ;;
      *) echo "ERROR: cannot convert '$1' to a native path for docker.exe" >&2; exit 3 ;;
    esac
  fi
}
COMMERCE_DIR="$(to_native_path "$COMMERCE_DIR")"
SIGNALS_DIR="$(to_native_path "$SIGNALS_DIR")"

# start_one <name> <port> [key]
# With a third argument the affiliate key env var IS set; without it, no affiliate
# key env var is passed at all, so AffiliateCtx::first_usable() returns None and
# every result degrades to affiliate: null.
start_one() {
  local cname="$1" cport="$2" key="${3:-}"
  docker rm -f "$cname" >/dev/null 2>&1
  if [ -n "$key" ]; then
    docker run -d --name "$cname" \
      --network "container:${NETNS_CONTAINER}" \
      -e GATEWAY_PORT="$cport" \
      -e SOVRN_COMMERCE_KEY="$key" \
      -v "${COMMERCE_DIR}:/app/data/commerce:ro" \
      -v "${SIGNALS_DIR}:/tmp/vpn-signals" \
      "$IMAGE" >/dev/null
    echo "started $cname on :${cport} (affiliate key PRESENT)"
  else
    docker run -d --name "$cname" \
      --network "container:${NETNS_CONTAINER}" \
      -e GATEWAY_PORT="$cport" \
      -v "${COMMERCE_DIR}:/app/data/commerce:ro" \
      -v "${SIGNALS_DIR}:/tmp/vpn-signals" \
      "$IMAGE" >/dev/null
    echo "started $cname on :${cport} (affiliate key ABSENT)"
  fi
}

case "${1:-start}" in
  start)
    # The twins join gluetun's network namespace, so if gluetun is RECREATED (its
    # netns is replaced) the still-"running" twins are silently orphaned onto a
    # dead namespace and every port refuses connections. Wait for a healthy
    # gluetun and recreate the twins so `start` is idempotent and self-healing.
    if [ "$(docker inspect -f '{{.State.Status}}' "$NETNS_CONTAINER" 2>/dev/null)" != "running" ]; then
      echo "waiting for $NETNS_CONTAINER to be running..." >&2
      for _ in $(seq 1 60); do
        [ "$(docker inspect -f '{{.State.Status}}' "$NETNS_CONTAINER" 2>/dev/null)" = "running" ] && break
        sleep 5
      done
    fi
    start_one "$KEYS_NAME"   "$KEYS_PORT"   "$DUMMY_KEY"
    start_one "$NAME"        "$PORT"
    start_one "$CONTROL_NAME" "$CONTROL_PORT"
    # Report readiness rather than assuming it: a twin that cannot bind or whose
    # upstream is cold shows up here, not as a confusing mid-probe failure.
    for c in "$KEYS_PORT" "$PORT" "$CONTROL_PORT"; do
      ok=""
      for _ in $(seq 1 24); do
        if docker run --rm --network "container:${NETNS_CONTAINER}" \
             curlimages/curl:8.11.1 -sf -m 10 "http://127.0.0.1:${c}/health" 2>/dev/null | grep -q OK; then
          ok=yes; break
        fi
        sleep 5
      done
      if [ -z "$ok" ]; then
        echo "WARNING: :${c} did not become healthy — check 'docker logs' before trusting a probe run" >&2
      else
        echo "  :${c} healthy"
      fi
    done
    # NON-VACUITY GATE (verified defect, 2026-09-26). The MSYS path mangle above
    # produced a container that was perfectly healthy, served real results, and
    # had loaded ZERO affiliate networks — so a keys-vs-no-keys comparison was
    # really "no decoration" vs "no decoration" and "passed" while proving
    # nothing. Health alone cannot catch this. Assert the config actually loaded
    # and that the mounted data file is really there, and fail LOUDLY otherwise.
    for c in "$KEYS_NAME" "$NAME" "$CONTROL_NAME"; do
      loaded=$(docker logs "$c" 2>&1 | grep -o 'affiliate: loaded [0-9]* network' | tail -1 | grep -o '[0-9]*')
      if [ "${loaded:-0}" = "0" ] || [ -z "${loaded:-}" ]; then
        echo "ABORT: $c loaded 0 affiliate networks — the commerce data mount is broken" >&2
        echo "       (this exact failure made a previous order-invariance run VACUOUS)." >&2
        echo "       Check: docker exec $c ls -la /app/data/commerce" >&2
        exit 4
      fi
      echo "  $c: loaded $loaded network(s) — non-vacuity OK"
    done
    ;;
  stop)
    for c in "$KEYS_NAME" "$NAME" "$CONTROL_NAME"; do
      docker rm -f "$c" >/dev/null 2>&1 && echo "stopped $c" || echo "$c not running"
    done
    ;;
  probe)
    # Runs INSIDE the shared netns so the twin ports are reachable.
    docker run --rm --network "container:${NETNS_CONTAINER}" \
      curlimages/curl:8.11.1 -sS -m "${3:-180}" "http://127.0.0.1:${2}${4}"
    ;;
  *)
    echo "usage: $0 {start|stop|probe <port> <timeout> <path+query>}" >&2
    exit 2
    ;;
esac
