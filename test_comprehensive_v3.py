#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite (2026-05-29, v3)
Covers: general, complex, positive/negative/multi constraints, edge cases,
        quality audit (top-5), stress testing, bottleneck identification.
"""
import json
import time
import urllib.request
import urllib.parse
import statistics
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import Optional

API = "http://localhost:4000/search"
FAST_API = "http://localhost:4000/search/fast"

# ============================================================
# HELPER
# ============================================================

def search(endpoint, query, timeout=30):
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

def extract_metrics(result):
    if not result["ok"]:
        return {"status": "ERROR", "error": result["error"], "time": result["time"]}
    d = result["data"]
    results = d.get("results", [])
    sc = d.get("structured_constraints", {})
    return {
        "status": "OK",
        "intent": d.get("intent", "?"),
        "confidence": d.get("confidence", 0),
        "count": len(results),
        "top_score": round(results[0]["score"], 4) if results else 0,
        "constraints_pos": sc.get("positive", []),
        "constraints_neg": sc.get("negative", []),
        "expanded_queries": d.get("expanded_queries", []),
        "top5": [(r["title"][:80], r.get("url","")[:80], round(r["score"],4), r.get("authority",0)) for r in results[:5]],
        "all_results": results,
        "time": round(result["time"], 2),
        "raw_len": result.get("raw_len", 0),
    }

@dataclass
class TestResult:
    category: str
    query: str
    description: str = ""
    status: str = ""
    intent: str = ""
    confidence: float = 0.0
    count: int = 0
    top_score: float = 0.0
    constraints_pos: list = field(default_factory=list)
    constraints_neg: list = field(default_factory=list)
    neg_violations: int = 0
    neg_violation_details: list = field(default_factory=list)
    top5: list = field(default_factory=list)
    latency: float = 0.0
    raw_len: int = 0
    grade: str = ""
    relevance_score: float = 0.0

# ============================================================
# TEST QUERIES
# ============================================================

GENERAL_QUERIES = [
    ("python programming tutorials", "basic informational"),
    ("how to learn guitar chords", "how-to intent"),
    ("climate change effects on agriculture", "informational / research"),
    ("best restaurants in New York", "local / recommendation"),
    ("machine learning basics", "educational / beginner"),
    ("how does blockchain work", "explanatory"),
    ("healthy breakfast recipes", "lifestyle / how-to"),
    ("latest space exploration news", "freshness / news intent"),
]

COMPLEX_QUERIES = [
    ("rust async runtime performance benchmarks 2026 comparison tokio vs async-std", "multi-facet: comparison + benchmark + year"),
    ("distributed consensus algorithm comparison raft paxos byzantine fault tolerance", "deep technical comparison"),
    ("neural network architecture search efficient transformer variants for edge deployment", "research + deployment constraint"),
    ("comparative analysis of container orchestration platforms kubernetes vs nomad vs docker swarm", "multi-way comparison"),
    ("post-quantum cryptography lattice-based schemes NIST standardization timeline", "niche technical + timeline"),
    ("implementing zero-knowledge proofs in Rust with arkworks library tutorial 2026", "implementation + specific lib + year"),
    ("WASM component model vs native modules performance trade-offs for server-side JavaScript", "emerging tech comparison"),
    ("CRDTs vs operational transforms for real-time collaborative editing algorithms", "algorithm-level comparison"),
]

POSITIVE_SINGLE = [
    ("python web framework with async support", ["async"], "single positive constraint"),
    ("rust library for parsing JSON", ["json", "parsing"], "single positive with 'for'"),
    ("javascript framework with typescript support", ["typescript"], "single positive"),
    ("database for time-series data", ["time-series"], "domain constraint"),
    ("css framework for responsive design", ["responsive"], "design constraint"),
]

POSITIVE_MULTI = [
    ("python web framework with async support and websocket", ["async", "websocket"], "two positive constraints"),
    ("rust library for parsing JSON with serde support", ["json", "serde"], "two positives with library name"),
    ("javascript testing framework with typescript and mocking support", ["typescript", "mocking"], "two positives"),
    ("database for time-series data with SQL interface and clustering", ["time-series", "sql", "clustering"], "three positives"),
]

NEGATIVE_SINGLE = [
    ("web framework not django", ["django"], "single negative"),
    ("css framework not bootstrap", ["bootstrap"], "single negative"),
    ("programming language for systems not rust", ["rust"], "negative after 'for'"),
    ("database not mysql", ["mysql"], "single negative"),
    ("frontend framework not react", ["react"], "single negative"),
    ("python web framework without flask", ["flask"], "'without' marker"),
]

NEGATIVE_MULTI = [
    ("web framework not django not flask", ["django", "flask"], "two negatives with 'not...not'"),
    ("css framework not bootstrap not tailwind", ["bootstrap", "tailwind"], "two negatives"),
    ("frontend framework not react not angular not vue", ["react", "angular", "vue"], "three negatives"),
    ("database not mysql not postgresql not mongodb", ["mysql", "postgresql", "mongodb"], "three negatives"),
    ("static site generator not jekyll not hugo", ["jekyll", "hugo"], "two negatives"),
    ("python web framework without flask except django", ["flask", "django"], "mixed markers: without + except"),
]

NEGATIVE_EXOTIC = [
    ("css framework other than bootstrap", ["bootstrap"], "'other than' marker"),
    ("javascript framework besides react", ["react"], "'besides' marker"),
    ("python ORM excluding sqlalchemy", ["sqlalchemy"], "'excluding' marker"),
    ("web server for rust excluding actix", ["actix"], "'excluding' after 'for'"),
    ("any database no sql not mongodb", ["mongodb"], "'no sql' + 'not' combo"),
]

MIXED_CONSTRAINTS = [
    ("fast web framework for python not django", {"pos": ["python", "web", "fast"], "neg": ["django"]}, "positive + negative"),
    ("modern css utility framework not tailwind", {"pos": ["css", "utility", "modern"], "neg": ["tailwind"]}, "positive + negative"),
    ("async web server for rust not actix", {"pos": ["rust", "async", "web"], "neg": ["actix"]}, "positive + negative"),
    ("lightweight database with json support not mongodb", {"pos": ["json", "lightweight"], "neg": ["mongodb"]}, "positive + negative"),
    ("python testing framework with coverage not pytest", {"pos": ["python", "testing", "coverage"], "neg": ["pytest"]}, "positive + negative"),
    ("javascript framework for mobile not react native not ionic", {"pos": ["javascript", "mobile"], "neg": ["react", "ionic"]}, "multi positive + multi negative"),
]

EDGE_CASES = [
    ("a", "single char — should fail or return minimal"),
    ("the", "stop word only"),
    ("x" * 500, "500 char query — stress on intent engine"),
    ("", "empty query — should 400"),
    ("   ", "whitespace only — should 400"),
    ("!!!@@@###", "special chars only — should 400"),
    ("what is the meaning of life", "philosophical / no clear intent"),
    ("COVID-19 mRNA vaccine efficacy 2026", "special chars + year + medical"),
    ("C++ vs Rust memory safety", "special chars in language names"),
    ("🎉 party supplies", "emoji in query"),
    ("SELECT * FROM users WHERE id = 1", "SQL injection attempt"),
    ("<script>alert('xss')</script>", "XSS attempt"),
    ("a" * 2000, "2000 char query — extreme length"),
]

QUALITY_AUDIT_QUERIES = [
    ("python web framework django flask fastapi", ["python", "web", "framework"]),
    ("rust memory safety ownership borrowing lifetimes", ["rust", "memory", "ownership"]),
    ("machine learning neural network training backpropagation", ["machine learning", "neural", "training"]),
    ("kubernetes container orchestration deployment pods", ["kubernetes", "container", "orchestration"]),
    ("javascript async await promise fetch API", ["javascript", "async", "promise"]),
    ("golang concurrency goroutines channels patterns", ["golang", "goroutine", "channel"]),
    ("typescript generics utility types mapped types", ["typescript", "generic", "type"]),
    ("postgresql query optimization indexing performance", ["postgresql", "query", "index"]),
]

# ============================================================
# RUNNER
# ============================================================

all_results: list[TestResult] = []

def run_category(name, items, extract_neg_terms=None):
    print(f"\n{'='*72}")
    print(f"  {name}")
    print(f"{'='*72}")
    cat_results = []
    for item in items:
        if isinstance(item, tuple) and len(item) == 2 and isinstance(item[1], str):
            q, desc = item
            expected_neg = []
            expected_pos = []
        elif isinstance(item, tuple) and len(item) == 3 and isinstance(item[1], list):
            q, expected_neg, desc = item
            expected_pos = []
        elif isinstance(item, tuple) and len(item) == 3 and isinstance(item[1], dict):
            q, exp, desc = item
            expected_neg = exp.get("neg", [])
            expected_pos = exp.get("pos", [])
        else:
            q, desc = item[0], item[-1] if len(item) > 1 else ""
            expected_neg = []
            expected_pos = []

        raw = search(API, q)
        m = extract_metrics(raw)

        tr = TestResult(category=name, query=q, description=desc, latency=m.get("time", 0))

        if m["status"] != "OK":
            tr.status = "ERROR"
            icon = "\u2717"
            print(f"  {icon} [{m['time']:.2f}s] {q[:58]}")
            print(f"          ERROR: {m.get('error','?')[:80]}")
            all_results.append(tr)
            cat_results.append(tr)
            continue

        tr.status = "OK"
        tr.intent = m["intent"]
        tr.confidence = m["confidence"]
        tr.count = m["count"]
        tr.top_score = m["top_score"]
        tr.constraints_pos = m["constraints_pos"]
        tr.constraints_neg = m["constraints_neg"]
        tr.top5 = m["top5"]
        tr.raw_len = m["raw_len"]

        icon = "\u2713" if m["count"] > 0 else "\u26a0"
        print(f"  {icon} [{m['time']:.2f}s] {q[:58]}")
        if desc:
            print(f"          Desc: {desc}")
        print(f"          Intent: {m['intent']} (conf={m['confidence']:.2f}) | Results: {m['count']} | Top: {m['top_score']:.4f}")

        # Constraint analysis
        actual_neg = m["constraints_neg"]
        actual_pos = m["constraints_pos"]
        if expected_neg:
            found = [n for n in expected_neg if n.lower() in [a.lower() for a in actual_neg]]
            missing = [n for n in expected_neg if n.lower() not in [a.lower() for a in actual_neg]]
            if found:
                print(f"          Neg constraints found: {found}")
            if missing:
                print(f"          \u26a0 Neg constraints MISSING: {missing}")
        if expected_pos:
            found_p = [p for p in expected_pos if any(p.lower() in a.lower() for a in actual_pos)]
            missing_p = [p for p in expected_pos if not any(p.lower() in a.lower() for a in actual_pos)]
            if found_p:
                print(f"          Pos constraints found: {found_p}")
            if missing_p:
                print(f"          \u26a0 Pos constraints partial/missing: {missing_p}")
        if actual_neg:
            print(f"          Constraints: +{actual_pos} -{actual_neg}")
        elif actual_pos:
            print(f"          Constraints: +{actual_pos}")

        # Check negative constraint violations in results
        if actual_neg:
            violations = 0
            violation_details = []
            for res in m["all_results"][:10]:
                title_lower = res["title"].lower()
                content_lower = res.get("content", "").lower()
                url_lower = res.get("url", "").lower()
                combined = f"{title_lower} {content_lower} {url_lower}"
                for neg in actual_neg:
                    neg_l = neg.lower()
                    # Check word boundary match
                    words = combined.split()
                    if any(neg_l in w or w.startswith(neg_l) for w in words if len(neg_l) >= 3):
                        violations += 1
                        violation_details.append((neg, res["title"][:60], round(res["score"], 4)))
                        break
            tr.neg_violations = violations
            tr.neg_violation_details = violation_details
            if violations > 0:
                print(f"          \u26a0 {violations}/10 results violate negative constraints")
                for neg, title, score in violation_details[:3]:
                    print(f"            -> '{neg}' found in: {title} (score={score})")
            else:
                print(f"          \u2713 No negative constraint violations in top-10")

        # Top 3 results
        for i, (title, url, score, auth) in enumerate(m["top5"][:3]):
            print(f"          [{i+1}] ({score:.4f}, auth={auth:.2f}) {title}")
            if url:
                print(f"              {url}")

        cat_results.append(tr)
        all_results.append(tr)

    return cat_results


# ============================================================
# RUN ALL TEST CATEGORIES
# ============================================================

print(f"\n{'#'*72}")
print(f"  INTENTFORGE v2 — COMPREHENSIVE API TEST SUITE (v3)")
print(f"  {time.strftime('%Y-%m-%d %H:%M:%S')}")
print(f"{'#'*72}")

run_category("GENERAL QUERIES", GENERAL_QUERIES)
run_category("COMPLEX QUERIES", COMPLEX_QUERIES)
run_category("POSITIVE CONSTRAINTS (SINGLE)", POSITIVE_SINGLE)
run_category("POSITIVE CONSTRAINTS (MULTI)", POSITIVE_MULTI)
run_category("NEGATIVE CONSTRAINTS (SINGLE)", NEGATIVE_SINGLE)
run_category("NEGATIVE CONSTRAINTS (MULTI)", NEGATIVE_MULTI)
run_category("NEGATIVE CONSTRAINTS (EXOTIC MARKERS)", NEGATIVE_EXOTIC)
run_category("MIXED CONSTRAINTS (POS + NEG)", MIXED_CONSTRAINTS)
run_category("EDGE CASES", EDGE_CASES)

# ============================================================
# FAST ENDPOINT
# ============================================================

print(f"\n{'='*72}")
print(f"  /search/fast ENDPOINT (local index only)")
print(f"{'='*72}")
for q in ["python programming", "rust async", "machine learning", "web framework"]:
    raw = search(FAST_API, q)
    if raw["ok"]:
        d = raw["data"]
        results = d.get("results", [])
        print(f"  \u2713 [{raw['time']:.2f}s] {q} -> {len(results)} results")
        for i, r in enumerate(results[:3]):
            print(f"      [{i+1}] ({r.get('score',0):.3f}) {r.get('title','')[:60]}")
    else:
        print(f"  \u2717 [{raw['time']:.2f}s] {q} -> ERROR: {raw['error'][:60]}")

# ============================================================
# STRESS TEST — Concurrent Requests
# ============================================================

print(f"\n{'='*72}")
print(f"  STRESS TEST (20 concurrent requests)")
print(f"{'='*72}")

stress_queries = [
    "python programming", "rust vs go performance", "css framework comparison",
    "machine learning algorithms", "web scraping tools", "react vs vue 2026",
    "database comparison sql nosql", "kubernetes tutorial deployment",
    "graphql vs rest api design", "docker best practices production",
    "linux kernel development guide", "typescript migration from javascript",
    "golang concurrency patterns goroutines", "swift ui framework tutorial",
    "java spring boot microservices", "flutter vs react native performance",
    "postgresql vs mysql benchmark 2026", "elasticsearch query optimization",
    "redis caching strategies patterns", "nginx reverse proxy configuration",
]

# Sequential baseline
print(f"\n  --- Sequential baseline (first 5) ---")
seq_times = []
for q in stress_queries[:5]:
    raw = search(API, q)
    seq_times.append(raw["time"])
    print(f"    [{raw['time']:.2f}s] {q}")
seq_avg = statistics.mean(seq_times)
print(f"    Sequential avg: {seq_avg:.2f}s")

# Concurrent burst
print(f"\n  --- Concurrent burst (all {len(stress_queries)}) ---")
start_all = time.time()
stress_results = []
with ThreadPoolExecutor(max_workers=20) as ex:
    futs = {ex.submit(search, API, q): q for q in stress_queries}
    for f in as_completed(futs):
        r = f.result()
        r["_query"] = futs[f]
        stress_results.append(r)
wall = time.time() - start_all

ok_results = [r for r in stress_results if r["ok"]]
err_results = [r for r in stress_results if not r["ok"]]
times_list = [r["time"] for r in ok_results]
result_counts = [len(r["data"].get("results", [])) for r in ok_results]

print(f"  Wall time:     {wall:.2f}s")
print(f"  Success:       {len(ok_results)}/{len(stress_queries)}")
print(f"  Errors:        {len(err_results)}")
if times_list:
    print(f"  Avg latency:   {statistics.mean(times_list):.2f}s")
    print(f"  Min latency:   {min(times_list):.2f}s")
    print(f"  Max latency:   {max(times_list):.2f}s")
    print(f"  P50 latency:   {statistics.median(times_list):.2f}s")
    sorted_t = sorted(times_list)
    p95_idx = int(len(sorted_t) * 0.95)
    p99_idx = int(len(sorted_t) * 0.99)
    print(f"  P95 latency:   {sorted_t[min(p95_idx, len(sorted_t)-1)]:.2f}s")
    print(f"  P99 latency:   {sorted_t[min(p99_idx, len(sorted_t)-1)]:.2f}s")
    print(f"  Stdev:         {statistics.stdev(times_list):.2f}s" if len(times_list) > 1 else "")
if result_counts:
    print(f"  Avg results:   {statistics.mean(result_counts):.0f}")
    print(f"  Min results:   {min(result_counts)}")
    print(f"  Zero-result:   {result_counts.count(0)}")
# Slowest queries
if ok_results:
    slowest = sorted(ok_results, key=lambda r: r["time"], reverse=True)[:5]
    print(f"\n  Slowest queries:")
    for r in slowest:
        print(f"    [{r['time']:.2f}s] {r['_query']}")
for e in err_results:
    print(f"  ERROR: {e['_query'][:40]} -> {e['error'][:60]}")

# ============================================================
# STRESS TEST 2 — Rapid-fire sequential (cache behavior)
# ============================================================

print(f"\n{'='*72}")
print(f"  CACHE BEHAVIOR TEST (same query 5x rapid)")
print(f"{'='*72}")
cache_q = "python web framework"
cache_times = []
for i in range(5):
    raw = search(API, cache_q)
    cache_times.append(raw["time"])
    status = "HIT" if raw["time"] < 0.5 else "MISS"
    print(f"  [{raw['time']:.2f}s] Attempt {i+1} ({status})")
print(f"  First call: {cache_times[0]:.2f}s | Subsequent avg: {statistics.mean(cache_times[1:]):.2f}s")
print(f"  Cache speedup: {cache_times[0] / max(statistics.mean(cache_times[1:]), 0.01):.1f}x")

# ============================================================
# QUALITY AUDIT — Deep relevance check on top-5
# ============================================================

print(f"\n{'='*72}")
print(f"  QUALITY AUDIT (top-5 relevance per query)")
print(f"{'='*72}")

quality_grades = []
for q, keywords in QUALITY_AUDIT_QUERIES:
    raw = search(API, q)
    if not raw["ok"]:
        print(f"  SKIP: {q[:40]} - {raw['error'][:40]}")
        continue
    d = raw["data"]
    results = d.get("results", [])
    print(f"\n  Query: \"{q}\"")
    print(f"  Intent: {d.get('intent','?')} (conf={d.get('confidence',0):.2f}) | Results: {len(results)}")

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
        print(f"    {icon} [{i+1}] Score={res['score']:.4f} Auth={res.get('authority',0):.2f}")
        print(f"        {res['title'][:70]}")
        print(f"        {res.get('url','')[:70]}")

    total = min(5, len(results))
    relevance_pct = (relevant / total * 100) if total > 0 else 0
    grade = "A" if relevance_pct >= 80 else "B" if relevance_pct >= 60 else "C" if relevance_pct >= 40 else "D" if relevance_pct >= 20 else "F"
    quality_grades.append((q, relevance_pct, grade, relevant, total))
    print(f"    >> Relevance: {relevant}/{total} ({relevance_pct:.0f}%) - Grade: {grade}")

# ============================================================
# BOTTLENECK IDENTIFICATION
# ============================================================

print(f"\n{'='*72}")
print(f"  BOTTLENECK IDENTIFICATION")
print(f"{'='*72}")

# Test /search/fast vs /search to isolate intent-engine overhead
print(f"\n  --- Intent Engine Overhead (search vs search/fast) ---")
bottleneck_queries = ["python programming", "rust async runtime", "machine learning basics", "web framework comparison"]
for q in bottleneck_queries:
    full_raw = search(API, q)
    fast_raw = search(FAST_API, q)
    if full_raw["ok"] and fast_raw["ok"]:
        overhead = full_raw["time"] - fast_raw["time"]
        pct = (overhead / full_raw["time"] * 100) if full_raw["time"] > 0 else 0
        print(f"  '{q[:35]}'")
        print(f"    Full: {full_raw['time']:.2f}s | Fast: {fast_raw['time']:.2f}s | Intent overhead: {overhead:.2f}s ({pct:.0f}%)")

# Payload size analysis
print(f"\n  --- Payload Size Analysis ---")
payload_sizes = []
for tr in all_results:
    if tr.status == "OK" and tr.raw_len > 0:
        payload_sizes.append((tr.query[:40], tr.raw_len, tr.count, tr.latency))
if payload_sizes:
    payload_sizes.sort(key=lambda x: x[1], reverse=True)
    print(f"  Largest payloads:")
    for q, size, count, lat in payload_sizes[:5]:
        print(f"    {size/1024:.1f}KB ({count} results, {lat:.2f}s) - {q}")
    avg_size = statistics.mean([s[1] for s in payload_sizes])
    print(f"  Average payload: {avg_size/1024:.1f}KB")
    print(f"  Total data transferred: {sum(s[1] for s in payload_sizes)/1024/1024:.2f}MB")

# Latency distribution
print(f"\n  --- Latency Distribution ---")
all_latencies = [tr.latency for tr in all_results if tr.status == "OK"]
if all_latencies:
    buckets = {"<1s": 0, "1-3s": 0, "3-5s": 0, "5-10s": 0, ">10s": 0}
    for l in all_latencies:
        if l < 1: buckets["<1s"] += 1
        elif l < 3: buckets["1-3s"] += 1
        elif l < 5: buckets["3-5s"] += 1
        elif l < 10: buckets["5-10s"] += 1
        else: buckets[">10s"] += 1
    for bucket, count in buckets.items():
        bar = "#" * count
        print(f"    {bucket:>5}: {count:3d} {bar}")

# Intent distribution
print(f"\n  --- Intent Distribution ---")
intent_counts = {}
for tr in all_results:
    if tr.status == "OK" and tr.intent:
        intent_counts[tr.intent] = intent_counts.get(tr.intent, 0) + 1
for intent, count in sorted(intent_counts.items(), key=lambda x: -x[1]):
    bar = "#" * count
    print(f"    {intent:<20}: {count:3d} {bar}")

# ============================================================
# OVERALL SUMMARY
# ============================================================

print(f"\n{'='*72}")
print(f"  OVERALL SUMMARY")
print(f"{'='*72}")

ok_results = [tr for tr in all_results if tr.status == "OK"]
err_results = [tr for tr in all_results if tr.status == "ERROR"]

print(f"  Total queries:        {len(all_results)}")
print(f"  Success:              {len(ok_results)}")
print(f"  Errors:               {len(err_results)}")

if ok_results:
    lats = [tr.latency for tr in ok_results]
    counts = [tr.count for tr in ok_results]
    scores = [tr.top_score for tr in ok_results if tr.top_score > 0]
    confs = [tr.confidence for tr in ok_results]

    print(f"\n  Timing:")
    print(f"    Avg:  {statistics.mean(lats):.2f}s")
    print(f"    Min:  {min(lats):.2f}s")
    print(f"    Max:  {max(lats):.2f}s")
    print(f"    P50:  {statistics.median(lats):.2f}s")

    print(f"\n  Results/query:")
    print(f"    Avg:  {statistics.mean(counts):.1f}")
    print(f"    Min:  {min(counts)}")
    print(f"    Max:  {max(counts)}")
    zero_res = [tr for tr in ok_results if tr.count == 0]
    print(f"    Zero: {len(zero_res)}/{len(ok_results)}")

    if scores:
        print(f"\n  Top scores:")
        print(f"    Avg:  {statistics.mean(scores):.4f}")
        print(f"    Min:  {min(scores):.4f}")
        print(f"    Max:  {max(scores):.4f}")

    print(f"\n  Confidence:")
    print(f"    Avg:  {statistics.mean(confs):.2f}")
    print(f"    Min:  {min(confs):.2f}")
    print(f"    Max:  {max(confs):.2f}")

    # Constraint extraction stats
    neg_queries = [tr for tr in ok_results if tr.constraints_neg]
    pos_queries = [tr for tr in ok_results if tr.constraints_pos]
    print(f"\n  Constraint extraction:")
    print(f"    Queries with neg constraints: {len(neg_queries)}")
    print(f"    Queries with pos constraints: {len(pos_queries)}")
    total_violations = sum(tr.neg_violations for tr in neg_queries)
    if neg_queries:
        total_checked = sum(min(10, tr.count) for tr in neg_queries)
        violation_rate = (total_violations / total_checked * 100) if total_checked > 0 else 0
        print(f"    Negative violations: {total_violations}/{total_checked} ({violation_rate:.1f}%)")

# Quality summary
if quality_grades:
    print(f"\n  Quality Audit:")
    grade_counts = {"A": 0, "B": 0, "C": 0, "D": 0, "F": 0}
    for q, pct, grade, rel, total in quality_grades:
        grade_counts[grade] = grade_counts.get(grade, 0) + 1
    for g in ["A", "B", "C", "D", "F"]:
        if grade_counts.get(g, 0) > 0:
            print(f"    Grade {g}: {grade_counts[g]} queries")
    avg_relevance = statistics.mean([pct for _, pct, _, _, _ in quality_grades])
    print(f"    Avg relevance: {avg_relevance:.0f}%")

# Bottleneck summary
print(f"\n  Bottleneck Summary:")
if all_latencies:
    slow = [l for l in all_latencies if l > 5]
    medium = [l for l in all_latencies if 3 < l <= 5]
    fast = [l for l in all_latencies if l <= 3]
    print(f"    Fast (<3s):   {len(fast)}/{len(all_latencies)}")
    print(f"    Medium (3-5s): {len(medium)}/{len(all_latencies)}")
    print(f"    Slow (>5s):   {len(slow)}/{len(all_latencies)}")
    if slow:
        print(f"    \u26a0 {len(slow)} queries exceeded 5s — investigate intent engine + SearXNG fan-out")

print(f"\n{'='*72}")
print(f"  TEST COMPLETE")
print(f"{'='*72}")
