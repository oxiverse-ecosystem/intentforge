#!/usr/bin/env python3
"""Pytest-collected live integration test for the secondary SearXNG fan-out
that fires expanded queries when primary results are sparse
(services/gateway/src/main.rs — parallel retry block).

The fan-out is ADDITIVE ONLY: retry results are merged/deduped by URL and
appended; they never reorder the already-ranked primary set (no-manipulation
guarantee). This test guards that a sparse primary query still returns a
non-empty, successful result set after fan-out rather than collapsing to 0.

Run:  pytest tests/test_secondary_fanout_sparse.py
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""

import os

import pytest
import requests

BASE = os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")

# A query whose primary SearXNG pass is expected to be thin; the secondary
# fan-out (expanded queries) should bring coverage up.
SPARSE_QUERY = "zygomatic architectural photography techniques"


@pytest.fixture(scope="module")
def session():
    s = requests.Session()
    try:
        r = s.get(f"{BASE}/health", timeout=5)
        assert r.status_code == 200, f"gateway /health -> {r.status_code}"
    except Exception as e:
        pytest.skip(f"IntentForge gateway not reachable at {BASE}: {e}")
    return s


def test_sparse_query_gains_coverage_after_fanout(session):
    """A sparse primary query must still return a successful, non-empty result set
    after the secondary (expanded-query) fan-out — never a silent empty collapse."""
    r = session.get(f"{BASE}/search", params={"q": SPARSE_QUERY}, timeout=40)
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()
    results = body.get("results", [])
    assert isinstance(results, list), f"results must be a list, got {type(results)}"
    assert len(results) >= 1, (
        f"secondary fan-out should yield at least one result for a sparse query; "
        f"got {len(results)}"
    )
    # No upstream-unavailable total collapse.
    assert body.get("upstream_unavailable") is not True, (
        f"fan-out should not leave the request upstream_unavailable: {body.get('upstream_unavailable')}"
    )
