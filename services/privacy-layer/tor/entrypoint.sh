#!/bin/sh
# entrypoint.sh

# Only wait for bridges if UseBridges is enabled in torrc
if grep -q "^UseBridges 1" /etc/tor/torrc 2>/dev/null; then
  echo "Waiting for bridges.txt..."
  while [ ! -f /data/bridges.txt ]; do
    sleep 2
  done
  echo "Preparing torrc.bridges..."
  sed 's/^/Bridge /' /data/bridges.txt > /data/torrc.bridges
else
  echo "Bridges disabled — starting Tor without bridges"
fi

# Fix permissions at runtime just in case
chown -R tor:tor /var/lib/tor

echo "Starting Tor..."
exec su-exec tor tor -f /etc/tor/torrc
