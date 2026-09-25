#!/usr/bin/env bash
# Compile and run the gateway test suite inside a throwaway Rust container.
# There is no local cargo/rustc on this host, so Docker is the only compiler --
# and CI is not the only gate that matters; a red local build is caught here
# before a round commit is made.
#
# Usage:  bash scripts/gateway_test_container.sh [test_name_filter]
set -uo pipefail

FILTER="${1:-}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/services/gateway"

# Bind-mount the source read-only and build in a container-local target dir so
# the host tree is never mutated by cargo.
#
# The calibration artifact is mounted where the gateway actually looks for it
# (./config/intent_calibration.json). Without it the loader correctly reports
# confidence_calibrated=false, which is the right production behaviour when the
# file is absent -- but it is NOT the configuration under test here, and would
# make calibrated-path tests fail for the wrong reason.
#
# It is mounted as a single file under /tmp (not as /src/config) because /src is
# a read-only bind: Docker cannot create the mountpoint directory inside it.
docker run --rm -v "$REPO_ROOT/services/gateway:/src:ro" -v if-gw-target:/src/target \
  -v "$REPO_ROOT/services/intent-engine/config/intent_calibration.json:/tmp/intent_calibration.json:ro" \
  -e INTENT_CALIBRATION_PATH=/tmp/intent_calibration.json \
  -w /src rust:1.88-slim-bookworm bash -c "
    set -e
    apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev >/dev/null
    if [ -n '$FILTER' ]; then
      cargo test --locked $FILTER -- --nocapture
    else
      cargo test --locked
    fi
  "
