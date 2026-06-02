#!/usr/bin/env python3
"""IntentForge v2 — Comprehensive Stress Test
Tests general queries, complex queries, latency percentiles, and result quality.
"""
import json, time, urllib.request, urllib.parse, sys, statistics

BASE = "http://localhost:4000/search?q="
TIMEOUT = 20

# ═══════════════════════════════════════════════════════════════
# QUERY BATTERY — 100 unique queries across all categories
# ═══════════════════════════════════════════════════════════════

GENERAL_QUERIES = [
    # Navigational
    ("github copilot", "navigational"),
    ("notion app", "navigational"),
    ("figma design tool", "navigational"),
    ("discord download", "navigational"),
    ("aws console login", "navigational"),
    ("npm registry", "navigational"),
    ("docker hub", "navigational"),
    ("stackoverflow developer", "navigational"),
    ("vercel deploy", "navigational"),
    ("stripe dashboard", "navigational"),

    # Informational
    ("what is quantum entanglement", "informational"),
    ("explain neural network backpropagation", "informational"),
    ("how does a blockchain consensus work", "informational"),
    ("what is the cap theorem in databases", "informational"),
    ("meaning of eventual consistency", "informational"),
    ("what are zero knowledge proofs", "informational"),
    ("explain the observer design pattern", "informational"),
    ("what is serverless computing", "informational"),
    ("difference between http and websocket", "informational"),
    ("what is a bloom filter", "informational"),

    # Technical
    ("rust async tokio runtime", "technical"),
    ("kubernetes pod scheduling affinity", "technical"),
    ("postgres partial indexes btree", "technical"),
    ("webassembly garbage collection proposal", "technical"),
    ("oauth2 pkce flow implementation", "technical"),
    ("nginx reverse proxy websocket config", "technical"),
    ("prometheus histogram quantile p99", "technical"),
    ("elasticsearch knn vector search", "technical"),
    ("grpc streaming bidirectional golang", "technical"),
    ("terraform state locking dynamodb", "technical"),

    # How-to
    ("how to set up wireguard vpn on vps", "how-to"),
    ("how to implement circuit breaker in microservices", "how-to"),
    ("how to configure haproxy for load balancing", "how-to"),
    ("how to build a cli app with rust clap", "how-to"),
    ("how to migrate postgresql to new server", "how-to"),
    ("how to set up grafana dashboards for kubernetes", "how-to"),
    ("how to implement redis caching in node.js", "how-to"),
    ("how to configure iptables firewall rules", "how-to"),
    ("how to automate backups with rclone", "how-to"),
    ("how to deploy static site to cloudflare pages", "how-to"),

    # Comparison
    ("clickhouse vs postgres for analytics", "comparison"),
    ("nats vs kafka message broker", "comparison"),
    ("deno fresh vs next.js app router", "comparison"),
    ("pulumi vs terraform for infrastructure", "comparison"),
    ("cockroachdb vs yugabytedb distributed sql", "comparison"),
    ("grafana vs datadog monitoring", "comparison"),
    ("vim vs neovim 2026", "comparison"),
    ("arm vs x86 server performance", "comparison"),
    ("sqlite vs duckdb embedded database", "comparison"),
    ("tailwind vs vanilla css performance", "comparison"),

    # Transactional
    ("buy raspberry pi 5", "transactional"),
    ("download ubuntu 24.04 server iso", "transactional"),
    ("sign up for github student pack", "transactional"),
    ("purchase .dev domain name", "transactional"),
    ("install nixos from scratch", "transactional"),
    ("subscribe to railway app hosting", "transactional"),

    # Fresh
    ("latest rust 2026 edition features", "fresh"),
    ("new security vulnerabilities cve may 2026", "fresh"),
    ("latest openai model release 2026", "fresh"),
    ("recent changes to docker licensing", "fresh"),
    ("linux kernel 6.14 release notes", "fresh"),
]

COMPLEX_QUERIES = [
    # Multi-intent / compound
    ("how to build a rust web server with actix and deploy on kubernetes with helm", "how-to"),
    ("best practices for migrating monolith to microservices while maintaining zero downtime", "how-to"),
    ("what is the difference between graphql federation and schema stitching in production", "comparison"),
    ("how to implement distributed tracing with opentelemetry in a polyglot microservices architecture", "how-to"),
    ("setup postgres with read replicas and connection pooling using pgbouncer for high availability", "how-to"),
    ("compare event sourcing with cqrs for building financial transaction systems", "comparison"),
    ("how to secure a rest api with jwt refresh tokens and rate limiting in express.js", "how-to"),
    ("implement blue green deployment strategy for docker containers on aws ecs", "how-to"),
    ("what is the best way to handle schema migrations in a distributed cockroachdb cluster", "informational"),
    ("how to optimize react server components with streaming ssr and suspense boundaries", "how-to"),

    # Ambiguous / tricky
    ("rust", "navigational"),
    ("python type hints performance overhead", "technical"),
    ("does redis support transactions", "informational"),
    ("should i use graphql or rest for mobile app", "comparison"),
    ("can postgres handle 10 million rows", "informational"),

    # Long-tail / natural language
    ("i need a lightweight alternative to elasticsearch for full text search in a small go project", "comparison"),
    ("what database should i use for a real time collaborative editing app like google docs", "comparison"),
    ("how do i debug memory leaks in a long running node.js process using heap snapshots", "how-to"),
    ("is there a way to run wasm modules inside a linux container without a browser", "informational"),
    ("what is the recommended way to handle authentication in a next.js app with server actions", "how-to"),

    # Edge cases
    ("", "informational"),                     # empty query
    ("a", "informational"),                    # single char
    ("react react react react", "informational"),  # repetition
    ("how to how to configure configure nginx", "how-to"),  # stutter
    ("best free open source no sql database 2026 for time series data iot", "comparison"),  # messy
]

ALL_QUERIES = GENERAL_QUERIES + COMPLEX_QUERIES

# ═══════════════════════════════════════════════════════════════
# RUN TESTS
# ═══════════════════════════════════════════════════════════════

def run_query(query, expected):
    encoded = urllib.parse.quote_plus(query)
    url = BASE + encoded
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "IntentForge-StressTest/1.0"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            body = resp.read().decode()
        elapsed_ms = round((time.time() - start) * 1000)
        data = json.loads(body)
        intent = data.get("intent", "?")
        confidence = data.get("confidence", 0)
        results = data.get("results", [])
        n = len(results)
        sources = set()
        for r in results:
            for s in r.get("sources", []):
                sources.add(s)
        top_title = results[0]["title"][:60] if n > 0 else "none"
        top_url = results[0]["url"][:70] if n > 0 else ""
        top_score = results[0].get("score", 0) if n > 0 else 0
        # Quality: avg score of top 3
        top_scores = [r.get("score", 0) for r in results[:3]]
        avg_top3 = round(sum(top_scores) / len(top_scores), 3) if top_scores else 0
        # Content availability
        has_content = sum(1 for r in results[:5] if len(r.get("content", "").strip()) > 20)
    except Exception as e:
        elapsed_ms = round((time.time() - start) * 1000)
        intent = "ERROR"; n = 0; sources = set(); top_title = str(e)[:60]
        top_url = ""; top_score = 0; avg_top3 = 0; confidence = 0; has_content = 0
    return {
        "query": query, "expected": expected, "actual": intent,
        "correct": intent == expected, "results": n, "sources": sources,
        "top_title": top_title, "top_url": top_url, "top_score": top_score,
        "avg_top3": avg_top3, "latency_ms": elapsed_ms, "confidence": confidence,
        "has_content": has_content,
    }

print("=" * 110)
print("  INTENTFORGE v2 — COMPREHENSIVE STRESS TEST")
print(f"  {len(ALL_QUERIES)} queries | {len(GENERAL_QUERIES)} general + {len(COMPLEX_QUERIES)} complex")
print("=" * 110)

print(f"\n{'─' * 110}")
print("  GENERAL QUERIES")
print(f"{'─' * 110}")

results = []
for query, expected in GENERAL_QUERIES:
    r = run_query(query, expected)
    results.append(r)
    status = "PASS" if r["correct"] else "FAIL"
    src_str = ",".join(sorted(r["sources"]))[:30] if r["sources"] else "none"
    print(f"  {status}  {r['latency_ms']:5d}ms  {query:48s} → {r['actual']:14s} [{r['results']:3d} res  conf={r['confidence']:.3f}  {src_str}]")

print(f"\n{'─' * 110}")
print("  COMPLEX QUERIES")
print(f"{'─' * 110}")

for query, expected in COMPLEX_QUERIES:
    r = run_query(query, expected)
    results.append(r)
    status = "PASS" if r["correct"] else "FAIL"
    q_display = query if query else "(empty)"
    src_str = ",".join(sorted(r["sources"]))[:30] if r["sources"] else "none"
    print(f"  {status}  {r['latency_ms']:5d}ms  {q_display:48s} → {r['actual']:14s} [{r['results']:3d} res  conf={r['confidence']:.3f}  {src_str}]")

# ═══════════════════════════════════════════════════════════════
# LATENCY ANALYSIS
# ═══════════════════════════════════════════════════════════════

latencies = [r["latency_ms"] for r in results]
errors = [r for r in results if r["actual"] == "ERROR"]
success_lats = [r["latency_ms"] for r in results if r["actual"] != "ERROR"]

print(f"\n{'=' * 110}")
print("  LATENCY ANALYSIS")
print(f"{'=' * 110}")
if success_lats:
    s = sorted(success_lats)
    p50 = s[len(s) // 2]
    p90 = s[int(len(s) * 0.9)]
    p95 = s[int(len(s) * 0.95)]
    p99 = s[int(len(s) * 0.99)]
    print(f"  Total queries:    {len(results)}")
    print(f"  Successful:       {len(success_lats)}")
    print(f"  Errors:           {len(errors)}")
    print(f"  Min latency:      {min(success_lats)}ms")
    print(f"  Max latency:      {max(success_lats)}ms")
    print(f"  Mean latency:     {round(statistics.mean(success_lats))}ms")
    print(f"  Median (p50):     {p50}ms")
    print(f"  p90:              {p90}ms")
    print(f"  p95:              {p95}ms")
    print(f"  p99:              {p99}ms")
    print(f"  Std dev:          {round(statistics.stdev(success_lats))}ms" if len(success_lats) > 1 else "")
    # Latency buckets
    buckets = [(0, 500), (500, 1000), (1000, 2000), (2000, 3000), (3000, 5000), (5000, 99999)]
    print(f"\n  Latency distribution:")
    for lo, hi in buckets:
        count = sum(1 for l in success_lats if lo <= l < hi)
        bar = "█" * (count * 2)
        label = f"{lo}-{hi}ms" if hi < 99999 else f"{lo}ms+"
        print(f"    {label:12s}: {count:3d} {bar}")

# Per-category latency
print(f"\n  Per-category latency:")
cats = ["navigational", "informational", "technical", "how-to", "comparison", "transactional", "fresh"]
for cat in cats:
    cat_r = [r for r in results if r["expected"] == cat and r["actual"] != "ERROR"]
    if cat_r:
        lats = [r["latency_ms"] for r in cat_r]
        avg = round(statistics.mean(lats))
        p = sum(1 for r in cat_r if r["correct"])
        print(f"    {cat:16s}: {avg:5d}ms avg  ({p}/{len(cat_r)} correct)  avg {round(statistics.mean([r['results'] for r in cat_r]), 1)} results")

# Complex category
cx = [r for r in results if r["expected"] not in cats and r["actual"] != "ERROR"]
if cx:
    lats = [r["latency_ms"] for r in cx]
    avg = round(statistics.mean(lats))
    p = sum(1 for r in cx if r["correct"])
    print(f"    {'complex':16s}: {avg:5d}ms avg  ({p}/{len(cx)} correct)  avg {round(statistics.mean([r['results'] for r in cx]), 1)} results")

# ═══════════════════════════════════════════════════════════════
# ACCURACY ANALYSIS
# ═══════════════════════════════════════════════════════════════

total_pass = sum(1 for r in results if r["correct"])
total_fail = sum(1 for r in results if not r["correct"])

print(f"\n{'=' * 110}")
print("  ACCURACY ANALYSIS")
print(f"{'=' * 110}")
print(f"  Overall: {total_pass}/{len(results)} correct ({round(total_pass * 100 / len(results), 1)}%)")

print(f"\n  Per-category accuracy:")
for cat in cats:
    cat_r = [r for r in results if r["expected"] == cat]
    if cat_r:
        p = sum(1 for r in cat_r if r["correct"])
        print(f"    {cat:16s}: {p}/{len(cat_r)} ({round(p * 100 / len(cat_r))}%)")

cx_all = [r for r in results if r["expected"] not in cats]
if cx_all:
    p = sum(1 for r in cx_all if r["correct"])
    print(f"    {'complex':16s}: {p}/{len(cx_all)} ({round(p * 100 / len(cx_all))}%)")

# ═══════════════════════════════════════════════════════════════
# QUALITY ANALYSIS
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 110}")
print("  QUALITY ANALYSIS")
print(f"{'=' * 110}")

# Result count distribution
result_counts = [r["results"] for r in results if r["actual"] != "ERROR"]
if result_counts:
    print(f"  Result count stats:")
    print(f"    Min:   {min(result_counts)}")
    print(f"    Max:   {max(result_counts)}")
    print(f"    Mean:  {round(statistics.mean(result_counts), 1)}")
    print(f"    Median:{sorted(result_counts)[len(result_counts)//2]}")
    empty = sum(1 for c in result_counts if c == 0)
    thin = sum(1 for c in result_counts if 0 < c <= 3)
    good = sum(1 for c in result_counts if c > 3)
    print(f"    Empty: {empty}  Thin(1-3): {thin}  Good(4+): {good}")

# Source diversity
print(f"\n  Source diversity:")
all_sources = {}
for r in results:
    for s in r["sources"]:
        all_sources[s] = all_sources.get(s, 0) + 1
for src, count in sorted(all_sources.items(), key=lambda x: -x[1]):
    bar = "█" * min(count, 50)
    print(f"    {src:20s}: {count:3d} {bar}")

# Top result quality
print(f"\n  Top-result relevance (score > 0.7 = GOOD):")
good_top = sum(1 for r in results if r["top_score"] > 0.7 and r["actual"] != "ERROR")
mid_top = sum(1 for r in results if 0.4 < r["top_score"] <= 0.7 and r["actual"] != "ERROR")
low_top = sum(1 for r in results if r["top_score"] <= 0.4 and r["actual"] != "ERROR")
err_count = sum(1 for r in results if r["actual"] == "ERROR")
print(f"    HIGH (>0.7):  {good_top}")
print(f"    MID  (0.4-0.7): {mid_top}")
print(f"    LOW  (<0.4):  {low_top}")
print(f"    ERROR:        {err_count}")

# Content availability
content_counts = [r["has_content"] for r in results if r["actual"] != "ERROR"]
if content_counts:
    avg_content = round(statistics.mean(content_counts), 1)
    print(f"\n  Content availability (top 5 results with >20 char snippets):")
    print(f"    Average: {avg_content}/5 results have content")

# Confidence stats
confs = [r["confidence"] for r in results if r["actual"] != "ERROR"]
if confs:
    print(f"\n  Intent confidence:")
    print(f"    Mean:   {round(statistics.mean(confs), 4)}")
    print(f"    Median: {round(sorted(confs)[len(confs)//2], 4)}")
    print(f"    Min:    {round(min(confs), 4)}")
    print(f"    Max:    {round(max(confs), 4)}")
    low_conf = sum(1 for c in confs if c < 0.1)
    print(f"    Low conf (<0.1): {low_conf}/{len(confs)}")

# ═══════════════════════════════════════════════════════════════
# FAILURES DETAIL
# ═══════════════════════════════════════════════════════════════

failures = [r for r in results if not r["correct"]]
print(f"\n{'=' * 110}")
print(f"  FAILURES ({len(failures)})")
print(f"{'=' * 110}")
for r in failures:
    q = r["query"] if r["query"] else "(empty)"
    print(f"  {q:50s} → got '{r['actual']}' (expected '{r['expected']}')  conf={r['confidence']:.3f}")

# ═══════════════════════════════════════════════════════════════
# ERRORS DETAIL
# ═══════════════════════════════════════════════════════════════

if errors:
    print(f"\n{'=' * 110}")
    print(f"  ERRORS ({len(errors)})")
    print(f"{'=' * 110}")
    for r in errors:
        q = r["query"] if r["query"] else "(empty)"
        print(f"  {q:50s} → {r['top_title']}")

# ═══════════════════════════════════════════════════════════════
# THIN RESULTS (queries with < 3 results)
# ═══════════════════════════════════════════════════════════════

thin = [r for r in results if r["results"] < 3 and r["actual"] != "ERROR"]
if thin:
    print(f"\n{'=' * 110}")
    print(f"  THIN RESULTS (< 3 results) — {len(thin)} queries")
    print(f"{'=' * 110}")
    for r in thin:
        q = r["query"] if r["query"] else "(empty)"
        print(f"  [{r['results']:2d}] {q:50s} → {r['top_title'][:50]}")

print(f"\n{'=' * 110}")
print(f"  STRESS TEST COMPLETE")
print(f"{'=' * 110}")
