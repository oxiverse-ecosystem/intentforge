#!/usr/bin/env python3
"""IntentForge v2 — Unique Query Quality Test
Deep-inspects top 5 results per query: relevance, content, URL validity, source diversity.
All queries are NEW — not reused from previous tests.
"""
import json, time, urllib.request, urllib.parse, sys, statistics, re
from collections import Counter

BASE = "http://localhost:4000/search?q="
TIMEOUT = 25

# ═══════════════════════════════════════════════════════════════
# UNIQUE QUERY BATTERY — 50 brand-new queries
# ═══════════════════════════════════════════════════════════════

GENERAL_QUERIES = [
    ("zustand vs redux toolkit 2026", "comparison"),
    ("how does crun compare to runc container runtime", "comparison"),
    ("tailscale mesh vpn architecture explained", "informational"),
    ("openai whisper v4 transcription accuracy", "technical"),
    ("how to migrate from webpack to vite in large react app", "how-to"),
    ("best linux distro for embedded systems 2026", "comparison"),
    ("what is structured concurrency in modern programming", "informational"),
    ("risc-v vs arm server chips power efficiency", "comparison"),
    ("how to set up traefik v3 with docker compose", "how-to"),
    ("deno 2 vs node.js benchmark performance", "comparison"),
    ("explain the raft consensus algorithm step by step", "informational"),
    ("how to implement rate limiting with redis sliding window", "how-to"),
    ("postgres logical replication vs physical replication", "comparison"),
    ("what are llm hallucinations and how to reduce them", "informational"),
    ("zig programming language vs rust for systems programming", "comparison"),
    ("how to configure wireguard split tunneling on linux", "how-to"),
    ("buy framework laptop 16 amd", "transactional"),
    ("download neovim nightly builds", "transactional"),
    ("latest nvidia cuda toolkit release notes", "fresh"),
    ("new react server components changes 2026", "fresh"),
    ("how to build a ray tracer in rust weekend project", "how-to"),
    ("what is the actor model in distributed systems", "informational"),
    ("cilium kubernetes networking deep dive ebpf", "technical"),
    ("sign up for docker desktop team plan", "transactional"),
    ("best open source vector database for rag applications", "comparison"),
]

COMPLEX_QUERIES = [
    ("how to design a multi-tenant saas architecture with postgres row level security and connection pooling", "how-to"),
    ("compare temporal workflow engine vs apache airflow for orchestrating long running distributed transactions", "comparison"),
    ("what is the most efficient way to implement a distributed lock with redis redlock algorithm in production", "informational"),
    ("how to set up zero trust network architecture using cloudflare access and tailscale for a remote engineering team", "how-to"),
    ("implement a custom derive macro in rust that generates serde serialization with field-level encryption", "how-to"),
    ("best strategy for migrating 50 million rows from mongodb to postgresql with minimal downtime", "how-to"),
    ("how does clickhouse materialized view performance compare to pre-aggregated tables for real time analytics dashboards", "comparison"),
    ("what are the tradeoffs between event sourcing with kafka and traditional crud with postgres for a fintech application", "informational"),
    ("how to implement canary deployments for stateful services with istio service mesh on kubernetes", "how-to"),
    ("design a high frequency trading data pipeline using rust tokio for websocket ingestion and apache arrow for columnar storage", "how-to"),
    ("should i use cockroachdb or citus for horizontal scaling of a postgres based multi-region application", "comparison"),
    ("how to debug intermittent segfaults in a production go service using delve core dump analysis and pprof", "how-to"),
    ("what is the best approach for implementing distributed tracing across python fastapi and golang grpc microservices", "informational"),
    ("how to build an offline first sync engine using crdt for a collaborative document editor with conflict resolution", "how-to"),
    ("compare wasm component model vs native plugins for building a high performance edge computing runtime", "comparison"),
]

ALL_QUERIES = GENERAL_QUERIES + COMPLEX_QUERIES

# ═══════════════════════════════════════════════════════════════
# QUALITY HELPERS
# ═══════════════════════════════════════════════════════════════

def tokenize(text):
    """Extract meaningful tokens (3+ chars) from text."""
    return set(w.lower() for w in re.findall(r'[a-z0-9+#.]+', text.lower()) if len(w) >= 3)

def relevance_score(query, title, content, url):
    """Score 0.0-1.0: how relevant is this result to the query?"""
    q_tokens = tokenize(query)
    if not q_tokens:
        return 0.0
    t_tokens = tokenize(title)
    c_tokens = tokenize(content[:300])
    url_lower = url.lower()
    
    # Title overlap
    title_hits = len(q_tokens & t_tokens)
    title_score = title_hits / len(q_tokens)
    
    # Content overlap
    content_hits = len(q_tokens & c_tokens)
    content_score = min(content_hits / len(q_tokens), 1.0)
    
    # URL contains query terms
    url_hits = sum(1 for t in q_tokens if t in url_lower)
    url_score = min(url_hits / len(q_tokens), 1.0)
    
    # Weighted combination
    return round(title_score * 0.45 + content_score * 0.4 + url_score * 0.15, 3)

def content_quality(text):
    """Quick content quality assessment."""
    if not text or len(text.strip()) < 10:
        return "EMPTY"
    text = text.strip()
    length = len(text)
    # Check for gibberish (low alpha ratio)
    alpha_ratio = sum(1 for c in text if c.isalpha()) / max(length, 1)
    if alpha_ratio < 0.4:
        return "GIBBERISH"
    if length < 30:
        return "THIN"
    if length < 80:
        return "SHORT"
    return "GOOD"

def url_valid(url):
    """Basic URL validity check."""
    if not url or len(url) < 8:
        return False
    return url.startswith("http://") or url.startswith("https://")

# ═══════════════════════════════════════════════════════════════
# RUN TESTS
# ═══════════════════════════════════════════════════════════════

def run_query(query, expected):
    encoded = urllib.parse.quote_plus(query)
    url = BASE + encoded
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "IntentForge-QualityTest/1.0"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            body = resp.read().decode()
        elapsed_ms = round((time.time() - start) * 1000)
        data = json.loads(body)
        intent = data.get("intent", "?")
        confidence = data.get("confidence", 0)
        results = data.get("results", [])
        expanded = data.get("expanded_queries", [])
        constraints = data.get("constraints", [])
        
        # Analyze top 5 results deeply
        top5 = results[:5]
        top5_analysis = []
        for i, r in enumerate(top5):
            title = r.get("title", "")
            rurl = r.get("url", "")
            content = r.get("content", "")
            score = r.get("score", 0)
            sources = r.get("sources", [])
            
            rel = relevance_score(query, title, content, rurl)
            cq = content_quality(content)
            valid = url_valid(rurl)
            
            top5_analysis.append({
                "rank": i + 1,
                "title": title[:80],
                "url": rurl[:100],
                "score": round(score, 3),
                "relevance": rel,
                "content_quality": cq,
                "content_len": len(content.strip()),
                "url_valid": valid,
                "sources": sources,
            })
        
        # Aggregate metrics
        all_sources = Counter()
        for r in results[:10]:
            for s in r.get("sources", []):
                all_sources[s] += 1
        
        avg_relevance = round(statistics.mean([a["relevance"] for a in top5_analysis]), 3) if top5_analysis else 0
        avg_score = round(statistics.mean([a["score"] for a in top5_analysis]), 3) if top5_analysis else 0
        content_good = sum(1 for a in top5_analysis if a["content_quality"] == "GOOD")
        urls_valid = sum(1 for a in top5_analysis if a["url_valid"])
        
    except Exception as e:
        elapsed_ms = round((time.time() - start) * 1000)
        return {
            "query": query, "expected": expected, "actual": "ERROR",
            "latency_ms": elapsed_ms, "error": str(e)[:80],
            "top5": [], "avg_relevance": 0, "avg_score": 0,
            "content_good": 0, "urls_valid": 0, "total_results": 0,
            "confidence": 0, "expanded": [], "constraints": [],
            "top_sources": [],
        }
    
    return {
        "query": query, "expected": expected, "actual": intent,
        "correct": intent == expected, "latency_ms": elapsed_ms,
        "top5": top5_analysis, "avg_relevance": avg_relevance,
        "avg_score": avg_score, "content_good": content_good,
        "urls_valid": urls_valid, "total_results": len(results),
        "confidence": confidence, "expanded": expanded,
        "constraints": constraints,
        "top_sources": all_sources.most_common(5),
    }

# ═══════════════════════════════════════════════════════════════
# EXECUTE
# ═══════════════════════════════════════════════════════════════

print("=" * 120)
print("  INTENTFORGE v2 — UNIQUE QUERY QUALITY TEST")
print(f"  {len(ALL_QUERIES)} unique queries | top-5 result inspection")
print("=" * 120)

results = []
section_labels = [("GENERAL QUERIES", GENERAL_QUERIES), ("COMPLEX QUERIES", COMPLEX_QUERIES)]

for section_name, queries in section_labels:
    print(f"\n{'─' * 120}")
    print(f"  {section_name} ({len(queries)} queries)")
    print(f"{'─' * 120}")
    
    for query, expected in queries:
        r = run_query(query, expected)
        results.append(r)
        
        status = "PASS" if r.get("correct") else "FAIL"
        if r["actual"] == "ERROR":
            status = "ERR "
        
        print(f"\n  {status}  [{r['latency_ms']:4d}ms]  \"{query}\"")
        print(f"       Intent: {r['actual']} (expected {r['expected']})  conf={r['confidence']:.3f}  results={r['total_results']}  avg_relevance={r['avg_relevance']:.3f}  avg_score={r['avg_score']:.3f}")
        
        if r["top_sources"]:
            src_str = ", ".join(f"{s}:{c}" for s, c in r["top_sources"])
            print(f"       Sources: {src_str}")
        
        if r["expanded"]:
            print(f"       Expanded: {r['expanded'][:3]}")
        
        # Show top 5 results
        for a in r["top5"]:
            rel_bar = "█" * int(a["relevance"] * 20)
            cq_tag = a["content_quality"]
            src_tag = ",".join(a["sources"][:3]) if a["sources"] else "?"
            print(f"         #{a['rank']} [{a['score']:.3f}] rel={a['relevance']:.3f} {rel_bar:20s} {cq_tag:10s} [{src_tag}]")
            print(f"              {a['title'][:90]}")
            print(f"              {a['url'][:100]}")
            if a["content_len"] > 0:
                print(f"              content: {a['content_len']} chars")

# ═══════════════════════════════════════════════════════════════
# AGGREGATE ANALYSIS
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  AGGREGATE ANALYSIS")
print(f"{'=' * 120}")

# Intent accuracy
total = len(results)
correct = sum(1 for r in results if r.get("correct"))
errors = sum(1 for r in results if r["actual"] == "ERROR")
print(f"\n  Intent Accuracy: {correct}/{total} ({round(correct*100/total, 1)}%)")
print(f"  Errors: {errors}")

# Latency
lats = [r["latency_ms"] for r in results if r["actual"] != "ERROR"]
if lats:
    s = sorted(lats)
    print(f"\n  Latency:")
    print(f"    Min: {min(lats)}ms | Mean: {round(statistics.mean(lats))}ms | Median: {s[len(s)//2]}ms | Max: {max(lats)}ms")
    print(f"    p90: {s[int(len(s)*0.9)]}ms | p95: {s[int(len(s)*0.95)]}ms")

# Result quality
all_rels = [r["avg_relevance"] for r in results if r["actual"] != "ERROR"]
all_scores = [r["avg_score"] for r in results if r["actual"] != "ERROR"]
all_content = [r["content_good"] for r in results if r["actual"] != "ERROR"]
all_valid_urls = [r["urls_valid"] for r in results if r["actual"] != "ERROR"]
all_counts = [r["total_results"] for r in results if r["actual"] != "ERROR"]

print(f"\n  Result Quality (top-5 averages):")
print(f"    Avg relevance score:    {round(statistics.mean(all_rels), 3)} (0-1 scale, higher=better)")
print(f"    Avg SearXNG score:      {round(statistics.mean(all_scores), 3)}")
print(f"    Avg content available:  {round(statistics.mean(all_content), 1)}/5 results have GOOD content")
print(f"    Avg valid URLs:         {round(statistics.mean(all_valid_urls), 1)}/5 URLs are valid")
print(f"    Avg total results:      {round(statistics.mean(all_counts), 1)}")

# Relevance distribution
high_rel = sum(1 for r in all_rels if r >= 0.5)
mid_rel = sum(1 for r in all_rels if 0.25 <= r < 0.5)
low_rel = sum(1 for r in all_rels if r < 0.25)
print(f"\n  Relevance distribution:")
print(f"    HIGH (>=0.5):  {high_rel}/{len(all_rels)} ({round(high_rel*100/len(all_rels))}%)")
print(f"    MID  (0.25-0.5): {mid_rel}/{len(all_rels)} ({round(mid_rel*100/len(all_rels))}%)")
print(f"    LOW  (<0.25):  {low_rel}/{len(all_rels)} ({round(low_rel*100/len(all_rels))}%)")

# Content quality breakdown
cq_counts = Counter()
for r in results:
    if r["actual"] != "ERROR":
        for a in r["top5"]:
            cq_counts[a["content_quality"]] += 1
print(f"\n  Content quality breakdown (all top-5 results):")
for cq, count in cq_counts.most_common():
    print(f"    {cq:12s}: {count}")

# Source diversity across all queries
all_src = Counter()
for r in results:
    for s, c in r.get("top_sources", []):
        all_src[s] += c
print(f"\n  Source diversity (top-10 result hits across all queries):")
for src, count in all_src.most_common(12):
    bar = "█" * min(count, 60)
    print(f"    {src:20s}: {count:3d} {bar}")

# Per-category breakdown
cats = ["navigational", "informational", "technical", "how-to", "comparison", "transactional", "fresh"]
print(f"\n  Per-category breakdown:")
for cat in cats:
    cat_r = [r for r in results if r["expected"] == cat and r["actual"] != "ERROR"]
    if cat_r:
        acc = sum(1 for r in cat_r if r.get("correct"))
        avg_rel = round(statistics.mean([r["avg_relevance"] for r in cat_r]), 3)
        avg_lat = round(statistics.mean([r["latency_ms"] for r in cat_r]))
        avg_cnt = round(statistics.mean([r["total_results"] for r in cat_r]), 1)
        avg_con = round(statistics.mean([r["content_good"] for r in cat_r]), 1)
        print(f"    {cat:16s}: {acc}/{len(cat_r)} acc  rel={avg_rel}  lat={avg_lat}ms  results={avg_cnt}  content={avg_con}/5")

# ═══════════════════════════════════════════════════════════════
# PROBLEM QUERIES — low relevance or missing content
# ═══════════════════════════════════════════════════════════════

print(f"\n{'=' * 120}")
print("  PROBLEM QUERIES (avg relevance < 0.3 or < 3 results with content)")
print(f"{'=' * 120}")

problems = [r for r in results if r["actual"] != "ERROR" and (r["avg_relevance"] < 0.3 or r["content_good"] < 3)]
if problems:
    for r in problems:
        print(f"  \"{r['query'][:60]}\"  rel={r['avg_relevance']:.3f}  content={r['content_good']}/5  results={r['total_results']}")
        for a in r["top5"]:
            if a["relevance"] < 0.3:
                print(f"    #{a['rank']} rel={a['relevance']:.3f}  {a['title'][:70]}")
else:
    print("  None — all queries have good relevance and content!")

# Intent misclassifications
print(f"\n{'=' * 120}")
print(f"  INTENT MISCLASSIFICATIONS ({sum(1 for r in results if not r.get('correct') and r['actual'] != 'ERROR')})")
print(f"{'=' * 120}")

for r in results:
    if not r.get("correct") and r["actual"] != "ERROR":
        print(f"  \"{r['query'][:60]}\"  got={r['actual']}  expected={r['expected']}  conf={r['confidence']:.3f}")

print(f"\n{'=' * 120}")
print("  TEST COMPLETE")
print(f"{'=' * 120}")
