#!/usr/bin/env python3
"""
IntentForge Stress Test — Latency + Result Quality
Measures: per-query latency, result count, title relevance, content quality,
          engine diversity, semantic score distribution.
No intent checks — focused on SEARCH RESULT quality.
"""
import json, time, statistics, urllib.request, urllib.parse, sys
from collections import defaultdict

GATEWAY = "http://127.0.0.1:4000/search"

# ─── 50 unique queries: complex multi-concept + general + edge cases ───
QUERIES = [
    # --- COMPLEX MULTI-CONCEPT (15) ---
    ("complex", "how to migrate a postgres database to cockroachdb without downtime"),
    ("complex", "best open source vector database for production RAG pipeline"),
    ("complex", "deploy next.js app to cloudflare workers with d1 database"),
    ("complex", "compare kubernetes autoscaling strategies HPA vs KEDA vs Knative"),
    ("complex", "set up wireguard mesh network across 3 data centers with failover"),
    ("complex", "implement zero trust authentication with passkeys and FIDO2 in rust"),
    ("complex", "how to benchmark GPU inference throughput for quantized LLM on consumer hardware"),
    ("complex", "build a real-time collaborative editor with CRDT and websockets"),
    ("complex", "self-hosted alternative to vercel analytics with clickhouse backend"),
    ("complex", "configure nixos flake for reproducible dev environment with direnv"),
    ("complex", "how to reverse engineer an android app's native .so library with ghidra"),
    ("complex", "implement distributed tracing across golang microservices with openTelemetry"),
    ("complex", "set up automated canary deployments with istio and flagger on EKS"),
    ("complex", "how to optimize postgres query performance for JSONB columns with GIN indexes"),
    ("complex", "build a serverless image processing pipeline with AWS Lambda and Sharp"),

    # --- GENERAL INFORMATIONAL (15) ---
    ("informational", "what causes northern lights and where best to see them"),
    ("informational", "how does end-to-end encryption work in messaging apps"),
    ("informational", "explain the difference between TCP and UDP with real world examples"),
    ("informational", "what is the significance of Riemann hypothesis in mathematics"),
    ("informational", "how do electric vehicle batteries work and what affects their lifespan"),
    ("informational", "what happened to the Aral Sea and environmental impact"),
    ("informational", "how does the human immune system respond to viral infections"),
    ("informational", "what are the main differences between REST and GraphQL APIs"),
    ("informational", "explain how DNS resolution works from browser to authoritative server"),
    ("informational", "what is the current state of nuclear fusion energy research 2026"),
    ("informational", "how do noise cancelling headphones work technically"),
    ("informational", "what are the health benefits and risks of intermittent fasting"),
    ("informational", "how does the James Webb Space Telescope capture infrared images"),
    ("informational", "what is the difference between machine learning and deep learning"),
    ("informational", "how does blockchain consensus mechanism proof of stake work"),

    # --- NAVIGATIONAL (5) ---
    ("navigational", "github copilot official pricing page"),
    ("navigational", "docker desktop download for windows"),
    ("navigational", "openai API documentation chat completions"),
    ("navigational", "tailscale admin console login"),
    ("navigational", "rust standard library documentation"),

    # --- AMBIGUOUS / EDGE CASE (8) ---
    ("ambiguous", "bass"),
    ("ambiguous", "python match"),
    ("ambiguous", "apple m4 chip review"),
    ("ambiguous", "swift concurrency tutorial"),
    ("ambiguous", "go channels vs mutex performance"),
    ("ambiguous", "rust borrow checker explained simply"),
    ("ambiguous", "c memory management best practices 2026"),
    ("ambiguous", "jaguar electric car specifications"),

    # --- TRANSACTIONAL / FRESH (7) ---
    ("transactional", "buy refurbished thinkpad x1 carbon gen 11"),
    ("transactional", "cheapest way to ship large packages internationally"),
    ("transactional", "best VPN service for streaming in 2026"),
    ("transactional", "order custom mechanical keyboard parts online"),
    ("news", "latest AI regulation news europe 2026"),
    ("news", "spacex starship next launch date"),
    ("news", "bitcoin price prediction june 2026"),
]

def fetch_query(query):
    """Hit gateway, return (latency_ms, response_dict, error)"""
    url = f"{GATEWAY}?q={urllib.parse.quote(query)}"
    start = time.perf_counter()
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
        elapsed = (time.perf_counter() - start) * 1000
        return elapsed, data, None
    except Exception as e:
        elapsed = (time.perf_counter() - start) * 1000
        return elapsed, None, str(e)

def score_title_relevance(query, results):
    """How many of top-10 results have query terms in their title."""
    q_terms = set(query.lower().split())
    # remove stop words
    stops = {"how","to","the","a","an","is","are","what","why","when","where",
             "does","do","and","or","of","in","for","with","on","from","it",
             "that","this","can","i","best","most","between","vs","difference",
             "explain","set","up"}
    q_terms -= stops
    if not q_terms:
        return 1.0
    top = results[:10]
    if not top:
        return 0.0
    hits = 0
    for r in top:
        title = (r.get("title","") or "").lower()
        if any(t in title for t in q_terms):
            hits += 1
    return hits / len(top)

def score_content_quality(query, results):
    """How many of top-10 results have query terms in description/content."""
    q_terms = set(query.lower().split())
    stops = {"how","to","the","a","an","is","are","what","why","when","where",
             "does","do","and","or","of","in","for","with","on","from","it",
             "that","this","can","i","best","most","between","vs","difference",
             "explain","set","up"}
    q_terms -= stops
    if not q_terms:
        return 1.0
    top = results[:10]
    if not top:
        return 0.0
    hits = 0
    for r in top:
        desc = (r.get("description","") or r.get("content","") or "").lower()
        if any(t in desc for t in q_terms):
            hits += 1
    return hits / len(top)

def engine_diversity(results):
    """Count unique engines in top-20 results."""
    engines = set()
    for r in results[:20]:
        e = r.get("engine","")
        if e:
            engines.add(e)
    return len(engines)

def domain_diversity(results):
    """Count unique domains in top-20 results."""
    domains = set()
    for r in results[:20]:
        url = r.get("url","")
        if url:
            try:
                from urllib.parse import urlparse
                domains.add(urlparse(url).netloc)
            except:
                pass
    return len(domains)

def score_distribution(results):
    """Compute semantic score distribution from results."""
    scores = []
    for r in results[:20]:
        s = r.get("semantic_score", r.get("score", 0))
        if s and s > 0:
            scores.append(float(s))
    if not scores:
        return {"best": 0, "mean": 0, "confidence": 0, "garbage": True}
    best = max(scores)
    mean = sum(scores)/len(scores)
    return {
        "best": round(best, 3),
        "mean": round(mean, 3),
        "confidence": round(best - mean, 3),
        "garbage": best < 0.15 and mean < 0.10,
        "count": len(scores),
    }

# ─── RUN TEST ───
print(f"{'='*80}")
print(f"  INTENTFORGE STRESS TEST — LATENCY + RESULT QUALITY")
print(f"  {len(QUERIES)} queries | Gateway: {GATEWAY}")
print(f"  {time.strftime('%Y-%m-%d %H:%M:%S')}")
print(f"{'='*80}\n")

results_all = []
latencies = []
errors = 0

for i, (category, query) in enumerate(QUERIES):
    sys.stdout.write(f"\r  [{i+1}/{len(QUERIES)}] {query[:60]:<60}")
    sys.stdout.flush()

    latency, data, err = fetch_query(query)
    latencies.append(latency)

    if err:
        errors += 1
        results_all.append({
            "category": category, "query": query, "latency_ms": round(latency),
            "error": err, "result_count": 0
        })
        continue

    search_results = data.get("results", [])
    result_count = len(search_results)

    title_rel = score_title_relevance(query, search_results)
    content_q = score_content_quality(query, search_results)
    eng_div = engine_diversity(search_results)
    dom_div = domain_diversity(search_results)
    dist = score_distribution(search_results)

    results_all.append({
        "category": category,
        "query": query,
        "latency_ms": round(latency),
        "result_count": result_count,
        "title_relevance": round(title_rel, 3),
        "content_quality": round(content_q, 3),
        "engine_diversity": eng_div,
        "domain_diversity": dom_div,
        "score_best": dist["best"],
        "score_mean": dist["mean"],
        "confidence": dist["confidence"],
        "garbage_cluster": dist["garbage"],
        "top3": [r.get("title","?")[:70] for r in search_results[:3]],
    })

print(f"\r{'':80}")
print()

# ─── LATENCY REPORT ───
latencies_sorted = sorted(latencies)
p50 = latencies_sorted[len(latencies_sorted)//2]
p90 = latencies_sorted[int(len(latencies_sorted)*0.9)]
p99 = latencies_sorted[int(len(latencies_sorted)*0.99)]
mean_lat = statistics.mean(latencies)
min_lat = min(latencies)
max_lat = max(latencies)

print(f"{'='*80}")
print(f"  LATENCY RESULTS")
print(f"{'='*80}")
print(f"  Mean:  {mean_lat:>8.0f} ms")
print(f"  P50:   {p50:>8.0f} ms")
print(f"  P90:   {p90:>8.0f} ms")
print(f"  P99:   {p99:>8.0f} ms")
print(f"  Min:   {min_lat:>8.0f} ms")
print(f"  Max:   {max_lat:>8.0f} ms")
print()

# Latency by category
cat_latencies = defaultdict(list)
for r in results_all:
    cat_latencies[r["category"]].append(r["latency_ms"])

print(f"  {'Category':<18} {'Count':>5} {'Mean':>8} {'P50':>8} {'P90':>8} {'Max':>8}")
print(f"  {'-'*18} {'-'*5} {'-'*8} {'-'*8} {'-'*8} {'-'*8}")
for cat in sorted(cat_latencies.keys()):
    cl = sorted(cat_latencies[cat])
    cm = statistics.mean(cl)
    c50 = cl[len(cl)//2]
    c90 = cl[int(len(cl)*0.9)]
    cmax = max(cl)
    print(f"  {cat:<18} {len(cl):>5} {cm:>7.0f}ms {c50:>7.0f}ms {c90:>7.0f}ms {cmax:>7.0f}ms")
print()

# ─── QUALITY REPORT ───
print(f"{'='*80}")
print(f"  RESULT QUALITY")
print(f"{'='*80}")
valid = [r for r in results_all if "error" not in r]
if valid:
    avg_count = statistics.mean([r["result_count"] for r in valid])
    avg_title = statistics.mean([r["title_relevance"] for r in valid])
    avg_content = statistics.mean([r["content_quality"] for r in valid])
    avg_eng_div = statistics.mean([r["engine_diversity"] for r in valid])
    avg_dom_div = statistics.mean([r["domain_diversity"] for r in valid])
    garbage = sum(1 for r in valid if r["garbage_cluster"])
    low_count = sum(1 for r in valid if r["result_count"] < 10)

    print(f"  Avg Result Count:    {avg_count:.1f}")
    print(f"  Low Count (<10):     {low_count}/{len(valid)}")
    print(f"  Title Relevance:     {avg_title:.3f}  (query terms in top-10 titles)")
    print(f"  Content Quality:     {avg_content:.3f}  (query terms in top-10 descriptions)")
    print(f"  Engine Diversity:    {avg_eng_div:.1f}  (unique engines in top-20)")
    print(f"  Domain Diversity:    {avg_dom_div:.1f}  (unique domains in top-20)")
    print(f"  Garbage Clusters:    {garbage}/{len(valid)}")
    print()

    # Quality by category
    cat_quality = defaultdict(list)
    for r in valid:
        cat_quality[r["category"]].append(r)

    print(f"  {'Category':<18} {'#':>3} {'Results':>8} {'TitleRel':>9} {'ContentQ':>9} {'EngDiv':>7} {'DomDiv':>7}")
    print(f"  {'-'*18} {'-'*3} {'-'*8} {'-'*9} {'-'*9} {'-'*7} {'-'*7}")
    for cat in sorted(cat_quality.keys()):
        items = cat_quality[cat]
        print(f"  {cat:<18} {len(items):>3} {statistics.mean([r['result_count'] for r in items]):>7.0f} "
              f"{statistics.mean([r['title_relevance'] for r in items]):>8.3f} "
              f"{statistics.mean([r['content_quality'] for r in items]):>8.3f} "
              f"{statistics.mean([r['engine_diversity'] for r in items]):>6.1f} "
              f"{statistics.mean([r['domain_diversity'] for r in items]):>6.1f}")
    print()

# ─── TOP PERFORMERS ───
print(f"{'='*80}")
print(f"  TOP 10 QUERIES (by title relevance)")
print(f"{'='*80}")
top = sorted(valid, key=lambda r: r["title_relevance"], reverse=True)[:10]
for i, r in enumerate(top):
    print(f"  {i+1}. [{r['category']:<13}] {r['query'][:55]}")
    print(f"     Latency: {r['latency_ms']}ms | Results: {r['result_count']} | TitleRel: {r['title_relevance']} | Content: {r['content_quality']}")
    for j, t in enumerate(r.get("top3", [])):
        print(f"     {j+1}. {t}")
    print()

# ─── WORST PERFORMERS ───
print(f"{'='*80}")
print(f"  BOTTOM 10 QUERIES (by title relevance)")
print(f"{'='*80}")
bottom = sorted(valid, key=lambda r: r["title_relevance"])[:10]
for i, r in enumerate(bottom):
    print(f"  {i+1}. [{r['category']:<13}] {r['query'][:55]}")
    print(f"     Latency: {r['latency_ms']}ms | Results: {r['result_count']} | TitleRel: {r['title_relevance']} | Content: {r['content_quality']}")
    for j, t in enumerate(r.get("top3", [])):
        print(f"     {j+1}. {t}")
    print()

# ─── SLOWEST QUERIES ───
print(f"{'='*80}")
print(f"  TOP 10 SLOWEST QUERIES")
print(f"{'='*80}")
slowest = sorted(valid, key=lambda r: r["latency_ms"], reverse=True)[:10]
for i, r in enumerate(slowest):
    print(f"  {i+1}. {r['latency_ms']:>5}ms | {r['query'][:60]}")
    print(f"     Results: {r['result_count']} | Engines: {r['engine_diversity']} | Domains: {r['domain_diversity']}")
print()

# ─── GARBAGE CLUSTERS ───
if garbage > 0:
    print(f"{'='*80}")
    print(f"  ⚠ GARBAGE CLUSTERS (best<0.15 AND mean<0.10)")
    print(f"{'='*80}")
    for r in valid:
        if r["garbage_cluster"]:
            print(f"  • {r['query'][:60]}")
            print(f"    Results: {r['result_count']} | Best: {r['score_best']} | Mean: {r['score_mean']}")
    print()

# ─── ERRORS ───
if errors:
    print(f"{'='*80}")
    print(f"  ERRORS ({errors})")
    print(f"{'='*80}")
    for r in results_all:
        if "error" in r:
            print(f"  ✗ {r['query'][:50]} → {r['error']}")
    print()

# ─── OVERALL GRADE ───
if valid:
    # Weighted score
    quality_score = (avg_title * 0.3 + avg_content * 0.2 +
                     min(avg_count/30, 1.0) * 0.15 +
                     min(avg_eng_div/5, 1.0) * 0.15 +
                     min(avg_dom_div/10, 1.0) * 0.1 +
                     (1 - garbage/len(valid)) * 0.1)

    if p50 < 1500 and p90 < 2500:
        latency_grade = "Excellent"
    elif p50 < 2000 and p90 < 3500:
        latency_grade = "Acceptable"
    else:
        latency_grade = "Slow"

    if quality_score >= 0.85:
        quality_grade = "A"
    elif quality_score >= 0.70:
        quality_grade = "B"
    elif quality_score >= 0.55:
        quality_grade = "C"
    else:
        quality_grade = "D"

    print(f"{'='*80}")
    print(f"  OVERALL")
    print(f"{'='*80}")
    print(f"  Queries:    {len(QUERIES)} | Errors: {errors}")
    print(f"  Quality:    {quality_score:.3f} ({quality_grade})")
    print(f"  Latency:    {latency_grade} (P50={p50:.0f}ms, P90={p90:.0f}ms)")
    print(f"  Garbage:    {garbage}/{len(valid)} clusters")
    print(f"{'='*80}")
