#!/usr/bin/env python3
"""Live API audit for IntentForge — hits every endpoint in API_REFERENCE.md
and asserts documented response shapes. Prints PASS/FAIL per check."""
import json, sys, time, urllib.request, urllib.error, urllib.parse

BASE = "http://localhost:4000"
results = []

def req(method, path, body=None, timeout=30):
    url = BASE + path
    data = json.dumps(body).encode() if body else None
    r = urllib.request.Request(url, data=data, method=method,
        headers={"Content-Type": "application/json"} if data else {})
    try:
        resp = urllib.request.urlopen(r, timeout=timeout)
        raw = resp.read()
        try: return resp.status, json.loads(raw)
        except: return resp.status, raw.decode(errors="replace")
    except urllib.error.HTTPError as e:
        raw = e.read()
        try: return e.code, json.loads(raw)
        except: return e.code, {"_raw": raw.decode(errors="replace")[:500]}
    except Exception as e:
        return -1, {"_error": str(e)}

def check(name, cond, detail=""):
    status = "PASS" if cond else "FAIL"
    results.append((status, name, detail))
    print(f"  [{status}] {name}" + (f" — {detail}" if detail and not cond else ""))

print("=" * 70)
print("LIVE API AUDIT — IntentForge @ localhost:4000")
print("=" * 70)

# ── 1. GET / ──────────────────────────────────────────────────────────
print("\n── GET / ──")
code, body = req("GET", "/")
check("GET / returns 200", code == 200, f"code={code}")
check("GET / returns text identifier", "IntentForge" in str(body), f"body={str(body)[:100]}")

# ── 2. GET /health ────────────────────────────────────────────────────
print("\n── GET /health ──")
code, body = req("GET", "/health")
check("GET /health returns 200", code == 200, f"code={code}")
check("GET /health returns 'OK'", str(body).strip() == "OK", f"body={str(body)[:100]}")

# ── 3. GET /search (complex multi-constraint query) ──────────────────
print("\n── GET /search (complex multi-constraint) ──")
q = "python web framework not django site:github.com"
code, body = req("GET", f"/search?q={urllib.parse.quote(q)}")
check("GET /search returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["query","intent","category","confidence","constraints",
                  "structured_constraints","expanded_queries","distribution",
                  "results","results_before_filter","results_after_filter",
                  "total","limit","offset","has_more"]:
        check(f"  /search has '{field}'", field in body, f"missing")
    check("  /search results is non-empty list", isinstance(body.get("results"), list) and len(body["results"]) > 0,
          f"results={type(body.get('results'))}")
    check("  /search confidence is float", isinstance(body.get("confidence"), float),
          f"confidence={body.get('confidence')}")
    check("  /search total == results_after_filter", body.get("total") == body.get("results_after_filter"),
          f"total={body.get('total')} results_after_filter={body.get('results_after_filter')}")
    check("  /search has_more is bool", isinstance(body.get("has_more"), bool),
          f"has_more={body.get('has_more')}")

# ── 4. GET /search/fast ───────────────────────────────────────────────
print("\n── GET /search/fast ──")
code, body = req("GET", "/search/fast?q=rust+async")
check("GET /search/fast returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["count","source","results"]:
        check(f"  /search/fast has '{field}'", field in body, f"missing")
    check("  /search/fast results is list", isinstance(body.get("results"), list))

# ── 5. GET /images ────────────────────────────────────────────────────
print("\n── GET /images ──")
code, body = req("GET", "/images?q=rust+logo")
check("GET /images returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["count","query","results"]:
        check(f"  /images has '{field}'", field in body, f"missing")
    if body.get("results"):
        r0 = body["results"][0]
        for field in ["title","url","image_url","thumbnail_url","source","score"]:
            check(f"  /images result has '{field}'", field in r0, f"missing")

# ── 6. GET /videos ────────────────────────────────────────────────────
print("\n── GET /videos ──")
code, body = req("GET", "/videos?q=kubernetes+tutorial")
check("GET /videos returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["count","query","results"]:
        check(f"  /videos has '{field}'", field in body, f"missing")
    if body.get("results"):
        r0 = body["results"][0]
        for field in ["title","url","thumbnail","source","score"]:
            check(f"  /videos result has '{field}'", field in r0, f"missing")

# ── 7. GET /news ──────────────────────────────────────────────────────
print("\n── GET /news ──")
code, body = req("GET", "/news?q=AI+news")
check("GET /news returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["count","query","results"]:
        check(f"  /news has '{field}'", field in body, f"missing")
    if body.get("results"):
        r0 = body["results"][0]
        for field in ["title","url","source","score"]:
            check(f"  /news result has '{field}'", field in r0, f"missing")

# ── 8. GET /spellcheck ────────────────────────────────────────────────
print("\n── GET /spellcheck ──")
code, body = req("GET", "/spellcheck?q=pythn+programing+langauge")
check("GET /spellcheck returns 200", code == 200, f"code={code}")
if code == 200:
    for field in ["query","corrected","changed","corrections"]:
        check(f"  /spellcheck has '{field}'", field in body, f"missing")
    check("  /spellcheck changed=true for typo", body.get("changed") == True,
          f"changed={body.get('changed')}")
    check("  /spellcheck corrected is str", isinstance(body.get("corrected"), str))

# ── 9. Goals subsystem ────────────────────────────────────────────────
print("\n── Goals subsystem ──")
# 9a. POST /goals
code, body = req("POST", "/goals", {"goal": "build an AI assistant"})
check("POST /goals returns 200", code == 200, f"code={code} body={str(body)[:200]}")
goal_id = body.get("goal_id", "")
check("  /goals has goal_id", bool(goal_id), f"goal_id={goal_id}")
questions = body.get("questions", [])
check("  /goals has non-empty questions[]", isinstance(questions, list) and len(questions) > 0,
      f"questions={questions}")

# 9b. POST /goals/:id/answers
if goal_id:
    code2, body2 = req("POST", f"/goals/{goal_id}/answers",
                       {"answers": [{"question_id": q.get("id", i), "answer": "beginner"}
                                    for i, q in enumerate(questions)]})
    check("POST /goals/:id/answers returns 200", code2 == 200, f"code={code2} body={str(body2)[:200]}")
    if code2 == 200:
        roadmap = body2.get("roadmap", {})
        tp = roadmap.get("total_phases")
        phases = roadmap.get("phases", [])
        check("  /answers roadmap.total_phases == len(phases)", tp == len(phases),
              f"total_phases={tp} len(phases)={len(phases)}")
        check("  /answers roadmap has phases list", isinstance(phases, list) and len(phases) > 0,
              f"phases={len(phases)}")

# 9c. GET /goals/:id
if goal_id:
    code3, body3 = req("GET", f"/goals/{goal_id}")
    check("GET /goals/:id returns 200", code3 == 200, f"code={code3}")
    check("  /goals/:id has status", "status" in body3, f"keys={list(body3.keys())[:10]}")

# 9d. GET /goals/leaderboard
code4, body4 = req("GET", "/goals/leaderboard")
check("GET /goals/leaderboard returns 200", code4 == 200, f"code={code4}")
check("  /goals/leaderboard is a LIST", isinstance(body4, list),
      f"type={type(body4).__name__}")

# 9e. POST /goals/quick
code5, body5 = req("POST", "/goals/quick", {"goal": "write a fantasy novel"})
check("POST /goals/quick returns 200", code5 == 200, f"code={code5} body={str(body5)[:200]}")
if code5 == 200:
    roadmap5 = body5.get("roadmap", {})
    tp5 = roadmap5.get("total_phases")
    phases5 = roadmap5.get("phases", [])
    check("  /goals/quick total_phases == len(phases)", tp5 == len(phases5),
          f"total_phases={tp5} len(phases)={len(phases5)}")

# ── Summary ───────────────────────────────────────────────────────────
print("\n" + "=" * 70)
passed = sum(1 for s,_,_ in results if s == "PASS")
failed = sum(1 for s,_,_ in results if s == "FAIL")
print(f"SUMMARY: {passed} passed, {failed} failed, {len(results)} total")
if failed:
    print("\nFAILURES:")
    for s, n, d in results:
        if s == "FAIL":
            print(f"  - {n}: {d}")
print("=" * 70)
sys.exit(1 if failed else 0)
