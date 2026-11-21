"""Full live QA of every IntentForge Goals endpoint.

Run:
  IF_API_KEY=<dev key> INTENTFORGE_BASE=http://localhost:4000 pytest -q test_goals_api_full.py

Key is read from the environment only; never hardcoded in this file.
"""

import json
import os
import re
import time
import uuid
import urllib.error
import urllib.parse
import urllib.request

import pytest

BASE = os.environ.get("INTENTFORGE_BASE", "http://localhost:4000").rstrip("/")
KEY = os.environ.get("IF_API_KEY", "")
TIMEOUT = 90

if not KEY:
    raise RuntimeError("IF_API_KEY must be set (read it from the docs/dev env, not source)")


def call(method, path, payload=None, key=KEY, raw=False):
    url = BASE + path
    data = None
    headers = {"Accept": "application/json"}
    if key:
        headers["x-api-key"] = key
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["Content-Type"] = "application/json"
    if raw is not None:
        if raw:
            data = raw.encode()
            headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            body = r.read().decode()
            return r.status, (json.loads(body) if body else None), body
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            parsed = json.loads(body)
        except Exception:
            parsed = body
        return e.code, parsed, body


def uid(prefix="goal qa"):
    return f"{prefix} {uuid.uuid4().hex[:10]}"


def create(text=None):
    text = text or uid()
    st, body, _ = call("POST", "/goals", {"goal": text})
    assert st == 200, (st, body)
    assert body["goal_id"].startswith("goal_")
    assert body["questions"] and all("id" in q for q in body["questions"])
    return body


def with_roadmap(text=None):
    g = create(text)
    answers = [
        {"question_id": q["id"], "answer": (q.get("options") or ["beginner"])[0]}
        for q in g["questions"]
    ]
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/answers", {"answers": answers})
    assert st == 200, (st, body)
    return g, body


def assert_roadmap(rm):
    assert isinstance(rm, dict) and rm
    phases = rm["phases"]
    assert rm["total_phases"] == len(phases) > 0
    assert rm["title"] and rm["overview"]
    for p in phases:
        for k in ("id", "title", "description", "duration_weeks", "deadline",
                  "buffer_days", "objectives", "resources", "deliverables",
                  "completion_type", "is_completed"):
            assert k in p, f"phase missing {k}: {p}"
        assert p["objectives"] and p["deliverables"]


# ---------------------------------------------------------------- auth / guard
def test_auth_required_on_all_goal_routes():
    # Phase 1 decision D4: /goals/leaderboard is public (returns [] until
    # accounts launch). Every other goal route still requires a credential.
    routes = [
        ("GET", "/goals"), ("POST", "/goals"),
        ("POST", "/goals/quick"), ("GET", "/goals/goal_x"),
        ("POST", "/goals/goal_x/answers"),
        ("POST", "/goals/goal_x/phases/1/complete"),
        ("GET", "/goals/goal_x/progress"), ("POST", "/goals/goal_x/progress"),
    ]
    for method, path in routes:
        st, body, _ = call(method, path, {} if method == "POST" else None, key=None)
        assert st == 401, f"{method} {path} without key -> {st} {body}"
    st, body, _ = call("GET", "/goals/leaderboard", None, key=None)
    assert st == 200 and isinstance(body, list), f"leaderboard must be public array, got {st} {body}"


def test_bearer_auth_works_too():
    st, body, _ = call("GET", "/goals", key=None)
    assert st == 401
    # exercise Authorization: Bearer <key> via a manual request
    req = urllib.request.Request(
        BASE + "/goals", headers={"Authorization": f"Bearer {KEY}"}, method="GET"
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        assert r.status == 200
        assert isinstance(json.loads(r.read().decode()), list)


# ---------------------------------------------------------------- create
def test_create_returns_questions_and_next_step():
    g = create()
    assert set(g) >= {"goal_id", "goal", "intent", "questions", "total_questions",
                      "created_at", "next_step"}
    assert g["total_questions"] == len(g["questions"])
    assert g["next_step"]["method"] == "POST"
    assert g["next_step"]["path"] == f"/goals/{g['goal_id']}/answers"
    assert re.match(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$", g["created_at"]), g["created_at"]


def test_create_validation_errors():
    for text, code, err in [("", 400, "empty_goal"), ("ab", 400, "empty_goal"),
                            ("123", 400, "invalid_goal"), ("   ", 400, "empty_goal")]:
        st, body, _ = call("POST", "/goals", {"goal": text})
        assert st == code and body.get("error") == err, (text, st, body)
    st, body, _ = call("POST", "/goals", {})
    assert st == 422 and body.get("error") == "invalid_payload"
    st, body, _ = call("POST", "/goals", raw="{bad json")
    assert st == 422 and body.get("error") == "invalid_payload"


def test_create_dedupes_same_goal_text():
    text = uid("dedup")
    a, b = create(text), create(text)
    assert a["goal_id"] == b["goal_id"], (a["goal_id"], b["goal_id"])


# ---------------------------------------------------------------- answers / roadmap
def test_answers_generate_non_degenerate_roadmap():
    g, resp = with_roadmap()
    assert resp["goal_id"] == g["goal_id"]
    assert resp["status"] == "active"
    assert resp["completed_phases"] == 0 and resp["score"] == 0
    assert resp["total_phases"] == resp["roadmap"]["total_phases"]
    assert_roadmap(resp["roadmap"])


def test_answers_empty_is_400():
    g = create()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/answers", {"answers": []})
    assert st == 400 and body.get("error") == "invalid_answers"


def test_answers_flat_strings_are_422():
    g = create()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/answers",
                       {"answers": ["beginner", "5 hours"]})
    assert st == 422 and body.get("error") == "invalid_payload", (st, body)


def test_answers_unknown_goal_is_404():
    st, body, _ = call("POST", "/goals/goal_does_not_exist/answers",
                       {"answers": [{"question_id": 1, "answer": "x"}]})
    assert st == 404 and body.get("error") == "not_found"


def test_answers_idempotent_and_preserves_completion():
    g = create()
    answers = [{"question_id": q["id"], "answer": (q.get("options") or ["x"])[0]}
               for q in g["questions"]]
    call("POST", f"/goals/{g['goal_id']}/answers", {"answers": answers})
    call("POST", f"/goals/{g['goal_id']}/progress",
         {"phase_id": 1, "is_completed": True})
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/answers", {"answers": answers})
    assert st == 200
    assert body["completed_phases"] == 1, body
    assert body["roadmap"]["phases"][0]["is_completed"] is True


# ---------------------------------------------------------------- quick
def test_quick_one_shot():
    text = uid("quick")
    st, body, _ = call("POST", "/goals/quick", {"goal": text})
    assert st == 200, (st, body)
    assert set(body) >= {"goal_id", "goal", "intent", "roadmap", "resource_count",
                         "created_at", "status", "completed_phases",
                         "total_phases", "score"}
    assert body["resource_count"] == sum(len(p["resources"])
                                         for p in body["roadmap"]["phases"])
    assert body["status"] == "active" and body["score"] == 0
    assert_roadmap(body["roadmap"])


def test_quick_validation_errors():
    st, body, _ = call("POST", "/goals/quick", {"goal": "ab"})
    assert st == 400 and body.get("error") == "empty_goal"
    st, body, _ = call("POST", "/goals/quick", {})
    assert st == 422 and body.get("error") == "invalid_payload"


# ---------------------------------------------------------------- get / list
def test_get_goal_states_pending_then_active():
    g = create()
    st, body, _ = call("GET", f"/goals/{g['goal_id']}")
    assert st == 200 and body["status"] == "pending_answers"
    assert body["roadmap"] is None and "message" in body
    g2, _ = with_roadmap()
    st, body, _ = call("GET", f"/goals/{g2['goal_id']}")
    assert st == 200 and body["status"] == "active"
    assert_roadmap(body["roadmap"])


def test_get_unknown_goal_is_404():
    st, body, _ = call("GET", "/goals/goal_does_not_exist")
    assert st == 404 and body.get("error") == "not_found"


def test_list_goals_contains_created():
    g = create()
    st, body, _ = call("GET", "/goals")
    assert st == 200 and isinstance(body, list)
    ids = [x["goal_id"] for x in body]
    assert g["goal_id"] in ids
    assert all(set(x) >= {"goal_id", "goal", "intent", "roadmap", "created_at",
                          "completed_phases", "total_phases", "score", "user_name"}
               for x in body)


def test_no_user_identity_field_anywhere():
    """There is no way to set a user name: StoredGoal.user_name is hardcoded."""
    g = create()
    st, body, _ = call("POST", "/goals", {"goal": uid("userfield"), "user_name": "alice"})
    assert st == 200
    gid = body["goal_id"]
    st, got, _ = call("GET", "/goals/leaderboard")
    mine = [x for x in got if x["goal_id"] == gid]
    # No answers submitted -> not on the board; drive it through /goals list.
    st, listing, _ = call("GET", "/goals")
    mine = [x for x in listing if x["goal_id"] == gid]
    assert mine and mine[0]["user_name"] == "Anonymous", mine


# ---------------------------------------------------------------- completion
def test_complete_phase_and_score_math():
    g, rb = with_roadmap()
    gid = g["goal_id"]
    total = rb["total_phases"]
    st, body, _ = call("POST", f"/goals/{gid}/phases/1/complete", {})
    assert st == 200, (st, body)
    assert body["completed_phase_id"] == 1
    assert body["completed_phases"] == 1 and body["total_phases"] == total
    assert body["score"] == 100
    assert body["status"] == "active"
    for p in range(2, total + 1):
        st, body, _ = call("POST", f"/goals/{gid}/phases/{p}/complete", {})
        assert st == 200, (st, body)
    assert body["completed_phases"] == total
    assert body["score"] == total * 100 + 500, body
    assert body["status"] == "completed"
    st, got, _ = call("GET", f"/goals/{gid}")
    assert got["status"] == "completed"


def test_complete_phase_invalid_phase():
    g, _ = with_roadmap()
    for phase in (0, 999):
        st, body, _ = call("POST", f"/goals/{g['goal_id']}/phases/{phase}/complete", {})
        assert st == 400 and body.get("error") == "invalid_phase", (phase, st, body)


def test_complete_phase_unknown_goal():
    st, body, _ = call("POST", "/goals/goal_does_not_exist/phases/1/complete", {})
    assert st == 404 and body.get("error") == "not_found"


def test_complete_phase_before_answers():
    g = create()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/phases/1/complete", {})
    assert st in (400, 404), (st, body)


# ---------------------------------------------------------------- progress
def test_update_progress_and_read_back():
    g, rb = with_roadmap()
    gid, total = g["goal_id"], rb["total_phases"]
    st, body, _ = call("POST", f"/goals/{gid}/progress",
                       {"phase_id": 1, "is_completed": True})
    assert st == 200, (st, body)
    assert body["phase_id"] == 1 and body["is_completed"] is True
    assert body["completed_phases"] == 1 and body["score"] == 100
    st, read, _ = call("GET", f"/goals/{gid}/progress")
    assert st == 200
    for k in ("goal_id", "goal", "status", "completed_phases", "total_phases",
              "score", "roadmap"):
        assert k in read, (k, read)
    assert read["completed_phases"] == 1 and read["status"] == "active"
    # toggling off must decrement, not just track a monotonic "completed ever"
    st, body, _ = call("POST", f"/goals/{gid}/progress",
                       {"phase_id": 1, "is_completed": False})
    assert st == 200 and body["completed_phases"] == 0 and body["score"] == 0


def test_progress_defaults_is_completed_true():
    g, _ = with_roadmap()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/progress", {"phase_id": 1})
    assert st == 200 and body["is_completed"] is True


def test_progress_invalid_phase_and_unknown_goal():
    g, _ = with_roadmap()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/progress", {"phase_id": 0})
    assert st == 400 and body.get("error") == "invalid_phase"
    st, body, _ = call("POST", "/goals/goal_does_not_exist/progress", {"phase_id": 1})
    assert st == 404 and body.get("error") == "not_found"


def test_progress_bad_payload():
    g, _ = with_roadmap()
    st, body, _ = call("POST", f"/goals/{g['goal_id']}/progress", {})
    assert st == 422 and body.get("error") == "invalid_payload"


# ---------------------------------------------------------------- leaderboard
def test_leaderboard_shape_fields_and_order():
    g, rb = with_roadmap()
    gid = g["goal_id"]
    time.sleep(0.2)
    st, body, _ = call("GET", "/goals/leaderboard")
    assert st == 200 and isinstance(body, list), (st, type(body))
    scores = [e["score"] for e in body]
    assert scores == sorted(scores, reverse=True), scores
    assert len(body) <= 50
    fields = ("goal_id", "goal", "user_name", "score", "completed_phases",
              "total_phases", "created_at")
    for entry in body:
        assert set(entry) >= set(fields), entry
    mine = [e for e in body if e["goal_id"] == gid]
    # Phase 1 decision D4: the Supabase leaderboard view is empty until
    # accounts launch; the in-memory store still surfaces service goals.
    # Shape/order invariants above hold in both modes; presence is optional.
    assert len(mine) <= 1, (gid, mine)
    if mine:
        assert mine[0]["total_phases"] == rb["total_phases"]


def test_leaderboard_updates_after_completion():
    g, rb = with_roadmap()
    gid, total = g["goal_id"], rb["total_phases"]
    for p in range(1, total + 1):
        call("POST", f"/goals/{gid}/phases/{p}/complete", {})
    time.sleep(0.2)
    st, body, _ = call("GET", "/goals/leaderboard")
    assert st == 200
    mine = [e for e in body if e["goal_id"] == gid]
    # D4: the Supabase leaderboard is empty until accounts launch.
    if mine:
        assert mine[0]["score"] == total * 100 + 500, mine
        assert mine[0]["completed_phases"] == total


def test_leaderboard_excludes_goals_without_roadmap():
    g = create()
    time.sleep(0.2)
    st, body, _ = call("GET", "/goals/leaderboard")
    assert st == 200
    assert g["goal_id"] not in [e["goal_id"] for e in body]
