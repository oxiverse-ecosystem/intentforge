#!/usr/bin/env python3
"""Permanent API-schema regression suite for IntentForge (gateway on :4000).

This is the audit-mandated regression net (round 2026-08-22T0703Z, defect C):
the Goals API and the documented endpoint contracts had ZERO automated tests,
so schema drift shipped silently. This file locks the documented shape of EVERY
endpoint in API_REFERENCE.md so CI / the QA loop catches drift without a human.

It runs against a LIVE gateway (default http://localhost:4000). The QA loop
already brings the stack up before running it; in CI the BASE_URL must point at
a running gateway (see .github/workflows/ci.yml — the `schema-regression` job
skips cleanly when no gateway is reachable, so it is a no-op on a bare runner
and a real guard on a runner that brings the stack up).

Run locally:
    pytest tests/goals_api_schema.py -v
    INTENTFORGE_BASE=http://other-host:4000 pytest tests/goals_api_schema.py -v

Expected-fail → pass transition (documented for the PR):
  * D1 (roadmap.total_phases)  — FIXED (t_cebacba8); asserted directly, GREEN.
  * D2 (/goals/leaderboard LIST) — FIXED (t_13475fa6); asserted directly, GREEN.
  * D3 (NL negation + price:<N parsing) — FIXED in part during the round:
    - price:<N parsing (price_lt) had LANDED by commit time → test_price_lt_parsed
      is GREEN.
    - 'not from <brand>' negation is still PENDING (t_fix_d3) →
      test_negation_not_from_sony is RED until that half lands.
    All assertions are plain pass/fail (no xfail), so each D3 sub-fix flips its
    test to GREEN automatically the moment the gateway redeploys the fix.
"""

import os
import json
import urllib.error
import urllib.parse
import urllib.request

import pytest

BASE_URL = os.environ.get(
    "INTENTFORGE_BASE", os.environ.get("BASE_URL", "http://localhost:4000")
)
# First request after a gateway cold-start can be ~10s; leave generous headroom.
HTTP_TIMEOUT = 60


# --------------------------------------------------------------------------- #
# Session-wide gateway guard
# --------------------------------------------------------------------------- #
@pytest.fixture(autouse=True, scope="session")
def _gateway_session():
    """Skip the entire schema suite if no gateway is reachable (unless required).

    On a bare CI runner there is no IntentForge stack, so the suite must be a
    clean no-op (SKIP), not a wall of failures. When the QA loop brings the
    stack up and sets INTENTFORGE_REQUIRE_GATEWAY=1, an unreachable gateway is a
    hard FAIL — the real regression guard.
    """
    require = os.environ.get("INTENTFORGE_REQUIRE_GATEWAY") == "1"
    try:
        status, body = _get_text("/health")
        alive = status == 200 and body.strip() == "OK"
    except urllib.error.URLError as exc:
        if require:
            pytest.fail(
                f"Gateway at {BASE_URL} is REQUIRED but not reachable ({exc})."
            )
        pytest.skip(
            f"Gateway at {BASE_URL} not reachable — skipping schema regression "
            f"(set INTENTFORGE_REQUIRE_GATEWAY=1 to fail when the stack is down)."
        )
    if not alive:
        if require:
            pytest.fail(f"Gateway /health not OK at {BASE_URL}: {status} {body!r}")
        pytest.skip(f"Gateway /health not OK at {BASE_URL} ({status} {body!r}).")


def _assert_gateway_up():
    """Per-test guard (redundant with the session fixture once the stack is up).

    Kept for explicitness at each endpoint test and for use when invoked as a
    plain script. The session fixture already skips the module if the gateway is
    down, so this only ever runs against a live gateway.
    """
    status, body = _get_text("/health")
    assert status == 200 and body.strip() == "OK", (
        f"/health not OK: {status} {body!r}"
    )


# --------------------------------------------------------------------------- #
# Transport helpers
# --------------------------------------------------------------------------- #
def _get_json(path, params=None):
    url = BASE_URL + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=HTTP_TIMEOUT) as r:
        return r.status, json.loads(r.read().decode())


def _post_json(path, payload):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        BASE_URL + path,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT) as r:
        return r.status, json.loads(r.read().decode())


def _get_text(path, params=None):
    url = BASE_URL + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=HTTP_TIMEOUT) as r:
        return r.status, r.read().decode()


# --------------------------------------------------------------------------- #
# Plain-text endpoints (the session fixture already guarantees a live gateway)
# --------------------------------------------------------------------------- #
def test_root_identifier():
    status, body = _get_text("/")
    assert status == 200
    assert "IntentForge-v2 Gateway" in body


def test_health_ok():
    status, body = _get_text("/health")
    assert status == 200
    assert body.strip() == "OK"


# --------------------------------------------------------------------------- #
# /search family
# --------------------------------------------------------------------------- #
SEARCH_TOPLEVEL_KEYS = [
    "query",
    "intent",
    "category",
    "confidence",
    "constraints",
    "structured_constraints",
    "distribution",
    "results",
    "results_before_filter",
    "results_after_filter",
    "total",
    "limit",
    "offset",
    "has_more",
]


def test_search_schema():
    """GET /search?q=<complex multi-constraint NL> → 200 + documented keys."""
    _assert_gateway_up()
    q = (
        "best noise cancelling headphones not from sony under $200 "
        "for travel with good battery life"
    )
    status, data = _get_json("/search", {"q": q})
    assert status == 200, f"/search returned {status}: {data!r}"
    missing = [k for k in SEARCH_TOPLEVEL_KEYS if k not in data]
    assert not missing, f"/search missing top-level keys: {missing}"
    assert isinstance(data["confidence"], float), (
        f"confidence must be a float, got {type(data['confidence']).__name__}"
    )


def test_search_fast_schema():
    """GET /search/fast → 200 + {count, source, results}."""
    _assert_gateway_up()
    status, data = _get_json("/search/fast", {"q": "rust web framework"})
    assert status == 200, f"/search/fast returned {status}: {data!r}"
    for key in ("count", "source", "results"):
        assert key in data, f"/search/fast missing key {key!r}: {data!r}"
    assert isinstance(data["results"], list)


# --------------------------------------------------------------------------- #
# Media endpoints
# --------------------------------------------------------------------------- #
def test_images_schema():
    """GET /images → 200 + {count, query, results[]} with image_url+thumbnail_url."""
    _assert_gateway_up()
    status, data = _get_json("/images", {"q": "rust programming"})
    assert status == 200, f"/images returned {status}: {data!r}"
    for key in ("count", "query", "results"):
        assert key in data, f"/images missing top-level key {key!r}"
    assert isinstance(data["results"], list) and data["results"], (
        "/images returned an empty result list"
    )
    first = data["results"][0]
    assert "image_url" in first and "thumbnail_url" in first, (
        f"/images result missing image_url/thumbnail_url: {first!r}"
    )


def test_videos_schema():
    """GET /videos → 200 + {count, query, results[]} with thumbnail+video_id."""
    _assert_gateway_up()
    status, data = _get_json("/videos", {"q": "rust tutorial"})
    assert status == 200, f"/videos returned {status}: {data!r}"
    for key in ("count", "query", "results"):
        assert key in data, f"/videos missing top-level key {key!r}"
    assert isinstance(data["results"], list) and data["results"], (
        "/videos returned an empty result list"
    )
    first = data["results"][0]
    assert "thumbnail" in first and "video_id" in first, (
        f"/videos result missing thumbnail/video_id: {first!r}"
    )


def test_news_schema():
    """GET /news → 200 + {count, query, results[]} with published_at."""
    _assert_gateway_up()
    status, data = _get_json("/news", {"q": "artificial intelligence"})
    assert status == 200, f"/news returned {status}: {data!r}"
    for key in ("count", "query", "results"):
        assert key in data, f"/news missing top-level key {key!r}"
    assert isinstance(data["results"], list) and data["results"], (
        "/news returned an empty result list"
    )
    first = data["results"][0]
    assert "published_at" in first, (
        f"/news result missing published_at: {first!r}"
    )


def test_spellcheck_schema():
    """GET /spellcheck?q=<typo> → 200 + {query, corrected, changed, corrections[]}."""
    _assert_gateway_up()
    status, data = _get_json("/spellcheck", {"q": "pythn programing langauge"})
    assert status == 200, f"/spellcheck returned {status}: {data!r}"
    for key in ("query", "corrected", "changed", "corrections"):
        assert key in data, f"/spellcheck missing key {key!r}: {data!r}"
    assert isinstance(data["corrections"], list)


# --------------------------------------------------------------------------- #
# Goals API
# --------------------------------------------------------------------------- #
def _create_goal(goal_text):
    status, created = _post_json("/goals", {"goal": goal_text})
    assert status == 200, f"POST /goals returned {status}: {created!r}"
    assert "goal_id" in created, f"POST /goals missing goal_id: {created!r}"
    questions = created.get("questions", [])
    assert isinstance(questions, list) and len(questions) > 0, (
        f"POST /goals questions[] must be non-empty: {created!r}"
    )
    return created


def test_goals_create_schema():
    """POST /goals {goal} → 200, goal_id present, questions[] non-empty."""
    _assert_gateway_up()
    _create_goal("learn rust programming language")


def test_goals_answers_roadmap_invariant():
    """POST /goals/:id/answers → 200 AND roadmap.total_phases == len(phases).

    D1 follow-through (t_cebacba8): the Roadmap struct now carries total_phases
    and generate_roadmap() sets it to the phase count.
    """
    _assert_gateway_up()
    created = _create_goal("build a full-stack web app for project management")
    goal_id = created["goal_id"]
    answers = [
        {"question_id": q["id"], "answer": "moderate"}
        for q in created["questions"]
    ]
    status, resp = _post_json(f"/goals/{goal_id}/answers", {"answers": answers})
    assert status == 200, f"POST /goals/:id/answers returned {status}: {resp!r}"
    roadmap = resp.get("roadmap") or {}
    assert "total_phases" in roadmap, (
        f"roadmap.total_phases missing from {json.dumps(roadmap)[:300]}"
    )
    assert roadmap["total_phases"] == len(roadmap.get("phases", [])), (
        f"roadmap.total_phases ({roadmap.get('total_phases')}) != "
        f"len(phases) ({len(roadmap.get('phases', []))})"
    )


def test_goals_get_status():
    """GET /goals/:id → 200, status present."""
    _assert_gateway_up()
    created = _create_goal("learn spanish language")
    goal_id = created["goal_id"]
    status, resp = _get_json(f"/goals/{goal_id}")
    assert status == 200, f"GET /goals/:id returned {status}: {resp!r}"
    assert "status" in resp, f"GET /goals/:id missing status: {resp!r}"


def test_goals_leaderboard_is_list():
    """GET /goals/leaderboard → 200 AND isinstance(list).

    D2 (t_13475fa6): the endpoint now returns a bare JSON ARRAY (Vec of
    entries), not a {"entries":[...],"total_entries":N} dict wrapper.
    """
    _assert_gateway_up()
    status, resp = _get_json("/goals/leaderboard")
    assert status == 200, f"GET /goals/leaderboard returned {status}: {resp!r}"
    assert isinstance(resp, list), (
        f"GET /goals/leaderboard must return a LIST, got "
        f"{type(resp).__name__}: {resp!r}"
    )


def test_goals_quick_roadmap_invariant():
    """POST /goals/quick {goal} → 200 AND roadmap.total_phases == len(phases).

    D1 follow-through for the one-shot endpoint.
    """
    _assert_gateway_up()
    status, resp = _post_json(
        "/goals/quick", {"goal": "learn japanese for travel"}
    )
    assert status == 200, f"POST /goals/quick returned {status}: {resp!r}"
    roadmap = resp.get("roadmap") or {}
    assert "total_phases" in roadmap, (
        f"quick roadmap.total_phases missing from {json.dumps(roadmap)[:300]}"
    )
    assert roadmap["total_phases"] == len(roadmap.get("phases", [])), (
        f"quick roadmap.total_phases ({roadmap.get('total_phases')}) != "
        f"len(phases) ({len(roadmap.get('phases', []))})"
    )


# --------------------------------------------------------------------------- #
# D3 — NL negation + price parsing (gated on t_fix_d3)
# These assert the DOCUMENTED contract for structured_constraints. They track
# the live state of t_fix_d3: as its sub-fixes land, the relevant assertion
# flips to GREEN automatically. At the time this suite was authored, the
# gateway had NOT implemented D3; during the round the price half landed
# (price_lt populated) while the negation half was still pending
# ('not from <brand>' still absorbed into positive).
# --------------------------------------------------------------------------- #
def test_negation_not_from_sony():
    """query '... not from sony' → structured_constraints.negative contains
    'sony' AND positive excludes it.

    Gated on the negation half of t_fix_d3: until 'not from <brand>' is routed
    to structured_constraints.negative, this is RED.
    """
    _assert_gateway_up()
    status, data = _get_json("/search", {"q": "wireless earbuds not from sony"})
    assert status == 200
    sc = data.get("structured_constraints") or {}
    negative = sc.get("negative", [])
    positive = sc.get("positive", [])
    assert "sony" in negative, (
        f"negation 'not from sony' not captured in negative: {sc!r}"
    )
    assert "sony" not in positive, (
        f"'sony' must be excluded from positive on a negated query: {sc!r}"
    )


def test_price_lt_parsed():
    """query '... price:<200' → structured_constraints.price_lt == 200.0.

    Gated on the price half of t_fix_d3. GREEN once 'price:<N' is parsed into
    price_lt (this half had landed by the time the suite was committed).
    """
    _assert_gateway_up()
    status, data = _get_json("/search", {"q": "wireless earbuds price:<200"})
    assert status == 200
    sc = data.get("structured_constraints") or {}
    assert sc.get("price_lt") == 200.0, (
        f"price:<200 not parsed into price_lt==200.0: {sc!r}"
    )


if __name__ == "__main__":
    # Minimal runner for environments without pytest installed.
    import sys

    funcs = [
        ("test_gateway_reachable", test_gateway_reachable),
        ("test_root_identifier", test_root_identifier),
        ("test_health_ok", test_health_ok),
        ("test_search_schema", test_search_schema),
        ("test_search_fast_schema", test_search_fast_schema),
        ("test_images_schema", test_images_schema),
        ("test_videos_schema", test_videos_schema),
        ("test_news_schema", test_news_schema),
        ("test_spellcheck_schema", test_spellcheck_schema),
        ("test_goals_create_schema", test_goals_create_schema),
        ("test_goals_answers_roadmap_invariant", test_goals_answers_roadmap_invariant),
        ("test_goals_get_status", test_goals_get_status),
        ("test_goals_leaderboard_is_list", test_goals_leaderboard_is_list),
        ("test_goals_quick_roadmap_invariant", test_goals_quick_roadmap_invariant),
        ("test_negation_not_from_sony", test_negation_not_from_sony),
        ("test_price_lt_parsed", test_price_lt_parsed),
    ]
    failed = []
    for name, fn in funcs:
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            failed.append((name, repr(exc)))
            print(f"FAIL {name}: {exc}")
    sys.exit(1 if failed else 0)
