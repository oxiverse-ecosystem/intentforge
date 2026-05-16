#!/bin/sh
while true; do
  sleep 7200
  wget -qO- --method=PUT --body-data='{"status":"stopped"}' http://127.0.0.1:8000/v1/vpn/status
done
