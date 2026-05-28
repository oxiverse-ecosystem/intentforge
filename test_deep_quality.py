#!/usr/bin/env python3
"""
IntentForge v2 — Deep Quality Audit
======================================
Checks at least 5 individual results per query for topical relevance.
Not just aggregate stats — actual result-by-result content analysis.

Also investigates:
- VPN rotation signal (why it triggered)
- p99 latency analysis
- Constraint enforcement (negative constraints actually suppressing results)
"""

import asyncio
import aiohttp
import time
import json
import re
import statistics
from collections import defaultdict

BASE_URL = "http://localhost:4000"

# ─── Queries to deep-audit (diverse categories) ──────────────────────

AUDIT_QUERIES = [
    # General
    ("python programming", ["python", "programming", "code", "developer", "language"]),
    ("machine learning tutorials", ["machine", "learning", "tutorial", "ml", "ai", "guide"]),
    ("climate change effects", ["climate", "change", "effect", "impact", "environment", "global"]),

    # Technical
    ("rust async runtime performance benchmarks 2026", ["rust", "async", "runtime", "performance", "benchmark"]),
    ("quantum computing error correction surface codes", ["quantum", "error", "correction", "surface", "code"]),
    ("zero knowledge proof implementations in production", ["zero", "knowledge", "proof", "zkp", "implementation", "production"]),

    # Comparison
    ("distributed systems consensus algorithm comparison raft vs paxos", ["raft", "paxos", "consensus", "distributed", "algorithm"]),
    ("node.js vs deno.js", ["node", "deno", "javascript", "runtime", "comparison"]),

    # How-to
    ("how to learn guitar", ["guitar", "learn", "play", "beginner", "lesson", "tutorial"]),
    ("how to migrate a monolith to microservices without downtime", ["monolith", "microservice", "migrate", "downtime"]),

    # Constraint: positive
    ("python web framework with async support", ["python", "async", "framework", "web"]),
    ("rust library with async support", ["rust", "async", "library"]),

    # Constraint: negative
    ("python web framework not django", ["python", "framework", "web"]),

    # Constraint: multi-negative
    ("javascript runtime not node and not deno", ["javascript", "runtime"]),

    # Constraint: mixed
    ("rust http library with tls not hyper", ["rust", "http", "tls"]),

    # Complex
    ("privacy preserving machine learning federated learning vs differential privacy",
     ["privacy", "federated", "learning", "differential", "ml"]),
    ("webassembly runtimes for server-side use",
     ["webassembly", "wasm", "runtime", "server"]),
]


async def fetch_results(session, query, timeout=30):
    """Fetch search results for a query."""
    start = time.monotonic()
    try:
        async with session.get(
            f"{BASE_URL}/search",
            params={"q": query},
            timeout=aiohttp.ClientTimeout(total=timeout)
        ) as resp:
            elapsed_ms = (time.monotonic() - start) * 1000
            if resp.status == 200:
                data = await resp.json()
                return data, elapsed_ms
            else:
                return None, elapsed_ms
    except Exception as e:
        elapsed_ms = (time.monotonic() - start) * 1000
        return None, elapsed_ms


def check_result_relevance(result, expected_terms, neg_constraints=None):
    """Check a single result for topical relevance. Returns (score, details)."""
    title = result.get("title", "").lower()
    content = result.get("content", "").lower()
    url = result.get("url", "").lower()
    combined = f"{title} {content} {url}"

    # Check if negative constraint terms appear prominently in title
    neg_violation = False
    if neg_constraints:
        for neg in neg_constraints:
            neg_lower = neg.lower()
            if neg_lower in title:
                neg_violation = True
                break

    # Count how many expected terms are found
    terms_found = []
    terms_missing = []
    for term in expected_terms:
        if term.lower() in combined:
            terms_found.append(term)
        else:
            # Check stemmed versions
            stemmed = term.lower().rstrip("s").rstrip("ed").rstrip("ing")
            if len(stemmed) > 3 and stemmed in combined:
                terms_found.append(f"{term}(stemmed)")
            else:
                terms_missing.append(term)

    relevance = len(terms_found) / max(len(expected_terms), 1)

    details = {
        "terms_found": terms_found,
        "terms_missing": terms_missing,
        "relevance_score": round(relevance, 2),
        "neg_violation": neg_violation,
        "title": title[:80],
        "url": url[:100],
        "has_content": len(content) > 20,
    }
    return details


async def deep_audit_query(session, query, expected_terms, neg_constraints=None):
    """Deep audit a single query: fetch results, check 5+ for relevance."""
    data, elapsed_ms = await fetch_results(session, query)
    if data is None:
        return {
            "query": query,
            "status": "FAILED",
            "elapsed_ms": elapsed_ms,
            "error": "Request failed or timed out",
        }

    intent_val = data.get("intent", "?")
    confidence_val = data.get("confidence", 0)
    web_results = data.get("web_results", [])
    local_results = data.get("local_results", [])
    constraints = data.get("structured_constraints", {})

    # Check top 5 results in detail
    results_to_check = web_results[:5]
    checked = []
    for r in results_to_check:
        detail = check_result_relevance(r, expected_terms, neg_constraints)
        detail["score"] = round(r.get("score", 0), 4)
        detail["sources"] = r.get("sources", [])
        checked.append(detail)

    # Aggregate stats
    avg_relevance = statistics.mean([c["relevance_score"] for c in checked]) if checked else 0
    neg_violations = sum(1 for c in checked if c["neg_violation"])
    all_relevant = avg_relevance >= 0.3
    terms_coverage = set()
    for c in checked:
        terms_coverage.update(c["terms_found"])

    return {
        "query": query,
        "status": "OK" if web_results else "EMPTY",
        "elapsed_ms": round(elapsed_ms, 0),
        "intent": intent_val,
        "confidence": round(confidence_val, 2),
        "num_web": len(web_results),
        "num_local": len(local_results),
        "positive_constraints": constraints.get("positive", []),
        "negative_constraints": constraints.get("negative", []),
        "expanded_queries": data.get("expanded_queries", [])[:3],
        "avg_relevance": round(avg_relevance, 2),
        "terms_coverage": sorted(list(terms_coverage)),
        "neg_violations": neg_violations,
        "top5_results": checked,
    }


async def investigate_vpn_signals():
    """Check gateway logs for VPN rotation triggers."""
    import subprocess
    print("\n" + "=" * 70)
    print("  VPN ROTATION INVESTIGATION")
    print("=" * 70)

    # Check signal file
    result = subprocess.run(
        ["docker", "exec", "gateway", "cat", "/tmp/vpn-signals/rotate_signal"],
        capture_output=True, text=True, timeout=5,
    )
    if result.returncode == 0 and result.stdout.strip():
        signal = result.stdout.strip()
        print(f"  Active signal: '{signal}'")
    else:
        print(f"  No active VPN signal (good)")

    # Check gateway logs for VPN-related messages
    result = subprocess.run(
        ["docker", "logs", "gateway", "--tail", "100"],
        capture_output=True, text=True, timeout=10,
    )
    vpn_lines = [l for l in result.stdout.split("\n") if "vpn" in l.lower() or "429" in l or "rate" in l.lower() or "rotation" in l.lower()]
    if vpn_lines:
        print(f"  VPN-related log entries ({len(vpn_lines)}):")
        for line in vpn_lines[-10:]:
            print(f"    {line[:120]}")
    else:
        print(f"  No VPN-related log entries in last 100 lines")

    # Check if vpn-rotator container is running
    result = subprocess.run(
        ["docker", "ps", "--filter", "name=vpn-rotator", "--format", "{{.Names}} {{.Status}}"],
        capture_output=True, text=True, timeout=5,
    )
    if result.stdout.strip():
        print(f"  VPN rotator container: {result.stdout.strip()}")
    else:
        print(f"  VPN rotator container: NOT RUNNING (may be separate compose)")


async def analyze_latency_profile(results):
    """Analyze latency distribution and identify bottlenecks."""
    print("\n" + "=" * 70)
    print("  LATENCY ANALYSIS")
    print("=" * 70)

    times = [r["elapsed_ms"] for r in results if r.get("elapsed_ms")]
    if not times:
        print("  No latency data available")
        return

    times_sorted = sorted(times)
    n = len(times_sorted)

    print(f"  Queries: {n}")
    print(f"  p50:  {times_sorted[n//2]:.0f}ms")
    print(f"  p75:  {times_sorted[int(n*0.75)]:.0f}ms")
    print(f"  p90:  {times_sorted[int(n*0.9)]:.0f}ms")
    print(f"  p95:  {times_sorted[int(n*0.95)]:.0f}ms")
    print(f"  p99:  {times_sorted[int(n*0.99)]:.0f}ms")
    print(f"  max:  {max(times):.0f}ms")
    print(f"  min:  {min(times):.0f}ms")
    print(f"  mean: {statistics.mean(times):.0f}ms")

    # Identify slow queries
    threshold = times_sorted[int(n * 0.9)]  # p90
    slow = [r for r in results if r.get("elapsed_ms", 0) > threshold]
    if slow:
        print(f"\n  Slow queries (>{threshold:.0f}ms):")
        for r in sorted(slow, key=lambda x: x.get("elapsed_ms", 0), reverse=True)[:5]:
            print(f"    {r['elapsed_ms']:.0f}ms | {r['query'][:50]} | web:{r.get('num_web', 0)}")

    # Correlation: latency vs number of results
    has_both = [(r["elapsed_ms"], r.get("num_web", 0)) for r in results if r.get("elapsed_ms")]
    if has_both:
        empty_results = [t for t, n in has_both if n == 0]
        with_results = [t for t, n in has_both if n > 0]
        if empty_results and with_results:
            print(f"\n  Latency correlation:")
            print(f"    Queries WITH results:  p50={statistics.median(with_results):.0f}ms")
            print(f"    Queries WITHOUT results: p50={statistics.median(empty_results):.0f}ms")
            if statistics.median(empty_results) > statistics.median(with_results) * 1.5:
                print(f"    >>> Empty-result queries are slower — likely hitting timeout/retry path")


async def main():
    print("=" * 70)
    print("  IntentForge v2 — Deep Quality Audit")
    print("  Checking 5+ individual results per query for topical relevance")
    print("=" * 70)

    async with aiohttp.ClientSession() as session:
        # Health check
        try:
            async with session.get(f"{BASE_URL}/health", timeout=aiohttp.ClientTimeout(total=5)) as resp:
                if resp.status != 200:
                    print(f"FATAL: Gateway not healthy")
                    return
        except Exception as e:
            print(f"FATAL: Cannot reach gateway: {e}")
            return

        # Run deep audit on each query
        all_results = []
        total_quality_issues = 0
        queries_with_issues = []

        for query, expected_terms in AUDIT_QUERIES:
            # Determine if this query has negative constraints
            neg_constraints = None
            if "not " in query or "without " in query or "except " in query:
                # Extract negative terms from query for checking
                neg_parts = re.findall(r'(?:not|without|except|excluding)\s+(\w+)', query)
                if neg_parts:
                    neg_constraints = neg_parts

            result = await deep_audit_query(session, query, expected_terms, neg_constraints)
            all_results.append(result)

            # Print detailed results
            status_icon = "PASS" if result["status"] == "OK" else "WARN" if result["status"] == "EMPTY" else "FAIL"
            print(f"\n  [{status_icon}] \"{query}\"")
            print(f"    Intent: {result.get('intent','?')} ({result.get('confidence',0):.2f}) | "
                  f"Web: {result.get('num_web',0)} | Local: {result.get('num_local',0)} | "
                  f"Time: {result.get('elapsed_ms',0):.0f}ms")
            if result.get("positive_constraints"):
                print(f"    +Constraints: {result['positive_constraints']}")
            if result.get("negative_constraints"):
                print(f"    -Constraints: {result['negative_constraints']}")
            if result.get("expanded_queries"):
                print(f"    Expanded: {result['expanded_queries']}")

            issues_in_query = 0
            for i, r in enumerate(result.get("top5_results", [])):
                rel = r["relevance_score"]
                neg_flag = " [NEG VIOLATION]" if r["neg_violation"] else ""
                rel_icon = "+++" if rel >= 0.6 else "++ " if rel >= 0.4 else "+  " if rel >= 0.2 else "   "
                has_src = f"src:{','.join(r['sources'][:3])}" if r.get("sources") else ""

                print(f"    {rel_icon} #{i+1} [{rel:.2f}] [{r['score']:.3f}] {r['title'][:55]}{neg_flag}")
                print(f"         Found: {r['terms_found']}")
                if r["terms_missing"]:
                    print(f"         Missing: {r['terms_missing']}")

                # Quality issue detection
                is_issue = False
                if rel < 0.2 and len(expected_terms) >= 3:
                    is_issue = True
                    print(f"         >>> LOW RELEVANCE (only {rel:.0%} of expected terms)")
                if r["neg_violation"]:
                    is_issue = True
                    print(f"         >>> NEGATIVE CONSTRAINT VIOLATED in title!")
                if is_issue:
                    issues_in_query += 1

            total_quality_issues += issues_in_query
            if issues_in_query > 0:
                queries_with_issues.append((query, issues_in_query))

            # Check if terms coverage is complete
            coverage = result.get("terms_coverage", [])
            if expected_terms:
                coverage_pct = len(coverage) / len(expected_terms) * 100
                if coverage_pct < 40:
                    print(f"    >>> LOW TERM COVERAGE: {coverage_pct:.0f}% ({len(coverage)}/{len(expected_terms)} terms found across top 5)")

            await asyncio.sleep(0.3)

    # ─── Summary ─────────────────────────────────────────────────
    print(f"\n{'='*70}")
    print(f"  DEEP QUALITY AUDIT SUMMARY")
    print(f"{'='*70}")

    successful = sum(1 for r in all_results if r["status"] == "OK")
    empty = sum(1 for r in all_results if r["status"] == "EMPTY")
    failed = sum(1 for r in all_results if r["status"] == "FAILED")

    print(f"\n  Queries audited: {len(all_results)}")
    print(f"  With results:    {successful}")
    print(f"  Empty results:   {empty}")
    print(f"  Failed:          {failed}")
    print(f"  Quality issues:  {total_quality_issues}")
    if queries_with_issues:
        print(f"\n  Queries with quality issues:")
        for q, n in queries_with_issues:
            print(f"    [{n} issues] {q[:55]}")

    # Aggregate relevance
    all_rels = []
    for r in all_results:
        for tr in r.get("top5_results", []):
            all_rels.append(tr["relevance_score"])
    if all_rels:
        print(f"\n  Relevance distribution (across all top-5 results):")
        print(f"    Mean: {statistics.mean(all_rels):.2f}")
        print(f"    Median: {statistics.median(all_rels):.2f}")
        high = sum(1 for r in all_rels if r >= 0.6)
        medium = sum(1 for r in all_rels if 0.3 <= r < 0.6)
        low = sum(1 for r in all_rels if r < 0.3)
        print(f"    High (>=0.6):   {high}/{len(all_rels)} ({high*100//len(all_rels)}%)")
        print(f"    Medium (0.3-0.6): {medium}/{len(all_rels)} ({medium*100//len(all_rels)}%)")
        print(f"    Low (<0.3):     {low}/{len(all_rels)} ({low*100//len(all_rels)}%)")

    # Negative constraint enforcement
    neg_checked = [r for r in all_results if r.get("negative_constraints")]
    if neg_checked:
        violations = sum(r.get("neg_violations", 0) for r in neg_checked)
        total_neg_results = sum(len(r.get("top5_results", [])) for r in neg_checked)
        print(f"\n  Negative constraint enforcement:")
        print(f"    Queries with neg constraints: {len(neg_checked)}")
        print(f"    Top-5 results checked: {total_neg_results}")
        print(f"    Violations (neg term in title): {violations}")
        if violations == 0:
            print(f"    All negative constraints properly enforced!")
        else:
            print(f"    WARNING: {violations} results contain negated terms in title")

    # Latency analysis
    await analyze_latency_profile(all_results)

    # VPN investigation
    await investigate_vpn_signals()

    print(f"\n{'='*70}")
    print(f"  Deep quality audit complete.")
    print(f"{'='*70}")


if __name__ == "__main__":
    asyncio.run(main())
