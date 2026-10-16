#!/usr/bin/env bash
# Start a KEYS-ABSENT twin of the gateway for live order-invariance proof.
#
# It is the SAME image as the production dev gateway (services-gateway:latest), so
# the only variable is the affiliate env keys — exactly what we are testing.
#
# NETWORK NOTE: the gateway addresses its upstreams by LOOPBACK (127.0.0.1:6000
# indexer, :3005 intent-engine, :8080 searxng) because in compose it shares
# gluetun's network namespace. The twin must therefore share that same namespace
# too, otherwise every upstream call is refused and it returns 0 results — which
# would make an order comparison meaningless.
#
# The twin binds GATEWAY_PORT=4001 (the bind port is env-configurable) so it does
# not collide with :4000. gluetun only publishes 4000 to the host, so the probe
# runs through a short-lived curl container attached to the same namespace.
#
# Start:  bash scripts/gateway-nokeys-twin.sh start
# Probe:  bash scripts/gateway-nokeys-twin.sh probe [timeout] [url]
# Stop:   bash scripts/gateway-nokeys-twin.sh stop
set -uo pipefail

NAME=if-nokeys-twin
PORT=4001
IMAGE=services-gateway:latest
NETNS_CONTAINER=if-dev-gluetun
COMPOSE_DIR="$(cd "$(dirname "$0")/../services" && pwd)"
COMMERCE_DIR="$COMPOSE_DIR/gateway/data/commerce"
SIGNALS_DIR="$COMPOSE_DIR/shared-signals"

case "${1:-start}" in
  start)
    docker rm -f "$NAME" >/dev/null 2>&1
    # NO affiliate key env vars are passed => AffiliateCtx::first_usable() returns
    # None => every result degrades to affiliate: null. Everything else is the
    # same image/config as the keys-present gateway on :4000.
    docker run -d --name "$NAME" \
      --network "container:${NETNS_CONTAINER}" \
      -e GATEWAY_PORT="$PORT" \
      -v "${COMMERCE_DIR}:/app/data/commerce:ro" \
      -v "${SIGNALS_DIR}:/tmp/vpn-signals" \
      "$IMAGE" >/dev/null
    echo "started $NAME on :${PORT} inside ${NETNS_CONTAINER} netns (no affiliate keys)"
    ;;
  stop)
    docker rm -f "$NAME" >/dev/null 2>&1 && echo "stopped $NAME" || echo "$NAME not running"
    ;;
  probe)
    # Runs INSIDE the shared netns so 127.0.0.1:4001 (the twin) is reachable.
    docker run --rm --network "container:${NETNS_CONTAINER}" \
      curlimages/curl:8.11.1 -sS -m "${2:-120}" "$3"
    ;;
  *)
    echo "usage: $0 {start|stop|probe [timeout] [url]}" >&2
    exit 2
    ;;
esac
