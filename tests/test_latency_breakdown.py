#!/usr/bin/env python3
"""Break down per-phase latency: intent, embed, indexer, searxng, merge."""
import time, json, urllib.request, urllib.parse

GATEWAY = "http://localhost:4000"
INTENT = "http://localhost:3005"
INDEXER = "http://localhost:6000"
SEARXNG = "http://localhost:8080"

def timed_request(url, timeout=15):
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode()
        elapsed = (time.time() - start) * 1000
        return elapsed, body
    except Exception as e:
        elapsed = (time.time() - start) * 1000
        return elapsed, str(e)[:200]

queries = [
    "python web framework",
    "best vector database for RAG production",
    "how to center a div css 2026",
    "CVE-2026-1234",
    "机器学习 入门教程",
]

print("=" * 90)
print("LATENCY BREAKDOWN PER PHASE")
print("=" * 90)

for q in queries:
    qe = urllib.parse.quote(q)
    print(f"\n{'─'*90}")
    print(f"Query: {q}")
    print(f"{'─'*90}")

    # Phase 1: Intent analysis
    t_intent, body_intent = timed_request(f"{INTENT}/analyze?q={qe}")
    print(f"  Intent analysis:     {t_intent:7.0f}ms")

    # Phase 2: Embedding
    t_embed, body_embed = timed_request(f"{INTENT}/embed?text={qe}")
    print(f"  Embedding:           {t_embed:7.0f}ms")

    # Phase 3: Indexer search (keyword only)
    t_idx, body_idx = timed_request(f"{INDEXER}/search?q={qe}")
    idx_count = 0
    try:
        idx_count = len(json.loads(body_idx))
    except:
        pass
    print(f"  Indexer search:      {t_idx:7.0f}ms  ({idx_count} results)")

    # Phase 4: SearXNG (single query)
    t_searx, body_searx = timed_request(f"{SEARXNG}/search?q={qe}&format=json&pageno=1")
    searx_count = 0
    try:
        searx_count = len(json.loads(body_searx).get("results", []))
    except:
        pass
    print(f"  SearXNG (1 query):   {t_searx:7.0f}ms  ({searx_count} results)")

    # Phase 5: Full gateway (includes 2x SearXNG fan-out + merge + rerank)
    t_full, body_full = timed_request(GATEWAY + "/search?q=" + qe, timeout=30)
    full_count = 0
    try:
        full_count = len(json.loads(body_full).get("results", []))
    except:
        pass
    print(f"  Full gateway:        {t_full:7.0f}ms  ({full_count} results)")

    # Phase 6: Cached gateway (should be instant)
    t_cached, body_cached = timed_request(GATEWAY + "/search?q=" + qe, timeout=10)
    cached_count = 0
    try:
        cached_count = len(json.loads(body_cached).get("results", []))
    except:
        pass
    print(f"  Cached gateway:      {t_cached:7.0f}ms  ({cached_count} results)")

    # Derived timings
    overhead = t_full - max(t_intent + t_embed, t_searx, t_idx)
    print(f"\n  ── Analysis ──")
    print(f"  Intent+Embed (parallel): {max(t_intent, t_embed):.0f}ms  (bottleneck of the two)")
    print(f"  SearXNG fan-out time:    ~{t_searx:.0f}ms per query × 2 = ~{t_searx*2:.0f}ms")
    print(f"  Merge+rerank overhead:   ~{max(0, overhead):.0f}ms")
    
    if t_full > t_searx * 2 + 500:
        print(f"  ⚠  Gateway ({t_full:.0f}ms) >> 2×SearXNG ({t_searx*2:.0f}ms) — merge/rerank is slow")
    elif t_searx * 2 > t_full * 0.7:
        print(f"  ⚠  SearXNG dominates — {t_searx*2:.0f}ms of {t_full:.0f}ms total ({t_searx*2/t_full*100:.0f}%)")
    else:
        print(f"  ✓  Balanced pipeline")

