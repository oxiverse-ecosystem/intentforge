# ─── GeoLite2 Database Updater ────────────────────────────────────
# Runs on startup and every 30 days to download the latest GeoLite2-City
# database, then restarts the gateway container so it picks up the update.
#
# Requires:
#   - /var/run/docker.sock mounted (to restart gateway)
#   - /data mounted to the same volume as gateway's /var/lib/geolite2
#   - MAXMIND_LICENSE_KEY env var set
#   - GATEWAY_CONTAINER_NAME env var (default: if-gateway)

FROM alpine:latest

RUN apk add --no-cache curl tar

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

VOLUME ["/data"]

# Default: prod container name; override for dev stack (if-dev-gateway)
ENV GATEWAY_CONTAINER_NAME=if-gateway

ENTRYPOINT ["/entrypoint.sh"]
