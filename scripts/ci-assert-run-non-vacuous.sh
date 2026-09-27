#!/usr/bin/env bash
# GATE 2 of the `schema-regression` job in .github/workflows/ci.yml: a
# `success` conclusion must imply real assertions RAN.
#
# Extracted verbatim from the workflow for the same reason as gate 1 — a guard
# that can only be exercised by pushing a branch is a guard nobody verifies.
# ci.yml invokes THIS file; do not re-inline it.
#
# It re-runs the suite and compares the passed count against the SAME CONTRACT
# floor gate 1 verified is selectable. One number for both gates means they
# cannot drift into disagreeing about what "enough" means, and it catches the
# failure modes gate 1 cannot see on its own: a blanket skip introduced in a
# fixture, or a gateway that answers 200 on /health but not on the endpoints.
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

PYTEST_BIN="${PYTEST_BIN:-pytest}"
EXPECTED_FILE=tests/EXPECTED_COLLECTION
SUMMARY_FILE="${SUMMARY_FILE:-/tmp/pytest-summary.txt}"

contract_floor=$(grep -E '^CONTRACT=[0-9]+$' "$EXPECTED_FILE" | tail -1 | cut -d= -f2)
if [ -z "$contract_floor" ]; then
  echo "::error::could not read CONTRACT from $EXPECTED_FILE" >&2
  exit 1
fi

set -o pipefail
"$PYTEST_BIN" tests/ -m "not requires_upstream" -q -rs 2>&1 | tee "$SUMMARY_FILE"

line=$(grep -E '^[0-9]+ (passed|failed)' "$SUMMARY_FILE" | tail -1)
echo "summary: ${line:-<none>}"
passed=$(echo "$line" | grep -oE '^[0-9]+' || echo 0)
skipped=$(grep -oE '[0-9]+ skipped' "$SUMMARY_FILE" | tail -1 | grep -oE '^[0-9]+' || echo 0)
echo "passed=${passed} skipped=${skipped:-0} floor=${contract_floor}"

if [ "$passed" -lt "$contract_floor" ]; then
  echo "::error::contract run executed fewer than ${contract_floor} assertions (got $passed, ${skipped:-0} skipped) — refusing to report success"
  exit 1
fi
echo "non-vacuity gate: OK (${passed} assertions executed)"
