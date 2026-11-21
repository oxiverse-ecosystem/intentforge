"""Phase 1 anonymous-capability tests for the live gateway.

Run against a dev stack (in-memory store):
    IF_API_KEY=intentforge-dev-key pytest tests/test_goals_anonymous.py -q
"""
import os
import uuid

import requests

BASE = os.environ.get("INTENTFORGE_BASE", "http://localhost:4000").rstrip("/")
KEY = os.environ.get("IF_API_KEY", "intentforge-dev-key")


def anon_headers(ref):
    return {"X-Anonymous-Owner": str(ref)}


def create_goal(ref, goal):
    r = requests.post(BASE + "/goals", json={"goal": goal}, headers=anon_headers(ref), timeout=30)
    assert r.status_code == 200, (r.status_code, r.text)
    return r.json()


def test_anonymous_goal_lifecycle_and_expiry_field():
    ref = uuid.uuid4()
    created = create_goal(ref, "learn rust匿名")  # multibyte exercises the UTF-8 fix
    gid = created["goal_id"]
    assert gid.startswith("goal_")
    assert "expires_at" in created, "anonymous responses must carry expires_at"

    r = requests.get(BASE + "/goals/" + gid, headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200, r.text
    assert "expires_at" in r.json()

    answers = [
        {"question_id": q["id"], "answer": q["options"][0] if q.get("options") else "x"}
        for q in created["questions"][:2]
    ]
    r = requests.post(BASE + "/goals/" + gid + "/answers", json={"answers": answers},
                      headers=anon_headers(ref), timeout=30)
    assert r.status_code == 200, r.text
    assert r.json()["total_phases"] > 0

    r = requests.post(BASE + "/goals/" + gid + "/progress",
                      json={"phase_id": 1, "is_completed": True},
                      headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200, r.text
    assert r.json()["score"] >= 100

    r = requests.get(BASE + "/goals", headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200
    assert any(g["goal_id"] == gid for g in r.json())


def test_owner_ref_isolates_goals():
    a, b = uuid.uuid4(), uuid.uuid4()
    gid = create_goal(a, "learn rust isolation")["goal_id"]

    r = requests.get(BASE + "/goals/" + gid, headers=anon_headers(b), timeout=15)
    assert r.status_code == 404, f"other owner must not read goal: {r.status_code} {r.text}"

    r = requests.get(BASE + "/goals", headers=anon_headers(b), timeout=15)
    assert r.status_code == 200
    assert all(g["goal_id"] != gid for g in r.json())


def test_invalid_owner_ref_rejected():
    r = requests.get(BASE + "/goals", headers={"X-Anonymous-Owner": "not-a-uuid"}, timeout=15)
    assert r.status_code == 401, (r.status_code, r.text)


def test_leaderboard_is_public_array():
    r = requests.get(BASE + "/goals/leaderboard", timeout=15)
    assert r.status_code == 200, r.text
    assert isinstance(r.json(), list)


def test_complete_sets_true_and_progress_toggles_false():
    """Contract: /complete only sets true; /progress can toggle false."""
    ref = uuid.uuid4()
    created = create_goal(ref, "learn completion semantics")
    gid = created["goal_id"]
    answers = [
        {"question_id": q["id"], "answer": q["options"][0] if q.get("options") else "x"}
        for q in created["questions"][:2]
    ]
    r = requests.post(BASE + f"/goals/{gid}/answers", json={"answers": answers},
                      headers=anon_headers(ref), timeout=30)
    assert r.status_code == 200, r.text

    r = requests.post(BASE + f"/goals/{gid}/phases/1/complete", json={},
                      headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200, r.text
    assert r.json()["completed_phases"] == 1

    r = requests.post(BASE + f"/goals/{gid}/progress",
                      json={"phase_id": 1, "is_completed": False},
                      headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200, r.text
    assert r.json()["completed_phases"] == 0, "progress must be able to unset"
    assert r.json()["score"] == 0

    r = requests.post(BASE + f"/goals/{gid}/phases/1/complete", json={},
                      headers=anon_headers(ref), timeout=15)
    assert r.status_code == 200
    assert r.json()["completed_phases"] == 1, "complete must set true again"
