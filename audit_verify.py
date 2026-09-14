#!/usr/bin/env python3
"""Live API verification script for IntentForge audit."""
import json
import sys
import urllib.request
import urllib.parse
import urllib.error

BASE = "http://localhost:4000"
results = []

def get(path, data=None, method="GET"):
    url = BASE + path
    if data is not None:
        body = json.dumps(data).encode()
        req = urllib.request.Request(url, data=body, method=method, headers={"Content-Type": "application/json"})
    else:
        req = urllib.request.Request(url, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            status = resp.status
            body = resp.read().decode()
            try:
                return status, json.loads(body)
            except:
                return status, body
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            return e.code, json.loads(body)
        except:
            return e.code, body
    except Exception as e:
        return -1, str(e)

def check(name, ok, detail=""):
    status = "PASS" if ok else "FAIL"
    results.append((name, ok, detail))
    print(f"[{status}] {name}" + (f" — {detail}" if detail else ""))

# 1. GET /
print("=== GET / ===")
code, body = get("/")
check("GET / returns 200", code == 200, f"code={code}")
check("GET / returns IntentForge-v2 Gateway", "IntentForge-v2 Gateway" in str(body), f"body={body[:100]}")

# 2. GET /health
print("\n=== GET /health ===")
code, body = get("/health")
check("GET /health returns 200", code == 200, f"code={code}")
check("GET /health returns OK", "OK" in str(body), f"body={body[:100]}")

# 3. GET /search (complex multi-constraint)
print("\n=== GET /search ===")
code, d = get("/search?q=rust+async+web+framework+not+django+site%3Agithub.com")
check("GET /search returns 200", code == 200, f"code={code}")
if code == 200:
    required_keys = ["query", "intent", "category", "confidence", "constraints", "structured_constraints", "expanded_queries", "distribution", "results", "results_before_filter", "results_after_filter", "total", "limit", "offset", "has_more"]
    missing = [k for k in required_keys if k not in d]
    check("GET /search has all required keys", len(missing) == 0, f"missing={missing}")
    check("GET /search results is non-empty", len(d.get("results", [])) > 0, f"count={len(d.get('results', []))}")
    check("GET /search confidence is float", isinstance(d.get("confidence"), (int, float)), f"confidence={d.get('confidence')}")
    sc = d.get("structured_constraints", {})
    check("GET /search structured_constraints.sites includes github.com", "github.com" in sc.get("sites", []), f"sites={sc.get('sites')}")
    check("GET /search structured_constraints.negative includes django", "django" in sc.get("negative", []), f"negative={sc.get('negative')}")

# 4. GET /search/fast
print("\n=== GET /search/fast ===")
code, d = get("/search/fast?q=python+machine+learning")
check("GET /search/fast returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /search/fast has count key", "count" in d)
    check("GET /search/fast has source key", "source" in d)
    check("GET /search/fast has results key", "results" in d)

# 5. GET /images
print("\n=== GET /images ===")
code, d = get("/images?q=rust+logo")
check("GET /images returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /images has count key", "count" in d)
    check("GET /images has query key", "query" in d)
    check("GET /images has results key", "results" in d)
    if d.get("results"):
        r = d["results"][0]
        img_keys = ["title", "url", "image_url", "thumbnail_url", "description", "source", "score"]
        missing = [k for k in img_keys if k not in r]
        check("GET /images result has all fields", len(missing) == 0, f"missing={missing}")

# 6. GET /videos
print("\n=== GET /videos ===")
code, d = get("/videos?q=kubernetes+tutorial")
check("GET /videos returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /videos has count key", "count" in d)
    check("GET /videos has query key", "query" in d)
    check("GET /videos has results key", "results" in d)
    if d.get("results"):
        r = d["results"][0]
        vid_keys = ["title", "url", "description", "thumbnail", "video_id", "source", "score"]
        missing = [k for k in vid_keys if k not in r]
        check("GET /videos result has all fields", len(missing) == 0, f"missing={missing}")

# 7. GET /news
print("\n=== GET /news ===")
code, d = get("/news?q=artificial+intelligence")
check("GET /news returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /news has count key", "count" in d)
    check("GET /news has query key", "query" in d)
    check("GET /news has results key", "results" in d)
    if d.get("results"):
        r = d["results"][0]
        news_keys = ["title", "url", "description", "published_at", "source", "score"]
        missing = [k for k in news_keys if k not in r]
        check("GET /news result has all fields", len(missing) == 0, f"missing={missing}")

# 8. GET /spellcheck
print("\n=== GET /spellcheck ===")
code, d = get("/spellcheck?q=pythn+programing+langauge")
check("GET /spellcheck returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /spellcheck has query key", "query" in d)
    check("GET /spellcheck has corrected key", "corrected" in d)
    check("GET /spellcheck has changed key", "changed" in d)
    check("GET /spellcheck has corrections key", "corrections" in d)
    check("GET /spellcheck corrected pythn->python", "python" in str(d.get("corrected", "")), f"corrected={d.get('corrected')}")
    check("GET /spellcheck changed is True", d.get("changed") == True, f"changed={d.get('changed')}")

# 9. GET /analyze
print("\n=== GET /analyze ===")
code, d = get("/analyze?q=javascript+not+java+not+typescript")
check("GET /analyze returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /analyze contrastive_framing is True", d.get("contrastive_framing") == True)
    check("GET /analyze exclusions includes java", "java" in d.get("exclusions", []))
    check("GET /analyze exclusions includes typescript", "typescript" in d.get("exclusions", []))

# 10. GET /inspect
print("\n=== GET /inspect ===")
code, d = get("/inspect?q=python+web+framework+not+django")
check("GET /inspect returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /inspect has spelling key", "spelling" in d)
    check("GET /inspect has negation key", "negation" in d)
    check("GET /inspect has intent key", "intent" in d)
    check("GET /inspect has constraints key", "constraints" in d)
    check("GET /inspect has recency key", "recency" in d)
    check("GET /inspect has quality key", "quality" in d)

# 11. GET /geolocate
print("\n=== GET /geolocate ===")
code, d = get("/geolocate?q=quiet+places+to+study+near+chennai")
check("GET /geolocate returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /geolocate source is explicit", d.get("source") == "explicit")
    check("GET /geolocate city is chennai", d.get("resolved", {}).get("city") == "chennai")

# 12. GET /intent
print("\n=== GET /intent ===")
code, d = get("/intent?q=violin+vs+viola+for+beginner")
check("GET /intent returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /intent has intent key", "intent" in d)
    check("GET /intent has category key", "category" in d)
    check("GET /intent has confidence key", "confidence" in d)
    check("GET /intent has contrastive_framing key", "contrastive_framing" in d)
    check("GET /intent contrastive_framing is True", d.get("contrastive_framing") == True)

# 13. POST /goals
print("\n=== POST /goals ===")
code, d = post_goals = None, None
code, d = post_goals = (None, None)
code, d = post_goals_data = (None, None)
# redo properly
code, d = get("/goals", data={"goal": "build a full-stack web app for project management"}, method="POST")
check("POST /goals returns 200", code == 200, f"code={code}")
goal_id = None
if code == 200:
    check("POST /goals has goal_id", "goal_id" in d, f"keys={list(d.keys())}")
    check("POST /goals has questions", "questions" in d and len(d.get("questions", [])) > 0)
    goal_id = d.get("goal_id")
    print(f"  goal_id = {goal_id}")

# 14. POST /goals/:id/answers
if goal_id:
    print(f"\n=== POST /goals/{goal_id}/answers ===")
    answers = [
        {"question_id": 1, "answer": "3 months — Quarter project"},
        {"question_id": 2, "answer": "5-10 hours — Evenings & weekends"},
        {"question_id": 3, "answer": "Microservices — independent, deployable services"},
        {"question_id": 4, "answer": "Hybrid — SQL + cache layer (Redis)"},
        {"question_id": 5, "answer": "Container / Kubernetes (Docker, EKS, GKE)"},
        {"question_id": 6, "answer": "WebSockets for live bidirectional communication"},
        {"question_id": 7, "answer": "A completed product ready for users"},
    ]
    # Trim to actual question count
    if d and "questions" in d:
        n = len(d["questions"])
        answers = answers[:n]
    code, d = get(f"/goals/{goal_id}/answers", data={"answers": answers}, method="POST")
    check("POST /goals/:id/answers returns 200", code == 200, f"code={code}")
    if code == 200:
        roadmap = d.get("roadmap", {})
        total_phases = d.get("total_phases")
        phases = roadmap.get("phases", [])
        check("POST /goals/:id/answers has roadmap", roadmap is not None)
        check("POST /goals/:id/answers total_phases == len(phases)", total_phases == len(phases), f"total_phases={total_phases}, len(phases)={len(phases)}")
        check("POST /goals/:id/answers total_phases is not None", total_phases is not None, f"total_phases={total_phases}")

# 15. GET /goals/:id
if goal_id:
    print(f"\n=== GET /goals/{goal_id} ===")
    code, d = get(f"/goals/{goal_id}")
    check("GET /goals/:id returns 200", code == 200, f"code={code}")
    if code == 200:
        check("GET /goals/:id has status", "status" in d)

# 16. GET /goals/leaderboard
print("\n=== GET /goals/leaderboard ===")
code, d = get("/goals/leaderboard")
check("GET /goals/leaderboard returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /goals/leaderboard is a list", isinstance(d, list), f"type={type(d).__name__}")

# 17. POST /goals/quick
print("\n=== POST /goals/quick ===")
code, d = get("/goals/quick", data={"goal": "build a rust web framework"}, method="POST")
check("POST /goals/quick returns 200", code == 200, f"code={code}")
if code == 200:
    roadmap = d.get("roadmap", {})
    total_phases = d.get("total_phases")
    phases = roadmap.get("phases", [])
    check("POST /goals/quick has roadmap", roadmap is not None)
    check("POST /goals/quick total_phases == len(phases)", total_phases == len(phases), f"total_phases={total_phases}, len(phases)={len(phases)}")
    check("POST /goals/quick total_phases is not None", total_phases is not None, f"total_phases={total_phases}")

# 18. GET /shopping
print("\n=== GET /shopping ===")
code, d = get("/shopping?q=best+wireless+earbuds+under+50&count=5")
check("GET /shopping returns 200", code == 200, f"code={code}")
if code == 200:
    check("GET /shopping has results", "results" in d and len(d.get("results", [])) > 0)
    if d.get("results"):
        r = d["results"][0]
        check("GET /shopping result has commerce_provenance", "commerce_provenance" in r)
        if "affiliate" in r:
            check("GET /shopping affiliate has disclosed=true", r["affiliate"].get("disclosed") == True)

# 19. POST /commerce/extract
print("\n=== POST /commerce/extract ===")
html = '<!doctype html><html><head><script type="application/ld+json">{"@context":"https://schema.org/","@type":"Product","name":"Test","offers":{"@type":"Offer","price":"29.99","priceCurrency":"USD"}}</script></head><body></body></html>'
code, d = get("/commerce/extract", data={"html": html, "url": "https://example.com/p"}, method="POST")
check("POST /commerce/extract returns 200", code == 200, f"code={code}")
if code == 200:
    check("POST /commerce/extract has data", "data" in d)

# 20. Negative constraint regression: "X other than Y"
print("\n=== Negative constraint regression ===")
code, d = get("/search?q=static+site+generators+other+than+nextjs&count=10")
check("Negative constraint query returns 200", code == 200, f"code={code}")
if code == 200:
    check("Negative constraint returns results", len(d.get("results", [])) > 0, f"count={len(d.get('results', []))}")

# 21. Positive-override regression: "python not django"
print("\n=== Positive-override regression ===")
code, d = get("/search?q=python+web+framework+not+django&count=10")
check("Positive-override query returns 200", code == 200, f"code={code}")
if code == 200:
    check("Positive-override returns results", len(d.get("results", [])) > 0, f"count={len(d.get('results', []))}")

# 22. NL long query regression
print("\n=== NL long query regression ===")
code, d = get("/search?q=I+am+a+frontend+developer+with+3+years+of+experience+and+I+want+to+become+a+full+stack+developer&count=10")
check("NL long query returns 200", code == 200, f"code={code}")
if code == 200:
    check("NL long query returns results", len(d.get("results", [])) > 0, f"count={len(d.get('results', []))}")

# Summary
print("\n" + "=" * 60)
passed = sum(1 for _, ok, _ in results if ok)
total = len(results)
print(f"SUMMARY: {passed}/{total} checks passed")
if passed < total:
    print("\nFAILED CHECKS:")
    for name, ok, detail in results:
        if not ok:
            print(f"  - {name}: {detail}")
sys.exit(0 if passed == total else 1)
