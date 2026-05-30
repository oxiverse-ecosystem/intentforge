#!/bin/sh
# entrypoint.sh for tor2 (second Tor instance, SocksPort 9051)

# Only wait for bridges if UseBridges is enabled in torrc
if grep -q "^UseBridges 1" /etc/tor/torrc 2>/dev/null; then
  echo "[tor2] Waiting for bridges.txt..."
  while [ ! -f /data/bridges.txt ]; do
    sleep 2
  done
  echo "[tor2] Preparing torrc.bridges..."
  sed 's/^/Bridge /' /data/bridges.txt > /data/torrc.bridges
else
  echo "[tor2] Bridges disabled — starting Tor without bridges"
fi

# Fix permissions at runtime
chown -R tor:tor /var/lib/tor

echo "[tor2] Starting Tor on SocksPort 9051..."
exec su-exec tor tor -f /etc/tor/torrc
