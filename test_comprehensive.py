#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
================================================
Tests the /search endpoint across multiple dimensions:
  1. General queries (simple, everyday searches)
  2. Complex queries (multi-topic, technical, nuanced)
  3. Single-constraint queries (positive constraints)
  4. Multi-constraint queries (multiple positive constraints)
  5. Negative constraint queries ("not X", "without X")
  6. Multi-negative + mixed constraint queries
  7. Edge cases (empty, unicode, very long, special chars)
  8. Stress testing (concurrent load)
  9. VPN rotation signal verification
  10. Bottleneck analysis

No hardcoding — all checks are algorithmic/dynamic.
"""

import asyncio
import aiohttp
import time
import json
import sys
import statistics
from dataclasses import dataclass, field
from typing import Optional
from collections import defaultdict

BASE_URL = "http://localhost:4000"

# ─── Test Query Definitions ───────────────────────────────────────────

GENERAL_QUERIES = [
    "python programming",
    "climate change effects",
    "best restaurants near me",
    "how to learn guitar",
    "machine learning tutorials",
    "world health organization",
    "latest space news",
    "javascript frameworks comparison",
]

COMPLEX_QUERIES = [
    "rust async runtime performance benchmarks 2026",
    "distributed systems consensus algorithm comparison raft vs paxos",
    "neural network architecture search for edge devices",
    "quantum computing error correction surface codes",
    "zero knowledge proof implementations in production systems",
    "how to migrate a monolith to microservices without downtime",
    "comparative analysis of webassembly runtimes for server-side use",
    "privacy preserving machine learning federated learning vs differential privacy",
]

SINGLE_CONSTRAINT_QUERIES = [
    ("python web framework for async", {"has_positive": True}),
    ("javascript library with typescript support", {"has_positive": True}),
    ("database for time series data", {"has_positive": True}),
    ("rust library with async support", {"has_positive": True}),
    ("editor for large files", {"has_positive": True}),
]

MULTI_CONSTRAINT_QUERIES = [
    ("python web framework with async support and type hints", {"min_positive": 2}),
    ("rust library for http with async and tls support", {"min_positive": 2}),
    ("database with replication and acid compliance and json support", {"min_positive": 2}),
    ("javascript framework with server side rendering and typescript and small bundle size", {"min_positive": 2}),
    ("editor with vim keybindings and lsp support and terminal integration", {"min_positive": 2}),
]

NEGATIVE_CONSTRAINT_QUERIES = [
    ("python web framework not django", {"has_negative": True, "negative_contains": "django"}),
    ("javascript framework not react", {"has_negative": True, "negative_contains": "react"}),
    ("database without mysql", {"has_negative": True, "negative_contains": "mysql"}),
    ("linux distro not ubuntu", {"has_negative": True, "negative_contains": "ubuntu"}),
    ("programming language except javascript", {"has_negative": True, "negative_contains": "javascript"}),
]

MULTI_NEGATIVE_CONSTRAINT_QUERIES = [
    ("python framework not django and not flask", {"min_negative": 2}),
    ("javascript runtime not node and not deno", {"min_negative": 2}),
    ("database without mysql and without postgres", {"min_negative": 2}),
]

MIXED_CONSTRAINT_QUERIES = [
    ("python web framework with async support not django", {"has_positive": True, "has_negative": True}),
    ("rust http library with tls not hyper", {"has_positive": True, "has_negative": True}),
    ("javascript framework with ssr not react and not angular", {"has_positive": True, "min_negative": 2}),
]

EDGE_CASE_QUERIES = [
    ("", {"expect_status": [400, 422, 200]}),  # empty query
    ("a", {"expect_status": [200]}),  # single char
    ("the", {"expect_status": [200]}),  # stop word only
    ("🚀🔥💻", {"expect_status": [200]}),  # emoji only
    ("a" * 500, {"expect_status": [200, 400, 414]}),  # very long query
    ("rust <> go & python | java", {"expect_status": [200]}),  # special chars
    ("C++ programming", {"expect_status": [200]}),  # C++ (special chars in name)
    ("node.js vs deno.js", {"expect_status": [200]}),  # dotted names
    ("what is the meaning of life", {"expect_status": [200]}),  # philosophical
    ("SELECT * FROM users; DROP TABLE users;--", {"expect_status": [200]}),  # SQL injection attempt
]


# ─── Data Structures ─────────────────────────────────────────────────

@dataclass
class TestResult:
    query: str
    category: str
    status_code: int
    response_time_ms: float
    intent: str = ""
    confidence: float = 0.0
    num_web_results: int = 0
    num_local_results: int = 0
    positive_constraints: list = field(default_factory=list)
    negative_constraints: list = field(default_factory=list)
    expanded_queries: list = field(default_factory=list)
    error: Optional[str] = None
    top_results: list = field(default_factory=list)


async def run_search(session: aiohttp.ClientSession, query: str, category: str, timeout: int = 30) -> TestResult:
    """Run a single search query and capture all response fields."""
    start = time.monotonic()
    try:
        url = f"{BASE_URL}/search"
        params = {"q": query}
        async with session.get(url, params=params, timeout=aiohttp.ClientTimeout(total=timeout)) as resp:
            elapsed = (time.monotonic() - start) * 1000
            status = resp.status
            if status == 200:
                data = await resp.json()
                intent_data = data.get("intent", {})
                web_results = data.get("web_results", [])
                local_results = data.get("local_results", [])
                constraints = intent_data.get("structured_constraints", {})
                top_results = []
                for r in web_results[:3]:
                    top_results.append({
                        "title": r.get("title", "")[:80],
                        "url": r.get("url", "")[:100],
                        "score": round(r.get("score", 0), 4),
                        "sources": r.get("sources", []),
                    })
                return TestResult(
                    query=query,
                    category=category,
                    status_code=status,
                    response_time_ms=round(elapsed, 1),
                    intent=intent_data.get("intent", "unknown"),
                    confidence=round(intent_data.get("confidence", 0), 3),
                    num_web_results=len(web_results),
                    num_local_results=len(local_results),
                    positive_constraints=constraints.get("positive", []),
                    negative_constraints=constraints.get("negative", []),
                    expanded_queries=intent_data.get("expanded_queries", []),
                    top_results=top_results,
                )
            else:
                elapsed = (time.monotonic() - start) * 1000
                text = await resp.text()
                return TestResult(
                    query=query, category=category, status_code=status,
                    response_time_ms=round(elapsed, 1),
                    error=text[:200],
                )
    except Exception as e:
        elapsed = (time.monotonic() - start) * 1000
        return TestResult(
            query=query, category=category, status_code=0,
            response_time_ms=round(elapsed, 1), error=str(e)[:200],
        )


def validate_result(result: TestResult, expectations: dict) -> list:
    """Validate a test result against expectations. Returns list of failures."""
    failures = []
    if "expect_status" in expectations:
        if result.status_code not in expectations["expect_status"]:
            failures.append(f"Status {result.status_code} not in {expectations['expect_status']}")
    if "has_positive" in expectations and expectations["has_positive"]:
        if not result.positive_constraints:
            failures.append("Expected positive constraints but got none")
    if "has_negative" in expectations and expectations["has_negative"]:
        if not result.negative_constraints:
            failures.append("Expected negative constraints but got none")
    if "min_positive" in expectations:
        if len(result.positive_constraints) < expectations["min_positive"]:
            failures.append(f"Expected >= {expectations['min_positive']} positive constraints, got {len(result.positive_constraints)}")
    if "min_negative" in expectations:
        if len(result.negative_constraints) < expectations["min_negative"]:
            failures.append(f"Expected >= {expectations['min_negative']} negative constraints, got {len(result.negative_constraints)}")
    if "negative_contains" in expectations:
        target = expectations["negative_contains"].lower()
        neg_lower = [n.lower() for n in result.negative_constraints]
        if not any(target in n for n in neg_lower):
            failures.append(f"Expected negative constraint containing '{target}', got {result.negative_constraints}")
    if result.status_code == 200 and result.num_web_results == 0:
        failures.append("Got 0 web results (possible engine failure)")
    return failures


def print_category_header(category: str):
    width = 70
    print(f"\n{'='*width}")
    print(f"  {category}")
    print(f"{'='*width}")


def print_result_summary(result: TestResult, index: int, failures: list):
    status_icon = "PASS" if not failures else "FAIL"
    print(f"\n  [{index+1}] [{status_icon}] {result.query[:60]}")
    print(f"      Status: {result.status_code} | Time: {result.response_time_ms:.0f}ms | Intent: {result.intent} ({result.confidence:.2f})")
    print(f"      Web: {result.num_web_results} | Local: {result.num_local_results}")
    if result.positive_constraints:
        print(f"      Positive: {result.positive_constraints}")
    if result.negative_constraints:
        print(f"      Negative: {result.negative_constraints}")
    if result.expanded_queries and len(result.expanded_queries) > 1:
        print(f"      Expanded: {result.expanded_queries[:4]}")
    if result.top_results:
        for j, tr in enumerate(result.top_results[:2]):
            print(f"      Top{j+1}: [{tr['score']:.3f}] {tr['title'][:50]}")
    if failures:
        for f in failures:
            print(f"      >>> {f}")
    if result.error:
        print(f"      ERROR: {result.error[:100]}")


async def run_category(session, category, queries, expectations_list=None):
    """Run a category of queries and return results."""
    print_category_header(category)
    results = []
    all_failures = []
    for i, q in enumerate(queries):
        if isinstance(q, tuple):
            query, expects = q
        else:
            query = q
            expects = expectations_list[i] if expectations_list else {}
        result = await run_search(session, query, category)
        failures = validate_result(result, expects)
        print_result_summary(result, i, failures)
        results.append(result)
        if failures:
            all_failures.append((query, failures))
        # Small delay to avoid overwhelming
        await asyncio.sleep(0.5)
    return results, all_failures


async def run_stress_test(session, num_concurrent: int, total_requests: int):
    """Run concurrent requests to identify bottlenecks."""
    print_category_header(f"STRESS TEST: {num_concurrent} concurrent x {total_requests} total")
    
    queries = [
        "rust programming", "python tutorial", "machine learning",
        "web development", "cloud computing", "data science",
        "cybersecurity", "blockchain", "devops", "linux kernel",
        "react hooks", "golang concurrency", "swift ios",
        "kubernetes deployment", "docker compose",
    ]
    
    semaphore = asyncio.Semaphore(num_concurrent)
    results = []
    
    async def bounded_search(i):
        async with semaphore:
            q = queries[i % len(queries)]
            return await run_search(session, q, f"stress_c{num_concurrent}")
    
    start = time.monotonic()
    tasks = [bounded_search(i) for i in range(total_requests)]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    total_time = (time.monotonic() - start) * 1000
    
    # Analyze
    valid = [r for r in results if isinstance(r, TestResult)]
    errors = [r for r in results if isinstance(r, Exception)]
    success = [r for r in valid if r.status_code == 200 and r.num_web_results > 0]
    empty = [r for r in valid if r.status_code == 200 and r.num_web_results == 0]
    failed = [r for r in valid if r.status_code != 200]
    
    times = [r.response_time_ms for r in valid]
    
    print(f"\n  Results:")
    print(f"    Total time: {total_time:.0f}ms")
    print(f"    Successful (with results): {len(success)}/{total_requests}")
    print(f"    Empty results: {len(empty)}")
    print(f"    HTTP errors: {len(failed)}")
    print(f"    Exceptions: {len(errors)}")
    if times:
        print(f"    Latency p50: {statistics.median(times):.0f}ms")
        print(f"    Latency p90: {sorted(times)[int(len(times)*0.9)]:.0f}ms")
        print(f"    Latency p99: {sorted(times)[int(len(times)*0.99)]:.0f}ms")
        print(f"    Latency max: {max(times):.0f}ms")
        print(f"    Latency min: {min(times):.0f}ms")
        print(f"    Throughput: {len(valid)/(total_time/1000):.1f} req/s")
    
    # Bottleneck detection
    bottlenecks = []
    if empty:
        bottleneck_ratio = len(empty) / len(valid) if valid else 0
        if bottleneck_ratio > 0.3:
            bottlenecks.append(f"HIGH EMPTY RATE: { bottleneck_ratio*100:.0f}% empty results — possible rate limiting or VPN issue")
    if times and max(times) > 15000:
        bottlenecks.append(f"HIGH LATENCY: max {max(times):.0f}ms — possible engine timeout")
    if times and statistics.median(times) > 5000:
        bottlenecks.append(f"HIGH MEDIAN LATENCY: {statistics.median(times):.0f}ms — engines may be slow")
    if failed:
        status_codes = defaultdict(int)
        for r in failed:
            status_codes[r.status_code] += 1
        bottlenecks.append(f"HTTP ERRORS: {dict(status_codes)}")
    if errors:
        bottlenecks.append(f"EXCEPTIONS: {len(errors)} — possible timeout or connection errors")
    
    if bottlenecks:
        print(f"\n  BOTTLENECKS DETECTED:")
        for b in bottlenecks:
            print(f"    ! {b}")
    else:
        print(f"\n  No bottlenecks detected.")
    
    return valid, bottlenecks


async def test_vpn_rotation_signals():
    """Check if VPN rotation signals are being generated."""
    print_category_header("VPN ROTATION SIGNAL CHECK")
    import subprocess
    try:
        result = subprocess.run(
            ["docker", "exec", "gateway", "cat", "/tmp/vpn-signals/rotate_signal"],
            capture_output=True, text=True, timeout=5,
        )
        if result.returncode == 0 and result.stdout.strip():
            print(f"  VPN signal file exists: '{result.stdout.strip()}'")
            print(f"  This indicates a recent rate-limit or failure event.")
        else:
            print(f"  No VPN rotation signal active (good — no recent rate limits)")
    except Exception as e:
        print(f"  Could not check VPN signal: {e}")


async def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("=" * 70)
    
    # Health check
    async with aiohttp.ClientSession() as session:
        try:
            async with session.get(f"{BASE_URL}/health", timeout=aiohttp.ClientTimeout(total=5)) as resp:
                if resp.status != 200:
                    print(f"FATAL: Gateway health check failed (status {resp.status})")
                    sys.exit(1)
                print(f"Gateway healthy (status {resp.status})")
        except Exception as e:
            print(f"FATAL: Cannot reach gateway at {BASE_URL}: {e}")
            sys.exit(1)
        
        all_results = []
        all_failures = []
        all_bottlenecks = []
        
        # 1. General queries
        results, failures = await run_category(session, "GENERAL QUERIES", GENERAL_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 2. Complex queries
        results, failures = await run_category(session, "COMPLEX QUERIES", COMPLEX_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 3. Single constraint
        results, failures = await run_category(session, "SINGLE CONSTRAINT QUERIES", SINGLE_CONSTRAINT_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 4. Multi constraint
        results, failures = await run_category(session, "MULTI-CONSTRAINT QUERIES", MULTI_CONSTRAINT_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 5. Negative constraint
        results, failures = await run_category(session, "NEGATIVE CONSTRAINT QUERIES", NEGATIVE_CONSTRAINT_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 6. Multi negative
        results, failures = await run_category(session, "MULTI-NEGATIVE CONSTRAINT QUERIES", MULTI_NEGATIVE_CONSTRAINT_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 7. Mixed constraints
        results, failures = await run_category(session, "MIXED CONSTRAINTS (POSITIVE + NEGATIVE)", MIXED_CONSTRAINT_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 8. Edge cases
        results, failures = await run_category(session, "EDGE CASES", EDGE_CASE_QUERIES)
        all_results.extend(results)
        all_failures.extend(failures)
        
        # 9. Stress tests
        for concurrency, total in [(3, 10), (5, 20), (10, 30)]:
            results, bottlenecks = await run_stress_test(session, concurrency, total)
            all_results.extend(results)
            all_bottlenecks.extend(bottlenecks)
        
        # 10. VPN signal check
        await test_vpn_rotation_signals()
        
        # ─── Final Summary ─────────────────────────────────────────
        print(f"\n{'='*70}")
        print(f"  FINAL SUMMARY")
        print(f"{'='*70}")
        
        total = len(all_results)
        successful = sum(1 for r in all_results if r.status_code == 200 and r.num_web_results > 0)
        empty_results = sum(1 for r in all_results if r.status_code == 200 and r.num_web_results == 0)
        http_errors = sum(1 for r in all_results if r.status_code not in (0, 200))
        timeouts = sum(1 for r in all_results if r.status_code == 0)
        
        times = [r.response_time_ms for r in all_results if r.response_time_ms > 0]
        
        print(f"\n  Queries tested: {total}")
        print(f"  Successful (with results): {successful}/{total} ({successful*100//max(total,1)}%)")
        print(f"  Empty results: {empty_results}")
        print(f"  HTTP errors: {http_errors}")
        print(f"  Timeouts: {timeouts}")
        if times:
            print(f"\n  Latency across all tests:")
            print(f"    p50: {statistics.median(times):.0f}ms")
            print(f"    p90: {sorted(times)[int(len(times)*0.9)]:.0f}ms")
            print(f"    p99: {sorted(times)[int(len(times)*0.99)]:.0f}ms")
        
        # Intent distribution
        intents = defaultdict(int)
        for r in all_results:
            if r.intent:
                intents[r.intent] += 1
        if intents:
            print(f"\n  Intent distribution:")
            for intent, count in sorted(intents.items(), key=lambda x: -x[1]):
                print(f"    {intent}: {count}")
        
        # Constraint extraction stats
        with_pos = sum(1 for r in all_results if r.positive_constraints)
        with_neg = sum(1 for r in all_results if r.negative_constraints)
        print(f"\n  Constraint extraction:")
        print(f"    Queries with positive constraints: {with_pos}")
        print(f"    Queries with negative constraints: {with_neg}")
        
        if all_failures:
            print(f"\n  FAILURES ({len(all_failures)}):")
            for query, fails in all_failures:
                print(f"    '{query[:50]}': {', '.join(fails)}")
        else:
            print(f"\n  All tests PASSED!")
        
        if all_bottlenecks:
            print(f"\n  BOTTLENECKS ({len(all_bottlenecks)}):")
            for b in all_bottlenecks:
                print(f"    ! {b}")
        
        # Quality audit: check top results for relevance
        print(f"\n  QUALITY AUDIT (top-5 results per query, checking topical relevance):")
        quality_issues = 0
        quality_checked = 0
        for r in all_results:
            if r.category in ("STRESS",) or not r.top_results:
                continue
            quality_checked += 1
            query_terms = set(r.query.lower().split())
            # Remove stop words
            stop = {"the","a","an","in","on","for","with","using","from","to","and","or","of","is","are","not","without","except","but","that","this","how","what","best","top"}
            query_terms -= stop
            if not query_terms:
                continue
            for tr in r.top_results:
                title_lower = tr["title"].lower()
                url_lower = tr["url"].lower()
                combined = title_lower + " " + url_lower
                matches = sum(1 for t in query_terms if t in combined)
                if matches == 0 and len(query_terms) >= 2:
                    quality_issues += 1
                    if quality_issues <= 5:
                        print(f"    LOW RELEVANCE: '{tr['title'][:50]}' for query '{r.query[:40]}' (0/{len(query_terms)} terms match)")
        
        if quality_checked > 0:
            print(f"\n    Checked {quality_checked} queries, {quality_issues} quality issues found")
        
        print(f"\n{'='*70}")
        print(f"  Test suite complete.")
        print(f"{'='*70}")


if __name__ == "__main__":
    asyncio.run(main())
