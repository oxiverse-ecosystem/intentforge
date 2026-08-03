#!/usr/bin/env python3
"""Round-4 NL quality harness — stdlib only, hits localhost:4000 cold."""
import json, sys, time, urllib.request, urllib.error, urllib.parse, os

BASE = "http://localhost:4000"
HERE = os.path.dirname(os.path.abspath(__file__))

def post(path, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data,
                                  headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.status, json.loads(r.read().decode())

def get(path):
    req = urllib.request.Request(BASE + path, method="GET")
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.status, json.loads(r.read().decode())

def run_item(it):
    ep = it["endpoint"]; qid = it["id"]; out = {"id": qid, "endpoint": ep}
    t0 = time.time()
    try:
        if ep == "search":
            q = it["q"]; out["q"] = q
            status, j = get("/search?q=" + urllib.parse.quote(q) + "&limit=24")
            out["status"] = status
            out["intent"] = j.get("intent"); out["confidence"] = j.get("confidence")
            out["category"] = j.get("category")
            out["results_before_filter"] = j.get("results_before_filter")
            out["total"] = j.get("total")
            out["neg"] = (j.get("structured_constraints") or {}).get("negative")
            out["pos"] = (j.get("structured_constraints") or {}).get("positive")
            out["spell"] = j.get("spell_corrected_query")
            out["error"] = j.get("error"); out["message"] = j.get("message")
            out["warnings"] = j.get("warnings")
            out["top5"] = [{"title": r.get("title"), "url": r.get("url"),
                            "score": round(r.get("score", 0), 3),
                            "authority": round(r.get("authority", 0), 3),
                            "sources": r.get("sources")} for r in j.get("results", [])[:15]]
        elif ep == "videos":
            q = it["q"]; out["q"] = q
            status, j = get("/videos?q=" + urllib.parse.quote(q))
            out["status"] = status; out["count"] = j.get("count")
            out["top5"] = [{"title": r.get("title"), "url": r.get("url"),
                            "score": round(r.get("score", 0), 3)} for r in j.get("results", [])[:5]]
        elif ep == "news":
            q = it["q"]; out["q"] = q
            status, j = get("/news?q=" + urllib.parse.quote(q))
            out["status"] = status; out["count"] = j.get("count")
            out["top5"] = [{"title": r.get("title"), "url": r.get("url")} for r in j.get("results", [])[:5]]
        elif ep == "images":
            q = it["q"]; out["q"] = q
            status, j = get("/images?q=" + urllib.parse.quote(q))
            out["status"] = status; out["count"] = j.get("count")
            out["top5"] = [{"title": r.get("title"), "url": r.get("image_url")} for r in j.get("results", [])[:5]]
        elif ep == "goals_quick":
            body = it["body"]; out["body"] = body
            status, j = post("/goals/quick", body)
            out["status"] = status; out["goal_id"] = j.get("goal_id")
            out["intent"] = j.get("intent"); out["resource_count"] = j.get("resource_count")
            rm = j.get("roadmap") or {}
            out["roadmap_title"] = rm.get("title")
            out["total_duration_weeks"] = rm.get("total_duration_weeks")
            out["phases"] = [p.get("title") for p in rm.get("phases", [])]
            out["snippet"] = (rm.get("overview") or "")[:300]
        elif ep == "goals_discovery":
            body = it["body"]; out["body"] = body
            status, j = post("/goals", body)
            out["status"] = status; out["goal_id"] = j.get("goal_id")
            out["intent"] = j.get("intent")
            out["questions"] = [{"id": q.get("id"), "q": q.get("question")} for q in j.get("questions", [])]
        else:
            out["error"] = "unknown endpoint"
    except urllib.error.HTTPError as e:
        out["status"] = e.code
        try: out["error_body"] = json.loads(e.read().decode())
        except Exception: out["error_body"] = e.read().decode()[:500]
    except Exception as e:
        out["status"] = "ERR"; out["error"] = repr(e)
    out["ms"] = round((time.time() - t0) * 1000)
    return out

def main():
    items = json.load(open(os.path.join(HERE, "queries.json")))
    only = sys.argv[1:] or [it["id"] for it in items]
    results = []
    for it in items:
        if it["id"] not in only: continue
        r = run_item(it)
        results.append(r)
        line = "[%s] %s ms status=%s" % (r["id"], r.get("ms"), r.get("status"))
        if r.get("intent"): line += " intent=%s conf=%s" % (r.get("intent"), r.get("confidence"))
        if r.get("total") is not None: line += " total=%s" % r.get("total")
        if r.get("error"): line += " ERR=%s" % r.get("error")
        print(line, flush=True)
    raw_path = os.path.join(HERE, "raw", "results.json")
    os.makedirs(os.path.dirname(raw_path), exist_ok=True)
    json.dump(results, open(raw_path, "w"), indent=2)
    print("WROTE", raw_path)

if __name__ == "__main__":
    main()
