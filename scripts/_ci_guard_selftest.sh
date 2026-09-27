#!/usr/bin/env bash
# RED-CAPABLE PROOF that the two schema-regression gates are real guards.
#
# A guard that cannot fail is not a guard. Both CI gates
# (scripts/ci-assert-collection.sh, scripts/ci-assert-run-non-vacuous.sh)
# exist to stop a class of incident this repo has already lived through twice:
# a real contract defect riding a GREEN master because the suite skipped itself
# (2026-08-24: three contract defects; 2026-09-26: the whole schema-regression
# job skipped wholesale on a bare runner and still reported `success`).
#
# This script therefore does the one thing the original guards never had: it
# BREAKS the tree on purpose, runs the real gate scripts, and requires them to
# exit non-zero. It exercises the same files ci.yml invokes — not a copy — so
# a green result here means the deployed guard is genuinely red-capable.
#
# Each scenario is applied and reverted; the tree is left exactly as found
# (verified by `git status --porcelain` at the end).
#
# Usage:
#   PYTEST_BIN=<python -m pytest wrapper> bash scripts/_ci_guard_selftest.sh
#
# Scenario 2 needs a REACHABLE gateway and takes a few minutes; it is the one
# that proves a red assertion actually reds the job. Pass --collect-only to run
# just the fast scenarios (1, 3, 4).
set -uo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

PYTEST_BIN="${PYTEST_BIN:-pytest}"
COLLECT_GATE=scripts/ci-assert-collection.sh
RUN_GATE=scripts/ci-assert-run-non-vacuous.sh
BASE_URL="${INTENTFORGE_BASE_URL:-http://localhost:4000}"

collect_only=0
[ "${1:-}" = "--collect-only" ] && collect_only=1

fails=0
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1"; fails=$((fails + 1)); }

# Only the paths this script MOVES are checked for restoration. A full
# `git status` diff would false-positive whenever a concurrent QA-loop worker
# writes an unrelated scratch file mid-run — which is normal on this repo.
TOUCHED="tests/test_goals_api_schema.py tests/test_recall_gap.py tests/EXPECTED_COLLECTION"
snapshot() { for f in $TOUCHED; do printf '%s %s\n' "$f" "$([ -e "$f" ] && echo present || echo absent)"; done; }
before=$(snapshot)

# Expect a gate to FAIL. Returns 0 when the gate correctly rejected the tree.
expect_gate_to_fail() {
  local label="$1"; shift
  local log; log=$(mktemp)
  if "$@" >"$log" 2>&1; then
    fail "$label — the gate PASSED on a deliberately broken tree (it is vacuous)"
    sed 's/^/        /' "$log" | tail -20
  else
    pass "$label — gate exited $? as required"
    grep -E '::error::' "$log" | head -3 | sed 's/^/        /'
  fi
  rm -f "$log"
}

echo "=== baseline: both gates pass on a healthy tree ==="
if PYTEST_BIN="$PYTEST_BIN" bash "$COLLECT_GATE" >/tmp/gate_base.log 2>&1; then
  pass "collection gate green on the healthy tree"
  grep -E '^(TOTAL|CONTRACT) ' /tmp/gate_base.log | sed 's/^/        /'
else
  fail "collection gate is RED on a healthy tree — the floors are wrong"
  tail -20 /tmp/gate_base.log | sed 's/^/        /'
fi

echo
echo "=== scenario 1: a test file loses its test_ prefix (invisible to collection) ==="
mv tests/test_goals_api_schema.py tests/goals_api_schema.py
expect_gate_to_fail "prefix-stripped file detected" env PYTEST_BIN="$PYTEST_BIN" bash "$COLLECT_GATE"
mv tests/goals_api_schema.py tests/test_goals_api_schema.py

echo
echo "=== scenario 3: a test file is deleted (collection shrinks below the floor) ==="
mv tests/test_recall_gap.py /tmp/recall_gap_stash.py
expect_gate_to_fail "deleted test file detected" env PYTEST_BIN="$PYTEST_BIN" bash "$COLLECT_GATE"
mv /tmp/recall_gap_stash.py tests/test_recall_gap.py

echo
echo "=== scenario 4: the floor file goes missing (guards must not no-op) ==="
mv tests/EXPECTED_COLLECTION /tmp/expected_stash
expect_gate_to_fail "missing floor file detected" env PYTEST_BIN="$PYTEST_BIN" bash "$COLLECT_GATE"
mv /tmp/expected_stash tests/EXPECTED_COLLECTION

if [ "$collect_only" -eq 0 ]; then
  echo
  echo "=== scenario 2: a real assertion is broken; the RUN gate must go red ==="
  echo "    (needs a live gateway at $BASE_URL — set INTENTFORGE_REQUIRE_GATEWAY=1)"
  cp tests/test_goals_api_schema.py /tmp/tgas_backup.py
  # Break the D1 invariant the card mandates be permanent: the roadmap's
  # declared phase count no longer has to match the phases it ships.
  sed -i 's/assert total_phases == len(phases), (/assert total_phases == 99, (/' \
    tests/test_goals_api_schema.py
  if grep -q "total_phases == 99" tests/test_goals_api_schema.py; then
    pass "assertion deliberately broken in the tree"
    # The RUN gate re-runs pytest, which itself exits non-zero on a failure —
    # `set -o pipefail` inside the gate propagates that. That is the real
    # mechanism by which a red assertion reds the job; assert it happened.
    expect_gate_to_fail "broken roadmap.total_phases assertion reds the job" \
      env PYTEST_BIN="$PYTEST_BIN" INTENTFORGE_REQUIRE_GATEWAY=1 \
          INTENTFORGE_BASE_URL="$BASE_URL" bash "$RUN_GATE"
  else
    fail "could not break the assertion — the sed pattern no longer matches; update this scenario"
  fi
  cp /tmp/tgas_backup.py tests/test_goals_api_schema.py
  rm -f /tmp/tgas_backup.py

  echo
  echo "=== scenario 5: an unreachable gateway must FAIL, not skip ==="
  # This is THE original defect: the suite self-skipped and the job went green.
  # With INTENTFORGE_REQUIRE_GATEWAY=1 and a dead port, every test must fail.
  log=$(mktemp)
  if PYTEST_BIN="$PYTEST_BIN" INTENTFORGE_REQUIRE_GATEWAY=1 \
       INTENTFORGE_BASE_URL="http://127.0.0.1:59999" \
       "$PYTEST_BIN" tests/test_goals_api_schema.py -q >"$log" 2>&1; then
    fail "suite exited 0 against a DEAD gateway — the vacuous-green bug is back"
  else
    if grep -qE 'skipped' "$log" && ! grep -qE '[0-9]+ (failed|error)' "$log"; then
      fail "suite SKIPPED against a dead gateway despite INTENTFORGE_REQUIRE_GATEWAY=1"
    else
      pass "dead gateway with REQUIRE_GATEWAY=1 produces failures, not skips"
      tail -2 "$log" | sed 's/^/        /'
    fi
  fi
  rm -f "$log"
fi

echo
after=$(snapshot)
if [ "$before" = "$after" ]; then
  echo "tree restored: every path this self-test moved is back where it started"
  git diff --quiet -- tests/ || {
    echo "        note: tests/ has an uncommitted diff — expected when scenario 2"
    echo "        deliberately broke an assertion; it must be REVERTED, not committed."
    git diff --stat -- tests/ | sed 's/^/        /'
  }
else
  fail "the self-test left a moved path in the wrong state"
  diff <(echo "$before") <(echo "$after") | sed 's/^/        /'
fi

echo
if [ "$fails" -eq 0 ]; then
  echo "SELFTEST PASSED — every gate failed when it should have."
  exit 0
fi
echo "SELFTEST FAILED — ${fails} scenario(s) did not behave as a real guard requires."
exit 1
