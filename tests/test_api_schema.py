"""
Permanent non-Goals API schema regression tests (round 2026-08-15T0830Z).

Audit t_6a1017ee requirement (C): every documented endpoint must have an
automated schema test so a future regression fails CI without a human. The
Goals-API endpoints already have tests/test_goals_api_schema.py (20 passing).
This file covers the remaining NON-Goals endpoints:

  1. GET  /            -> 200, body == "IntentForge-v2 Gateway"
  2. GET  /health      -> 200, body == "OK"
  3. GET  /search      -> 200, 15 documented top-level keys,
                          confidence is a real number (float/int)
  4. GET  /search/fast -> 200, keys count/results/source, source == "local"
  5. GET  /images      -> 200, keys count/query/results,
                          each result has the documented image fields
  6. GET  /videos      -> 200, keys count/query/results,
                          each result has the documented video fields
  7. GET  /news        -> 200, keys count/query/results,
                          each result has the documented news fields
  8. GET  /spellcheck  -> 200, keys query/corrected/changed/corrections,
                          on a typo changed == True and corrections[] non-empty
  9. negated brand transparency (applied vs ignored contradiction)
  10. other brand negatives enforced (applied only)

Each test hits the already-running dev gateway (default http://localhost:4000).
GATE SEMANTICS (matches test_goals_api_schema.py):
  * If INTENTFORGE_REQUIRE_GATEWAY=1 and gateway is down, the suite FAILS.
  * Otherwise, unreachable gateway -> SKIP (local/dev convenience).

Run:  pytest tests/ -v       (collected by directory)
"""

import os

import pytest
import requests

BASE = os.environ.get(
    "INTENTFORGE_BASE_URL",
    os.environ.get("INTENTFORGE_BASE", "http://localhost:4000"),
).rstrip("/")

_REQUIRE_GATEWAY = os.environ.get("INTENTFORGE_REQUIRE_GATEWAY") == "1"


def _reachable() -> bool:
    try:
        r = requests.get(f"{BASE}/health", timeout=5)
        return r.status_code == 200 and r.text.strip() == "OK"
    except Exception:
        return False


@pytest.fixture(scope="module")
def session():
    """Gateway guard. REQUIRE=1 + down -> FAIL; otherwise SKIP."""
    if _reachable():
        return requests.Session()
    if _REQUIRE_GATEWAY:
        pytest.fail(
            f"Gateway at {BASE} is REQUIRED (INTENTFORGE_REQUIRE_GATEWAY=1) "
            f"but not reachable. Schema regression cannot be green-washed."
        )
    pytest.skip(
        f"IntentForge gateway not reachable at {BASE} — skipping schema "
        f"regression (set INTENTFORGE_REQUIRE_GATEWAY=1 to FAIL when down)."
    )


def _require_keys(label, obj, expected):
    """Assert every documented key is present (tolerates extra fields)."""
    assert isinstance(obj, dict), f"{label}: expected JSON object, got {type(obj).__name__}"
    missing = [k for k in expected if k not in obj]
    assert not missing, f"{label}: missing keys {missing}; have {sorted(obj.keys())}"


# 1. Root endpoint
def test_root_schema(session):
    """GET / -> 200, body == 'IntentForge-v2 Gateway'."""
    r = session.get(f"{BASE}/", timeout=5)
    assert r.status_code == 200, f"GET / -> {r.status_code}"
    assert r.text == "IntentForge-v2 Gateway", f"GET / body == {r.text!r}"


# 2. Health endpoint
def test_health_schema(session):
    """GET /health -> 200, body == 'OK'."""
    r = session.get(f"{BASE}/health", timeout=5)
    assert r.status_code == 200, f"GET /health -> {r.status_code}"
    assert r.text == "OK", f"GET /health body == {r.text!r}"


# 3. /search full schema
SEARCH_KEYS = [
    "query",
    "intent",
    "category",
    "confidence",
    "constraints",
    "structured_constraints",
    "expanded_queries",
    "distribution",
    "results",
    "results_before_filter",
    "results_after_filter",
    "total",
    "limit",
    "offset",
    "has_more",
]


def test_search_schema(session):
    """GET /search -> 200, all 15 documented top-level keys present, confidence is numeric."""
    r = session.get(f"{BASE}/search", params={"q": "schema test rust systems"}, timeout=30)
    assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /search", body, SEARCH_KEYS)
    confidence = body.get("confidence")
    assert isinstance(confidence, (int, float)) and not isinstance(confidence, bool), (
        f"GET /search 'confidence' must be a real number, got {type(confidence).__name__}: {confidence!r}"
    )


# 4. /search/fast schema
def test_search_fast_schema(session):
    """GET /search/fast -> 200, keys count/results/source, source == 'local'."""
    r = session.get(f"{BASE}/search/fast", params={"q": "schema fast test rust"}, timeout=30)
    assert r.status_code == 200, f"GET /search/fast -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /search/fast", body, ["count", "results", "source"])
    assert body.get("source") == "local", f"GET /search/fast source != 'local': {body.get('source')!r}"


# 5. /images schema
IMAGE_RESULT_KEYS = ["title", "url", "image_url", "thumbnail_url", "source", "score"]


def test_images_schema(session):
    """GET /images -> 200, keys count/query/results, each result has image fields."""
    r = session.get(f"{BASE}/images", params={"q": "northern lights aurora"}, timeout=30)
    assert r.status_code == 200, f"GET /images -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /images", body, ["count", "query", "results"])
    results = body.get("results", [])
    assert isinstance(results, list), f"GET /images 'results' must be a list, got {type(results).__name__}"
    for i, item in enumerate(results):
        _require_keys(f"GET /images result[{i}]", item, IMAGE_RESULT_KEYS)


# 6. /videos schema
VIDEO_RESULT_KEYS = ["title", "url", "thumbnail", "video_id", "source", "score"]


def test_videos_schema(session):
    """GET /videos -> 200, keys count/query/results, each result has video fields."""
    r = session.get(f"{BASE}/videos", params={"q": "lofi hip hop beats"}, timeout=30)
    assert r.status_code == 200, f"GET /videos -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /videos", body, ["count", "query", "results"])
    results = body.get("results", [])
    assert isinstance(results, list), f"GET /videos 'results' must be a list, got {type(results).__name__}"
    for i, item in enumerate(results):
        _require_keys(f"GET /videos result[{i}]", item, VIDEO_RESULT_KEYS)


# 7. /news schema
NEWS_RESULT_KEYS = ["title", "url", "description", "published_at", "source", "score"]


def test_news_schema(session):
    """GET /news -> 200, keys count/query/results, each result has news fields."""
    r = session.get(f"{BASE}/news", params={"q": "latest ai news"}, timeout=30)
    assert r.status_code == 200, f"GET /news -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /news", body, ["count", "query", "results"])
    results = body.get("results", [])
    assert isinstance(results, list), f"GET /news 'results' must be a list, got {type(results).__name__}"
    for i, item in enumerate(results):
        _require_keys(f"GET /news result[{i}]", item, NEWS_RESULT_KEYS)


# 8. /spellcheck schema (typo path)
def test_spellcheck_typo_schema(session):
    """GET /spellcheck -> 200, keys query/corrected/changed/corrections;
    on a typo (pythn) changed==True and corrections[] non-empty."""
    r = session.get(f"{BASE}/spellcheck", params={"q": "pythn langauge"}, timeout=10)
    assert r.status_code == 200, f"GET /spellcheck -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /spellcheck", body, ["query", "corrected", "changed", "corrections"])
    assert body.get("changed") is True, (
        f"GET /spellcheck on a typo should set changed==True; got {body.get('changed')!r}"
    )
    corrections = body.get("corrections", [])
    assert isinstance(corrections, list) and len(corrections) > 0, (
        f"GET /spellcheck on a typo should yield non-empty corrections[]; got {corrections!r}"
    )


# 9. Negated-brand-negative transparency must not contradict applied constraints
def _neg_terms_from_applied(applied):
    out = set()
    for entry in applied or []:
        if entry.startswith("not:"):
            out.add(entry[len("not:"):].strip())
    return out


def _neg_terms_from_ignored(ignored):
    out = set()
    for entry in ignored or []:
        if entry.startswith("not:"):
            term = entry[len("not:"):].split(" —")[0].strip()
            out.add(term)
    return out


def test_negated_brand_no_applied_ignored_contradiction(session):
    """A negated brand must NOT appear in BOTH applied_constraints and ignored_constraints."""
    q = "wireless headphones price:<100 not sony after:2025-01-01"
    for i in range(5):
        r = session.get(f"{BASE}/search", params={"q": q}, timeout=60)
        assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
        body = r.json()
        applied_neg = _neg_terms_from_applied(body.get("applied_constraints"))
        ignored_neg = _neg_terms_from_ignored(body.get("ignored_constraints"))
        overlap = applied_neg & ignored_neg
        assert not overlap, (
            f"iteration {i}: negated term(s) {sorted(overlap)} appear in BOTH "
            f"applied_constraints and ignored_constraints (contradiction)."
        )


def test_other_brand_negatives_applied_only(session):
    """bose/logitech/nike negatives are enforced (applied) and NOT ignored."""
    for brand in ("bose", "logitech", "nike"):
        q = f"wireless headphones price:<100 not {brand} after:2025-01-01"
        r = session.get(f"{BASE}/search", params={"q": q}, timeout=60)
        assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
        body = r.json()
        applied_neg = _neg_terms_from_applied(body.get("applied_constraints"))
        ignored_neg = _neg_terms_from_ignored(body.get("ignored_constraints"))
        assert brand in applied_neg, (
            f"brand '{brand}' should be enforced (in applied_constraints); "
            f"got applied_neg={sorted(applied_neg)}"
        )
        assert brand not in ignored_neg, (
            f"brand '{brand}' must not be in ignored_constraints; "
            f"got ignored_neg={sorted(ignored_neg)}"
        )


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
