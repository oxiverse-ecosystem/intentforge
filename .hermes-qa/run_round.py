#!/usr/bin/env python3
"""IntentForge NL quality round runner.

Runs a set of unique natural-language queries against the live dev stack
(http://localhost:4000), capturing REAL responses. Each query is run COLD
(no warmup repeat), so the first hit per query reflects fresh upstream fetch.

Outputs:
  .hermes-qa/round_raw.json  -- full captured responses (for the report)
  <query_log.txt lines>      -- appended by the calling step
"""
import json
import sys
import time
import urllib.parse
import urllib.request

BASE = "http://localhost:4000"

# 30 unique natural-language /search queries. Pure NL, no operators/dorks.
SEARCH_QUERIES = [
    "what is the best way to learn rust programming language as a complete beginner",
    "history of the world wide web and how it changed society",
    "restaurants serving authentic south indian food in hyderabad",
    "compare postgresql and mysql for a small web application",
    "text editor without vim keybindings for people who hate modal editing",
    "search engine alternative to google that respects privacy",
    "latest developments in quantum computing research this year",
    "how much does it cost to buy a decent gaming laptop under 80000 rupees",
    "what is the difference between ramen and raven in japanese culture",
    "steps to deploy a docker container to a production kubernetes cluster",
    "coffee shops with free wifi in tokyo shinjuku area",
    "why is the night sky blue but sunsets red explained simply",
    "best open source password manager that is not lastpass",
    "how to train a small neural network for image classification using pytorch",
    "what movies were released in theaters during summer of 2026",
    "programming language other than java for building android apps",
    "how does photosynthesis actually work at the molecular level",
    "compare rust and go for building high performance network servers",
    "where can i find free online courses about machine learning from stanford",
    "static site generator instead of jekyll for a technical blog",
    "what are the health benefits of drinking green tea every morning",
    "css framework besides bootstrap for rapid ui prototyping",
    "how to set up a wireguard vpn server on a cheap vps",
    "what is the meaning of the word biryani and its origin",
    "linux distribution that is good for older laptops with low ram",
    "explain how bitcoin mining works and why it uses so much electricity",
    "javascript framework except react for building single page applications",
    "what are some good books about astrophysics for beginners not textbooks",
    "how to make a simple rest api with go and postgresql",
    "what happened in the field of artificial intelligence during the month of july 2026",
]

# Rotating subset of other documented endpoints (goals + media).
OTHER_CALLS = [
    ("GET", "/videos", "how to cook chicken biryani at home"),
    ("GET", "/videos", "rust programming language full tutorial"),
    ("GET", "/news", "latest news about artificial intelligence breakthroughs"),
    ("GET", "/images", "photos of the andromeda galaxy"),
    ("POST_GOALS_QUICK", "/goals/quick", "learn to play the classical piano as an adult beginner"),
    ("GET", "/goals/leaderboard", None),
]


def fetch_get(path, params=None, timeout=60):
    url = BASE + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, headers={"User-Agent": "if-qa-round/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        body = r.read().decode("utf-8", "replace")
        return r.status, body


def fetch_post(path, payload, timeout=90):
    url = BASE + path
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url, data=data, headers={"Content-Type": "application/json"}, method="POST"
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.status, r.read().decode("utf-8", "replace")


def main():
    out = {"generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "base": BASE, "search": [], "other": []}
    # Search queries (cold, unique)
    for i, q in enumerate(SEARCH_QUERIES):
        rec = {"idx": i, "endpoint": "/search", "query": q}
        try:
            t0 = time.time()
            status, body = fetch_get("/search", {"q": q})
            rec["elapsed_s"] = round(time.time() - t0, 2)
            rec["status"] = status
            try:
                j = json.loads(body)
                rec["json"] = j
                rec["intent"] = j.get("intent")
                rec["confidence"] = j.get("confidence")
                rec["category"] = j.get("category")
                rec["spell_corrected_query"] = j.get("spell_corrected_query")
                rec["results_before_filter"] = j.get("results_before_filter")
                rec["results_after_filter"] = j.get("results_after_filter")
                rec["total"] = j.get("total")
                rec["n_results"] = len(j.get("results", []))
                rec["top5"] = [
                    {"title": r.get("title"), "url": r.get("url"),
                     "score": r.get("score"), "sources": r.get("sources")}
                    for r in j.get("results", [])[:5]
                ]
            except Exception as e:
                rec["parse_error"] = str(e)
                rec["raw_head"] = body[:500]
        except Exception as e:
            rec["error"] = str(e)
        out["search"].append(rec)
        print(f"[{i+1}/{len(SEARCH_QUERIES)}] /search q={q!r} -> "
              f"{rec.get('status', rec.get('error'))} "
              f"intent={rec.get('intent')} n={rec.get('n_results')} "
              f"before={rec.get('results_before_filter')} after={rec.get('results_after_filter')} "
              f"{rec.get('elapsed_s')}s", flush=True)
    # Other endpoints
    for kind, path, q in OTHER_CALLS:
        rec = {"endpoint": path, "kind": kind, "query": q}
        try:
            if kind == "GET":
                t0 = time.time()
                status, body = fetch_get(path, {"q": q})
                rec["elapsed_s"] = round(time.time() - t0, 2)
                rec["status"] = status
                try:
                    j = json.loads(body)
                    rec["json"] = j
                    if path in ("/videos", "/news", "/images"):
                        rec["n_results"] = len(j.get("results", []))
                        rec["top3"] = [
                            {"title": r.get("title"), "url": r.get("url"),
                             "source": r.get("source")}
                            for r in j.get("results", [])[:3]
                        ]
                except Exception as e:
                    rec["parse_error"] = str(e)
                    rec["raw_head"] = body[:500]
            elif kind == "POST_GOALS_QUICK":
                t0 = time.time()
                status, body = fetch_post(path, {"goal": q})
                rec["elapsed_s"] = round(time.time() - t0, 2)
                rec["status"] = status
                try:
                    j = json.loads(body)
                    rec["json"] = j
                    rec["goal_id"] = j.get("goal_id")
                    rm = j.get("roadmap") or {}
                    rec["n_phases"] = len(rm.get("phases", []))
                    rec["roadmap_title"] = rm.get("title")
                except Exception as e:
                    rec["parse_error"] = str(e)
                    rec["raw_head"] = body[:500]
            print(f"[other] {kind} {path} q={q!r} -> {rec.get('status', rec.get('error'))} "
                  f"n={rec.get('n_results', rec.get('goal_id', ''))} {rec.get('elapsed_s')}s",
                  flush=True)
        except Exception as e:
            rec["error"] = str(e)
            print(f"[other] {kind} {path} q={q!r} -> ERROR {e}", flush=True)
        out["other"].append(rec)
    with open(".hermes-qa/round_raw.json", "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, ensure_ascii=False)
    print(f"\nWrote .hermes-qa/round_raw.json with {len(out['search'])} search + "
          f"{len(out['other'])} other calls.")


if __name__ == "__main__":
    main()
