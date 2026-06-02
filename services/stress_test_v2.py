#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive Stress Test
Tests: unique general queries, complex queries, latency, quality (top 5 results)
Also monitors gateway logs for rotation/retry/degradation signals.
"""

import requests
import time
import json
import sys
from collections import defaultdict

GATEWAY = "http://localhost:4000"

# ─── Unique General Queries (brand new, never tested before) ───────
GENERAL_QUERIES = [
    # Informational
    "quantum computing applications in cryptography",
    "history of the silk road trade route",
    "how does photosynthesis work in plants",
    "causes of the 2008 financial crisis",
    "neural network backpropagation explained",
    "difference between TCP and UDP protocols",
    "how black holes are formed in space",
    "climate change impact on coral reefs",
    "basics of supply and demand economics",
    "how vaccines train the immune system",
    # Technical
    "implementing OAuth2 with PKCE flow",
    "rust async runtime tokio internals",
    "kubernetes pod scheduling algorithms",
    "PostgreSQL query plan optimization",
    "WebAssembly garbage collection proposal",
    # How-to
    "set up a reverse proxy with nginx",
    "configure wireguard VPN on linux",
    "deploy a static site to cloudflare pages",
    "write unit tests in golang table-driven",
    "optimize React re-renders with memo",
    # Comparison
    "react vs svelte performance benchmarks 2025",
    "postgresql vs mongodb for time series data",
    "terraform vs pulumi infrastructure as code",
    "grpc vs REST API design tradeoffs",
    "next.js app router vs pages router",
    # Fresh/news
    "latest AI regulation developments europe",
    "recent CVE vulnerabilities in linux kernel",
    "new features in python 3.13",
    # Navigational
    "github copilot pricing plans",
    "docker hub official images",
    "mdn web docs javascript",
    "stackoverflow developer survey 2025",
    "hugging face model hub",
]

# ─── Complex / Multi-Concept Queries ──────────────────────────────
COMPLEX_QUERIES = [
    # Multi-constraint with exclusions
    "python web framework not django with async support",
    "javascript testing library not jest with snapshot testing",
    "css framework not bootstrap with utility classes",
    # Multi-concept fusion
    "real-time collaborative code editor with CRDT synchronization",
    "distributed tracing for microservices with OpenTelemetry and Jaeger",
    "server-side rendering React with streaming and Suspense boundaries",
    "zero-knowledge proof implementation for identity verification",
    "event sourcing with CQRS pattern in domain-driven design",
    # Ambiguous / edge cases
    "rust",  # single word — should be navigational
    "apple",  # ambiguous: fruit vs company
    "jaguar",  # ambiguous: animal vs car vs OS
    "bass",  # ambiguous: fish vs musical
    # Very specific multi-hop
    "compare memory safety guarantees of rust borrow checker vs zig allocator model",
    "how to migrate a monolithic spring boot application to quarkus with reactive extensions",
    "implementing zero-downtime database schema migrations with gh-ost on MySQL replication",
    "benchmarking gRPC bidirectional streaming vs websocket throughput under high concurrency",
    "setting up multi-cluster service mesh with istio east-west gateway across GKE and EKS",
    # Long-tail
    "best practices for implementing rate limiting in a distributed system with Redis sliding window",
    "how to set up Prometheus alerting rules for Kubernetes pod restart loops and OOM kills",
    "comparing WebGPU compute shaders vs WebGL fragment shaders for particle simulations",
]

ALL_QUERIES = GENERAL_QUERIES + COMPLEX_QUERIES


def test_query(q, idx, total):
    """Test a single query and return detailed metrics."""
    start = time.time()
    try:
        resp = requests.get(f"{GATEWAY}/search", params={"q": q}, timeout=30)
        elapsed = time.time() - start
        data = resp.json()
    except Exception as e:
        elapsed = time.time() - start
        return {
            "query": q,
            "status": "ERROR",
            "error": str(e),
            "latency_s": round(elapsed, 3),
        }

    intent = data.get("intent", "unknown")
    category = data.get("category", "unknown")
    confidence = data.get("confidence", 0.0)
    results = data.get("results", [])
    expanded = data.get("expanded_queries", [])
    constraints = data.get("structured_constraints", {})

    # Analyze top 5 results quality
    top5 = results[:5]
    quality_issues = []
    for i, r in enumerate(top5):
        title = r.get("title", "")
        content = r.get("content", "")
        url = r.get("url", "")
        score = r.get("score", 0.0)
        sources = r.get("sources", [])

        if not title or len(title) < 5:
            quality_issues.append(f"  R{i+1}: EMPTY/TINY title")
        if not content or len(content) < 20:
            quality_issues.append(f"  R{i+1}: EMPTY/TINY content ({len(content)} chars)")
        if not url or not url.startswith("http"):
            quality_issues.append(f"  R{i+1}: BAD url: {url[:60]}")
        if score < 0.1:
            quality_issues.append(f"  R{i+1}: LOW score ({score:.3f})")
        if not sources:
            quality_issues.append(f"  R{i+1}: NO sources")

    # Check if results are relevant to query (basic: at least 1 of top 5 should have query terms in title)
    q_words = set(q.lower().split())
    stop = {"the","a","an","in","on","for","with","from","to","and","or","of","is","are","how","what","not","best","top","vs","new","latest","recent"}
    q_terms = q_words - stop
    title_matches = 0
    for r in top5[:3]:
        title_lower = r.get("title", "").lower()
        if any(t in title_lower for t in q_terms if len(t) > 2):
            title_matches += 1
    relevance = "GOOD" if title_matches >= 1 else "WEAK"

    return {
        "query": q,
        "status": "OK",
        "latency_s": round(elapsed, 3),
        "intent": intent,
        "category": category,
        "confidence": round(confidence, 3),
        "total_results": len(results),
        "top5_scores": [round(r.get("score", 0), 3) for r in top5],
        "top5_sources": [r.get("sources", []) for r in top5],
        "top5_titles": [r.get("title", "")[:80] for r in top5],
        "expanded_queries": expanded,
        "constraints": constraints,
        "relevance": relevance,
        "quality_issues": quality_issues,
    }


def check_gateway_logs():
    """Check gateway logs for rotation/retry/degradation signals."""
    try:
        resp = subprocess.run(
            ["docker", "logs", "if-dev-gateway", "--tail", "200"],
            capture_output=True, text=True, timeout=10
        )
        logs = resp.stderr  # docker logs go to stderr
        signals = {
            "vpn_rotation": 0,
            "smart_retry": 0,
            "degraded_engines": 0,
            "circuit_open": 0,
            "rate_limit_429": 0,
            "zero_results_retry": 0,
        }
        for line in logs.split('\n'):
            if 'VPN rotation triggered' in line or 'PROACTIVE VPN ROTATION' in line:
                signals["vpn_rotation"] += 1
            if 'SMART RETRY' in line:
                signals["smart_retry"] += 1
            if 'DEGRADED' in line:
                signals["degraded_engines"] += 1
            if 'Circuit OPEN' in line:
                signals["circuit_open"] += 1
            if '429' in line or 'TOO_MANY_REQUESTS' in line:
                signals["rate_limit_429"] += 1
            if 'zero_results_after_retry' in line:
                signals["zero_results_retry"] += 1
        return signals, logs
    except Exception as e:
        return {"error": str(e)}, ""


def main():
    print("=" * 80)
    print("INTENTFORGE v2 — COMPREHENSIVE STRESS TEST")
    print(f"Date: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")
    print(f"Queries: {len(ALL_QUERIES)} ({len(GENERAL_QUERIES)} general + {len(COMPLEX_QUERIES)} complex)")
    print("=" * 80)

    # Pre-flush: clear cache by waiting
    print("\nWarming up (first query clears any stale cache)...")
    try:
        requests.get(f"{GATEWAY}/search", params={"q": "warmup test"}, timeout=15)
    except:
        pass

    results = []
    latencies = []
    intent_dist = defaultdict(int)
    quality_ok = 0
    quality_weak = 0
    errors = 0

    print(f"\nRunning {len(ALL_QUERIES)} queries...\n")

    for i, q in enumerate(ALL_QUERIES):
        sys.stdout.write(f"\r  [{i+1}/{len(ALL_QUERIES)}] {q[:60]:<60}")
        sys.stdout.flush()
        r = test_query(q, i, len(ALL_QUERIES))
        results.append(r)
        if r["status"] == "OK":
            latencies.append(r["latency_s"])
            intent_dist[r["intent"]] += 1
            if r["relevance"] == "GOOD":
                quality_ok += 1
            else:
                quality_weak += 1
        else:
            errors += 1
        # Small delay to avoid cache hits and let rotation/retry logic activate
        time.sleep(0.5)

    print("\n")

    # ─── Summary ───────────────────────────────────────────────────
    print("=" * 80)
    print("RESULTS SUMMARY")
    print("=" * 80)

    # Latency
    if latencies:
        latencies_sorted = sorted(latencies)
        n = len(latencies_sorted)
        avg_lat = sum(latencies_sorted) / n
        p50 = latencies_sorted[n // 2]
        p90 = latencies_sorted[int(n * 0.9)]
        p95 = latencies_sorted[int(n * 0.95)]
        p99 = latencies_sorted[min(int(n * 0.99), n - 1)]
        min_lat = latencies_sorted[0]
        max_lat = latencies_sorted[-1]

        print(f"\nLATENCY ({n} successful queries):")
        print(f"  Min:    {min_lat:.3f}s")
        print(f"  Avg:    {avg_lat:.3f}s")
        print(f"  P50:    {p50:.3f}s")
        print(f"  P90:    {p90:.3f}s")
        print(f"  P95:    {p95:.3f}s")
        print(f"  P99:    {p99:.3f}s")
        print(f"  Max:    {max_lat:.3f}s")

        # Latency buckets
        fast = sum(1 for l in latencies if l < 1.0)
        medium = sum(1 for l in latencies if 1.0 <= l < 3.0)
        slow = sum(1 for l in latencies if 3.0 <= l < 5.0)
        very_slow = sum(1 for l in latencies if l >= 5.0)
        print(f"\n  LATENCY BUCKETS:")
        print(f"    <1s:   {fast:3d} ({fast/n*100:.0f}%)")
        print(f"    1-3s:  {medium:3d} ({medium/n*100:.0f}%)")
        print(f"    3-5s:  {slow:3d} ({slow/n*100:.0f}%)")
        print(f"    >5s:   {very_slow:3d} ({very_slow/n*100:.0f}%)")

    # Intent distribution
    print(f"\nINTENT DISTRIBUTION:")
    for intent, count in sorted(intent_dist.items(), key=lambda x: -x[1]):
        print(f"  {intent:20s}: {count:3d} ({count/len(ALL_QUERIES)*100:.0f}%)")

    # Quality
    total_ok = quality_ok + quality_weak
    print(f"\nRELEVANCE (top-5 title match to query terms):")
    print(f"  GOOD:   {quality_ok}/{total_ok} ({quality_ok/max(total_ok,1)*100:.0f}%)")
    print(f"  WEAK:   {quality_weak}/{total_ok} ({quality_weak/max(total_ok,1)*100:.0f}%)")
    print(f"  Errors: {errors}")

    # Result counts
    result_counts = [r["total_results"] for r in results if r["status"] == "OK"]
    if result_counts:
        print(f"\nRESULT COUNTS:")
        print(f"  Avg:    {sum(result_counts)/len(result_counts):.1f}")
        print(f"  Min:    {min(result_counts)}")
        print(f"  Max:    {max(result_counts)}")
        zero_result = sum(1 for c in result_counts if c == 0)
        print(f"  Zero:   {zero_result}")

    # ─── Detailed: Queries with Issues ────────────────────────────
    print(f"\n{'=' * 80}")
    print("QUERIES WITH QUALITY ISSUES")
    print("=" * 80)
    issue_count = 0
    for r in results:
        if r["status"] != "OK":
            print(f"\n  ERROR: {r['query']}")
            print(f"    {r.get('error', 'unknown')}")
            issue_count += 1
            continue
        issues = r.get("quality_issues", [])
        weak = r.get("relevance") == "WEAK"
        low_results = r.get("total_results", 0) < 3
        slow = r.get("latency_s", 0) > 5.0
        if issues or weak or low_results or slow:
            issue_count += 1
            print(f"\n  Query: {r['query']}")
            print(f"    Intent: {r['intent']} ({r['category']}) | Confidence: {r['confidence']}")
            print(f"    Results: {r['total_results']} | Latency: {r['latency_s']}s | Relevance: {r['relevance']}")
            if issues:
                for iss in issues:
                    print(f"    ⚠ {iss}")
            if weak:
                print(f"    ⚠ WEAK relevance — top 3 titles don't match query terms")
                for i, t in enumerate(r.get("top5_titles", [])[:3]):
                    print(f"      [{i+1}] {t}")
            if low_results:
                print(f"    ⚠ LOW result count: {r['total_results']}")
            if slow:
                print(f"    ⚠ SLOW: {r['latency_s']}s")

    if issue_count == 0:
        print("  (none — all queries passed quality checks)")

    # ─── Top 5 Result Details (sample) ────────────────────────────
    print(f"\n{'=' * 80}")
    print("SAMPLE TOP-5 RESULTS (first 5 queries)")
    print("=" * 80)
    for r in results[:5]:
        if r["status"] != "OK":
            continue
        print(f"\n  Query: {r['query']}")
        print(f"  Intent: {r['intent']} | Confidence: {r['confidence']} | Results: {r['total_results']}")
        print(f"  Expanded: {r.get('expanded_queries', [])}")
        print(f"  Constraints: {r.get('constraints', {})}")
        for i in range(min(5, r["total_results"])):
            score = r["top5_scores"][i] if i < len(r["top5_scores"]) else 0
            title = r["top5_titles"][i] if i < len(r["top5_titles"]) else "?"
            sources = r["top5_sources"][i] if i < len(r["top5_sources"]) else []
            print(f"    [{i+1}] score={score:.3f} | {', '.join(sources):<20} | {title}")

    # ─── Gateway Log Signals ──────────────────────────────────────
    print(f"\n{'=' * 80}")
    print("GATEWAY LOG SIGNALS (rotation/retry/degradation)")
    print("=" * 80)
    import subprocess
    signals, raw_logs = check_gateway_logs()
    if "error" in signals:
        print(f"  Error reading logs: {signals['error']}")
    else:
        for key, val in signals.items():
            status = "✓ ACTIVE" if val > 0 else "— none"
            print(f"  {key:25s}: {val:3d} {status}")
        # Show relevant log lines
        relevant = [l for l in raw_logs.split('\n') if any(kw in l for kw in
            ['DEGRADED', 'SMART RETRY', 'PROACTIVE', 'VPN rotation', 'Circuit OPEN', '429'])]
        if relevant:
            print(f"\n  Relevant log lines ({len(relevant)}):")
            for line in relevant[-15:]:
                # Trim timestamp prefix for readability
                short = line[27:] if len(line) > 27 else line
                print(f"    {short[:120]}")

    # ─── Final Verdict ────────────────────────────────────────────
    print(f"\n{'=' * 80}")
    print("FINAL VERDICT")
    print("=" * 80)
    pass_rate = quality_ok / max(total_ok, 1) * 100
    avg_latency = sum(latencies) / len(latencies) if latencies else 999
    error_rate = errors / len(ALL_QUERIES) * 100
    print(f"  Pass rate:     {pass_rate:.0f}% ({quality_ok}/{total_ok})")
    print(f"  Avg latency:   {avg_latency:.3f}s")
    print(f"  Error rate:    {error_rate:.0f}% ({errors}/{len(ALL_QUERIES)})")
    print(f"  Issue queries: {issue_count}/{len(ALL_QUERIES)}")
    if pass_rate >= 80 and avg_latency < 4.0 and error_rate < 5:
        print(f"\n  ✅ PASS — System performing well")
    elif pass_rate >= 60 and avg_latency < 6.0:
        print(f"\n  ⚠️  MARGINAL — Some issues to address")
    else:
        print(f"\n  ❌ FAIL — Significant degradation detected")


if __name__ == "__main__":
    main()
