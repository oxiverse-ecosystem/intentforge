#!/usr/bin/env python3
"""
IntentForge Comprehensive Stress Test — Latency + Quality + Intent
Measures: latency percentiles, result quality, source diversity, content
availability, relevance scores, negative constraint violations, concurrent
throughput, and cache performance.

Run from host machine. Uses urllib.request (no deps).
"""

import json, time, urllib.request, urllib.parse, statistics, re, sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed

BASE = "http://localhost:4000"
TIMEOUT = 25
PACING = 1.8  # seconds between sequential requests

STOPWORDS = {
    "the","a","an","is","are","was","were","be","been","being","have","has",
    "had","do","does","did","will","would","shall","should","may","might",
    "can","could","of","in","on","at","to","for","with","from","by","as",
    "into","through","during","before","after","above","below","between",
    "out","off","over","under","again","further","then","once","and","but",
    "or","nor","not","so","very","just","than","too","also","about","up",
    "it","its","i","me","my","we","our","you","your","he","she","they",
    "them","this","that","these","those","what","which","who","whom",
    "how","where","when","why","all","each","every","both","few","more",
    "most","other","some","such","no","only","own","same","if","else",
    "because","while","until","although","since","unless","whether",
    "get","use","using","used","make","made","set","like","new","one",
    "two","first","best","good","better","great","many","much","any",
}

def search(query, timeout=TIMEOUT):
    url = f"{BASE}/search?q={urllib.parse.quote(query)}"
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read().strip())
            return {"ok": True, "data": data, "time": time.time() - start}
    except Exception as e:
        return {"ok": False, "error": str(e), "time": time.time() - start}

def keyword_relevance(query, title, content="", url=""):
    """Fraction of query keywords found in result fields."""
    q_words = set(re.findall(r'\b\w+\b', query.lower())) - STOPWORDS
    q_words = {w for w in q_words if len(w) > 2}
    if not q_words:
        return 1.0
    combined = (title + " " + content + " " + url).lower()
    matched = sum(1 for w in q_words if w in combined)
    return matched / len(q_words)

# ============================================================
# QUERY DEFINITIONS — unique, diverse, complex
# ============================================================

# General queries across all intent categories
GENERAL = [
    ("how does pattern matching work in rust", "informational", 3),
    ("python requests library timeout configuration", "how-to", 3),
    ("best static site generators 2026", "comparison", 3),
    ("buy refurbished thinkpad x1 carbon", "transactional", 2),
    ("docker compose healthcheck restart policy", "how-to", 3),
    ("tailwindcss v4 migration guide", "how-to", 3),
    ("why do microservices need a service mesh", "informational", 2),
    ("figma", "navigational", 3),
    ("deno vs bun vs node performance", "comparison", 3),
    ("latest zero-day exploit windows 2026", "fresh", 2),
    ("aws s3 presigned url expiration", "technical", 3),
    ("neovim kickstart configuration lua", "how-to", 3),
    ("stripe api webhook retry behavior", "technical", 3),
    ("what is vector database used for", "informational", 3),
    ("install arch linux uefi dual boot", "how-to", 3),
    ("react server components vs client components", "comparison", 3),
    ("github copilot pricing plans", "transactional", 2),
    ("sqlite wal mode concurrent reads", "technical", 3),
    ("causes of the 2008 financial crisis", "informational", 2),
    ("mongodb change stream alternatives", "comparison", 3),
    ("cloudflare workers vs aws lambda edge", "comparison", 3),
    ("oauth2 authorization code flow with pkce", "technical", 3),
    ("postgres explain analyze output interpretation", "how-to", 3),
    ("rust ownership borrowing lifetimes explained", "informational", 3),
    ("netflix", "navigational", 3),
]

# Complex / multi-concept / edge-case queries
COMPLEX = [
    ("implement rate limiting with token bucket in go gin middleware", "how-to", 2),
    ("kubernetes horizontal pod autoscaler custom metrics prometheus adapter", "technical", 2),
    ("elasticsearch query string syntax nested object array field", "technical", 2),
    ("react native expo eas build android keystore signing", "how-to", 2),
    ("grpc server streaming client load balancing envoy proxy", "technical", 2),
    ("terraform import existing aws resources into state file", "how-to", 2),
    ("nginx location precedence regex exact prefix match", "technical", 2),
    ("webpack module federation shared dependencies version mismatch", "how-to", 2),
    ("kafka consumer group lag monitoring alerting prometheus", "how-to", 2),
    ("wasm component model interface types proposal progress", "technical", 2),
    ("difference between graphql subscription and websocket real-time", "comparison", 2),
    ("redis cluster failover sentinel vs cluster mode tradeoffs", "comparison", 2),
    ("python web framework not django not flask", "technical", 3),
    ("javascript bundler besides webpack", "technical", 3),
    ("alternative to elasticsearch for log search", "comparison", 3),
    ("postgresql vacuum autovacuum bloat prevention tuning", "how-to", 2),
    ("oauth2 device authorization flow smart tv application", "technical", 2),
    ("c++20 modules migration from header based codebase", "how-to", 2),
    ("aws step functions vs temporal workflow orchestration", "comparison", 2),
    ("linux kernel io_uring vs epoll performance benchmark", "comparison", 2),
    ("next.js app router parallel routes intercepting routes", "technical", 2),
    ("hashicorp vault transit engine auto-unseal aws kms", "how-to", 2),
    ("risc-v vector extension simd performance comparison arm neon", "comparison", 1),
    ("svelte 5 runes migration from svelte 4 stores", "how-to", 2),
    ("causes of database connection pool exhaustion in production", "informational", 2),
    ("principles of distributed consensus algorithm safety liveness", "informational", 2),
    ("mechanism of linux page cache writeback dirty ratio tuning", "informational", 2),
    ("role of sidecar proxy in service mesh observability", "informational", 2),
    ("relationship between cap theorem and distributed database design", "informational", 2),
    ("evolution of javascript bundling from browserify to turbopack", "informational", 2),
]

ALL_QUERIES = GENERAL + COMPLEX

# ============================================================
# RUN TESTS
# ============================================================

print("=" * 72)
print("  INTENTFORGE COMPREHENSIVE STRESS TEST")
print("  Latency + Quality + Intent + Constraints")
print("=" * 72)
print(f"  Target: {BASE}")
print(f"  Queries: {len(ALL_QUERIES)} ({len(GENERAL)} general + {len(COMPLEX)} complex)")
print(f"  Pacing: {PACING}s between requests")
print()

# Health check
try:
    req = urllib.request.Request(f"{BASE}/health")
    with urllib.request.urlopen(req, timeout=5) as resp:
        assert resp.status == 200
    print("  Health: OK")
except Exception as e:
    print(f"  Health: FAILED — {e}")
    sys.exit(1)

print()

# ---- Section 1: Sequential queries with full quality analysis ----
print("-" * 72)
print("  SECTION 1: SEQUENTIAL QUERY TESTS")
print("-" * 72)
print()

results = []
latencies = []
intent_hits = 0
total = len(ALL_QUERIES)
empty_count = 0
thin_count = 0
all_sources = defaultdict(int)
all_relevance_scores = []
per_intent = defaultdict(lambda: {"correct": 0, "total": 0, "lats": [], "counts": [], "relevance": []})

for i, (query, expected, min_res) in enumerate(ALL_QUERIES):
    r = search(query)
    elapsed_ms = r["time"] * 1000
    latencies.append(r["time"])

    if r["ok"]:
        d = r["data"]
        intent = d.get("intent", "unknown")
        conf = d.get("confidence", 0.0)
        dist = d.get("distribution", {})
        res_list = d.get("results", [])
        count = len(res_list)
        constraints = d.get("structured_constraints", {})
        expanded = d.get("expanded_queries", [])

        # Intent accuracy
        match = intent == expected
        if match:
            intent_hits += 1

        # Result volume
        if count == 0:
            empty_count += 1
            status = "EMPTY"
        elif count < min_res:
            thin_count += 1
            status = "THIN"
        else:
            status = "OK"

        # Source diversity
        query_sources = set()
        for res in res_list:
            for s in res.get("sources", []):
                all_sources[s] += 1
                query_sources.add(s)

        # Content quality: top-5 relevance
        top5_relevance = []
        snippet_lengths = []
        authority_scores = []
        res_scores = []
        for j, res in enumerate(res_list[:5]):
            title = res.get("title", "")
            content = res.get("content", "")
            url = res.get("url", "")
            score = res.get("score", 0.0)
            authority = res.get("authority", 0.0)
            rel = keyword_relevance(query, title, content, url)
            top5_relevance.append(rel)
            snippet_lengths.append(len(content.strip()))
            authority_scores.append(authority)
            res_scores.append(score)
            all_relevance_scores.append(rel)

        avg_relevance = statistics.mean(top5_relevance) if top5_relevance else 0.0
        avg_snippet_len = statistics.mean(snippet_lengths) if snippet_lengths else 0.0
        snippet_avail = sum(1 for s in snippet_lengths if s > 20)
        avg_authority = statistics.mean(authority_scores) if authority_scores else 0.0

        # Negative constraint violations
        neg_constraints = constraints.get("negative", [])
        violations = 0
        if neg_constraints:
            for res in res_list[:10]:
                text = (res.get("title","") + " " + res.get("content","") + " " + res.get("url","")).lower()
                for neg in neg_constraints:
                    if re.search(r'\b' + re.escape(neg.lower()) + r'\b', text):
                        violations += 1

        results.append({
            "query": query, "expected": expected, "actual": intent,
            "confidence": conf, "distribution": dist, "count": count,
            "latency_ms": elapsed_ms, "status": status,
            "intent_match": match, "avg_relevance": avg_relevance,
            "avg_snippet_len": avg_snippet_len, "snippet_avail": snippet_avail,
            "avg_authority": avg_authority, "source_count": len(query_sources),
            "sources": query_sources, "neg_constraints": neg_constraints,
            "violations": violations, "expanded_count": len(expanded),
            "res_scores": res_scores, "top1_title": res_list[0]["title"][:60] if res_list else "(none)",
        })

        # Per-intent tracking
        pi = per_intent[expected]
        pi["total"] += 1
        if match: pi["correct"] += 1
        pi["lats"].append(elapsed_ms)
        pi["counts"].append(count)
        pi["relevance"].append(avg_relevance)

        # Progress
        mk = "Y" if match else "N"
        neg_str = f" neg_viol={violations}" if neg_constraints else ""
        print(f"  [{i+1:2d}/{total}] {mk} {elapsed_ms:6.0f}ms | {intent:15s} {conf:.3f} | {count:3d} res | rel={avg_relevance:.2f} | src={len(query_sources):2d}{neg_str} | {query[:48]}")

    else:
        results.append({"query": query, "expected": expected, "actual": "ERROR",
            "confidence": 0, "count": 0, "latency_ms": elapsed_ms,
            "status": "ERROR", "intent_match": False, "avg_relevance": 0,
            "avg_snippet_len": 0, "snippet_avail": 0, "avg_authority": 0,
            "source_count": 0, "sources": set(), "neg_constraints": [],
            "violations": 0, "expanded_count": 0, "res_scores": [],
            "top1_title": r.get("error","")[:60], "distribution": {}})
        empty_count += 1
        print(f"  [{i+1:2d}/{total}] E {elapsed_ms:6.0f}ms | ERROR           |   0 res | {query[:48]}")

    time.sleep(PACING)

# ---- Section 2: Cache Performance ----
print()
print("-" * 72)
print("  SECTION 2: CACHE PERFORMANCE")
print("-" * 72)
print()

cache_query = "python web framework"
cache_latencies = []
for i in range(5):
    r = search(cache_query)
    cache_latencies.append(r["time"] * 1000)
    print(f"  Cache hit {i+1}: {r['time']*1000:.0f}ms | {len(r.get('data',{}).get('results',[]))} results")

# ---- Section 3: Concurrent Stress Test ----
print()
print("-" * 72)
print("  SECTION 3: CONCURRENT STRESS TEST (10 unique queries)")
print("-" * 72)
print()

concurrent_queries = [
    "rust async runtime comparison 2026",
    "kubernetes ingress controller nginx traefik",
    "python data pipeline airflow prefect dagster",
    "react native vs flutter performance benchmark",
    "postgresql partitioning strategy large table",
    "go channels vs mutex concurrent patterns",
    "terraform aws ecs fargate service definition",
    "elasticsearch index lifecycle management hot warm cold",
    "redis streams vs kafka lightweight messaging",
    "linux cgroup v2 resource limit cpu memory",
]

conc_results = []
conc_start = time.time()

with ThreadPoolExecutor(max_workers=5) as pool:
    futures = {pool.submit(search, q): q for q in concurrent_queries}
    for fut in as_completed(futures):
        q = futures[fut]
        try:
            r = fut.result()
            ms = r["time"] * 1000
            count = len(r.get("data", {}).get("results", [])) if r["ok"] else 0
            conc_results.append({"query": q, "ms": ms, "count": count, "ok": r["ok"]})
            print(f"  {ms:6.0f}ms | {count:3d} results | {q[:50]}")
        except Exception as e:
            conc_results.append({"query": q, "ms": 0, "count": 0, "ok": False})
            print(f"  ERROR: {e} | {q[:50]}")

conc_wall = (time.time() - conc_start) * 1000

# ---- REPORT ----
print()
print("=" * 72)
print("  COMPREHENSIVE RESULTS")
print("=" * 72)
print()

# Intent accuracy
print(f"  INTENT ACCURACY:     {intent_hits}/{total} ({100*intent_hits/total:.1f}%)")
print(f"  Result availability:  {total - empty_count}/{total} non-empty ({100*(total-empty_count)/total:.1f}%)")
print(f"  Thin results:         {thin_count}")
print()

# Latency
s_lat = sorted([l * 1000 for l in latencies])
n = len(s_lat)
p50 = s_lat[n // 2]
p90 = s_lat[int(n * 0.9)]
p95 = s_lat[int(n * 0.95)]
p99 = s_lat[min(int(n * 0.99), n-1)]
avg_lat = statistics.mean(s_lat)

print(f"  LATENCY (uncached, {PACING}s pacing):")
print(f"    Average:  {avg_lat:7.0f}ms")
print(f"    p50:      {p50:7.0f}ms")
print(f"    p90:      {p90:7.0f}ms")
print(f"    p95:      {p95:7.0f}ms")
print(f"    p99:      {p99:7.0f}ms")
print(f"    Min:      {min(s_lat):7.0f}ms")
print(f"    Max:      {max(s_lat):7.0f}ms")
print()

print("  Latency distribution:")
buckets = [(0, 500), (500, 1500), (1500, 2500), (2500, 3500), (3500, 5000), (5000, 99999)]
for lo, hi in buckets:
    cnt = sum(1 for l in s_lat if lo <= l < hi)
    bar = "#" * cnt
    print(f"    {lo:5d}-{hi:5d}ms: {cnt:3d} {bar}")
print()

# Cache performance
print(f"  CACHE PERFORMANCE:")
if len(cache_latencies) >= 2:
    first_ms = cache_latencies[0]
    cached_avg = statistics.mean(cache_latencies[1:])
    print(f"    First hit (uncached):  {first_ms:.0f}ms")
    print(f"    Cached avg (hits 2-5): {cached_avg:.0f}ms")
    print(f"    Speedup:               {first_ms/cached_avg:.1f}x")
print()

# Source diversity
print(f"  SOURCE DIVERSITY:")
for src, cnt in sorted(all_sources.items(), key=lambda x: -x[1])[:15]:
    print(f"    {src:25s}: {cnt:4d} results")
unique_sources = len(all_sources)
print(f"    Unique sources total:  {unique_sources}")
print()

# Per-intent breakdown
print(f"  PER-INTENT BREAKDOWN:")
print(f"    {'Intent':20s}  {'Acc':>7s}  {'Avg ms':>7s}  {'Avg res':>7s}  {'Avg rel':>7s}")
print(f"    {'-'*20}  {'-'*7}  {'-'*7}  {'-'*7}  {'-'*7}")
for intent_name in sorted(per_intent.keys()):
    d = per_intent[intent_name]
    acc = f"{d['correct']}/{d['total']}"
    pct = 100 * d['correct'] / d['total'] if d['total'] else 0
    avg_l = statistics.mean(d['lats']) if d['lats'] else 0
    avg_c = statistics.mean(d['counts']) if d['counts'] else 0
    avg_r = statistics.mean(d['relevance']) if d['relevance'] else 0
    print(f"    {intent_name:20s}  {acc:>5s} ({pct:3.0f}%)  {avg_l:6.0f}ms  {avg_c:6.0f}  {avg_r:.3f}")
print()

# Quality metrics
print(f"  QUALITY METRICS:")
relevance_ok = sum(1 for r in all_relevance_scores if r >= 0.4)
relevance_high = sum(1 for r in all_relevance_scores if r >= 0.7)
print(f"    Top-5 relevance >= 40%:  {relevance_ok}/{len(all_relevance_scores)} ({100*relevance_ok/max(1,len(all_relevance_scores)):.0f}%)")
print(f"    Top-5 relevance >= 70%:  {relevance_high}/{len(all_relevance_scores)} ({100*relevance_high/max(1,len(all_relevance_scores)):.0f}%)")

# Snippet availability
snippets_avail = [r for r in results if r.get("snippet_avail", 0) >= 3]
print(f"    Queries with 3+ snippets in top-5: {len(snippets_avail)}/{len(results)} ({100*len(snippets_avail)/max(1,len(results)):.0f}%)")

# Authority
auth_scores = [r["avg_authority"] for r in results if r.get("avg_authority", 0) > 0]
if auth_scores:
    print(f"    Avg authority (top-5):   {statistics.mean(auth_scores):.3f}")
    print(f"    Authority range:         {min(auth_scores):.3f} - {max(auth_scores):.3f}")

# Score spread
all_top_scores = [r["res_scores"][0] for r in results if r.get("res_scores")]
if all_top_scores:
    print(f"    Avg top-1 score:         {statistics.mean(all_top_scores):.3f}")
    print(f"    Score range (top-1):     {min(all_top_scores):.3f} - {max(all_top_scores):.3f}")
print()

# Negative constraints
neg_queries = [r for r in results if r.get("neg_constraints")]
total_violations = sum(r["violations"] for r in neg_queries)
total_checked = sum(min(r["count"], 10) * len(r["neg_constraints"]) for r in neg_queries)
print(f"  NEGATIVE CONSTRAINTS:")
print(f"    Queries with neg constraints: {len(neg_queries)}")
print(f"    Total violations:            {total_violations}/{total_checked} checked")
for r in neg_queries:
    if r["violations"] > 0:
        print(f"      VIOLATION: \"{r['query']}\" — {r['violations']} violations (neg: {r['neg_constraints']})")
print()

# Concurrent results
print(f"  CONCURRENT STRESS (10 queries, 5 workers):")
ok_conc = [r for r in conc_results if r["ok"]]
if ok_conc:
    conc_lats = sorted([r["ms"] for r in ok_conc])
    conc_p50 = conc_lats[len(conc_lats)//2]
    conc_p95 = conc_lats[min(int(len(conc_lats)*0.95), len(conc_lats)-1)]
    throughput = len(ok_conc) / (conc_wall / 1000)
    print(f"    Succeeded:     {len(ok_conc)}/{len(concurrent_queries)}")
    print(f"    Wall time:     {conc_wall:.0f}ms")
    print(f"    Throughput:    {throughput:.2f} req/s")
    print(f"    Concurrent p50: {conc_p50:.0f}ms")
    print(f"    Concurrent p95: {conc_p95:.0f}ms")
print()

# Expanded queries
exp_counts = [r["expanded_count"] for r in results if r.get("expanded_count", 0) > 1]
print(f"  QUERY EXPANSION:")
print(f"    Queries with 2+ expanded queries: {len(exp_counts)}/{len(results)}")
if exp_counts:
    print(f"    Avg expanded count: {statistics.mean(exp_counts):.1f}")
print()

# Confidence calibration
confs = [r["confidence"] for r in results if r.get("confidence", 0) > 0]
correct_confs = [r["confidence"] for r in results if r.get("intent_match") and r.get("confidence", 0) > 0]
wrong_confs = [r["confidence"] for r in results if not r.get("intent_match") and r.get("confidence", 0) > 0]

print(f"  CONFIDENCE CALIBRATION:")
if confs:
    print(f"    Mean confidence:      {statistics.mean(confs):.3f}")
    print(f"    Median confidence:    {statistics.median(confs):.3f}")
    print(f"    Range:                {min(confs):.3f} - {max(confs):.3f}")
if correct_confs:
    print(f"    Correct predictions:  {statistics.mean(correct_confs):.3f} avg")
if wrong_confs:
    print(f"    Wrong predictions:    {statistics.mean(wrong_confs):.3f} avg")
print()

# Distribution field check
dists_present = sum(1 for r in results if r.get("distribution"))
print(f"  DISTRIBUTION FIELD:    {dists_present}/{len(results)} queries have distribution")
print()

# ---- FAILURES ----
failures = [r for r in results if not r["intent_match"]]
print(f"  FAILURES ({len(failures)}):")
for r in failures:
    dist_str = ""
    if r.get("distribution"):
        top2 = sorted(r["distribution"].items(), key=lambda x: -x[1])[:2]
        dist_str = f" dist: {top2[0][0]}={top2[0][1]:.3f} {top2[1][0]}={top2[1][1]:.3f}" if len(top2) >= 2 else ""
    print(f"    \"{r['query'][:55]}\"")
    print(f"      exp={r['expected']} got={r['actual']} ({r['confidence']:.3f}) | {r['count']} results | rel={r['avg_relevance']:.2f}{dist_str}")
print()

# ---- IRRELEVANT RESULTS DEEP INSPECT ----
print(f"  LOW-RELEVANCE QUERIES (top-5 avg < 30% keyword match):")
low_rel = [r for r in results if r.get("avg_relevance", 1) < 0.3 and r.get("count", 0) > 0]
if low_rel:
    for r in low_rel[:8]:
        print(f"    \"{r['query'][:50]}\" — avg relevance: {r['avg_relevance']:.2f}, top-1: \"{r['top1_title']}\"")
else:
    print("    None — all queries have >= 30% keyword relevance in top-5")
print()

# ---- SCORECARD ----
print("=" * 72)
print("  SCORECARD")
print("=" * 72)
print(f"    Intent accuracy:         {intent_hits}/{total} ({100*intent_hits/total:.1f}%)")
print(f"    Result availability:     {total - empty_count}/{total} ({100*(total-empty_count)/total:.1f}%)")
print(f"    Top-5 relevance >= 40%:  {relevance_ok}/{len(all_relevance_scores)} ({100*relevance_ok/max(1,len(all_relevance_scores)):.0f}%)")
print(f"    Source diversity:        {unique_sources} unique sources")
print(f"    Neg constraint viol:     {total_violations} violations")
print(f"    Latency p50:             {p50:.0f}ms (uncached)")
print(f"    Latency cached:          {statistics.mean(cache_latencies[1:]):.0f}ms" if len(cache_latencies) > 1 else "")
print(f"    Concurrent p50:          {conc_p50:.0f}ms" if ok_conc else "")
print(f"    Confidence range:        {min(confs):.3f} - {max(confs):.3f}" if confs else "")
print("=" * 72)
