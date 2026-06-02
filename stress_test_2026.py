#!/usr/bin/env python3
"""IntentForge v2 — Deep Stress Test: Latency + Search Quality (not just intent)
Focuses on: latency percentiles, result relevance, source diversity, score
distribution, content availability, and per-query quality inspection.
"""
import json, time, urllib.request, urllib.parse, sys, statistics, re, concurrent.futures

BASE = "http://localhost:4000"
TIMEOUT = 25
PACE_DELAY = 1.8  # seconds between requests to avoid circuit breaker

# ═══════════════════════════════════════════════════════════════
# QUERY BATTERY — unique, complex, and general queries
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
    ("cloudflare dashboard", "navigational"),

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

    # Fresh
    ("latest rust 2026 edition features", "fresh"),
    ("new security vulnerabilities cve may 2026", "fresh"),
    ("latest openai model release 2026", "fresh"),
    ("recent changes to docker licensing", "fresh"),
    ("linux kernel 6.14 release notes", "fresh"),

    # Exploration / descriptive (often misclassified)
    ("causes of the 2008 financial crisis", "informational"),
    ("evolution of database systems", "informational"),
    ("impact of social media on politics", "informational"),
    ("principles of distributed systems", "informational"),
    ("mechanism of neural network backpropagation", "informational"),
    ("overview of garbage collection algorithms", "informational"),
    ("role of mitochondria in cells", "informational"),
]

COMPLEX_QUERIES = [
    # Multi-concept compound
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

    # Deeply specific technical
    ("rayon work stealing thread pool internals and NUMA awareness", "technical"),
    ("io_uring vs epoll for high-frequency trading systems", "technical"),
    ("wasm component model interface types proposal status 2026", "technical"),
    ("cuda unified memory thrashing prevention on multi-gpu systems", "technical"),
    ("zerocopy serde deserialization without heap allocation", "technical"),

    # Long-tail natural language
    ("i need a lightweight alternative to elasticsearch for full text search in a small go project", "comparison"),
    ("what database should i use for a real time collaborative editing app like google docs", "comparison"),
    ("how do i debug memory leaks in a long running node.js process using heap snapshots", "how-to"),
    ("is there a way to run wasm modules inside a linux container without a browser", "informational"),
    ("what is the recommended way to handle authentication in a next.js app with server actions", "how-to"),

    # Ambiguous / tricky
    ("rust", "navigational"),
    ("python type hints performance overhead", "technical"),
    ("does redis support transactions", "informational"),
    ("should i use graphql or rest for mobile app", "comparison"),
    ("can postgres handle 10 million rows", "informational"),

    # Edge cases
    ("", "informational"),
    ("a", "informational"),
    ("react react react react", "informational"),
    ("how to how to configure configure nginx", "how-to"),
    ("C++ template metaprogramming constexpr if", "technical"),
    ("<html> tags semantic meaning", "informational"),
    ("what is the meaning of life the universe and everything", "informational"),
    ("best free open source no sql database 2026 for time series data iot", "comparison"),

    # Novel / unusual queries (real-world messiness)
    ("why does my docker container keep restarting with exit code 137", "how-to"),
    ("kubernetes pod stuck in pending state with insufficient cpu", "how-to"),
    ("nginx 502 bad gateway upstream timed out", "how-to"),
    ("postgresql deadlock detected while trying to get lock", "how-to"),
    ("redis cluster cross slot operation error", "how-to"),
    ("terraform error: provider produced inconsistent result", "how-to"),
    ("webpack build out of memory javascript heap", "how-to"),
    ("github actions workflow timeout exceeded", "how-to"),
    ("vscode remote ssh connection refused", "how-to"),
    ("helm install failed create could not find a ready tiller pod", "how-to"),
]

ALL_QUERIES = GENERAL_QUERIES + COMPLEX_QUERIES

LABEL_ALIASES = {
    "comparative": "comparison", "compare": "comparison", "howto": "how-to",
    "information": "informational", "navigate": "navigational",
}

# ═══════════════════════════════════════════════════════════════
# HELPERS
# ═══════════════════════════════════════════════════════════════

STOPWORDS = {"the","is","a","an","to","of","in","for","and","or","on","with",
    "how","what","why","does","do","i","my","should","can","is","there",
    "way","best","free","open","source","new","latest","recent"}

def normalize_intent(label):
    label = label.strip().lower()
    return LABEL_ALIASES.get(label, label)

def keyword_relevance(query, titles, urls, snippets):
    """Score 0-1: fraction of query keywords found in top results."""
    q_words = set(re.findall(r'\b\w+\b', query.lower())) - STOPWORDS
    q_words = {w for w in q_words if len(w) > 2}
    if not q_words:
        return 1.0  # can't judge, assume OK
    matches = 0
    total = 0
    for title, url, snip in zip(titles, urls, snippets):
        combined = f"{title} {url} {snip}".lower()
        for w in q_words:
            total += 1
            if w in combined:
                matches += 1
    return matches / total if total > 0 else 0

def search(query, timeout=TIMEOUT):
    encoded = urllib.parse.quote_plus(query)
    url = f"{BASE}/search?q={encoded}"
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={
            "Accept": "application/json",
            "User-Agent": "IntentForge-StressTest/2.0"
        })
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode()
        elapsed_ms = round((time.time() - start) * 1000)
        data = json.loads(body)
        return {"ok": True, "data": data, "time": elapsed_ms}
    except Exception as e:
        return {"ok": False, "error": str(e), "time": round((time.time() - start) * 1000)}

def run_query(query, expected):
    """Run a single query and compute all quality metrics."""
    if not query or not query.strip():
        return {
            "query": "(empty)", "expected": expected, "actual": "ERROR",
            "correct": False, "results": 0, "sources": set(),
            "top_title": "empty query", "top_url": "", "top_score": 0,
            "avg_top3": 0, "latency_ms": 0, "confidence": 0,
            "has_content": 0, "distribution": {},
            "relevance": 0, "domain_count": 0, "unique_engines": 0,
            "score_spread": 0, "top5_details": [],
        }

    r = search(query)
    if not r["ok"]:
        return {
            "query": query, "expected": expected, "actual": "ERROR",
            "correct": False, "results": 0, "sources": set(),
            "top_title": str(r["error"])[:80], "top_url": "", "top_score": 0,
            "avg_top3": 0, "latency_ms": r["time"], "confidence": 0,
            "has_content": 0, "distribution": {},
            "relevance": 0, "domain_count": 0, "unique_engines": 0,
            "score_spread": 0, "top5_details": [],
        }

    data = r["data"]
    intent = normalize_intent(data.get("intent", "?"))
    conf = data.get("confidence", 0)
    dist = data.get("distribution", {})
    results = data.get("results", [])
    n = len(results)

    # Sources and domains
    sources = set()
    domains = set()
    for res in results:
        for s in res.get("sources", []):
            sources.add(s)
        url = res.get("url", "")
        try:
            from urllib.parse import urlparse
            domains.add(urlparse(url).netloc)
        except:
            pass

    # Top results
    titles = [res.get("title", "") for res in results[:5]]
    urls = [res.get("url", "") for res in results[:5]]
    snippets = [res.get("content", "") or res.get("snippet", "") for res in results[:5]]

    top_title = results[0].get("title", "")[:60] if n > 0 else "none"
    top_url = results[0].get("url", "")[:80] if n > 0 else ""
    top_score = results[0].get("score", 0) if n > 0 else 0

    # Quality metrics
    top_scores = [res.get("score", 0) for res in results[:3]]
    avg_top3 = round(sum(top_scores) / len(top_scores), 3) if top_scores else 0

    score_spread = 0
    if n >= 2:
        scores_all = [res.get("score", 0) for res in results]
        score_spread = round(max(scores_all) - min(scores_all), 4)

    has_content = sum(1 for res in results[:5] if len(res.get("content", "").strip()) > 20)

    relevance = keyword_relevance(query, titles, urls, snippets)

    # Top-5 detail for quality inspection
    top5_details = []
    for i, res in enumerate(results[:5]):
        top5_details.append({
            "rank": i + 1,
            "title": res.get("title", "")[:70],
            "url": res.get("url", "")[:80],
            "score": res.get("score", 0),
            "sources": res.get("sources", []),
        })

    return {
        "query": query, "expected": expected, "actual": intent,
        "correct": intent == expected, "results": n, "sources": sources,
        "top_title": top_title, "top_url": top_url, "top_score": top_score,
        "avg_top3": avg_top3, "latency_ms": r["time"], "confidence": conf,
        "has_content": has_content, "distribution": dist,
        "relevance": relevance, "domain_count": len(domains),
        "unique_engines": len(sources), "score_spread": score_spread,
        "top5_details": top5_details,
    }

# ═══════════════════════════════════════════════════════════════
# MAIN TEST LOOP
# ═══════════════════════════════════════════════════════════════

print("=" * 120)
print("  INTENTFORGE v2 — DEEP STRESS TEST (Latency + Quality + Relevance)")
print(f"  {len(ALL_QUERIES)} queries | {len(GENERAL_QUERIES)} general + {len(COMPLEX_QUERIES)} complex")
print(f"  Base: {BASE} | Timeout: {TIMEOUT}s | Pace: {PACE_DELAY}s")
print("=" * 120)

results = []
start_time = time.time()

# --- Section 1: General Queries ---
print(f"\n{'─' * 120}")
print("  SECTION 1: GENERAL QUERIES (61 queries across 8 intent categories)")
print(f"{'─' * 120}")

for i, (query, expected) in enumerate(GENERAL_QUERIES):
    r = run_query(query, expected)
    results.append(r)
    status = "PASS" if r["correct"] else "FAIL"
    src_str = ",".join(sorted(r["sources"]))[:35] if r["sources"] else "none"
    rel_str = f"rel={r['relevance']:.0%}" if query else "rel=N/A"
    print(f"  {status}  {r['latency_ms']:5d}ms  {query:52s} → {r['actual']:14s} [{r['results']:3d} res  {rel_str}  conf={r['confidence']:.3f}  {src_str}]")
    time.sleep(PACE_DELAY)

# --- Section 2: Complex Queries ---
print(f"\n{'─' * 120}")
print("  SECTION 2: COMPLEX + EDGE CASE QUERIES (45 queries)")
print(f"{'─' * 120}")

for i, (query, expected) in enumerate(COMPLEX_QUERIES):
    r = run_query(query, expected)
    results.append(r)
    status = "PASS" if r["correct"] else "FAIL"
    q_display = query if query else "(empty)"
    src_str = ",".join(sorted(r["sources"]))[:35] if r["sources"] else "none"
    rel_str = f"rel={r['relevance']:.0%}" if query else "rel=N/A"
    print(f"  {status}  {r['latency_ms']:5d}ms  {q_display:52s} → {r['actual']:14s} [{r['results']:3d} res  {rel_str}  conf={r['confidence']:.3f}  {src_str}]")
    time.sleep(PACE_DELAY)

total_wall = round(time.time() - start_time)

# ═══════════════════════════════════════════════════════════════
# LATENCY ANALYSIS
# ═══════════════════════════════════════════════════════════════

success_lats = [r["latency_ms"] for r in results if r["actual"] != "ERROR" and r["query"] != "(empty)"]
error_lats = [r["latency_ms"] for r in results if r["actual"] == "ERROR"]

print(f"\n{'=' * 120}")
print("  LATENCY ANALYSIS")
print(f"{'=' * 120}")

if success_lats:
    s = sorted(success_lats)
    p50 = s[len(s) // 2]
    p75 = s[int(len(s) * 0.75)]
    p90 = s[int(len(s) * 0.9)]
    p95 = s[int(len(s) * 0.95)]
    p99 = s[min(int(len(s) * 0.99), len(s)-1)]

    print(f"  Successful queries: {len(success_lats)}")
    print(f"  Errors:             {len(error_lats)}")
    print(f"  Wall time:          {total_wall}s ({round(total_wall/60, 1)} min)")
    print(f"  Min latency:        {min(success_lats)}ms")
    print(f"  Max latency:        {max(success_lats)}ms")
    print(f"  Mean latency:       {round(statistics.mean(success_lats))}ms")
    print(f"  Median (p50):       {p50}ms")
    print(f"  p75:                {p75}ms")
    print(f"  p90:                {p90}ms")
    print(f"  p95:                {p95}ms")
    print(f"  p99:                {p99}ms")
    print(f"  Std dev:            {round(statistics.stdev(success_lats))}ms" if len(success_lats) > 1 else "")

    # Latency histogram
    buckets = [(0, 500), (500, 1000), (1000, 1500), (1500, 2000), (2000, 3000), (3000, 5000), (5000, 99999)]
    print(f"\n  Latency distribution:")
    for lo, hi in buckets:
        count = sum(1 for l in success_lats if lo <= l < hi)
        bar = "█" * min(count * 2, 80)
        label = f"{lo}-{hi}ms" if hi < 99999 else f"{lo}ms+"
        pct = round(count * 100 / len(success_lats), 1)
        print(f"    {label:12s}: {count:3d} ({pct:5.1f}%) {bar}")

# Per-category latency
print(f"\n  Per-category breakdown:")
cats = ["navigational", "informational", "technical", "how-to", "comparison", "transactional", "fresh"]
for cat in cats:
    cat_r = [r for r in results if r["expected"] == cat and r["actual"] != "ERROR" and r["query"] != "(empty)"]
    if cat_r:
        lats = [r["latency_ms"] for r in cat_r]
        avg_l = round(statistics.mean(lats))
        p = sum(1 for r in cat_r if r["correct"])
        avg_res = round(statistics.mean([r["results"] for r in cat_r]), 1)
        avg_rel = round(statistics.mean([r["relevance"] for r in cat_r]) * 100)
        print(f"    {cat:16s}: {avg_l:5d}ms avg | {p:2d}/{len(cat_r):2d} intent correct | {avg_res:5.1f} results avg | {avg_rel}% relevance")

# Complex group
cx = [r for r in results if r["expected"] not in cats and r["actual"] != "ERROR" and r["query"] != "(empty)"]
if cx:
    lats = [r["latency_ms"] for r in cx]
    avg_l = round(statistics.mean(lats))
    p = sum(1 for r in cx if r["correct"])
    avg_res = round(statistics.mean([r["results"] for r in cx]), 1)
    avg_rel = round(statistics.mean([r["relevance"] for r in cx]) * 100)
    print(f"    {'complex/other':16s}: {avg_l:5d}ms avg | {p:2d}/{len(cx):2d} intent correct | {avg_res:5.1f} results avg | {avg_rel}% relevance")

# ═══════════════════════════════════════════════════════════════
# INTENT ACCURACY
# ═══════════════════════════════════════════════════════════════

valid = [r for r in results if r["actual"] != "ERROR" and r["query"] != "(empty)"]
total_pass = sum(1 for r in valid if r["correct"])
total_fail = len(valid) - total_pass

print(f"\n{'=' * 120}")
print("  INTENT ACCURACY")
print(f"{'=' * 120}")
print(f"  Overall: {total_pass}/{len(valid)} correct ({round(total_pass * 100 / len(valid), 1)}%)")

print(f"\n  Per-category:")
for cat in cats:
    cat_r = [r for r in valid if r["expected"] == cat]
    if cat_r:
        p = sum(1 for r in cat_r if r["correct"])
        print(f"    {cat:16s}: {p:2d}/{len(cat_r):2d} ({round(p*100/len(cat_r))}%)")

if cx:
    p = sum(1 for r in cx if r["correct"])
    print(f"    {'complex/other':16s}: {p:2d}/{len(cx):2d} ({round(p*100/len(cx))}%)")

# ═══════════════════════════════════════════════════════════════
# SEARCH QUALITY ANALYSIS (the main event)
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  SEARCH QUALITY ANALYSIS")
print(f"{'=' * 120}")

# Result count distribution
result_counts = [r["results"] for r in valid]
if result_counts:
    empty = sum(1 for c in result_counts if c == 0)
    thin = sum(1 for c in result_counts if 1 <= c <= 3)
    ok = sum(1 for c in result_counts if 4 <= c <= 10)
    good = sum(1 for c in result_counts if c > 10)
    print(f"  Result count distribution:")
    print(f"    Empty (0):     {empty:3d} ({round(empty*100/len(result_counts))}%)")
    print(f"    Thin (1-3):    {thin:3d} ({round(thin*100/len(result_counts))}%)")
    print(f"    OK (4-10):     {ok:3d} ({round(ok*100/len(result_counts))}%)")
    print(f"    Good (11+):    {good:3d} ({round(good*100/len(result_counts))}%)")
    print(f"    Mean:          {round(statistics.mean(result_counts), 1)}")
    print(f"    Median:        {sorted(result_counts)[len(result_counts)//2]}")

# Top-result score distribution
print(f"\n  Top-1 result score distribution:")
high = sum(1 for r in valid if r["top_score"] > 0.7)
mid = sum(1 for r in valid if 0.4 < r["top_score"] <= 0.7)
low = sum(1 for r in valid if 0.1 < r["top_score"] <= 0.4)
zero = sum(1 for r in valid if r["top_score"] <= 0.1)
print(f"    HIGH (>0.7):   {high:3d} ({round(high*100/len(valid))}%)")
print(f"    MID (0.4-0.7): {mid:3d} ({round(mid*100/len(valid))}%)")
print(f"    LOW (0.1-0.4): {low:3d} ({round(low*100/len(valid))}%)")
print(f"    ZERO (<=0.1):  {zero:3d} ({round(zero*100/len(valid))}%)")

# Score spread (differentiation)
spreads = [r["score_spread"] for r in valid if r["score_spread"] > 0]
if spreads:
    print(f"\n  Score spread (max - min per query):")
    print(f"    Mean:   {round(statistics.mean(spreads), 4)}")
    print(f"    Median: {round(sorted(spreads)[len(spreads)//2], 4)}")
    print(f"    Min:    {round(min(spreads), 4)}")
    print(f"    Max:    {round(max(spreads), 4)}")
    uniform = sum(1 for s in spreads if s < 0.05)
    print(f"    Uniform (<0.05 spread): {uniform}/{len(spreads)} queries — these have score compression")

# Content availability
content_avgs = [r["has_content"] for r in valid]
if content_avgs:
    print(f"\n  Content availability (top-5 with >20 char snippets):")
    print(f"    Mean:   {round(statistics.mean(content_avgs), 1)}/5")
    full5 = sum(1 for c in content_avgs if c >= 4)
    empty_c = sum(1 for c in content_avgs if c == 0)
    print(f"    Full (4-5/5):  {full5}/{len(content_avgs)}")
    print(f"    Empty (0/5):   {empty_c}/{len(content_avgs)}")

# Keyword relevance
relevances = [r["relevance"] for r in valid]
if relevances:
    print(f"\n  Keyword relevance (query terms found in top-5 results):")
    print(f"    Mean:   {round(statistics.mean(relevances)*100, 1)}%")
    print(f"    Median: {round(sorted(relevances)[len(relevances)//2]*100, 1)}%")
    print(f"    Min:    {round(min(relevances)*100, 1)}%")
    print(f"    Max:    {round(max(relevances)*100, 1)}%")
    high_rel = sum(1 for r in relevances if r >= 0.6)
    low_rel = sum(1 for r in relevances if r < 0.3)
    print(f"    High (>=60%):  {high_rel}/{len(relevances)}")
    print(f"    Low (<30%):    {low_rel}/{len(relevances)}")

# Domain diversity
domain_counts = [r["domain_count"] for r in valid if r["domain_count"] > 0]
if domain_counts:
    print(f"\n  Domain diversity (unique domains in result set):")
    print(f"    Mean:   {round(statistics.mean(domain_counts), 1)}")
    print(f"    Median: {sorted(domain_counts)[len(domain_counts)//2]}")
    print(f"    Min:    {min(domain_counts)}")
    print(f"    Max:    {max(domain_counts)}")

# Source engine diversity
print(f"\n  Source engine contributions:")
all_sources = {}
for r in results:
    for s in r["sources"]:
        all_sources[s] = all_sources.get(s, 0) + 1
for src, count in sorted(all_sources.items(), key=lambda x: -x[1]):
    bar = "█" * min(count, 60)
    print(f"    {src:20s}: {count:3d} {bar}")

# Confidence distribution
confs = [r["confidence"] for r in valid]
if confs:
    print(f"\n  Intent confidence distribution:")
    print(f"    Mean:   {round(statistics.mean(confs), 4)}")
    print(f"    Median: {round(sorted(confs)[len(confs)//2], 4)}")
    print(f"    Min:    {round(min(confs), 4)}")
    print(f"    Max:    {round(max(confs), 4)}")
    low_conf = sum(1 for c in confs if c < 0.1)
    high_conf = sum(1 for c in confs if c > 0.5)
    print(f"    Low (<0.1):     {low_conf}/{len(confs)}")
    print(f"    High (>0.5):    {high_conf}/{len(confs)}")
    # Uniformity check
    unique_confs = len(set(round(c, 4) for c in confs))
    print(f"    Unique values:  {unique_confs}/{len(confs)} — {'UNIFORM (suspicious)' if unique_confs < len(confs)//3 else 'GOOD distribution'}")

# Distribution field
dists = [r for r in valid if r.get("distribution")]
if dists:
    print(f"\n  Intent distribution field:")
    print(f"    Present: {len(dists)}/{len(valid)} queries")
    # Show a few examples
    for ex in dists[:3]:
        dist = ex["distribution"]
        sorted_dist = sorted(dist.items(), key=lambda x: -x[1])
        print(f"    '{ex['query'][:45]}' → top3: ", end="")
        print(", ".join(f"{k}={v:.3f}" for k,v in sorted_dist[:3]))
else:
    print(f"\n  Intent distribution: NOT PRESENT (API returns single label only)")

# ═══════════════════════════════════════════════════════════════
# DEEP QUALITY INSPECTION (spot-check top results)
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  DEEP QUALITY INSPECTION — Top-5 Results Per Query (15 spot-checks)")
print(f"{'=' * 120}")

# Pick 15 diverse queries for deep inspection
spot_check_queries = [
    "rust async tokio runtime",
    "clickhouse vs postgres for analytics",
    "how to implement circuit breaker in microservices",
    "what is quantum entanglement",
    "buy raspberry pi 5",
    "nginx reverse proxy websocket config",
    "how to build a rust web server with actix and deploy on kubernetes with helm",
    "prometheus histogram quantile p99",
    "causes of the 2008 financial crisis",
    "why does my docker container keep restarting with exit code 137",
    "io_uring vs epoll for high-frequency trading systems",
    "kubernetes pod stuck in pending state with insufficient cpu",
    "postgresql deadlock detected while trying to get lock",
    "what database should i use for a real time collaborative editing app like google docs",
    "rayon work stealing thread pool internals and NUMA awareness",
]

for sq in spot_check_queries:
    match = [r for r in results if r["query"] == sq]
    if not match:
        continue
    r = match[0]
    print(f"\n  QUERY: '{r['query']}'")
    print(f"  Intent: {r['actual']} ({r['confidence']:.3f}) | {r['results']} results | {r['latency_ms']}ms | relevance: {r['relevance']:.0%}")
    if r["top5_details"]:
        print(f"  {'#':>3} {'Score':>6} {'Sources':>12}  {'Title':<65} {'URL'}")
        print(f"  {'─'*3} {'─'*6} {'─'*12}  {'─'*65} {'─'*40}")
        for d in r["top5_details"]:
            src_str = ",".join(d["sources"])[:12] if d["sources"] else "none"
            print(f"  {d['rank']:3d} {d['score']:6.3f} {src_str:>12}  {d['title']:<65} {d['url'][:50]}")
    else:
        print(f"  (no results)")

# ═══════════════════════════════════════════════════════════════
# FAILURES DETAIL
# ═══════════════════════════════════════════════════════════════

failures = [r for r in valid if not r["correct"]]
print(f"\n{'=' * 120}")
print(f"  INTENT FAILURES ({len(failures)})")
print(f"{'=' * 120}")
for r in failures:
    q = r["query"] if r["query"] else "(empty)"
    print(f"  {q:55s} → got '{r['actual']}' (expected '{r['expected']}')  conf={r['confidence']:.3f}")

# ═══════════════════════════════════════════════════════════════
# LOW RELEVANCE QUERIES (quality problems)
# ═══════════════════════════════════════════════════════════════

low_rel_queries = [r for r in valid if r["relevance"] < 0.3 and r["results"] > 0]
if low_rel_queries:
    print(f"\n{'=' * 120}")
    print(f"  LOW RELEVANCE QUERIES (<30% keyword match in top-5) — {len(low_rel_queries)} queries")
    print(f"{'=' * 120}")
    for r in sorted(low_rel_queries, key=lambda x: x["relevance"]):
        q = r["query"] if r["query"] else "(empty)"
        print(f"  [{r['relevance']:.0%}] {q:55s} → {r['top_title'][:60]}")

# ═══════════════════════════════════════════════════════════════
# THIN RESULTS
# ═══════════════════════════════════════════════════════════════

thin = [r for r in valid if r["results"] < 3]
if thin:
    print(f"\n{'=' * 120}")
    print(f"  THIN RESULTS (< 3 results) — {len(thin)} queries")
    print(f"{'=' * 120}")
    for r in thin:
        q = r["query"] if r["query"] else "(empty)"
        print(f"  [{r['results']:2d}] {q:55s} → {r['top_title'][:60]}")

# ═══════════════════════════════════════════════════════════════
# SCORE COMPRESSION CHECK
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  SCORE COMPRESSION CHECK")
print(f"{'=' * 120}")
# Check if top-1 scores are all nearly identical
top1_scores = [r["top_score"] for r in valid if r["top_score"] > 0]
if top1_scores:
    unique_top1 = len(set(round(s, 3) for s in top1_scores))
    print(f"  Unique top-1 score values (3dp): {unique_top1}/{len(top1_scores)}")
    if unique_top1 < len(top1_scores) // 5:
        print(f"  *** SCORE COMPRESSION DETECTED — {unique_top1} unique scores across {len(top1_scores)} queries")
        print(f"      Top-1 scores cluster at: {round(statistics.mean(top1_scores), 3)} ± {round(statistics.stdev(top1_scores), 4)}")
    else:
        print(f"  GOOD — scores are well-differentiated")
        print(f"  Top-1 stats: mean={round(statistics.mean(top1_scores),3)} std={round(statistics.stdev(top1_scores),4)} min={round(min(top1_scores),3)} max={round(max(top1_scores),3)}")

# ═══════════════════════════════════════════════════════════════
# CONCURRENT STRESS TEST
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  CONCURRENT STRESS TEST (15 unique queries in parallel)")
print(f"{'=' * 120}")

concurrent_queries = [
    "distributed consensus raft vs paxos",
    "how to configure nginx load balancer upstream",
    "what is event driven architecture",
    "rust vs go for systems programming",
    "kubernetes horizontal pod autoscaler custom metrics",
    "how to set up postgres streaming replication",
    "elasticsearch vs meilisearch performance benchmark",
    "oauth2 authorization code flow with pkce",
    "terraform import existing resources",
    "prometheus alertmanager slack integration",
    "redis sentinel vs cluster mode",
    "how to debug segfault in c++ core dump",
    "webassembly threads and shared memory",
    "nginx proxy pass websockets",
    "github actions self-hosted runner docker",
]

def run_concurrent(query):
    r = search(query, timeout=30)
    if r["ok"]:
        return {"query": query, "ok": True, "time": r["time"],
                "results": len(r["data"].get("results", [])),
                "intent": r["data"].get("intent", "?")}
    return {"query": query, "ok": False, "time": r["time"], "error": r.get("error", "")}

with concurrent.futures.ThreadPoolExecutor(max_workers=15) as executor:
    start_c = time.time()
    futures = {executor.submit(run_concurrent, q): q for q in concurrent_queries}
    concurrent_results = []
    for f in concurrent.futures.as_completed(futures):
        concurrent_results.append(f.result())
    wall = round((time.time() - start_c) * 1000)

ok_count = sum(1 for r in concurrent_results if r["ok"])
fail_count = len(concurrent_results) - ok_count
ok_times = [r["time"] for r in concurrent_results if r["ok"]]

print(f"  Wall time:    {wall}ms")
print(f"  Succeeded:    {ok_count}/{len(concurrent_queries)}")
print(f"  Failed:       {fail_count}")
print(f"  Throughput:   {round(ok_count / (wall/1000), 2)} req/s")

if ok_times:
    cs = sorted(ok_times)
    print(f"  p50:          {cs[len(cs)//2]}ms")
    print(f"  p90:          {cs[int(len(cs)*0.9)]}ms")
    print(f"  p95:          {cs[min(int(len(cs)*0.95), len(cs)-1)]}ms")
    print(f"  Max:          {max(cs)}ms")
    under3s = sum(1 for t in ok_times if t < 3000)
    print(f"  Under 3s:     {under3s}/{len(ok_times)}")

    print(f"\n  Per-query concurrent results:")
    for r in sorted(concurrent_results, key=lambda x: -x["time"]):
        status = "OK" if r["ok"] else "FAIL"
        res = r.get("results", 0)
        err = r.get("error", "")[:50]
        print(f"    {status} {r['time']:5d}ms [{res:2d} res] {r['query'][:55]} {err}")

# ═══════════════════════════════════════════════════════════════
# CACHE PERFORMANCE
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  CACHE PERFORMANCE (same query 5x)")
print(f"{'=' * 120}")

cache_query = "rust programming language"
cache_times = []
for i in range(5):
    r = search(cache_query, timeout=15)
    cache_times.append(r["time"])
    time.sleep(0.3)

print(f"  Query: '{cache_query}'")
for i, t in enumerate(cache_times):
    label = "cold" if i == 0 else "cached"
    print(f"    Hit {i+1} ({label}): {t}ms")

if len(cache_times) > 1:
    cached_avg = round(statistics.mean(cache_times[1:]))
    print(f"  Cache speedup: {round(cache_times[0] / cached_avg)}x" if cached_avg > 0 else "")

# ═══════════════════════════════════════════════════════════════
# SCORECARD
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  SCORECARD")
print(f"{'=' * 120}")

intent_acc = round(total_pass * 100 / len(valid), 1) if valid else 0
avg_latency = round(statistics.mean(success_lats)) if success_lats else 0
avg_results = round(statistics.mean(result_counts), 1) if result_counts else 0
avg_relevance = round(statistics.mean(relevances) * 100, 1) if relevances else 0
avg_content = round(statistics.mean(content_avgs), 1) if content_avgs else 0
p50_val = sorted(success_lats)[len(success_lats)//2] if success_lats else 0

print(f"  Intent accuracy:         {total_pass}/{len(valid)} ({intent_acc}%)")
print(f"  Latency p50:             {p50_val}ms")
print(f"  Latency p95:             {sorted(success_lats)[int(len(success_lats)*0.95)]}ms" if len(success_lats) > 1 else "")
print(f"  Mean latency:            {avg_latency}ms")
print(f"  Avg results per query:   {avg_results}")
print(f"  Avg keyword relevance:   {avg_relevance}%")
print(f"  Avg content available:   {avg_content}/5")
print(f"  Empty results:           {empty}/{len(result_counts)}")
print(f"  Concurrent throughput:   {round(ok_count / (wall/1000), 2)} req/s")
print(f"  Cache cached avg:        {cached_avg}ms" if len(cache_times) > 1 else "")

# Top-level quality grade
if intent_acc >= 85 and avg_relevance >= 50 and avg_results >= 10 and p50_val < 3000:
    grade = "A — Production ready"
elif intent_acc >= 75 and avg_relevance >= 40 and avg_results >= 5:
    grade = "B — Good, some tuning needed"
elif intent_acc >= 60 and avg_results >= 3:
    grade = "C — Functional, significant gaps"
else:
    grade = "D — Needs work"

print(f"\n  OVERALL GRADE: {grade}")

print(f"\n{'=' * 120}")
print(f"  STRESS TEST COMPLETE — {len(ALL_QUERIES)} queries in {total_wall}s")
print(f"{'=' * 120}")
