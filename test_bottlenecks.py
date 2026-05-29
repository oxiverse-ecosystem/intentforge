#!/usr/bin/env python3
"""Bottleneck finder: tests gateway with general + complex queries, measures latency."""
import time, json, sys, urllib.request, urllib.parse

GATEWAY = "http://localhost:4000"

QUERIES = {
    # ── General queries ──
    "general": [
        "python web framework",
        "best restaurants in tokyo",
        "how to learn machine learning",
        "climate change solutions 2026",
        "javascript async await tutorial",
    ],
    # ── Complex / constrained queries ──
    "complex": [
        "python web framework not django not flask",
        "rust async runtime comparison 2026",
        "alternative to jira for small teams",
        "low code platform enterprise free open source",
        "best vector database for RAG production",
    ],
    # ── Edge cases ──
    "edge": [
        "CVE-2026-1234",
        "x",
        "a very long query " * 20,
        "机器学习 入门教程",
        "how to center a div css 2026",
    ],
}

def query_gateway(q, timeout=20):
    """Send query to gateway, return (elapsed_ms, result_count, intent, first_3_titles)."""
    encoded = urllib.parse.quote(q)
    url = f"{GATEWAY}/search?q={encoded}"
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode()
        elapsed = (time.time() - start) * 1000
        data = json.loads(body)
        results = data.get("results", [])
        intent = data.get("intent", "?")
        confidence = data.get("confidence", 0)
        titles = [r.get("title", "")[:60] for r in results[:3]]
        sources = {}
        for r in results:
            for s in r.get("sources", []):
                sources[s] = sources.get(s, 0) + 1
        return {
            "ok": True,
            "ms": round(elapsed),
            "count": len(results),
            "intent": intent,
            "confidence": confidence,
            "titles": titles,
            "sources": sources,
        }
    except Exception as e:
        elapsed = (time.time() - start) * 1000
        return {"ok": False, "ms": round(elapsed), "error": str(e)[:100]}

# Also test individual service latencies
def test_service(name, url, timeout=10):
    start = time.time()
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            resp.read()
        return round((time.time() - start) * 1000)
    except Exception as e:
        return f"FAIL: {str(e)[:60]}"

print("=" * 80)
print("INTENTFORGE BOTTLENECK ANALYSIS")
print("=" * 80)

# 1. Individual service health/latency
print("\n--- SERVICE HEALTH & LATENCY ---")
services = [
    ("Gateway",    f"{GATEWAY}/health"),
    ("Indexer",    "http://localhost:6000/health"),
    ("Indexer /stats", "http://localhost:6000/stats"),
]
for name, url in services:
    latency = test_service(name, url)
    print(f"  {name:20s}: {latency}ms")

# 2. Query tests
all_results = []
for category, queries in QUERIES.items():
    print(f"\n{'='*80}")
    print(f"  CATEGORY: {category.upper()}")
    print(f"{'='*80}")
    for q in queries:
        r = query_gateway(q)
        all_results.append((category, q, r))
        if r["ok"]:
            src_str = ", ".join(f"{k}:{v}" for k,v in sorted(r["sources"].items(), key=lambda x:-x[1]))
            print(f"\n  Query: {q[:70]}")
            print(f"  Time: {r['ms']}ms | Results: {r['count']} | Intent: {r['intent']} ({r['confidence']:.2f})")
            print(f"  Sources: {src_str}")
            for i, t in enumerate(r["titles"]):
                print(f"    [{i+1}] {t}")
        else:
            print(f"\n  Query: {q[:70]}")
            print(f"  FAILED after {r['ms']}ms: {r['error']}")

# 3. Summary / bottleneck analysis
print(f"\n{'='*80}")
print("  BOTTLENECK ANALYSIS")
print(f"{'='*80}")

ok_results = [r for _, _, r in all_results if r["ok"]]
if ok_results:
    latencies = [r["ms"] for r in ok_results]
    latencies.sort()
    avg = sum(latencies) / len(latencies)
    p50 = latencies[len(latencies)//2]
    p90 = latencies[int(len(latencies)*0.9)]
    p99 = latencies[-1]
    print(f"\n  Latency (n={len(ok_results)}):")
    print(f"    Avg: {avg:.0f}ms | P50: {p50}ms | P90: {p90}ms | P99: {p99}ms")
    print(f"    Min: {min(latencies)}ms | Max: {max(latencies)}ms")
    
    # Count by speed bucket
    fast = sum(1 for l in latencies if l < 2000)
    medium = sum(1 for l in latencies if 2000 <= l < 5000)
    slow = sum(1 for l in latencies if l >= 5000)
    print(f"    <2s: {fast} | 2-5s: {medium} | >5s: {slow}")

    # Result count stats
    counts = [r["count"] for r in ok_results]
    print(f"\n  Result counts:")
    print(f"    Avg: {sum(counts)/len(counts):.0f} | Min: {min(counts)} | Max: {max(counts)}")
    
    # Source availability
    all_sources = {}
    for r in ok_results:
        for s, c in r["sources"].items():
            all_sources[s] = all_sources.get(s, 0) + c
    print(f"\n  Source contribution (total results across all queries):")
    for s, c in sorted(all_sources.items(), key=lambda x: -x[1]):
        print(f"    {s:15s}: {c}")

    # Failed queries
    failed = [r for r in all_results if not r[2]["ok"]]
    if failed:
        print(f"\n  FAILED queries ({len(failed)}):")
        for cat, q, r in failed:
            print(f"    [{cat}] {q[:50]} -> {r['error']}")

    # Identify bottleneck: intent analysis vs search
    print(f"\n  Bottleneck indicators:")
    slow_queries = [(cat, q, r) for cat, q, r in all_results if r["ok"] and r["ms"] > 5000]
    if slow_queries:
        print(f"    SLOW QUERIES (>5s): {len(slow_queries)}")
        for cat, q, r in slow_queries:
            print(f"      [{cat}] {q[:50]} -> {r['ms']}ms ({r['count']} results)")
    else:
        print(f"    No queries >5s — pipeline is fast")

