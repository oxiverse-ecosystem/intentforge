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
docker run --rm -v "$REPO_ROOT/services/gateway:/src:ro" -v if-gw-target:/src/target \
  -w /src rust:1.88-slim-bookworm bash -c "
    set -e
    apt-get update -qq && apt-get install -y -qq pkg-config libssl-dev >/dev/null
    if [ -n '$FILTER' ]; then
      cargo test --locked $FILTER -- --nocapture
    else
      cargo test --locked
    fi
  "
