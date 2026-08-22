#!/usr/bin/env python3
"""Permanent schema regression tests for the IntentForge Goals API.

These run against a LIVE gateway (default http://localhost:4000). They assert the
documented JSON contracts — including the audit-mandated invariants from round
2026-08-22T0703Z (D1 roadmap.total_phases, D2 leaderboard is a LIST).

Run:  pytest tests/goals_schema_test.py -v
(Requires the gateway to be up: `curl -s localhost:4000/health` -> OK)
"""
import os
import json
import urllib.request

BASE = os.environ.get("INTENTFORGE_BASE", "http://localhost:4000")


def _get(path, params=None):
    url = BASE + path
    if params:
        from urllib.parse import urlencode
        url += "?" + urlencode(params)
    with urllib.request.urlopen(url, timeout=30) as r:
        return r.status, json.loads(r.read().decode())


def _post(path, payload):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        BASE + path, data=data, headers={"Content-Type": "application/json"}, method="POST"
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.status, json.loads(r.read().decode())


def test_leaderboard_is_a_list():
    """D2: /goals/leaderboard MUST return a bare JSON ARRAY (iterable of goal objects),
    NOT a dict wrapper like {"entries":[...],"total_entries":N}."""
    status, resp = _get("/goals/leaderboard")
    assert status == 200
    assert isinstance(resp, list), (
        f"GET /goals/leaderboard must return a LIST, got {type(resp).__name__}: {resp!r}"
    )
    # Each entry is a goal object with the documented keys.
    for entry in resp:
        assert isinstance(entry, dict)
        for key in ("goal_id", "goal", "user_name", "score",
                    "completed_phases", "total_phases", "created_at"):
            assert key in entry, f"leaderboard entry missing key {key!r}: {entry!r}"


def test_roadmap_has_total_phases():
    """D1 follow-through: a generated roadmap must carry total_phases equal to len(phases)."""
    status, created = _post("/goals", {"goal": "learn rust programming language"})
    assert status == 200
    goal_id = created["goal_id"]
    # Submit placeholder answers for the question bank.
    questions = created.get("questions", [])
    answers = [{"question_id": i, "answer": "moderate"} for i in range(len(questions))]
    _post(f"/goals/{goal_id}/answers", {"answers": answers})
    _, goal = _get(f"/goals/{goal_id}")
    roadmap = goal.get("roadmap") or {}
    assert "total_phases" in roadmap, (
        f"roadmap.total_phases missing from {json.dumps(roadmap)[:200]}"
    )
    assert roadmap["total_phases"] == len(roadmap.get("phases", []))


if __name__ == "__main__":
    import sys
    # Tiny runner so it also works as a plain script (no pytest installed).
    failures = []
    for name, fn in (("test_leaderboard_is_a_list", test_leaderboard_is_a_list),
                     ("test_roadmap_has_total_phases", test_roadmap_has_total_phases)):
        try:
            fn()
            print(f"PASS {name}")
        except Exception as e:  # noqa: BLE001
            failures.append((name, repr(e)))
            print(f"FAIL {name}: {e}")
    sys.exit(1 if failures else 0)
