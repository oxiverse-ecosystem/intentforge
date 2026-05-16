#!/bin/sh
# entrypoint.sh

# Wait for bridges.txt to exist
echo "Waiting for bridges.txt..."
while [ ! -f /data/bridges.txt ]; do
  sleep 2
done

# Convert bridges.txt to torrc format
echo "Preparing torrc.bridges..."
sed 's/^/Bridge /' /data/bridges.txt > /data/torrc.bridges

# Fix permissions at runtime just in case
chown -R tor:tor /var/lib/tor

echo "Starting Tor..."
exec su-exec tor tor -f /etc/tor/torrc
