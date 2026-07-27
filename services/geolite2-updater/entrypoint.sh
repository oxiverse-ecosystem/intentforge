#!/bin/sh
# ─── GeoLite2 Updater Entrypoint ─────────────────────────────────
# Downloads the latest GeoLite2-City database every 30 days,
# then restarts the gateway container so it picks up the update.
#
# The database is downloaded to /data/ (mounted volume), which
# is the same volume the gateway mounts at /var/lib/geolite2.

# NOTE: intentionally NOT using `set -e` — a failed MaxMind download must not
# exit the script (which, combined with restart:always, would crash-loop the
# container ~every 18s and, on any successful download, SIGTERM the gateway
# mid-request causing intermittent aborted API responses).

BASE_URL="https://download.maxmind.com/app/geoip_download"
EDITION_ID="GeoLite2-City"
SUFFIX="tar.gz"
DEST_DIR="/data"
DEST_FILE="${DEST_DIR}/GeoLite2-City.mmdb"

update_and_restart() {
    echo "[$(date)] Starting GeoLite2 database update..."

    # Resolve license key
    LICENSE_KEY="${MAXMIND_LICENSE_KEY}"
    if [ -z "${LICENSE_KEY}" ]; then
        echo "ERROR: MAXMIND_LICENSE_KEY not set. Skipping update."
        return 1
    fi

    # Download with retry and exponential backoff (respect rate limits)
    DOWNLOAD_URL="${BASE_URL}?edition_id=${EDITION_ID}&suffix=${SUFFIX}&license_key=${LICENSE_KEY}"
    echo "  Downloading from MaxMind..."

    HTTP_CODE=""
    for attempt in 1 2 3 4 5; do
        HTTP_CODE=$(curl -s -S -L -o "/tmp/geolite2.tar.gz" -w "%{http_code}" "${DOWNLOAD_URL}" 2>&1 || echo "000")
        if [ "${HTTP_CODE}" = "200" ]; then
            break
        fi
        echo "  Attempt ${attempt} failed (HTTP ${HTTP_CODE}). Retrying in ${attempt}s..."
        rm -f /tmp/geolite2.tar.gz
        sleep "${attempt}"
    done

    if [ "${HTTP_CODE}" != "200" ]; then
        echo "  ERROR: Download failed after 5 attempts (HTTP ${HTTP_CODE}). Retaining old database."
        rm -f /tmp/geolite2.tar.gz
        return 1
    fi

    # Extract .mmdb to /tmp first, then atomically replace via mv
    echo "  Extracting database..."
    tar -xzf /tmp/geolite2.tar.gz -C /tmp --strip-components=1 '*.mmdb'
    rm -f /tmp/geolite2.tar.gz

    TEMP_MMDB="/tmp/GeoLite2-City.mmdb"
    if [ ! -f "${TEMP_MMDB}" ]; then
        echo "  ERROR: Extraction did not produce GeoLite2-City.mmdb."
        return 1
    fi

    # Validate by size: GeoLite2-City is ~63MB
    FILE_SIZE=$(wc -c < "${TEMP_MMDB}" 2>/dev/null | tr -d ' ')
    if [ -z "${FILE_SIZE}" ] || [ "${FILE_SIZE}" -lt 10000000 ]; then
        echo "  ERROR: Extracted database too small (${FILE_SIZE:-0} bytes). Keeping old database."
        rm -f "${TEMP_MMDB}"
        return 1
    fi

    # Atomically replace the old database
    mv "${TEMP_MMDB}" "${DEST_FILE}"
    echo "  SUCCESS: Database updated ($(ls -lh "${DEST_FILE}" | awk '{print $5}'))"

    # Restart the gateway container to pick up the new database.
    # Disabled by default: the gateway mounts GeoLite2 read-only and only loads
    # it at boot, so restarting it mid-traffic causes aborted API responses.
    # Set RESTART_GATEWAY_ON_UPDATE=1 to re-enable (e.g. if hot-reload is added).
    if [ "${RESTART_GATEWAY_ON_UPDATE}" = "1" ]; then
        echo "  Restarting container: ${GATEWAY_CONTAINER_NAME}..."
        if [ -S /var/run/docker.sock ]; then
            HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
                -X POST --unix-socket /var/run/docker.sock \
                "http://localhost/containers/${GATEWAY_CONTAINER_NAME}/restart" \
                -d '{"signal": "SIGTERM"}' 2>&1 || echo "000")
            if [ "${HTTP_STATUS}" = "204" ]; then
                echo "  Gateway restarted successfully."
            else
                echo "  WARNING: Gateway restart returned HTTP ${HTTP_STATUS}."
                echo "  (Expected 204. Container may need manual restart.)"
            fi
        else
            echo "  WARNING: Docker socket not mounted. Cannot restart gateway."
            echo "  Restart manually: docker restart ${GATEWAY_CONTAINER_NAME}"
        fi
    else
        echo "  NOTE: Gateway NOT restarted (RESTART_GATEWAY_ON_UPDATE not set)."
        echo "  The new database will be picked up on the gateway's next natural restart."
    fi
}

# Run immediately on startup
update_and_restart

# Then loop every 30 days
echo ""
echo "Next update in 30 days. Sleeping..."
while true; do
    sleep 2592000  # 30 days = 30 * 24 * 60 * 60
    echo ""
    update_and_restart
    echo ""
    echo "Next update in 30 days. Sleeping..."
done
