#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite (2026-05-29, v2)
Field name fix: uses 'results' not 'web_results'
Tests: general, complex, constraints, edge cases, quality audit, stress
"""
import json
import time
import urllib.request
import urllib.parse
from concurrent.futures import ThreadPoolExecutor, as_completed

API = "http://localhost:4000/search"
FAST_API = "http://localhost:4000/search/fast"
IMAGES_API = "http://localhost:4000/images"
VIDEOS_API = "http://localhost:4000/videos"
NEWS_API = "http://localhost:4000/news"

def search(endpoint, query, timeout=25):
    start = time.time()
    try:
        url = f"{endpoint}?q={urllib.parse.quote(query)}"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode().strip()
            data = json.loads(raw)
        return {"ok": True, "data": data, "time": time.time() - start, "raw_len": len(raw)}
    except Exception as e:
        return {"ok": False, "error": str(e), "time": time.time() - start}

def analyze(result):
    if not result["ok"]:
        return {"status": "ERROR", "error": result["error"], "time": result["time"]}
    d = result["data"]
    results = d.get("results", [])
    return {
        "status": "OK",
        "intent": d.get("intent", "?"),
        "confidence": d.get("confidence", 0),
        "count": len(results),
        "top_score": round(results[0]["score"], 4) if results else 0,
        "constraints": d.get("structured_constraints", {}),
        "expanded_queries": d.get("expanded_queries", []),
        "top3": [(r["title"][:70], r.get("url","")[:60], round(r["score"],3), r.get("authority",0)) for r in results[:3]],
        "time": round(result["time"], 2),
        "raw_len": result.get("raw_len", 0),
    }

# ============================================================
# TEST QUERIES
# ============================================================

GENERAL_QUERIES = [
    "python programming tutorials",
    "how to learn guitar chords",
    "climate change effects on agriculture",
    "best restaurants in New York",
    "machine learning basics",
    "how does blockchain work",
    "healthy breakfast recipes",
    "latest space exploration news",
]

COMPLEX_QUERIES = [
    "rust async runtime performance benchmarks 2026 comparison tokio vs async-std",
    "distributed consensus algorithm comparison raft paxos byzantine fault tolerance",
    "neural network architecture search efficient transformer variants for edge deployment",
    "comparative analysis of container orchestration platforms kubernetes vs nomad vs docker swarm",
    "post-quantum cryptography lattice-based schemes NIST standardization timeline",
]

CONSTRAINT_POSITIVE = [
    ("python web framework with async support", "should have async results"),
    ("rust library for parsing JSON", "should have JSON/serde results"),
    ("javascript framework with typescript", "should have TS results"),
]

CONSTRAINT_NEGATIVE = [
    ("web framework not django", "should avoid django"),
    ("css framework not bootstrap not tailwind", "should avoid bootstrap/tailwind"),
    ("programming language for systems not rust", "should avoid rust"),
    ("database not mysql not postgresql", "should avoid mysql/postgresql"),
    ("frontend framework not react not angular", "should avoid react/angular"),
]

CONSTRAINT_MIXED = [
    ("fast web framework for python not django", "noise + positive + negative"),
    ("modern css utility framework not tailwind", "noise + negative"),
    ("async web server for rust not actix", "positive + negative"),
]

EDGE_CASES = [
    ("a", "single char"),
    ("the", "stop word only"),
    ("x" * 500, "500 char query"),
    ("", "empty query"),
    ("what is the meaning of life", "philosophical"),
    ("COVID-19 mRNA vaccine efficacy 2026", "special chars + year"),
    ("C++ vs Rust memory safety", "special chars"),
]

# ============================================================
# RUN TESTS
# ============================================================

results_log = []

def run_category(name, items, endpoint=API, analyze_fn=analyze):
    print(f"\n{'='*70}")
    print(f"  {name}")
    print(f"{'='*70}")
    cat_results = []
    for item in items:
        if isinstance(item, tuple):
            q, desc = item
        else:
            q, desc = item, ""
        r = analyze_fn(search(endpoint, q))
        icon = "\u2713" if r["status"] == "OK" else "\u2717"
        time_str = f"{r['time']:.2f}s"
        print(f"  {icon} [{time_str:>6}] {q[:55]}")
        if desc:
            print(f"          Expected: {desc}")
        if r["status"] == "OK":
            print(f"          Intent: {r['intent']} (conf={r['confidence']:.2f}) | Results: {r['count']} | Top: {r['top_score']}")
            cons = r.get("constraints", {})
            if cons.get("positive") or cons.get("negative"):
                print(f"          Constraints: +{cons.get('positive',[])} -{cons.get('negative',[])}")
            eq = r.get("expanded_queries", [])
            if eq and len(eq) > 1:
                print(f"          Expanded: {eq}")
            for i, (title, url, score, auth) in enumerate(r.get("top3", [])):
                print(f"          [{i+1}] ({score:.3f}, auth={auth:.2f}) {title}")
                if url:
                    print(f"              {url}")
        else:
            print(f"          ERROR: {r.get('error','?')[:80]}")
        cat_results.append((q, r))
        results_log.append({"category": name, "query": q, "desc": desc, **r})
    return cat_results

# ---- GENERAL ----
run_category("GENERAL QUERIES", GENERAL_QUERIES)

# ---- COMPLEX ----
run_category("COMPLEX QUERIES", COMPLEX_QUERIES)

# ---- CONSTRAINTS: POSITIVE ----
run_category("POSITIVE CONSTRAINTS", CONSTRAINT_POSITIVE)

# ---- CONSTRAINTS: NEGATIVE ----
run_category("NEGATIVE CONSTRAINTS", CONSTRAINT_NEGATIVE)

# ---- CONSTRAINTS: MIXED ----
run_category("MIXED CONSTRAINTS", CONSTRAINT_MIXED)

# ---- EDGE CASES ----
run_category("EDGE CASES", EDGE_CASES)

# ---- FAST ENDPOINT ----
def analyze_fast(result):
    if not result["ok"]:
        return {"status": "ERROR", "error": result["error"], "time": result["time"]}
    d = result["data"]
    results = d.get("results", [])
    return {
        "status": "OK",
        "count": len(results),
        "top3": [(r.get("title","")[:60], round(r.get("score",0),3)) for r in results[:3]],
        "time": round(result["time"], 2),
    }

print(f"\n{'='*70}")
print(f"  /search/fast ENDPOINT (local index only)")
print(f"{'='*70}")
for q in ["python programming", "rust async", "machine learning", "web framework"]:
    r = analyze_fast(search(FAST_API, q))
    icon = "\u2713" if r["status"] == "OK" else "\u2717"
    print(f"  {icon} [{r['time']:.2f}s] {q}")
    if r["status"] == "OK":
        print(f"          Results: {r['count']}")
        for i, (title, score) in enumerate(r.get("top3", [])):
            print(f"          [{i+1}] ({score}) {title}")

# ---- MEDIA ENDPOINTS ----
for ep_name, ep_url, queries, results_key in [
    ("IMAGES", IMAGES_API, ["sunset over mountains", "python programming logo"], "image_results"),
    ("VIDEOS", VIDEOS_API, ["rust programming tutorial", "machine learning explained"], "video_results"),
    ("NEWS", NEWS_API, ["artificial intelligence latest developments", "climate summit 2026"], "news_results"),
]:
    print(f"\n{'='*70}")
    print(f"  /{ep_name.lower()} ENDPOINT")
    print(f"{'='*70}")
    for q in queries:
        r = search(ep_url, q)
        icon = "\u2713" if r["ok"] else "\u2717"
        print(f"  {icon} [{r['time']:.2f}s] {q}")
        if r["ok"]:
            d = r["data"]
            items = d.get(results_key, d.get("results", []))
            print(f"          Count: {len(items)}")
            for i, item in enumerate(items[:3]):
                print(f"          [{i+1}] {item.get('title','')[:60]}")
        else:
            print(f"          ERROR: {r['error'][:80]}")

# ---- STRESS TEST ----
print(f"\n{'='*70}")
print(f"  STRESS TEST (15 concurrent requests)")
print(f"{'='*70}")
stress_queries = [
    "python programming", "rust vs go performance", "css framework comparison",
    "machine learning algorithms", "web scraping tools", "react vs vue 2026",
    "database comparison", "kubernetes tutorial", "graphql vs rest",
    "docker best practices", "linux kernel development", "typescript migration guide",
    "golang concurrency patterns", "swift ui framework", "java spring boot",
]
start_all = time.time()
stress_results = []
with ThreadPoolExecutor(max_workers=15) as ex:
    futs = {ex.submit(search, API, q): q for q in stress_queries}
    for f in as_completed(futs):
        r = f.result()
        stress_results.append(r)
wall = time.time() - start_all
ok_count = sum(1 for r in stress_results if r["ok"])
times_list = [r["time"] for r in stress_results if r["ok"]]
result_counts = []
for r in stress_results:
    if r["ok"]:
        result_counts.append(len(r["data"].get("results", [])))
print(f"  Wall time:   {wall:.2f}s")
print(f"  Success:     {ok_count}/{len(stress_queries)}")
if times_list:
    print(f"  Avg latency: {sum(times_list)/len(times_list):.2f}s")
    print(f"  Min latency: {min(times_list):.2f}s")
    print(f"  Max latency: {max(times_list):.2f}s")
    sorted_t = sorted(times_list)
    print(f"  P50 latency: {sorted_t[len(sorted_t)//2]:.2f}s")
    print(f"  P95 latency: {sorted_t[int(len(sorted_t)*0.95)]:.2f}s")
if result_counts:
    print(f"  Avg results: {sum(result_counts)/len(result_counts):.0f}")
    print(f"  Min results: {min(result_counts)}")
    print(f"  Zero-result: {result_counts.count(0)}")
errors = [r for r in stress_results if not r["ok"]]
for e in errors:
    print(f"  ERROR: {e['error'][:80]}")

# ---- QUALITY AUDIT: Deep check on 5 results per query ----
print(f"\n{'='*70}")
print(f"  QUALITY AUDIT (checking top-5 relevance per query)")
print(f"{'='*70}")
quality_queries = [
    ("python web framework django flask", ["python", "web", "framework"]),
    ("rust memory safety ownership borrowing", ["rust", "memory"]),
    ("machine learning neural network training", ["machine learning", "neural"]),
    ("kubernetes container orchestration deployment", ["kubernetes", "container"]),
    ("javascript async await promise", ["javascript", "async", "promise"]),
]
for q, keywords in quality_queries:
    r = search(API, q)
    if not r["ok"]:
        print(f"  SKIP: {q[:40]} - {r['error'][:40]}")
        continue
    d = r["data"]
    results = d.get("results", [])
    print(f"\n  Query: \"{q}\"")
    print(f"  Intent: {d.get('intent','?')} | Results: {len(results)}")
    
    relevant = 0
    for i, res in enumerate(results[:5]):
        title_lower = res["title"].lower()
        content_lower = res.get("content", "").lower()
        url_lower = res.get("url", "").lower()
        combined = f"{title_lower} {content_lower} {url_lower}"
        
        is_relevant = any(kw.lower() in combined for kw in keywords)
        if is_relevant:
            relevant += 1
        icon = "\u2713" if is_relevant else "\u2717"
        print(f"    {icon} [{i+1}] Score={res['score']:.3f} Auth={res.get('authority',0):.2f}")
        print(f"        {res['title'][:65]}")
        print(f"        {res.get('url','')[:60]}")
    
    relevance_pct = (relevant / min(5, len(results))) * 100
    grade = "A" if relevance_pct >= 80 else "B" if relevance_pct >= 60 else "C" if relevance_pct >= 40 else "D"
    print(f"    >> Relevance: {relevant}/5 ({relevance_pct:.0f}%) - Grade: {grade}")

# ---- SUMMARY ----
print(f"\n{'='*70}")
print(f"  OVERALL SUMMARY")
print(f"{'='*70}")
all_ok = [r for r in results_log if r["status"] == "OK"]
all_err = [r for r in results_log if r["status"] != "OK"]
print(f"  Total queries:    {len(results_log)}")
print(f"  Success:          {len(all_ok)}")
print(f"  Errors:           {len(all_err)}")
if all_ok:
    t = [r["time"] for r in all_ok]
    counts = [r.get("count", 0) for r in all_ok]
    print(f"  Timing:           avg={sum(t)/len(t):.2f}s  min={min(t):.2f}s  max={max(t):.2f}s")
    print(f"  Results/query:    avg={sum(counts)/len(counts):.1f}  min={min(counts)}  max={max(counts)}")
    zero = [r for r in all_ok if r.get("count", 0) == 0]
    print(f"  Zero-result:      {len(zero)}/{len(all_ok)}")
    scores = [r["top_score"] for r in all_ok if r.get("top_score", 0) > 0]
    if scores:
        print(f"  Top scores:       avg={sum(scores)/len(scores):.3f}  min={min(scores):.3f}  max={max(scores):.3f}")

print(f"\n{'='*70}")
print(f"  TEST COMPLETE")
print(f"{'='*70}")
