#!/usr/bin/env bash
# IntentForge v2 — Docker Compose helper
# Usage: ./do.sh <command>
#
# PROD: Full stack with VPN + Tor + Traefik + SSL
# DEV:  Minimal stack for local testing (no VPN, no Traefik)

set -euo pipefail
cd "$(dirname "$0")/services"

PROD="docker-compose.prod.yml"
DEV="docker-compose.dev.yml"

cmd="${1:-help}"

case "$cmd" in
  # ── PRODUCTION ──────────────────────────────────────────────
  prod-up)       docker compose -f "$PROD" up -d --build ;;
  prod-down)     docker compose -f "$PROD" down ;;
  prod-nuke)     docker compose -f "$PROD" down -v --rmi all --remove-orphans ;;
  prod-rebuild)  $0 prod-nuke && $0 prod-up ;;
  prod-logs)     docker compose -f "$PROD" logs -f ;;
  prod-ps)       docker compose -f "$PROD" ps ;;

  # ── DEVELOPMENT ─────────────────────────────────────────────
  dev-up)        docker compose -f "$DEV" up -d --build ;;
  dev-down)      docker compose -f "$DEV" down ;;
  dev-nuke)      docker compose -f "$DEV" down -v --rmi all --remove-orphans ;;
  dev-rebuild)   $0 dev-nuke && $0 dev-up ;;
  dev-logs)      docker compose -f "$DEV" logs -f ;;
  dev-ps)        docker compose -f "$DEV" ps ;;
  dev-shell)     docker exec -it "if-dev-${2:-gateway}" sh ;;

  # ── NUCLEAR ─────────────────────────────────────────────────
  nuke-all)
    docker compose -f "$PROD" down -v --rmi all --remove-orphans 2>/dev/null || true
    docker compose -f "$DEV"  down -v --rmi all --remove-orphans 2>/dev/null || true
    docker image prune -f
    docker volume prune -f
    ;;

  # ── HELP ────────────────────────────────────────────────────
  help|*)
    cat <<'EOF'
IntentForge v2 — Docker Compose Commands

  PRODUCTION (full stack: VPN + Tor + Traefik + SSL):
    ./do.sh prod-up         Build + start
    ./do.sh prod-down       Stop
    ./do.sh prod-nuke       Stop + delete containers/volumes/images
    ./do.sh prod-rebuild    Nuke + fresh build
    ./do.sh prod-logs       Tail logs
    ./do.sh prod-ps         List containers

  DEVELOPMENT (minimal: no VPN, no Traefik, gateway on :4000):
    ./do.sh dev-up          Build + start
    ./do.sh dev-down        Stop
    ./do.sh dev-nuke        Stop + delete containers/volumes/images
    ./do.sh dev-rebuild     Nuke + fresh build
    ./do.sh dev-logs        Tail logs
    ./do.sh dev-ps          List containers
    ./do.sh dev-shell [SVC] Shell into container (default: gateway)

  NUCLEAR:
    ./do.sh nuke-all        Wipe ALL intentforge containers/images/volumes
EOF
    ;;
esac
