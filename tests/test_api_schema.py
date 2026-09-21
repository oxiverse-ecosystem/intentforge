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


# 8. /intent schema
def test_intent_schema(session):
    """GET /intent -> 200, keys query/intent/category/confidence/contrastive_framing/local_intent/structured_constraints/expanded_queries."""
    r = session.get(f"{BASE}/intent", params={"q": "how to learn python"}, timeout=10)
    assert r.status_code == 200, f"GET /intent -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /intent",
        body,
        [
            "query",
            "intent",
            "category",
            "confidence",
            "contrastive_framing",
            "local_intent",
            "structured_constraints",
            "expanded_queries",
        ],
    )
    assert isinstance(body.get("intent"), str) and body["intent"], (
        f"GET /intent 'intent' must be a non-empty string; got {body.get('intent')!r}"
    )
    assert isinstance(body.get("category"), str) and body["category"], (
        f"GET /intent 'category' must be a non-empty string; got {body.get('category')!r}"
    )
    conf = body.get("confidence")
    assert isinstance(conf, (int, float)) and not isinstance(conf, bool), (
        f"GET /intent 'confidence' must be a real number; got {type(conf).__name__}: {conf!r}"
    )
    assert isinstance(body.get("contrastive_framing"), bool), (
        f"GET /intent 'contrastive_framing' must be a bool; got {type(body.get('contrastive_framing')).__name__}"
    )
    assert isinstance(body.get("local_intent"), bool), (
        f"GET /intent 'local_intent' must be a bool; got {type(body.get('local_intent')).__name__}"
    )
    assert isinstance(body.get("structured_constraints"), dict), (
        f"GET /intent 'structured_constraints' must be a dict; got {type(body.get('structured_constraints')).__name__}"
    )
    assert isinstance(body.get("expanded_queries"), list) and len(body["expanded_queries"]) > 0, (
        f"GET /intent 'expanded_queries' must be a non-empty list; got {body.get('expanded_queries')!r}"
    )


def test_intent_contrastive_framing_signal(session):
    """GET /intent on an X-vs-Y query -> contrastive_framing == true."""
    r = session.get(f"{BASE}/intent", params={"q": "violin vs viola for beginner"}, timeout=10)
    assert r.status_code == 200, f"GET /intent -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert body.get("contrastive_framing") is True, (
        f"GET /intent on a vs-query should set contrastive_framing==True; got {body.get('contrastive_framing')!r}"
    )


# 9. /analyze schema
def test_analyze_schema(session):
    """GET /analyze -> 200, keys query/contrastive_framing/exclusions/declined/manner_qualifiers/decisions."""
    r = session.get(
        f"{BASE}/analyze",
        params={"q": "javascript not java not typescript"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /analyze -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /analyze",
        body,
        ["query", "contrastive_framing", "exclusions", "declined", "manner_qualifiers", "decisions"],
    )
    assert isinstance(body.get("contrastive_framing"), bool), (
        f"GET /analyze 'contrastive_framing' must be a bool; got {type(body.get('contrastive_framing')).__name__}"
    )
    for bucket in ("exclusions", "declined", "manner_qualifiers", "decisions"):
        assert isinstance(body.get(bucket), list), (
            f"GET /analyze '{bucket}' must be a list; got {type(body.get(bucket)).__name__}"
        )
    # Contrastive framing: every negation candidate must appear in exactly one bucket.
    exclusions = body.get("exclusions", [])
    declined = body.get("declined", [])
    manner = body.get("manner_qualifiers", [])
    seen = set(exclusions) | set(declined) | set(manner)
    decisions = [d.get("term") for d in body.get("decisions", []) if d.get("term")]
    assert seen == set(decisions), (
        f"GET /analyze bucket union {sorted(seen)} != decisions terms {sorted(decisions)}"
    )


def test_analyze_manner_qualifiers(session):
    """GET /analyze on a 'without X' query -> manner_qualifiers populated, exclusions empty."""
    r = session.get(
        f"{BASE}/analyze",
        params={"q": "best way to cook salmon without an oven"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /analyze -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert "oven" in body.get("manner_qualifiers", []), (
        f"GET /analyze 'oven' should be in manner_qualifiers for 'without an oven'; got {body.get('manner_qualifiers')!r}"
    )
    assert "oven" not in body.get("exclusions", []), (
        f"GET /analyze manner term 'oven' must NOT appear in exclusions; got {body.get('exclusions')!r}"
    )


# 10. /inspect schema
def test_inspect_schema(session):
    """GET /inspect -> 200, keys query/spelling/negation/intent/constraints/recency/quality;
    constraints carries 'applied_constraints'."""
    r = session.get(
        f"{BASE}/inspect",
        params={"q": "python web framework not django"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /inspect -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /inspect",
        body,
        ["query", "spelling", "negation", "intent", "constraints", "recency", "quality"],
    )
    constraints = body.get("constraints")
    assert isinstance(constraints, dict), (
        f"GET /inspect 'constraints' must be a dict; got {type(constraints).__name__}"
    )
    assert "applied_constraints" in constraints, (
        f"GET /inspect 'constraints' missing 'applied_constraints'; have {sorted(constraints.keys())}"
    )
    assert isinstance(constraints["applied_constraints"], list), (
        f"GET /inspect 'applied_constraints' must be a list; got {type(constraints['applied_constraints']).__name__}"
    )
    spelling = body.get("spelling", {})
    assert isinstance(spelling, dict), (
        f"GET /inspect 'spelling' must be a dict; got {type(spelling).__name__}"
    )
    assert "corrected" in spelling, "GET /inspect 'spelling' missing 'corrected'"
    assert "changed" in spelling, "GET /inspect 'spelling' missing 'changed'"


def test_inspect_recency_detection(session):
    """GET /inspect on a 'latest this week' query -> recency.phrase_detected == true."""
    r = session.get(
        f"{BASE}/inspect",
        params={"q": "latest AI news this week"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /inspect -> {r.status_code} {r.text[:300]}"
    body = r.json()
    recency = body.get("recency", {})
    assert recency.get("phrase_detected") is True, (
        f"GET /inspect 'recency.phrase_detected' should be True for 'this week'; got {recency}"
    )


# 11. /geolocate schema
def test_geolocate_schema(session):
    """GET /geolocate -> 200, keys query/resolved/source/explicit_location/local_intent."""
    r = session.get(
        f"{BASE}/geolocate",
        params={"q": "quiet places to study near chennai"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /geolocate -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /geolocate",
        body,
        ["query", "resolved", "source", "explicit_location", "local_intent"],
    )
    assert isinstance(body.get("explicit_location"), bool), (
        f"GET /geolocate 'explicit_location' must be a bool; got {type(body.get('explicit_location')).__name__}"
    )
    assert isinstance(body.get("local_intent"), bool), (
        f"GET /geolocate 'local_intent' must be a bool; got {type(body.get('local_intent')).__name__}"
    )
    assert body.get("source") == "explicit", (
        f"GET /geolocate on a gazetteer place should yield source=='explicit'; got {body.get('source')!r}"
    )
    resolved = body.get("resolved")
    assert isinstance(resolved, dict), (
        f"GET /geolocate 'resolved' must be a dict when source=='explicit'; got {type(resolved).__name__}"
    )
    assert resolved.get("city") == "chennai", (
        f"GET /geolocate resolved city should be 'chennai'; got {resolved.get('city')!r}"
    )


def test_geolocate_no_signal(session):
    """GET /geolocate on a non-geo query -> source == 'none', resolved == null."""
    r = session.get(
        f"{BASE}/geolocate",
        params={"q": "how does a cpu pipeline work"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /geolocate -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert body.get("source") == "none", (
        f"GET /geolocate on a non-geo query should yield source=='none'; got {body.get('source')!r}"
    )
    assert body.get("resolved") is None, (
        f"GET /geolocate on a non-geo query should yield resolved==None; got {body.get('resolved')!r}"
    )


# 12. /video schema
def test_video_schema(session):
    """GET /video -> 200, keys query/video_intent/video_intent_markers/would_pin_non_video_sources/is_video_source_examples/intent/note."""
    r = session.get(
        f"{BASE}/video",
        params={"q": "rust vs go high concurrency servers"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /video -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /video",
        body,
        [
            "query",
            "video_intent",
            "video_intent_markers",
            "would_pin_non_video_sources",
            "is_video_source_examples",
            "intent",
            "note",
        ],
    )
    assert isinstance(body.get("video_intent"), bool), (
        f"GET /video 'video_intent' must be a bool; got {type(body.get('video_intent')).__name__}"
    )
    assert isinstance(body.get("would_pin_non_video_sources"), bool), (
        f"GET /video 'would_pin_non_video_sources' must be a bool; got {type(body.get('would_pin_non_video_sources')).__name__}"
    )
    markers = body.get("video_intent_markers")
    assert isinstance(markers, list) and len(markers) > 0, (
        f"GET /video 'video_intent_markers' must be a non-empty list; got {markers!r}"
    )
    assert isinstance(body.get("is_video_source_examples"), dict), (
        f"GET /video 'is_video_source_examples' must be a dict; got {type(body.get('is_video_source_examples')).__name__}"
    )


def test_video_intent_exempts_pin(session):
    """GET /video on a video-intent query -> video_intent == true, would_pin_non_video_sources == false."""
    r = session.get(
        f"{BASE}/video",
        params={"q": "best youtube tutorial for rust async"},
        timeout=10,
    )
    assert r.status_code == 200, f"GET /video -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert body.get("video_intent") is True, (
        f"GET /video on a video query should set video_intent==True; got {body.get('video_intent')!r}"
    )
    assert body.get("would_pin_non_video_sources") is False, (
        f"GET /video on a video query should set would_pin_non_video_sources==False; got {body.get('would_pin_non_video_sources')!r}"
    )


# 13. /shopping schema
def test_shopping_schema(session):
    """GET /shopping -> 200, keys query/results; each result may carry commerce + commerce_provenance."""
    r = session.get(
        f"{BASE}/shopping",
        params={"q": "best wireless earbuds under 50", "count": "3"},
        timeout=60,
    )
    assert r.status_code == 200, f"GET /shopping -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys("GET /shopping", body, ["query", "results"])
    results = body.get("results")
    assert isinstance(results, list), (
        f"GET /shopping 'results' must be a list; got {type(results).__name__}"
    )
    assert len(results) > 0, "GET /shopping returned zero results to assert shape against"
    # At least one result should have a commerce_provenance (honest-facts contract).
    has_provenance = any("commerce_provenance" in item for item in results)
    assert has_provenance, (
        f"GET /shopping every result must carry 'commerce_provenance'; none found in {len(results)} results"
    )
    # commerce_provenance structure check on the first result that has one.
    for item in results:
        prov = item.get("commerce_provenance")
        if isinstance(prov, dict):
            for field in ("url", "observed_at", "source"):
                assert field in prov, (
                    f"GET /shopping 'commerce_provenance' missing '{field}'; have {sorted(prov.keys())}"
                )
            break


# 14. /goals/:id/progress schema
def test_goals_progress_schema(session):
    """GET /goals/:id/progress -> 200, keys goal_id/goal/status/completed_phases/total_phases/score/roadmap."""
    # Step 1: create a goal.
    r_create = session.post(
        f"{BASE}/goals",
        json={"goal": "learn rust for systems programming in 6 months"},
        timeout=30,
    )
    assert r_create.status_code == 200, f"POST /goals -> {r_create.status_code} {r_create.text[:300]}"
    goal_id = r_create.json().get("goal_id")
    assert isinstance(goal_id, str) and goal_id.startswith("goal_"), (
        f"POST /goals response missing 'goal_id' starting with 'goal_'; got {goal_id!r}"
    )

    # Step 2: GET /goals/:id/progress BEFORE answers submitted -> status == "pending_answers".
    r = session.get(f"{BASE}/goals/{goal_id}/progress", timeout=10)
    assert r.status_code == 200, f"GET /goals/{goal_id}/progress -> {r.status_code} {r.text[:300]}"
    body = r.json()
    _require_keys(
        "GET /goals/:id/progress",
        body,
        ["goal_id", "goal", "status", "completed_phases", "total_phases", "score", "roadmap"],
    )
    assert body.get("status") == "pending_answers", (
        f"GET /goals/:id/progress before answers should yield status=='pending_answers'; got {body.get('status')!r}"
    )
    assert isinstance(body.get("completed_phases"), int), (
        f"GET /goals/:id/progress 'completed_phases' must be an int; got {type(body.get('completed_phases')).__name__}"
    )
    assert isinstance(body.get("total_phases"), int), (
        f"GET /goals/:id/progress 'total_phases' must be an int; got {type(body.get('total_phases')).__name__}"
    )

    # Step 3: submit answers to get a roadmap.
    questions = r_create.json().get("questions", [])
    answers = []
    for q in questions:
        if "id" not in q:
            continue
        opts = q.get("options") or []
        ans = opts[0] if opts else (q.get("question") or "x")
        answers.append({"question_id": q["id"], "answer": ans})
    if not answers:
        # Fallback: build a minimal answer set from the goal itself.
        answers = [{"question_id": "q1", "answer": "6 months"}]

    r_answers = session.post(
        f"{BASE}/goals/{goal_id}/answers",
        json={"answers": answers},
        timeout=60,
    )
    assert r_answers.status_code == 200, (
        f"POST /goals/{goal_id}/answers -> {r_answers.status_code} {r_answers.text[:300]}"
    )

    # Step 4: GET /goals/:id/progress AFTER answers submitted -> status active/completed, roadmap present.
    r2 = session.get(f"{BASE}/goals/{goal_id}/progress", timeout=10)
    assert r2.status_code == 200, f"GET /goals/{goal_id}/progress (post-answers) -> {r2.status_code}"
    body2 = r2.json()
    _require_keys(
        "GET /goals/:id/progress (post-answers)",
        body2,
        ["goal_id", "goal", "status", "completed_phases", "total_phases", "score", "roadmap"],
    )
    assert body2.get("status") in ("active", "completed"), (
        f"GET /goals/:id/progress after answers should be 'active' or 'completed'; got {body2.get('status')!r}"
    )
    roadmap2 = body2.get("roadmap")
    assert isinstance(roadmap2, dict) and roadmap2, (
        f"GET /goals/:id/progress 'roadmap' must be a non-empty dict after answers; got {type(roadmap2).__name__}"
    )
    assert "total_phases" in roadmap2, (
        f"GET /goals/:id/progress 'roadmap' missing 'total_phases'; have {sorted(roadmap2.keys())}"
    )
    assert roadmap2["total_phases"] == body2.get("total_phases"), (
        f"roadmap.total_phases ({roadmap2['total_phases']}) != body.total_phases ({body2.get('total_phases')})"
    )


def test_goals_progress_not_found(session):
    """GET /goals/:id/progress on a nonexistent id -> 404."""
    r = session.get(f"{BASE}/goals/goal_does_not_exist_xyz/progress", timeout=10)
    assert r.status_code == 404, (
        f"GET /goals/nonexistent/progress should return 404; got {r.status_code} {r.text[:300]}"
    )


# 15. /spellcheck schema (typo path)
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
    """Extract the set of negative terms reported in applied_constraints.

    applied_constraints entries look like 'not:sony' / 'site:...' / 'price:<100'.
    Only the 'not:<term>' entries are genuine negations.
    """
    out = set()
    for entry in applied or []:
        if entry.startswith("not:"):
            out.add(entry[len("not:"):].strip())
    return out


def _neg_terms_from_ignored(ignored):
    """Extract the set of negative terms named in ignored_constraints.

    ignored_constraints entries look like
    'not:sony — exclusion not applied (...)'. The term is the text before ' —'.
    """
    out = set()
    for entry in ignored or []:
        if entry.startswith("not:"):
            term = entry[len("not:"):].split(" —")[0].strip()
            out.add(term)
    return out


def test_negated_brand_no_applied_ignored_contradiction(session):
    """FIX t_b6764006: a rescued protected-brand negative (e.g. 'sony') must NOT
    appear in BOTH applied_constraints ('not:sony') AND ignored_constraints
    ('not:sony — exclusion not applied ...'). A negation cannot be both enforced
    and declined.

    The intent engine tags 'sony' as an Exclusion non-deterministically, so we
    loop the query several times to catch the intermittent case where the engine
    DID tag it (which is exactly when the old code would contradict itself).
    """
    q = "wireless headphones price:<100 not sony after:2025-01-01"
    for i in range(5):
        r = session.get(f"{BASE}/search", params={"q": q}, timeout=60)
        assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
        body = r.json()
        applied = body.get("applied_constraints")
        ignored = body.get("ignored_constraints")
        applied_neg = _neg_terms_from_applied(applied)
        ignored_neg = _neg_terms_from_ignored(ignored)
        overlap = applied_neg & ignored_neg
        assert not overlap, (
            f"iteration {i}: negated term(s) {sorted(overlap)} appear in BOTH "
            f"applied_constraints and ignored_constraints (contradiction). "
            f"applied_neg={sorted(applied_neg)} ignored_neg={sorted(ignored_neg)}"
        )


def test_other_brand_negatives_applied_only(session):
    """Control: bose/logitech/nike negatives are enforced (applied) and must NOT
    be surfaced as ignored (they were never in the contradiction class).
    """
    for brand in ("bose", "logitech", "nike"):
        q = f"wireless headphones price:<100 not {brand} after:2025-01-01"
        r = session.get(f"{BASE}/search", params={"q": q}, timeout=60)
        assert r.status_code == 200, f"GET /search -> {r.status_code} {r.text[:300]}"
        body = r.json()
        applied = body.get("applied_constraints")
        ignored = body.get("ignored_constraints")
        applied_neg = _neg_terms_from_applied(applied)
        ignored_neg = _neg_terms_from_ignored(ignored)
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
