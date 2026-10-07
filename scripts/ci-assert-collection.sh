#!/usr/bin/env bash
# GATE 1 of the `schema-regression` job in .github/workflows/ci.yml.
# Extracted verbatim from the workflow so it can be tested without a CI run
# (see scripts/_ci_guard_selftest.sh). ci.yml invokes THIS file — do not
# re-inline the logic, or the self-test stops proving anything.
#
# Catches the class of defect this job was created to kill: a renamed / moved /
# prefix-stripped file that silently drops out of collection, leaving the suite
# green because nothing ran.
#
# Two floors are read from tests/EXPECTED_COLLECTION rather than hand-tuned
# here, because a stale floor is worse than none:
#   TOTAL    — everything `pytest tests/` must collect.
#   CONTRACT — what `-m "not requires_upstream"` must still select, i.e. how
#              many assertions the next gate must really execute.
# Both are compared with `-lt`, so ADDING tests never needs a change here;
# only SHRINKAGE fails, and shrinkage is the defect.
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

PYTEST_BIN="${PYTEST_BIN:-pytest}"
EXPECTED_FILE=tests/EXPECTED_COLLECTION

floor() { grep -E "^$1=[0-9]+$" "$EXPECTED_FILE" | tail -1 | cut -d= -f2; }

total_floor=$(floor TOTAL)
contract_floor=$(floor CONTRACT)
if [ -z "$total_floor" ] || [ -z "$contract_floor" ]; then
  echo "::error::could not read TOTAL/CONTRACT from $EXPECTED_FILE" >&2
  exit 1
fi

# (1) No file in tests/ may lack the test_ prefix. pytest skips such a file
#     SILENTLY under directory collection — that is exactly how
#     tests/goals_api_schema.py (15 assertions) stayed invisible to every run
#     for months while CI reported success.
unprefixed=$(find tests -maxdepth 1 -name '*.py' ! -name 'test_*' \
  ! -name 'conftest.py' ! -name '__init__.py' -printf '%f\n' | sort)
if [ -n "$unprefixed" ]; then
  echo "::error::file(s) in tests/ lack the 'test_' prefix and are INVISIBLE to directory collection:"
  echo "$unprefixed" | sed 's/^/::error::  /'
  exit 1
fi

# (2) Collection floors.
summary=$("$PYTEST_BIN" tests/ --collect-only -q | tail -1)
count=$(echo "$summary" | grep -oE '^[0-9]+' || echo 0)
echo "TOTAL floor=${total_floor} collected=${count} (${summary})"
if [ "$count" -lt "$total_floor" ]; then
  echo "::error::directory collection found $count tests, expected at least $total_floor."
  echo "::error::A test file has likely stopped being collected (renamed without a test_ prefix, moved, or deleted)."
  exit 1
fi

csummary=$("$PYTEST_BIN" tests/ --collect-only -q -m "not requires_upstream" | tail -1)
ccount=$(echo "$csummary" | grep -oE '^[0-9]+' || echo 0)
echo "CONTRACT floor=${contract_floor} selected=${ccount} (${csummary})"
if [ "$ccount" -lt "$contract_floor" ]; then
  echo "::error::the gateway-contract run would execute only $ccount assertions, expected at least $contract_floor."
  exit 1
fi

echo "collection gate: OK"
