#!/bin/sh
# ─── GeoLite2 Database Updater ──────────────────────────────────────
# Downloads the latest MaxMind GeoLite2-City database.
# Should be run monthly via cron/systemd timer.
#
# Usage:
#   ./services/geolite2-update.sh              # Download to default path
#   ./services/geolite2-update.sh /custom/path  # Download to custom path
#
# Prerequisites:
#   1. Register for a free MaxMind account: https://www.maxmind.com/en/geolite2/signup
#   2. Generate a license key: https://www.maxmind.com/en/accounts/current/license-key
#   3. Set MAXMIND_LICENSE_KEY environment variable or create .env file
#
# Example cron (monthly on the 1st):
#   0 0 1 * * /path/to/services/geolite2-update.sh
#
# This script is idempotent — it only overwrites the database if the
# download succeeds. On failure, the old database is preserved.

set -e

# ── Configuration ──
DEST_DIR="${1:-/var/lib/geolite2}"
DEST_FILE="${DEST_DIR}/GeoLite2-City.mmdb"
TEMP_FILE="${DEST_DIR}/.GeoLite2-City.mmdb.tmp"
LOCK_FILE="${DEST_DIR}/.update.lock"

# MaxMind download URL for GeoLite2 City
BASE_URL="https://download.maxmind.com/app/geoip_download"
EDITION_ID="GeoLite2-City"
SUFFIX="tar.gz"

# ── Resolve license key ──
# Check: env var → .env file → prompt user
if [ -n "${MAXMIND_LICENSE_KEY}" ]; then
    LICENSE_KEY="${MAXMIND_LICENSE_KEY}"
elif [ -f ".env" ]; then
    LICENSE_KEY=$(grep -E '^MAXMIND_LICENSE_KEY=' .env | cut -d '=' -f2 | tr -d '"' | tr -d "'")
fi

if [ -z "${LICENSE_KEY}" ]; then
    echo "ERROR: MAXMIND_LICENSE_KEY not set."
    echo ""
    echo "To get a license key:"
    echo "  1. Register at https://www.maxmind.com/en/geolite2/signup"
    echo "  2. Generate a key at https://www.maxmind.com/en/accounts/current/license-key"
    echo "  3. Set it: export MAXMIND_LICENSE_KEY='your_key_here'"
    echo "     Or add to .env: MAXMIND_LICENSE_KEY='your_key_here'"
    exit 1
fi

# ── Create destination directory ──
mkdir -p "${DEST_DIR}"

# ── Acquire lock (prevent concurrent updates) ──
if ! mkdir "${LOCK_FILE}" 2>/dev/null; then
    echo "Update already in progress (lock held). Exiting."
    exit 0
fi
trap 'rm -rf "${LOCK_FILE}"' EXIT

# ── Download ──
DOWNLOAD_URL="${BASE_URL}?edition_id=${EDITION_ID}&suffix=${SUFFIX}&license_key=${LICENSE_KEY}"
echo "Downloading GeoLite2-City database..."
echo "  URL: ${BASE_URL}?edition_id=${EDITION_ID}&suffix=${SUFFIX}&license_key=***"

# Download to temporary file
TARBALL="${DEST_DIR}/.geolite2.tar.gz"
if command -v curl > /dev/null 2>&1; then
    HTTP_CODE=$(curl -s -S -L -o "${TARBALL}" -w "%{http_code}" "${DOWNLOAD_URL}" 2>&1)
elif command -v wget > /dev/null 2>&1; then
    HTTP_CODE=$(wget -q -O "${TARBALL}" --server-response "${DOWNLOAD_URL}" 2>&1 | awk '/^  HTTP/{print $2}' | tail -1)
    [ -z "${HTTP_CODE}" ] && HTTP_CODE="200"
else
    echo "ERROR: Neither curl nor wget found. Install one of them."
    exit 1
fi

if [ "${HTTP_CODE}" != "200" ]; then
    echo "ERROR: Download failed with HTTP ${HTTP_CODE}"
    rm -f "${TARBALL}"
    exit 1
fi

# Extract the .mmdb file from the tarball
echo "Extracting database..."
EXTRACTED=$(tar -tzf "${TARBALL}" | grep '\.mmdb$' | head -1)
if [ -z "${EXTRACTED}" ]; then
    echo "ERROR: No .mmdb file found in the archive."
    rm -f "${TARBALL}"
    exit 1
fi

tar -xzf "${TARBALL}" -C "${DEST_DIR}" "${EXTRACTED}"
rm -f "${TARBALL}"

# Move to final location
mv "${DEST_DIR}/${EXTRACTED}" "${TEMP_FILE}"

# Validate the file is a valid mmdb (check metadata at end of file)
# MaxMind DB format stores metadata (with magic bytes) at the END of the file.
# Valid databases contain 'GeoIP2-City' or 'GeoLite2-City' in the trailing metadata.
TAIL_CHECK=$(tail -c 200 "${TEMP_FILE}" 2>/dev/null | grep -c 'City' || true)
if [ "${TAIL_CHECK}" -eq 0 ]; then
    echo "ERROR: Downloaded file does not appear to be a valid MMDB (no 'City' marker in metadata). Keeping old database."
    rm -f "${TEMP_FILE}"
    exit 1
fi

# Atomically replace the old database
mv "${TEMP_FILE}" "${DEST_FILE}"
echo "SUCCESS: GeoLite2-City database updated at ${DEST_FILE}"

# Show file info
FILE_SIZE=$(ls -lh "${DEST_FILE}" | awk '{print $5}')
echo "  Size: ${FILE_SIZE}"

# Reload hint: services that use the database should reopen the file.
# For the Gateway, it re-reads the file on next request via Arc<Reader<Vec<u8>>>.
echo ""
echo "NOTE: Restart the gateway service to pick up the new database:"
echo "  docker compose restart gateway"
echo ""
echo "Next update: $(date -d '+1 month' '+%Y-%m-%d')"
