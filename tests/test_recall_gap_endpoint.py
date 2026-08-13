#!/usr/bin/env python3
"""
Live integration test for the `/search` honest recall-gap signal
(`recall_gap_terms` on the UnifiedResponse).

This is the ONLY piece of the round-2026-08-12T1234Z D2 feature not covered by
the gateway unit tests (which already lock the `compute_recall_gap_terms` /
`distinctive_query_terms` pure functions in `hardcoding_ruling_tests`): whether
the field actually serializes onto a real `/search` response, and is omitted
(`skip_serializing_if = "Option::is_none"`) when the result set covers the
query's subject.

It runs against a live gateway (default http://localhost:4000). If the gateway
is unreachable it SKIPs cleanly (exit 0) rather than failing a suite that has no
server; if the server IS up, it asserts the real, documented behaviour.

Run:
    python3 tests/test_recall_gap_endpoint.py
    BASE_URL=https://api.oxiverse.com python3 tests/test_recall_gap_endpoint.py
"""

import json
import os
import sys
import urllib.request

BASE_URL = os.environ.get("BASE_URL", "http://localhost:4000").rstrip("/")


def _get_json(path: str, timeout: int = 40):
    url = f"{BASE_URL}{path}"
    req = urllib.request.Request(url, headers={"User-Agent": "recall-gap-itest/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _server_up() -> bool:
    try:
        with urllib.request.urlopen(f"{BASE_URL}/health", timeout=5) as resp:
            return resp.status == 200 and b"OK" in resp.read()
    except Exception:
        return False


# A contrived query whose rare distinctive term ("zygomatic") is essentially
# never present in generic "architectural photography techniques" results, so
# the honest gap signal must fire for it while the covered terms stay silent.
GAP_QUERY = "zygomatic architectural photography techniques"
# A fully-covered query whose distinctive terms all appear in results -> the
# field must be omitted entirely from the JSON.
COVERED_QUERY = "rust web framework"


def main() -> int:
    if not _server_up():
        print(f"[SKIP] gateway not reachable at {BASE_URL}; cannot run live test")
        return 0

    # --- Gap case: distinctive absent term must surface in recall_gap_terms ---
    gap = _get_json("/search?q=" + urllib.parse.quote(GAP_QUERY))
    assert isinstance(gap, dict), "response must be a JSON object"
    assert "recall_gap_terms" in gap, (
        "recall_gap_terms must be present for a query with an uncovered facet, "
        f"got keys: {list(gap.keys())}"
    )
    rg = gap["recall_gap_terms"]
    assert isinstance(rg, list), f"recall_gap_terms must be a list, got {type(rg)}"
    assert all(isinstance(t, str) for t in rg), f"recall_gap_terms entries must be strings: {rg}"
    assert "zygomatic" in rg, (
        f"expected the uncovered distinctive term 'zygomatic' in recall_gap_terms, got {rg}"
    )
    # Algorithm must NOT flag the terms that ARE covered.
    assert "architectural" not in rg, f"covered term wrongly flagged as gap: {rg}"
    assert "photography" not in rg, f"covered term wrongly flagged as gap: {rg}"
    print(f"[PASS] gap case: {GAP_QUERY!r} -> recall_gap_terms={rg}")

    # --- Covered case: field omitted entirely (skip_serializing_if) ---
    cov = _get_json("/search?q=" + urllib.parse.quote(COVERED_QUERY))
    assert "recall_gap_terms" not in cov, (
        "recall_gap_terms must be omitted (not null) when the result set covers "
        f"the query's subject, but it was present: {cov.get('recall_gap_terms')}"
    )
    print(f"[PASS] covered case: {COVERED_QUERY!r} -> field omitted (no recall gap)")

    print("\nAll recall_gap_terms live assertions passed.")
    return 0


if __name__ == "__main__":
    import urllib.parse  # noqa: E402  (kept local so _server_up import is cheap)

    sys.exit(main())
