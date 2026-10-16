#!/usr/bin/env python3
"""Pytest-collected live integration test for the `/search` honest recall-gap signal
(`recall_gap_terms` on the UnifiedResponse).

Converted from the standalone `test_recall_gap_endpoint.py` main() script (round
2026-08-12T1234Z) so it is collected by `pytest tests/` instead of sitting
silent. The behavior is preserved exactly:

- Skip cleanly when the gateway is unreachable (no false red on a bare runner).
- Assert the gap case (rare distinctive term surfaces) and covered case (field
  omitted entirely from the JSON) against the live server.

The pure-function behavior of `compute_recall_gap_terms` /
`distinctive_query_terms` is separately locked by gateway unit tests
(`recall_gap_detects_missing_distinctive_term`,
`recall_gap_absent_when_distinctive_term_covered`,
`recall_gap_none_for_empty_results`). This file only covers the end-to-end
serialization contract onto `/search`.

Run:  pytest tests/test_recall_gap.py
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""

import os

import pytest
import requests

BASE = os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")

GAP_QUERY = "zygomatic architectural photography techniques"
COVERED_QUERY = "rust web framework"


@pytest.fixture(scope="module")
def session():
    s = requests.Session()
    try:
        r = s.get(f"{BASE}/health", timeout=5)
        assert r.status_code == 200, f"gateway /health -> {r.status_code}"
    except Exception as e:
        pytest.skip(f"IntentForge gateway not reachable at {BASE}: {e}")
    return s


def test_recall_gap_detects_missing_distinctive_term(session):
    """A query with a rare distinctive term must surface that term in recall_gap_terms."""
    r = session.get(f"{BASE}/search", params={"q": GAP_QUERY}, timeout=40)
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert "recall_gap_terms" in body, (
        f"recall_gap_terms must be present for a query with an uncovered facet; "
        f"got keys: {list(body.keys())}"
    )
    rg = body["recall_gap_terms"]
    assert isinstance(rg, list), f"recall_gap_terms must be a list, got {type(rg)}"
    assert all(isinstance(t, str) for t in rg), f"entries must be strings: {rg}"
    assert "zygomatic" in rg, f"expected 'zygomatic' in recall_gap_terms, got {rg}"
    # Covered terms must NOT be flagged as gaps.
    assert "architectural" not in rg, f"covered term wrongly flagged: {rg}"
    assert "photography" not in rg, f"covered term wrongly flagged: {rg}"


def test_recall_gap_omitted_when_query_covered(session):
    """A fully-covered query must omit recall_gap_terms entirely (skip_serializing_if)."""
    r = session.get(f"{BASE}/search", params={"q": COVERED_QUERY}, timeout=40)
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert "recall_gap_terms" not in body, (
        f"recall_gap_terms must be omitted (not null) when results cover the query; "
        f"got recall_gap_terms={body.get('recall_gap_terms')!r}"
    )
