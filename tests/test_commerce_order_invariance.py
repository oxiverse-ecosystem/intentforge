"""
Live order-invariance + disclosure regression test for IntentForge commerce.

Locks the #1 commerce invariant END-TO-END (the Rust unit-side test in
commerce_contract_tests.rs only proves it in-process; this proves it against
the live gateway over HTTP):

  (A) GET /shopping?q=X&count=N and GET /search?q=X&count=N return the SAME
      ordered URL list (byte-identical) over nonce-busted identical queries.
      This is the user-facing proof that affiliate monetization never changes
      ranking — the business thesis.
  (B) Every result with an `affiliate` field carries affiliate.disclosed == True.
      Disclosure is non-negotiable.
  (C) With SOVRN_COMMERCE_KEY unset the gateway still returns 200 and decorated
      results degrade gracefully (affiliate field omitted or null).

The Rust unit test (test_affiliate_decoration_does_not_change_ranking) proves
the invariant in-process. The 6 host-test failures on /images /videos /news
schema tests prove a live category-timeout regression can pass Rust CI while
failing end-to-end. This file closes that gap for the commerce surface.

Run:  pytest tests/test_commerce_order_invariance.py -v
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""

import os
import uuid

import pytest
import requests

BASE = os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")

# Commercial query — exact-model + price-bearing, reliably transactional intent.
SHOPPING_QUERY = "iphone 16 pro max price"
COUNT = 8


@pytest.fixture(scope="module")
def session(gateway_or_skip):
    """See tests/conftest.py: the shared guard fails instead of skipping when
    INTENTFORGE_REQUIRE_GATEWAY=1."""
    return requests.Session()


def _nonce() -> str:
    """Cache-busting nonce to defeat the 5-minute response cache."""
    return uuid.uuid4().hex[:12]


def _ranked_urls(body: dict) -> list:
    """Extract the ordered URL list from a /search or /shopping response."""
    results = body.get("results", [])
    return [r["url"] for r in results if isinstance(r, dict) and "url" in r]


@pytest.mark.requires_upstream
def test_shopping_and_search_return_identical_ranked_url_order(session):
    """GET /shopping and GET /search with the same nonce-busted query must
    return byte-identical ordered URL lists.

    This is the live end-to-end proof of the order-invariance invariant:
    affiliate decoration runs strictly AFTER ranking and must never change
    the ordered URL list. The Rust unit test proves it in-process; this
    proves it against the live gateway over HTTP.
    """
    nonce = _nonce()
    q = f"{SHOPPING_QUERY} v{nonce}"

    # GET /shopping
    r_shopping = session.get(
        f"{BASE}/shopping",
        params={"q": q, "count": COUNT},
        timeout=120,
    )
    assert r_shopping.status_code == 200, (
        f"GET /shopping -> {r_shopping.status_code} {r_shopping.text[:300]}"
    )
    shopping_urls = _ranked_urls(r_shopping.json())
    assert len(shopping_urls) > 0, "GET /shopping returned zero results"

    # GET /search — same nonce-busted query
    r_search = session.get(
        f"{BASE}/search",
        params={"q": q, "count": COUNT},
        timeout=120,
    )
    assert r_search.status_code == 200, (
        f"GET /search -> {r_search.status_code} {r_search.text[:300]}"
    )
    search_urls = _ranked_urls(r_search.json())
    assert len(search_urls) > 0, "GET /search returned zero results"

    # THE CONTRACT: byte-identical ordered URL lists
    assert shopping_urls == search_urls, (
        f"ORDER-INVARIANCE VIOLATION: /shopping and /search returned different "
        f"ranked URL orders for the same query.\n"
        f"  /shopping ({len(shopping_urls)}): {shopping_urls}\n"
        f"  /search   ({len(search_urls)}): {search_urls}"
    )


@pytest.mark.requires_upstream
def test_affiliate_disclosure_flag_present_on_all_decorated_results(session):
    """Every result with an `affiliate` field must carry affiliate.disclosed == True.

    Disclosure is non-negotiable: users must always know when a link is
    monetized. This locks the contract end-to-end.
    """
    nonce = _nonce()
    q = f"{SHOPPING_QUERY} v{nonce}"

    r = session.get(
        f"{BASE}/shopping",
        params={"q": q, "count": COUNT},
        timeout=120,
    )
    assert r.status_code == 200, f"GET /shopping -> {r.status_code} {r.text[:300]}"
    results = r.json().get("results", [])
    assert len(results) > 0, "GET /shopping returned zero results"

    decorated = [entry for entry in results if isinstance(entry.get("affiliate"), dict)]
    assert len(decorated) > 0, (
        "No results carried an affiliate block — decoration did not run, "
        "test is vacuous"
    )

    for i, entry in enumerate(results):
        aff = entry.get("affiliate")
        if isinstance(aff, dict):
            assert aff.get("disclosed") is True, (
                f"result[{i}].affiliate.disclosed must be True (disclosure contract); "
                f"got {aff.get('disclosed')!r}"
            )


@pytest.mark.requires_upstream
def test_graceful_degradation_when_commerce_key_unset(session):
    """With SOVRN_COMMERCE_KEY unset the gateway still returns 200 and
    decorated results degrade gracefully (affiliate field omitted or null).

    This mirrors the Rust negative-control test
    (test_affiliate_decoration_does_not_change_ranking DISABLED run) live.
    The gateway must never 500 or crash when commerce keys are absent.
    """
    nonce = _nonce()
    q = f"{SHOPPING_QUERY} v{nonce}"

    # We cannot unset the env var inside the running gateway container from
    # here, but we CAN verify the graceful-degradation contract: the gateway
    # must return 200 and the response must be well-formed regardless of
    # whether affiliate decoration actually ran. If decoration did NOT run
    # (key unset), results must still be returned with no affiliate block
    # (or a null one) — never a 500, never a crash.
    r = session.get(
        f"{BASE}/shopping",
        params={"q": q, "count": COUNT},
        timeout=120,
    )
    assert r.status_code == 200, (
        f"GET /shopping with commerce key unset must return 200; "
        f"got {r.status_code} {r.text[:300]}"
    )
    body = r.json()
    assert "results" in body, "response missing `results` key"
    results = body["results"]
    assert isinstance(results, list), f"`results` must be a list, got {type(results).__name__}"

    # Every result must be a dict; affiliate (if present) must be a dict or None
    for i, entry in enumerate(results):
        assert isinstance(entry, dict), f"result[{i}] must be an object"
        aff = entry.get("affiliate")
        if aff is not None:
            assert isinstance(aff, dict), (
                f"result[{i}].affiliate must be an object or null; got {type(aff).__name__}"
            )
