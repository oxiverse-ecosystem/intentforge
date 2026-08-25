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
    return body["goal_id"]


def test_create_goal_schema(session):
    """#4 POST /goals -> 200, goal_id present, questions[] non-empty."""
    goal_id = _create_goal(session)
    assert isinstance(goal_id, str) and goal_id.startswith("goal_")


def test_get_goal_schema(session):
    """#5 GET /goals/{id} -> 200, status present."""
    goal_id = _create_goal(session)
    r = session.get(f"{BASE}/goals/{goal_id}", timeout=10)
    assert r.status_code == 200, f"GET /goals/{goal_id} -> {r.status_code}"
    body = r.json()
    assert "status" in body, "GET /goals/{id} missing 'status'"


def test_submit_answers_roadmap_phase_count(session):
    """#1 POST /goals/{id}/answers -> 200 AND total_phases == len(phases)."""
    goal_id = _create_goal(session)
    # Pull the generated questions so we can answer them.
    get_r = session.get(f"{BASE}/goals/{goal_id}", timeout=10)
    assert get_r.status_code == 200
    questions = get_r.json().get("questions", [])
    answers = [
        {"question_id": q["id"], "answer": "yes"}
        for q in questions
        if "id" in q
    ]
    if not answers:
        # Some flows answer inline; fall back to a minimal numeric payload.
        answers = [{"question_id": 1, "answer": "yes"}]

    r = session.post(
        f"{BASE}/goals/{goal_id}/answers", json={"answers": answers}, timeout=60
    )
    assert r.status_code == 200, f"POST answers -> {r.status_code} {r.text[:300]}"
    body = r.json()
    roadmap = body.get("roadmap", {})
    phases = roadmap.get("phases", [])
    total_phases = roadmap.get("total_phases")
    assert isinstance(total_phases, int), "roadmap.total_phases missing/not int"
    assert total_phases == len(phases), (
        f"roadmap.total_phases ({total_phases}) != len(phases) ({len(phases)})"
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
