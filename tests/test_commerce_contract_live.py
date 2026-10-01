"""
Live regression tests for IntentForge commerce surfaces (ROADMAP items 2, 3, 7).

Locks three contracts that a future regression could silently break:

  (A) GET /shopping endpoint (deterministic) — every result carries
      `commerce_provenance` + an `affiliate` block with `disclosed: true`.
      This is the user-facing commerce endpoint; it must always enrich and
      decorate regardless of whether upstream pages expose product markup.
  (B) Anti-broadening on /search — an informational query must NEVER gain a
      `shopping` block. If it does, commercial-intent detection has been
      widened (keyword-branch regression).
  (C) Signal path on /search — a commercial query resolves to a transactional
      intent label OR a transactional distribution >= 0.50. This locks the
      upstream signal that gates the main-path shopping block.

These hit the live gateway (default http://localhost:4000). If the gateway is
unreachable the module skips cleanly.

Run:  pytest tests/test_commerce_contract_live.py -v
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""
import os

import pytest
import requests

BASE = os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")

# Commercial: exact-model + price-bearing — reliably transactional intent.
COMMERCIAL_Q = "iphone 16 pro max price"
# Purely informational — language-concept lookup, no product/price.
INFORMATIONAL_Q = "rust ownership"


@pytest.fixture(scope="module")
def session(gateway_or_skip):
    """See tests/conftest.py: the shared guard fails instead of skipping when
    INTENTFORGE_REQUIRE_GATEWAY=1."""
    return requests.Session()


# ──────────────────────────────────────────────────────────────────────
# (A) GET /shopping endpoint — always enriches + decorates
# ──────────────────────────────────────────────────────────────────────

@pytest.mark.requires_upstream
def test_shopping_endpoint_returns_results_with_provenance_and_affiliate(session):
    """GET /shopping must return results, each with `commerce_provenance` and an
    `affiliate` block carrying `disclosed: true`."""
    r = session.get(
        f"{BASE}/shopping",
        params={"q": COMMERCIAL_Q, "count": 5},
        timeout=120,
    )
    assert r.status_code == 200, f"GET /shopping -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert "results" in body, "`shopping` response missing `results` key"
    results = body["results"]
    assert isinstance(results, list) and len(results) > 0, (
        "GET /shopping returned zero results — commerce pipeline broken"
    )

    for i, entry in enumerate(results):
        assert "commerce_provenance" in entry, (
            f"result[{i}] missing `commerce_provenance` — enrichment not run"
        )
        prov = entry["commerce_provenance"]
        assert isinstance(prov, dict), f"result[{i}].commerce_provenance must be an object"
        assert "observed_at" in prov and "source" in prov and "url" in prov, (
            f"result[{i}] commerce_provenance missing required keys"
        )

        # Affiliate block must be present with disclosure flag
        aff = entry.get("affiliate")
        assert isinstance(aff, dict), (
            f"result[{i}] missing `affiliate` block — decoration not run"
        )
        assert aff.get("disclosed") is True, (
            f"result[{i}].affiliate.disclosed must be True (disclosure contract)"
        )
        assert aff.get("network"), f"result[{i}].affiliate.network missing"


# ──────────────────────────────────────────────────────────────────────
# (B) Anti-broadening: informational query → no shopping block on /search
# ──────────────────────────────────────────────────────────────────────

def test_informational_query_omits_shopping_block_on_search(session):
    """An informational query must NEVER gain a `shopping` block on /search.
    If it does, commercial-intent detection has been widened (keyword-branch
    regression). The `shopping` key must be genuinely absent, not null."""
    r = session.get(
        f"{BASE}/search",
        params={"q": INFORMATIONAL_Q, "count": 8},
        timeout=60,
    )
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()
    intent = body.get("intent")
    assert intent != "transactional", (
        f"informational query {INFORMATIONAL_Q!r} got transactional intent — misfire"
    )
    assert "shopping" not in body, (
        f"informational query {INFORMATIONAL_Q!r} unexpectedly gained a `shopping` "
        f"block: {body.get('shopping')!r} — detection widened (regression)"
    )


# ──────────────────────────────────────────────────────────────────────
# (C) Signal path: commercial query → transactional intent on /search
# ──────────────────────────────────────────────────────────────────────

def test_commercial_query_resolves_to_transactional_intent(session):
    """A commercial query on /search must resolve to a transactional intent
    label OR a transactional distribution >= 0.50. This locks the upstream
    signal that gates the main-path shopping block."""
    r = session.get(
        f"{BASE}/search",
        params={"q": COMMERCIAL_Q, "count": 8},
        timeout=60,
    )
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()

    intent = body.get("intent")
    dist = body.get("distribution") or {}
    txn_prob = dist.get("transactional", 0.0) if isinstance(dist, dict) else 0.0

    is_commercial_signal = (intent == "transactional") or (txn_prob >= 0.50)
    assert is_commercial_signal, (
        f"commercial query {COMMERCIAL_Q!r} did not resolve to a commercial "
        f"intent signal — intent={intent!r}, transactional_prob={txn_prob:.3f}. "
        "The intent classifier or distribution regression broke the gate signal."
    )
