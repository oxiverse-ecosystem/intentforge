"""Permanent wire regressions for the 2026-11 API audit fixes.

These exercise the live gateway (http://localhost:4000 by default) and use a
fresh nonce per call so a warm service-level cache cannot make a broken fix
look correct. Upstream-dependent checks carry @pytest.mark.requires_upstream so
CI's gateway-only job can exclude them.

Run:  INTENTFORGE_REQUIRE_GATEWAY=1 pytest tests/test_api_regressions.py -q
"""

import re
import uuid

import pytest
import requests

BASE = __import__("os").environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000").rstrip("/")


@pytest.fixture(scope="module")
def session(gateway_or_skip):
    return requests.Session()


def _nonce():
    return uuid.uuid4().hex[:10]


def _search(s, q, **params):
    params = {"q": q, **params}
    r = s.get(f"{BASE}/search", params=params, timeout=75)
    assert r.status_code == 200, r.text[:200]
    return r.json()


def _urls(body):
    return [row.get("url", "") for row in body.get("results", [])]


@pytest.mark.requires_upstream
def test_search_cache_key_includes_operators(session):
    """Priming a bare query must not serve that body to an operator variant."""
    token = _nonce()
    plain = _search(session, token, limit=5)
    variant = _search(session, f"after:2024-01-01 {token}", limit=5)

    assert variant.get("query") == f"after:2024-01-01 {token}"
    assert "after:2024-01-01" in (variant.get("applied_constraints") or [])
    assert (variant.get("structured_constraints") or {}).get("after_date") == "2024-01-01"

    # Reverse direction: a primed site: body must not leak to a bare query.
    token2 = _nonce()
    _search(session, f"site:example.org {token2}", limit=5)
    bare = _search(session, token2, limit=5)
    assert bare.get("query") == token2
    assert not any("site:" in str(c) for c in (bare.get("applied_constraints") or []))


@pytest.mark.requires_upstream
def test_date_and_price_operators_are_reported(session):
    before = _search(session, f"before:2025-01-01 python {_nonce()}", limit=5)
    sc = before.get("structured_constraints") or {}
    assert sc.get("before_date") == "2025-01-01"
    assert "before:2025-01-01" in (before.get("applied_constraints") or [])

    priced = _search(session, f"price_min:10 price_max:100 shoes {_nonce()}", limit=5)
    sc = priced.get("structured_constraints") or {}
    assert sc.get("price_min") == 10.0
    assert sc.get("price_max") == 100.0
    assert not any("price_min:" in str(p) for p in sc.get("positive", []))


@pytest.mark.requires_upstream
def test_filetype_constraint_is_enforced_or_disclosed(session):
    body = _search(session, f"filetype:pdf react {_nonce()}", limit=10)
    urls = _urls(body)
    assert urls, "a real query must return results"
    sc = body.get("structured_constraints") or {}
    assert sc.get("file_types") == ["pdf"]
    pdf_urls = [u for u in urls if u.lower().endswith(".pdf")]
    if not pdf_urls:
        notes = " ".join(body.get("ignored_constraints") or [])
        assert "filetype" in notes, "unenforced filetype must be disclosed"


@pytest.mark.requires_upstream
def test_snapshot_pagination_is_stable_and_disjoint(session):
    q = f"{_nonce()} web framework"
    first = _search(session, q, limit=5, offset=0)
    token = first.get("snapshot_id")
    assert token, "first page must return a snapshot_id"
    pages = [_urls(first)]
    for offset in (5, 10):
        page = _search(session, q, limit=5, offset=offset, snapshot_id=token)
        assert page.get("total") == first.get("total")
        assert page.get("snapshot_id") == token
        pages.append(_urls(page))
    for i in range(len(pages)):
        for j in range(i + 1, len(pages)):
            assert not (set(pages[i]) & set(pages[j])), "snapshot pages must not overlap"


@pytest.mark.requires_upstream
def test_videos_return_twenty_rows_with_invidious_source(session):
    r = session.get(f"{BASE}/videos", params={"q": "rust programming"}, timeout=75)
    assert r.status_code == 200, r.text[:200]
    body = r.json()
    rows = body.get("results", [])
    assert body.get("count") == len(rows)
    assert len(rows) == 20, "default /videos page size is 20"
    sources = {row.get("source") for row in rows}
    assert "invidious" in sources, "Invidious must be able to enter /videos"
    youtube_ids = []
    for u in _urls(body):
        match = re.search(r"[?&]v=([^&]+)", u)
        if match:
            youtube_ids.append(match.group(1))
    assert len(youtube_ids) == len(set(youtube_ids)), "no duplicate videos in a page"


def test_framework_errors_are_json_envelopes(session):
    cases = [("GET", "/definitely-not-a-route", 404, "not_found"),
             ("POST", "/search", 405, "method_not_allowed")]
    for method, path, status, code in cases:
        r = session.request(method, f"{BASE}{path}", timeout=20)
        assert r.status_code == status
        body = r.json()
        assert body.get("error") == code
        assert body.get("message")

    r = session.get(f"{BASE}/search", params={"q": "rust", "limit": "abc"}, timeout=20)
    assert r.status_code == 400
    body = r.json()
    assert body.get("error") == "invalid_parameter"
    assert "limit" in body.get("message", "")
    assert isinstance(body.get("structured_constraints"), dict)
