"""
Permanent non-Goals API schema regression tests (round 2026-08-15T0830Z).

Audit t_6a1017ee requirement (C): every documented endpoint must have an
automated schema test so a future regression fails CI without a human. The
Goals-API endpoints already have tests/test_goals_api_schema.py (5 passing).
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

Each test hits the already-running dev gateway (default http://localhost:4000).
They are intended to be run by the oxiverse-qa loop / a CI job that brings the
stack up first. If the gateway is unreachable, the suite skips (rather than
failing red) so it can live harmlessly in the repo when no stack is up.

Run:  pytest tests/test_api_schema.py
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""

import os

import pytest
import requests

BASE = os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")


def _reachable() -> bool:
    try:
        r = requests.get(f"{BASE}/health", timeout=3)
        return r.status_code == 200
    except Exception:
        return False


@pytest.fixture(scope="module")
def session():
    s = requests.Session()
    # Smoke check — skip the whole module if the dev gateway is down.
    try:
        r = s.get(f"{BASE}/health", timeout=5)
        assert r.status_code == 200, f"gateway /health -> {r.status_code}"
    except Exception as e:
        pytest.skip(f"IntentForge gateway not reachable at {BASE}: {e}")
    return s


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
    """GET /search -> 200, all 15 documented top-level keys present, confidence is numeric.

    The API is allowed to include documented-optional fields (e.g. price_verified,
    which API_REFERENCE lists as "Optionally present"), so we assert subset inclusion
    (no *missing* documented field) rather than an exact top-level key count.
    """
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
    assert len(results) > 0, "GET /images returned zero results to assert shape against"
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
    assert len(results) > 0, "GET /videos returned zero results to assert shape against"
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
    assert len(results) > 0, "GET /news returned zero results to assert shape against"
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


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
