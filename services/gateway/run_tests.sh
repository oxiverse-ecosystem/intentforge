#!/usr/bin/env bash
cd /usr/src/gateway
apt-get update >/dev/null 2>&1
apt-get install -y pkg-config libssl-dev >/dev/null 2>&1
cargo test --locked >/usr/src/gateway/test_log.txt 2>&1
echo "EXITCODE=${PIPESTATUS[0]}" >>/usr/src/gateway/test_log.txt
