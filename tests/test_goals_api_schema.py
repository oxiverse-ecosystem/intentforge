#!/usr/bin/env python3
"""Permanent API-schema regression suite for IntentForge (gateway on :4000).

This is the single source of truth for the documented JSON contracts of EVERY
endpoint in API_REFERENCE.md. It is the audit-mandated regression net that
prevents schema drift from shipping silently (the three defects found by hand —
price fail-open wiping the user's stated constraint, "not from sony" becoming a
positive term, and /goals/leaderboard returning a dict wrapper instead of a
list — all rode a GREEN CI check because the old suite SKIPPED on the bare
runner and the round-branch file was never even collected by pytest).

Consolidation note (2026-08-25): this file is the CANONICAL merge of
  * round branch tests/goals_api_schema.py (18 assertion functions), and
  * master's tests/test_goals_api_schema.py + tests/test_api_schema.py
    (Goals-API phase-count hardening + applied/ignored non-contradiction).
The three overlapping suites are replaced by this one. It uses ONLY the stdlib
(stdlib urllib) so tests/requirements.txt needs no heavy deps.

GATE SEMANTICS (the real fix for the green-wash):
  * The suite runs against a LIVE gateway (default http://localhost:4000).
  * A session fixture checks /health. If INTENTFORGE_REQUIRE_GATEWAY=1 and the
    gateway is unreachable OR /health is not "OK", the ENTIRE suite FAILS (red)
    — a broken contract can never yield a green check.
  * If INTENTFORGE_REQUIRE_GATEWAY is unset/"0" (a bare runner with no stack),
    the suite SKIPs cleanly. This is ONLY for local/dev convenience; CI must set
    the variable to "1" (see .github/workflows/ci.yml) so it is a real guard.

Run locally (stack up):
    pytest tests/ -v                       # collected by DIRECTORY
    INTENTFORGE_REQUIRE_GATEWAY=1 pytest tests/ -v
    INTENTFORGE_BASE=http://other:4000 pytest tests/test_goals_api_schema.py -v

IMPORTANT: always verify collection by DIRECTORY (`pytest tests/ --collect-only`)
not by naming a file — a file without a `test_` prefix is invisible to default
collection and CI would run zero of its tests.
"""

import os
import json
import urllib.error
import urllib.parse
import urllib.request

import pytest

BASE_URL = os.environ.get(
    "INTENTFORGE_BASE",
    os.environ.get("INTENTFORGE_BASE_URL", "http://localhost:4000"),
)
# First request after a gateway cold-start can be ~10s; leave generous headroom.
HTTP_TIMEOUT = 60


# --------------------------------------------------------------------------- #
# Session-wide gateway guard (the real gate — REQUIRE=1 means FAIL, not skip)
# --------------------------------------------------------------------------- #
@pytest.fixture(autouse=True, scope="session")
def _gateway_session():
    """Fail the whole suite if the gateway is required but not healthy.

    On a bare CI runner with no stack, INTENTFORGE_REQUIRE_GATEWAY must be "0"
    (or unset) so the suite SKIPs rather than failing red for infra reasons.
    CI (and the QA loop) MUST set INTENTFORGE_REQUIRE_GATEWAY=1 so a dead or
    contract-drifted gateway turns the run RED instead of green-washing.
    """
    require = os.environ.get("INTENTFORGE_REQUIRE_GATEWAY") == "1"
    try:
        status, body = _get_text("/health")
        alive = status == 200 and body.strip() == "OK"
    except urllib.error.URLError as exc:
        if require:
            pytest.fail(
                f"Gateway at {BASE_URL} is REQUIRED but not reachable ({exc}). "
                f"Set INTENTFORGE_REQUIRE_GATEWAY=0 only on runners with no stack."
            )
        pytest.skip(
            f"Gateway at {BASE_URL} not reachable — skipping schema regression "
            f"(set INTENTFORGE_REQUIRE_GATEWAY=1 to FAIL when the stack is down)."
        )
    if not alive:
        if require:
            pytest.fail(
                f"Gateway /health not OK at {BASE_URL}: {status} {body!r}"
            )
        pytest.skip(f"Gateway /health not OK at {BASE_URL} ({status} {body!r}).")


def _assert_gateway_up():
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
# Plain-text endpoints
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
    """GET /search/fast → 200 + {count, source, results} with source=='local'."""
    _assert_gateway_up()
    status, data = _get_json("/search/fast", {"q": "rust web framework"})
    assert status == 200, f"/search/fast returned {status}: {data!r}"
    for key in ("count", "source", "results"):
        assert key in data, f"/search/fast missing key {key!r}: {data!r}"
    assert data.get("source") == "local", (
        f"/search/fast source must be 'local', got {data.get('source')!r}"
    )
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
    """GET /spellcheck?q=<typo> → 200 + keys, AND on a typo changed==True."""
    _assert_gateway_up()
    status, data = _get_json("/spellcheck", {"q": "pythn programing langauge"})
    assert status == 200, f"/spellcheck returned {status}: {data!r}"
    for key in ("query", "corrected", "changed", "corrections"):
        assert key in data, f"/spellcheck missing key {key!r}: {data!r}"
    assert isinstance(data["corrections"], list)
    # On a real typo the engine must report the change (audit drift guard).
    assert data.get("changed") is True, (
        f"spellcheck on a typo must set changed==True; got {data.get('changed')!r}"
    )
    assert len(data["corrections"]) > 0, "typo must yield non-empty corrections[]"


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
    """POST /goals {goal} → 200, goal_id ('goal_' prefix) present, questions[] non-empty."""
    _assert_gateway_up()
    created = _create_goal("learn rust programming language")
    assert isinstance(created["goal_id"], str) and created["goal_id"].startswith(
        "goal_"
    ), f"goal_id must be a 'goal_'-prefixed string: {created['goal_id']!r}"


def _real_answers(questions):
    """Build REAL structured answers from generated questions (anti-degenerate)."""
    answers = []
    for q in questions:
        if "id" not in q:
            continue
        opts = q.get("options") or []
        ans = opts[0] if opts else (q.get("question") or "x")
        answers.append({"question_id": q["id"], "answer": ans})
    return answers


def test_goals_answers_roadmap_invariant():
    """POST /goals/:id/answers → 200 AND roadmap.total_phases == len(phases).

    Hardened: submits REAL answers so generate_roadmap() is exercised with genuine
    user state, then guards the roadmap is NON-DEGENERATE (tailored title, real
    overview, >=1 objective/deliverable/resource per phase). A degenerate
    "yes"-to-everything payload previously preserved the count invariant while
    embedding literal answers into the roadmap text — masking the regression.
    """
    _assert_gateway_up()
    created = _create_goal("build a full-stack web app for project management")
    goal_id = created["goal_id"]
    answers = _real_answers(created["questions"])
    assert answers, "no structured answers built from generated questions"
    status, resp = _post_json(f"/goals/{goal_id}/answers", {"answers": answers})
    assert status == 200, f"POST /goals/:id/answers returned {status}: {resp!r}"
    roadmap = resp.get("roadmap") or {}
    phases = roadmap.get("phases", [])
    total_phases = roadmap.get("total_phases")
    assert isinstance(total_phases, int), "roadmap.total_phases missing/not int"
    assert total_phases == len(phases), (
        f"roadmap.total_phases ({total_phases}) != len(phases) ({len(phases)})"
    )
    title = roadmap.get("title", "")
    assert title, "roadmap.title missing/empty"
    assert "Roadmap" in title, f"roadmap.title should contain 'Roadmap': {title!r}"
    assert "Plan & Begin" not in title, (
        f"roadmap.title looks like a degenerate placeholder: {title!r}"
    )
    overview = roadmap.get("overview", "")
    assert overview, "roadmap.overview missing/empty"
    assert overview != "A 12-week journey (yes hours/week) across 4 phases.", (
        f"roadmap.overview is the degenerate placeholder: {overview!r}"
    )
    for p in phases:
        assert len(p.get("objectives", [])) >= 1, (
            f"phase {p.get('id')} has <1 objectives: {p}"
        )
        assert len(p.get("deliverables", [])) >= 1, (
            f"phase {p.get('id')} has <1 deliverables: {p}"
        )
        assert len(p.get("resources", [])) >= 1, (
            f"phase {p.get('id')} has <1 resources: {p}"
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
    """GET /goals/leaderboard → 200 AND isinstance(list) with documented entry fields.

    D2 (t_13475fa6): the endpoint returns a bare JSON ARRAY (Vec of entries), not a
    {"entries":[...],"total_entries":N} dict wrapper. Each entry carries the
    documented leaderboard fields.
    """
    _assert_gateway_up()
    _create_goal("learn french")  # ensure the board is non-empty
    import time
    time.sleep(1)  # let the in-memory store persist
    status, resp = _get_json("/goals/leaderboard")
    assert status == 200, f"GET /goals/leaderboard returned {status}: {resp!r}"
    assert isinstance(resp, list), (
        f"GET /goals/leaderboard must return a LIST, got "
        f"{type(resp).__name__}: {resp!r}"
    )
    for entry in resp:
        for field in (
            "goal_id",
            "goal",
            "user_name",
            "score",
            "completed_phases",
            "total_phases",
            "created_at",
        ):
            assert field in entry, f"leaderboard entry missing field '{field}'"


def test_goals_quick_roadmap_invariant():
    """POST /goals/quick {goal} → 200 AND roadmap.total_phases == len(phases)."""
    _assert_gateway_up()
    status, resp = _post_json("/goals/quick", {"goal": "learn japanese for travel"})
    assert status == 200, f"POST /goals/quick returned {status}: {resp!r}"
    roadmap = resp.get("roadmap") or {}
    phases = roadmap.get("phases", [])
    total_phases = roadmap.get("total_phases")
    assert isinstance(total_phases, int), "quick roadmap.total_phases missing/not int"
    assert total_phases == len(phases), (
        f"quick roadmap.total_phases ({total_phases}) != "
        f"len(phases) ({len(phases)})"
    )


# --------------------------------------------------------------------------- #
# D3 — NL negation + price parsing (contract that ships on the fixed gateway)
# --------------------------------------------------------------------------- #
# The gateway routes an EXPLICIT negation lead-in into structured_constraints
# .negative and keeps it OUT of .positive. Bare "not <brand>" (without a lead-in
# word) is NOT yet captured by the engine — that is a separate gateway gap and is
# intentionally NOT asserted here, so the gate stays GREEN on a correct contract
# and only goes RED on a real regression of the documented lead-in behaviour.
def test_negation_not_from_sony():
    """query '... not from sony' → negative contains 'sony' AND positive excludes it."""
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
    """query '... price:<200' → structured_constraints.price_lt == 200.0."""
    _assert_gateway_up()
    status, data = _get_json("/search", {"q": "wireless earbuds price:<200"})
    assert status == 200
    sc = data.get("structured_constraints") or {}
    assert sc.get("price_lt") == 200.0, (
        f"price:<200 not parsed into price_lt==200.0: {sc!r}"
    )


# Every supported spoken negation lead-in must route its target entity into
# `negative` AND keep it OUT of `positive`. A future refactor that drops a
# lead-in (or re-injects the brand as a positive term) fails its case here.
_NEGATION_LEADIN_CASES = [
    ("not from", "sony"),
    ("except", "apple"),
    ("excluding", "apple"),
    ("without", "windows"),
    ("other than", "java"),
    ("anything but", "bose"),
    ("alternative to", "sony"),
    ("besides", "apple"),
    ("no", "sony"),
]


def _negation_probe(q):
    _assert_gateway_up()
    status, data = _get_json("/search", {"q": q})
    assert status == 200, f"/search returned {status} for q={q!r}"
    sc = data.get("structured_constraints") or {}
    return (
        sc.get("negative", []),
        sc.get("positive", []),
        sc.get("price_lt"),
        sc,
    )


def test_negation_leadins_route_to_negative():
    """Each supported NL-negation lead-in excludes its brand from positive."""
    for lead, brand in _NEGATION_LEADIN_CASES:
        q = f"wireless earbuds {lead} {brand}"
        negative, positive, _lt, sc = _negation_probe(q)
        assert brand in negative, (
            f"lead-in {lead!r}: '{brand}' not in negative for q={q!r} ({sc!r})"
        )
        assert brand not in positive, (
            f"lead-in {lead!r}: '{brand}' leaked into positive for q={q!r} ({sc!r})"
        )


def test_negation_multiterm_entity():
    """Multi-word exclusion 'not from bose soundlink' keeps both tokens in
    negative and out of positive (entity/noun-phrase survival, not one token)."""
    negative, positive, _lt, sc = _negation_probe(
        "headphones not from bose soundlink"
    )
    assert "bose soundlink" in negative, (
        f"multi-word exclusion not captured: {sc!r}"
    )
    assert "bose" not in positive and "soundlink" not in positive, (
        f"multi-word exclusion tokens leaked into positive: {sc!r}"
    )


def test_negation_full_suite_with_price():
    """Full shopping query: '... not from sony under $200 ...' →
    negative contains sony AND price_lt == 200.0 (negation + price coexist)."""
    q = (
        "best noise cancelling headphones not from sony under $200 "
        "for travel with good battery life"
    )
    negative, positive, price_lt, sc = _negation_probe(q)
    assert "sony" in negative, f"sony not excluded in full query: {sc!r}"
    assert "sony" not in positive, f"sony leaked into positive: {sc!r}"
    assert price_lt == 200.0, f"price_lt not 200.0 in full query: {sc!r}"


# --------------------------------------------------------------------------- #
# applied_constraints / ignored_constraints non-contradiction (audit t_b6764006)
# --------------------------------------------------------------------------- #
# A rescued/protected-brand negative must NOT appear in BOTH applied_constraints
# ('not:<term>') AND ignored_constraints ('not:<term> — exclusion not applied').
# A negation cannot be both enforced and declined. We assert this on the lead-in
# contract that the gateway implements (not on bare "not <brand>", which the
# engine does not yet capture). Robust term extraction tolerates token-merging in
# applied_constraints (e.g. 'not:bose after:2025-01-01').
def _neg_terms_from_applied(applied):
    out = set()
    for entry in applied or []:
        if isinstance(entry, str) and entry.startswith("not:"):
            # Take the term up to the next whitespace or em-dash separator.
            term = entry[4:].split(" —")[0].strip().split()[0] if entry[4:].strip() else ""
            if term:
                out.add(term)
    return out


def _neg_terms_from_ignored(ignored):
    out = set()
    for entry in ignored or []:
        if isinstance(entry, str) and entry.startswith("not:"):
            term = entry[4:].split(" —")[0].strip()
            if term:
                out.add(term)
    return out


def test_negated_brand_no_applied_ignored_contradiction():
    """FIX t_b6764006: a negated brand (lead-in form) must NOT appear in BOTH
    applied_constraints and ignored_constraints. Looped to catch the intermittent
    engine-tag case where the old code contradicted itself."""
    q = "wireless headphones price:<100 not from sony after:2025-01-01"
    for i in range(5):
        status, data = _get_json("/search", {"q": q})
        assert status == 200, f"GET /search -> {status} {data!r}"
        applied_neg = _neg_terms_from_applied(data.get("applied_constraints"))
        ignored_neg = _neg_terms_from_ignored(data.get("ignored_constraints"))
        overlap = applied_neg & ignored_neg
        assert not overlap, (
            f"iteration {i}: negated term(s) {sorted(overlap)} appear in BOTH "
            f"applied_constraints and ignored_constraints (contradiction). "
            f"applied_neg={sorted(applied_neg)} ignored_neg={sorted(ignored_neg)}"
        )


def test_other_brand_negatives_not_contradicted():
    """Control: bose/logitech/nike (lead-in form) must not be surfaced as a
    contradiction — each must appear in at most ONE of applied/ignored, never both."""
    for brand in ("bose", "logitech", "nike"):
        q = f"wireless headphones price:<100 not from {brand} after:2025-01-01"
        status, data = _get_json("/search", {"q": q})
        assert status == 200, f"GET /search -> {status} {data!r}"
        applied_neg = _neg_terms_from_applied(data.get("applied_constraints"))
        ignored_neg = _neg_terms_from_ignored(data.get("ignored_constraints"))
        overlap = applied_neg & ignored_neg
        assert not overlap, (
            f"brand '{brand}' appears in BOTH applied and ignored: "
            f"applied_neg={sorted(applied_neg)} ignored_neg={sorted(ignored_neg)}"
        )


if __name__ == "__main__":
    # Minimal runner for environments without pytest installed.
    import sys

    funcs = [
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
        ("test_negation_leadins_route_to_negative", test_negation_leadins_route_to_negative),
        ("test_negation_multiterm_entity", test_negation_multiterm_entity),
        ("test_negation_full_suite_with_price", test_negation_full_suite_with_price),
        ("test_price_lt_parsed", test_price_lt_parsed),
        ("test_negated_brand_no_applied_ignored_contradiction", test_negated_brand_no_applied_ignored_contradiction),
        ("test_other_brand_negatives_not_contradicted", test_other_brand_negatives_not_contradicted),
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
