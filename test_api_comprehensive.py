#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general queries, complex queries, constraints, multi-constraints,
       negative constraints, stress testing, bottleneck identification.
"""

import requests
import json
import time
import sys
import statistics
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import Optional

BASE_URL = "http://localhost:4000"
SEARCH_URL = f"{BASE_URL}/search"
TIMEOUT = 30  # seconds per request

# ─── Test Categories ──────────────────────────────────────────────────

GENERAL_QUERIES = [
    "python programming",
    "machine learning tutorials",
    "best restaurants near me",
    "weather forecast",
    "how to learn guitar",
]

COMPLEX_QUERIES = [
    "comparing rust vs go for systems programming in 2025",
    "how to implement zero-knowledge proofs in blockchain",
    "best practices for migrating monolith to microservices at scale",
    "neural architecture search for efficient transformer models",
    "setting up kubernetes multi-cluster federation with istio service mesh",
]

CONSTRAINT_QUERIES = [
    # Single positive constraint
    {"q": "python web framework", "expect_intent": True},
    {"q": "javascript tutorial for beginners only", "expect_intent": True},
    {"q": "rust documentation official", "expect_intent": True},
]

MULTI_CONSTRAINT_QUERIES = [
    # Multiple positive constraints
    {"q": "python async web framework with websocket support", "expect_intent": True},
    {"q": "lightweight javascript framework for mobile responsive single page apps", "expect_intent": True},
    {"q": "free open source machine learning library for computer vision", "expect_intent": True},
]

NEGATIVE_CONSTRAINT_QUERIES = [
    # Negative constraints (explicit exclusions)
    {"q": "python web framework not django", "expect_negative": True},
    {"q": "javascript framework except react", "expect_negative": True},
    {"q": "programming language comparison without java", "expect_negative": True},
]

EDGE_CASE_QUERIES = [
    "",  # empty query
    "a",  # single char
    "   ",  # whitespace only
    "the the the the",  # stop words only
    "a]b[c{d}e@#f",  # special chars
    "x" * 500,  # very long query
    "python\x00null\x01bytes",  # control characters
    "query with emoji 🔥🚀💡",  # unicode/emoji
    "C++ vs C#",  # special language names
    "node.js express.js",  # dots in names
]


@dataclass
class TestResult:
    name: str
    query: str
    status_code: int = 0
    response_time_ms: float = 0.0
    intent: str = ""
    confidence: float = 0.0
    num_web_results: int = 0
    num_local_results: int = 0
    constraints_positive: list = field(default_factory=list)
    constraints_negative: list = field(default_factory=list)
    expanded_queries: list = field(default_factory=list)
    error: Optional[str] = None
    raw_response: Optional[dict] = None


def run_query(query: str, name: str = "") -> TestResult:
    """Execute a single search query and capture all metrics."""
    result = TestResult(name=name, query=query[:80])
    try:
        start = time.time()
        resp = requests.get(SEARCH_URL, params={"q": query}, timeout=TIMEOUT)
        elapsed = (time.time() - start) * 1000
        result.response_time_ms = elapsed
        result.status_code = resp.status_code

        if resp.status_code == 200:
            data = resp.json()
            result.raw_response = data
            result.intent = data.get("intent", "unknown")
            result.confidence = data.get("confidence", 0.0)
            result.constraints_positive = data.get("structured_constraints", {}).get("positive", [])
            result.constraints_negative = data.get("structured_constraints", {}).get("negative", [])
            result.expanded_queries = data.get("expanded_queries", [])
            result.num_web_results = len(data.get("web_results", []))
            result.num_local_results = len(data.get("local_results", []))
        else:
            result.error = f"HTTP {resp.status_code}: {resp.text[:200]}"
    except requests.exceptions.Timeout:
        result.error = "TIMEOUT"
        result.response_time_ms = TIMEOUT * 1000
    except requests.exceptions.ConnectionError as e:
        result.error = f"CONNECTION_ERROR: {str(e)[:100]}"
    except Exception as e:
        result.error = f"ERROR: {str(e)[:100]}"

    return result


def print_result(r: TestResult, verbose=False):
    """Print a single test result."""
    status = "✓" if r.status_code == 200 and not r.error else "✗"
    print(f"  {status} [{r.response_time_ms:7.0f}ms] {r.status_code} | "
          f"intent={r.intent:<15s} conf={r.confidence:.2f} | "
          f"web={r.num_web_results:2d} local={r.num_local_results:2d} | "
          f"{r.query}")
    if r.error:
        print(f"    ERROR: {r.error}")
    if r.constraints_negative:
        print(f"    NEGATIVE CONSTRAINTS: {r.constraints_negative}")
    if r.expanded_queries and len(r.expanded_queries) > 1:
        print(f"    EXPANDED QUERIES ({len(r.expanded_queries)}): {r.expanded_queries[:3]}")
    if verbose and r.raw_response:
        # Show top 3 web results with scores
        web = r.raw_response.get("web_results", [])[:3]
        for i, w in enumerate(web):
            print(f"    [{i+1}] score={w.get('score',0):.3f} | {w.get('title','')[:60]}")
            print(f"         {w.get('url','')[:80]}")


def run_category(name, queries, verbose=False):
    """Run a category of tests and return results."""
    print(f"\n{'='*70}")
    print(f"  CATEGORY: {name}")
    print(f"{'='*70}")
    results = []
    for q_data in queries:
        if isinstance(q_data, dict):
            query = q_data["q"]
        else:
            query = q_data
        r = run_query(query, name=name)
        results.append(r)
        print_result(r, verbose=verbose)
    return results


def stress_test(concurrency, num_requests, query="python programming"):
    """Run concurrent requests and measure throughput/latency."""
    print(f"\n{'='*70}")
    print(f"  STRESS TEST: {concurrency} concurrent x {num_requests} total")
    print(f"  Query: '{query}'")
    print(f"{'='*70}")

    latencies = []
    errors = 0
    successes = 0

    start_total = time.time()

    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = []
        for i in range(num_requests):
            futures.append(executor.submit(run_query, query, f"stress-{i}"))

        for future in as_completed(futures):
            r = future.result()
            if r.status_code == 200 and not r.error:
                successes += 1
                latencies.append(r.response_time_ms)
            else:
                errors += 1
                if errors <= 3:  # print first 3 errors
                    print(f"  ERROR: {r.error or f'HTTP {r.status_code}'}")

    total_time = time.time() - start_total

    print(f"\n  Results:")
    print(f"    Total time:   {total_time:.1f}s")
    print(f"    Throughput:   {num_requests / total_time:.1f} req/s")
    print(f"    Successes:    {successes}/{num_requests}")
    print(f"    Errors:       {errors}/{num_requests}")

    if latencies:
        print(f"    Latency (ms):")
        print(f"      Min:    {min(latencies):.0f}")
        print(f"      Max:    {max(latencies):.0f}")
        print(f"      Mean:   {statistics.mean(latencies):.0f}")
        print(f"      Median: {statistics.median(latencies):.0f}")
        print(f"      P95:    {sorted(latencies)[int(len(latencies)*0.95)]:.0f}")
        print(f"      P99:    {sorted(latencies)[int(len(latencies)*0.99)]:.0f}")

    return latencies, errors


def bottleneck_analysis(results_by_category, stress_latencies):
    """Analyze results and identify bottlenecks."""
    print(f"\n{'='*70}")
    print(f"  BOTTLENECK ANALYSIS")
    print(f"{'='*70}")

    all_results = []
    for cat_results in results_by_category.values():
        all_results.extend(cat_results)

    # 1. Latency analysis
    successful = [r for r in all_results if r.status_code == 200 and not r.error]
    failed = [r for r in all_results if r.error]

    if successful:
        times = [r.response_time_ms for r in successful]
        print(f"\n  1. LATENCY PROFILE (n={len(times)} successful queries):")
        print(f"     Min:    {min(times):7.0f}ms")
        print(f"     Max:    {max(times):7.0f}ms")
        print(f"     Mean:   {statistics.mean(times):7.0f}ms")
        print(f"     Median: {statistics.median(times):7.0f}ms")

        slow_queries = [r for r in successful if r.response_time_ms > 5000]
        if slow_queries:
            print(f"\n     ⚠ SLOW QUERIES (>5s):")
            for r in sorted(slow_queries, key=lambda x: -x.response_time_ms):
                print(f"       {r.response_time_ms:7.0f}ms | {r.query}")

    # 2. Failure analysis
    if failed:
        print(f"\n  2. FAILURE ANALYSIS ({len(failed)} failures):")
        for r in failed:
            print(f"     ✗ {r.name}: {r.error} | query='{r.query[:60]}'")

    # 3. Intent classification accuracy
    print(f"\n  3. INTENT CLASSIFICATION:")
    intent_counts = {}
    for r in successful:
        intent_counts[r.intent] = intent_counts.get(r.intent, 0) + 1
    for intent, count in sorted(intent_counts.items(), key=lambda x: -x[1]):
        print(f"     {intent:<20s}: {count:3d} queries")

    # 4. Constraint extraction quality
    with_constraints = [r for r in successful if r.constraints_positive or r.constraints_negative]
    print(f"\n  4. CONSTRAINT EXTRACTION:")
    print(f"     Queries with constraints: {len(with_constraints)}/{len(successful)}")
    negative_queries = [r for r in successful if r.constraints_negative]
    print(f"     Queries with negative constraints: {len(negative_queries)}")
    for r in negative_queries:
        print(f"       '{r.query[:50]}' → negative={r.constraints_negative}")

    # 5. Result availability
    empty_web = [r for r in successful if r.num_web_results == 0]
    print(f"\n  5. RESULT AVAILABILITY:")
    print(f"     Queries with 0 web results: {len(empty_web)}/{len(successful)}")
    if empty_web:
        for r in empty_web[:5]:
            print(f"       '{r.query[:60]}' (intent={r.intent})")

    # 6. Expanded query fan-out
    with_expansion = [r for r in successful if len(r.expanded_queries) > 1]
    print(f"\n  6. QUERY EXPANSION:")
    print(f"     Queries with fan-out: {len(with_expansion)}/{len(successful)}")
    if with_expansion:
        avg_expansions = statistics.mean([len(r.expanded_queries) for r in with_expansion])
        print(f"     Avg expanded queries: {avg_expansions:.1f}")

    # 7. Stress test analysis
    if stress_latencies:
        print(f"\n  7. STRESS TEST BOTTLENECK:")
        if len(stress_latencies) > 1:
            latency_increase = max(stress_latencies) / min(stress_latencies)
            print(f"     Latency spread (max/min): {latency_increase:.1f}x")
            if latency_increase > 5:
                print(f"     ⚠ HIGH LATENCY VARIANCE under load — possible contention")
            p95 = sorted(stress_latencies)[int(len(stress_latencies)*0.95)]
            p50 = statistics.median(stress_latencies)
            if p95 > p50 * 3:
                print(f"     ⚠ P95 ({p95:.0f}ms) >> P50 ({p50:.0f}ms) — tail latency problem")

    # 8. Confidence analysis
    if successful:
        confs = [r.confidence for r in successful]
        low_conf = [r for r in successful if r.confidence < 0.5]
        print(f"\n  8. CONFIDENCE ANALYSIS:")
        print(f"     Mean confidence: {statistics.mean(confs):.3f}")
        print(f"     Low confidence (<0.5): {len(low_conf)}/{len(successful)}")
        if low_conf:
            for r in low_conf[:5]:
                print(f"       conf={r.confidence:.2f} | '{r.query[:50]}' → {r.intent}")


def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("=" * 70)
    print(f"  Target: {SEARCH_URL}")
    print(f"  Timeout: {TIMEOUT}s per request")
    print()

    # Verify connectivity
    try:
        health = requests.get(f"{BASE_URL}/health", timeout=5)
        print(f"  Gateway health: {health.text}")
    except Exception as e:
        print(f"  FATAL: Cannot reach gateway: {e}")
        sys.exit(1)

    results_by_category = {}

    # 1. General queries
    results_by_category["general"] = run_category("GENERAL QUERIES", GENERAL_QUERIES, verbose=True)

    # 2. Complex queries
    results_by_category["complex"] = run_category("COMPLEX QUERIES", COMPLEX_QUERIES, verbose=True)

    # 3. Single constraint queries
    results_by_category["single_constraint"] = run_category("SINGLE CONSTRAINT", CONSTRAINT_QUERIES, verbose=True)

    # 4. Multi-constraint queries
    results_by_category["multi_constraint"] = run_category("MULTI-CONSTRAINT", MULTI_CONSTRAINT_QUERIES, verbose=True)

    # 5. Negative constraint queries
    results_by_category["negative_constraint"] = run_category("NEGATIVE CONSTRAINTS", NEGATIVE_CONSTRAINT_QUERIES, verbose=True)

    # 6. Edge case queries
    results_by_category["edge_cases"] = run_category("EDGE CASES", EDGE_CASE_QUERIES, verbose=True)

    # 7. Stress tests
    stress_latencies_10, errors_10 = stress_test(concurrency=5, num_requests=10)
    stress_latencies_20, errors_20 = stress_test(concurrency=10, num_requests=20)
    stress_latencies_50, errors_50 = stress_test(concurrency=15, num_requests=50)

    # 8. Bottleneck analysis
    bottleneck_analysis(results_by_category, stress_latencies_50)


if __name__ == "__main__":
    main()
