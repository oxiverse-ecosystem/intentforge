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

# Clear stale Tor state on restart — prevents guard exclusion cascades
# Old guard state + cached-microdescs from prior runs cause:
#   "All current guards excluded by path restriction type 2"
#   97 circuit timeouts in 35 minutes
if [ -f /var/lib/tor/state ]; then
  echo "[tor2] Clearing stale Tor state for clean bootstrap..."
  rm -f /var/lib/tor/state
  rm -f /var/lib/tor/cached-microdescs
  rm -f /var/lib/tor/cached-microdescs.new
  rm -f /var/lib/tor/cached-certs
  rm -f /var/lib/tor/cached-microdesc-consensus
fi

# Fix permissions at runtime
chown -R tor:tor /var/lib/tor

echo "[tor2] Starting Tor on SocksPort 9051..."
exec su-exec tor tor -f /etc/tor/torrc
