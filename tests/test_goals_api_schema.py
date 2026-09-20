"""
Permanent Goals-API schema regression tests (round 2026-08-15T0326Z).

These assert the schema invariants the independent QA audit requires, so a
future regression fails CI without a human noticing:

  1. POST /goals/{id}/answers  -> 200 AND roadmap.total_phases == len(roadmap.phases)
  2. POST /goals/quick         -> 200 AND roadmap.total_phases == len(roadmap.phases)
  3. GET  /goals/leaderboard   -> 200 AND response is a LIST (JSON array)
  4. POST /goals               -> 200, goal_id present, questions[] non-empty
  5. GET  /goals/{id}          -> 200, status present
  6. GET  /goals/leaderboard   -> 200 (smoke)

The tests hit the already-running dev gateway (default http://localhost:4000).
They are intended to be run by the oxiverse-qa loop / a CI job that brings the
stack up first. If the gateway is unreachable, the suite skips (rather than
failing red) so it can live harmlessly in the repo when no stack is up.

Run:  pytest tests/test_goals_api_schema.py
Env:  INTENTFORGE_BASE_URL (default http://localhost:4000)
"""

import os
import time

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


def _create_goal(s, goal_text="learn rust for systems programming in 6 months"):
    r = s.post(f"{BASE}/goals", json={"goal": goal_text}, timeout=30)
    assert r.status_code == 200, f"POST /goals -> {r.status_code} {r.text[:300]}"
    body = r.json()
    assert "goal_id" in body and body["goal_id"], "no goal_id in create response"
    questions = body.get("questions", [])
    assert isinstance(questions, list) and len(questions) > 0, "questions[] empty"
    return body["goal_id"], questions


def test_create_goal_schema(session):
    """#4 POST /goals -> 200, goal_id present, questions[] non-empty."""
    goal_id, _ = _create_goal(session)
    assert isinstance(goal_id, str) and goal_id.startswith("goal_")


def test_get_goal_schema(session):
    """#5 GET /goals/{id} -> 200, status present."""
    goal_id, _ = _create_goal(session)
    r = session.get(f"{BASE}/goals/{goal_id}", timeout=10)
    assert r.status_code == 200, f"GET /goals/{goal_id} -> {r.status_code}"
    body = r.json()
    assert "status" in body, "GET /goals/{id} missing 'status'"


def _real_answers(questions):
    """Build REAL structured answers from the questions emitted by POST /goals.

    Picks the first option of each question (falling back to the question text
    itself when a question has no options, e.g. free-text questions) so the
    gateway's generate_roadmap() consumes genuine user structure instead of a
    degenerate "yes"-to-everything payload. A degenerate payload previously
    embedded the literal answer into the roadmap text (e.g. "yes hours/week")
    while the phase-count invariant still held, so the regression was masked.

    NOTE: questions come from the POST /goals response — GET /goals/{id} does
    NOT return the questions array, so re-fetching it from GET would yield an
    empty list and silently degrade back to the "yes"/empty path.
    """
    answers = []
    for q in questions:
        if "id" not in q:
            continue
        opts = q.get("options") or []
        ans = opts[0] if opts else (q.get("question") or "x")
        answers.append({"question_id": q["id"], "answer": ans})
    return answers


def test_submit_answers_roadmap_phase_count(session):
    """#1 POST /goals/{id}/answers -> 200 AND total_phases == len(phases).

    Hardened (round 2026-08-18T0937Z): submits REAL structured answers (first
    option of each generated question) so the roadmap generation path is
    exercised with genuine user state, then guards the roadmap is
    NON-DEGENERATE — not merely count-correct. A degenerate "yes"-to-everything
    payload previously embedded the literal answer into the roadmap text (e.g.
    "yes hours/week") while the phase-count invariant still held, masking the
    regression. The single generate_roadmap() path emits a tailored
    "Your Personalized Roadmap: <goal>" title (never a "Plan & Begin" wrapper)
    and curates >=1 objective / deliverable / resource per phase, so these
    guards catch a real regression in that path.
    """
    goal_id, questions = _create_goal(session)
    answers = _real_answers(questions)
    assert answers, "no structured answers built from generated questions"

    r = session.post(
        f"{BASE}/goals/{goal_id}/answers", json={"answers": answers}, timeout=60
    )
    assert r.status_code == 200, f"POST answers -> {r.status_code} {r.text[:300]}"
    body = r.json()
    roadmap = body.get("roadmap", {})
    phases = roadmap.get("phases", [])
    total_phases = roadmap.get("total_phases")

    # Hard invariant (existing): count must be internally consistent.
    assert isinstance(total_phases, int), "roadmap.total_phases missing/not int"
    assert total_phases == len(phases), (
        f"roadmap.total_phases ({total_phases}) != len(phases) ({len(phases)})"
    )

    # Non-degenerate guards: the roadmap must reflect REAL submitted state,
    # not a placeholder that merely preserved the count invariant.
    title = roadmap.get("title", "")
    assert title, "roadmap.title missing/empty"
    assert "Roadmap" in title, f"roadmap.title should contain 'Roadmap': {title!r}"
    # The degenerate placeholder wraps the raw goal as a phase-style title;
    # the real (single) generate_roadmap path emits a tailored title with no
    # "Plan & Begin" wrapper.
    assert "Plan & Begin" not in title, (
        f"roadmap.title looks like a degenerate placeholder: {title!r}"
    )

    overview = roadmap.get("overview", "")
    assert overview, "roadmap.overview missing/empty"
    # Exact degenerate string the old "yes"-payload produced (hours answer was
    # literally "yes"). Real answers yield a different hours value.
    assert overview != "A 12-week journey (yes hours/week) across 4 phases.", (
        f"roadmap.overview is the degenerate placeholder: {overview!r}"
    )

    # Every phase must carry >=1 objective, deliverable, and resource. The real
    # path always curates these; a regression collapsing them must be caught.
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


def test_quick_roadmap_phase_count(session):
    """#2 POST /goals/quick -> 200 AND total_phases == len(phases)."""
    r = session.post(
        f"{BASE}/goals/quick",
        json={"goal": "build a personal finance tracker with rust in 4 months"},
        timeout=60,
    )
    assert r.status_code == 200, f"POST /goals/quick -> {r.status_code} {r.text[:300]}"
    body = r.json()
    roadmap = body.get("roadmap", {})
    phases = roadmap.get("phases", [])
    total_phases = roadmap.get("total_phases")
    assert isinstance(total_phases, int), "quick roadmap.total_phases missing/not int"
    assert total_phases == len(phases), (
        f"quick roadmap.total_phases ({total_phases}) != len(phases) ({len(phases)})"
    )


def test_leaderboard_is_list(session):
    """#3 + #6 GET /goals/leaderboard -> 200 AND response is a LIST (JSON array)."""
    # Ensure at least one goal exists so the board is non-empty.
    _create_goal(session)
    time.sleep(1)  # let the store persist
    r = session.get(f"{BASE}/goals/leaderboard", timeout=10)
    assert r.status_code == 200, f"GET /goals/leaderboard -> {r.status_code}"
    body = r.json()
    assert isinstance(body, list), (
        f"/goals/leaderboard must return a JSON array (list); got "
        f"{type(body).__name__}: {str(body)[:200]}"
    )
    # Each entry must carry the documented leaderboard fields.
    for entry in body:
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


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
