#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general, complex, single-constraint, multi-constraint, negative constraint,
       edge cases, stress (cached + unique concurrent)
"""

import requests
import json
import time
import sys
import re
import statistics
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import Optional, List

BASE_URL = "http://localhost:4000"
SEARCH_URL = f"{BASE_URL}/search"
TIMEOUT = 30

# ─── Test Data ──────────────────────────────────────────────────────

GENERAL_QUERIES = [
    "python programming",
    "machine learning tutorials",
    "best restaurants near me",
    "weather forecast today",
    "how to learn guitar",
    "world war 2 history",
    "healthy breakfast recipes",
    "electric car comparison",
]

COMPLEX_QUERIES = [
    "rust vs go for systems programming 2025",
    "zero-knowledge proofs in blockchain applications",
    "how to build a distributed message queue from scratch",
    "neural network architecture search for edge devices",
    "comparative analysis of container orchestration platforms",
    "quantum error correction codes surface codes",
    "homomorphic encryption practical implementations",
    "federated learning privacy preserving machine learning",
]

SINGLE_CONSTRAINT_QUERIES = [
    {"q": "python web framework", "expect_positive": ["python", "web", "framework"]},
    {"q": "rust documentation official", "expect_positive": ["rust", "documentation"]},
    {"q": "javascript tutorial beginners", "expect_positive": ["javascript", "tutorial"]},
    {"q": "golang concurrency patterns", "expect_positive": ["golang", "concurrency"]},
    {"q": "kubernetes deployment best practices", "expect_positive": ["kubernetes", "deployment"]},
]

MULTI_CONSTRAINT_QUERIES = [
    {"q": "python async web framework with websocket support", "expect_positive": ["python", "async", "web", "websocket"]},
    {"q": "rust memory safe systems programming no garbage collector", "expect_positive": ["rust", "memory", "systems"]},
    {"q": "lightweight javascript framework for mobile responsive single page app", "expect_positive": ["javascript", "lightweight", "mobile", "responsive"]},
    {"q": "free open source machine learning platform with GPU support and python API", "expect_positive": ["machine learning", "open source", "GPU", "python"]},
    {"q": "database for time series high throughput write heavy workloads", "expect_positive": ["database", "time series", "high throughput"]},
]

NEGATIVE_CONSTRAINT_QUERIES = [
    {"q": "python web framework not django", "neg": ["django"]},
    {"q": "javascript framework except react", "neg": ["react"]},
    {"q": "text editor without vim", "neg": ["vim"]},
    {"q": "linux distro not ubuntu for gaming", "neg": ["ubuntu"]},
    {"q": "database alternative to mongodb", "neg": ["mongodb"]},
    {"q": "css framework no bootstrap lightweight", "neg": ["bootstrap"]},
    {"q": "programming language other than java for backend", "neg": ["java"]},
    {"q": "cloud provider excluding aws cheaper", "neg": ["aws"]},
]

EDGE_CASES = [
    {"q": "", "desc": "empty string"},
    {"q": "a", "desc": "single char"},
    {"q": "   ", "desc": "whitespace only"},
    {"q": "the the the", "desc": "stop words only"},
    {"q": "a]b[c{d}", "desc": "special chars"},
    {"q": "x" * 500, "desc": "500-char query"},
    {"q": "C++ vs C# performance", "desc": "special chars in language names"},
    {"q": "node.js async await", "desc": "dots in query"},
    {"q": "query with emoji \U0001f525", "desc": "emoji"},
    {"q": "SELECT * FROM users; DROP TABLE", "desc": "SQL injection attempt"},
    {"q": "<script>alert('xss')</script>", "desc": "XSS attempt"},
    {"q": "a" * 2000, "desc": "2000-char query (extreme)"},
]

# ─── Helpers ────────────────────────────────────────────────────────

@dataclass
class TestResult:
    name: str
    query: str
    desc: str = ""
    status_code: int = 0
    response_time_ms: float = 0.0
    intent: str = ""
    confidence: float = 0.0
    num_results: int = 0
    constraints_positive: list = field(default_factory=list)
    constraints_negative: list = field(default_factory=list)
    expanded_queries: list = field(default_factory=list)
    error: Optional[str] = None
    raw_response: Optional[dict] = None
    # Quality checks
    negative_violations: List[str] = field(default_factory=list)
    relevance_score: float = 0.0


def run_query(query: str, name: str = "", desc: str = "") -> TestResult:
    result = TestResult(name=name, query=query[:100], desc=desc)
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
            sc = data.get("structured_constraints", {})
            result.constraints_positive = sc.get("positive", [])
            result.constraints_negative = sc.get("negative", [])
            result.expanded_queries = data.get("expanded_queries", [])
            results = data.get("results", [])
            result.num_results = len(results)
        else:
            result.error = f"HTTP {resp.status_code}: {resp.text[:200]}"
    except requests.exceptions.Timeout:
        result.error = "TIMEOUT"
    except requests.exceptions.ConnectionError as e:
        result.error = f"CONNECTION_ERROR: {str(e)[:100]}"
    except Exception as e:
        result.error = f"ERROR: {str(e)[:100]}"

    return result


def check_negative_violations(result: TestResult, neg_terms: list) -> TestResult:
    """Check if top 10 results violate negative constraints using word-boundary matching."""
    if not result.raw_response or not neg_terms:
        return result
    results = result.raw_response.get("results", [])[:10]
    for r in results:
        title = (r.get("title", "") or "").lower()
        content = (r.get("content", "") or "").lower()
        url = (r.get("url", "") or "").lower()
        for term in neg_terms:
            pattern = r'\b' + re.escape(term.lower()) + r'\b'
            if re.search(pattern, title) or re.search(pattern, content):
                result.negative_violations.append(
                    f"'{term}' found in: {r.get('title', '')[:60]}"
                )
    return result


def check_relevance(result: TestResult, expected_terms: list) -> TestResult:
    """Check if top 5 results contain expected terms."""
    if not result.raw_response or not expected_terms:
        return result
    results = result.raw_response.get("results", [])[:5]
    if not results:
        return result
    total_matches = 0
    for r in results:
        text = f"{r.get('title','')} {r.get('content','')} {r.get('url','')}".lower()
        for term in expected_terms:
            # stem: strip common suffixes
            stems = [term.lower()]
            if term.lower().endswith('s'):
                stems.append(term.lower()[:-1])
            if term.lower().endswith('ing'):
                stems.append(term.lower()[:-3])
            if term.lower().endswith('ed'):
                stems.append(term.lower()[:-2])
            if any(s in text for s in stems):
                total_matches += 1
    max_possible = len(results) * len(expected_terms)
    result.relevance_score = total_matches / max_possible if max_possible > 0 else 0
    return result


def print_header(title):
    print(f"\n{'='*70}")
    print(f"  {title}")
    print(f"{'='*70}")


def print_result(r: TestResult, verbose=False, show_constraints=False):
    status = "\u2713" if r.status_code == 200 and not r.error else "\u2717"
    print(f"  {status} [{r.response_time_ms:7.0f}ms] {r.status_code} | "
          f"intent={r.intent:<15s} conf={r.confidence:.2f} | "
          f"results={r.num_results:3d} | {r.query}")
    if r.desc:
        print(f"    DESC: {r.desc}")
    if r.error:
        print(f"    ERROR: {r.error}")
    if show_constraints:
        if r.constraints_positive:
            print(f"    POSITIVE: {r.constraints_positive}")
        if r.constraints_negative:
            print(f"    NEGATIVE: {r.constraints_negative}")
    if r.negative_violations:
        print(f"    VIOLATIONS ({len(r.negative_violations)}):")
        for v in r.negative_violations[:3]:
            print(f"      - {v}")
    if r.relevance_score > 0:
        marker = "GOOD" if r.relevance_score >= 0.6 else "WEAK" if r.relevance_score >= 0.3 else "POOR"
        print(f"    RELEVANCE: {r.relevance_score:.2f} ({marker})")
    if verbose and r.raw_response:
        web = r.raw_response.get("results", [])[:3]
        for i, w in enumerate(web):
            print(f"    [{i+1}] score={w.get('score',0):.3f} auth={w.get('authority',0):.2f} | {w.get('title','')[:60]}")
            if w.get('sources'):
                print(f"        sources={w.get('sources')}")


def print_stats(results: list, label: str):
    successful = [r for r in results if r.status_code == 200 and not r.error]
    failed = [r for r in results if r.error]
    latencies = [r.response_time_ms for r in successful]

    print(f"\n  --- {label} Summary ---")
    print(f"  Total: {len(results)}, Success: {len(successful)}, Failed: {len(failed)}")
    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        avg = sum(ls) / n
        p50 = ls[n // 2]
        p95 = ls[int(n * 0.95)] if n >= 20 else ls[-1]
        print(f"  Latency: avg={avg:.0f}ms  p50={p50:.0f}ms  p95={p95:.0f}ms  min={ls[0]:.0f}ms  max={ls[-1]:.0f}ms")
    if failed:
        for r in failed:
            print(f"  FAILED: {r.query[:60]} -> {r.error}")


def flush_cache():
    """Restart gateway to flush in-memory cache."""
    import subprocess
    print("\n  Flushing gateway cache (stop/rm/create/start)...")
    subprocess.run(["docker", "stop", "gateway"], capture_output=True)
    subprocess.run(["docker", "rm", "gateway"], capture_output=True)
    subprocess.run(["docker", "compose", "create", "gateway"], capture_output=True,
                   cwd=r"C:\Users\Likhith\Documents\projects\intentforge-v2\services")
    subprocess.run(["docker", "start", "gateway"], capture_output=True)
    # Wait for health
    for _ in range(20):
        try:
            r = requests.get(f"{BASE_URL}/health", timeout=2)
            if r.status_code == 200:
                print("  Gateway back online.")
                return True
        except:
            pass
        time.sleep(1)
    print("  WARNING: Gateway did not come back online in 20s")
    return False


# ─── Test Categories ────────────────────────────────────────────────

def test_general():
    print_header("GENERAL QUERIES")
    results = []
    for q in GENERAL_QUERIES:
        r = run_query(q, "general")
        check_relevance(r, q.lower().split()[:3])
        print_result(r, verbose=True)
        results.append(r)
    print_stats(results, "General")
    return results


def test_complex():
    print_header("COMPLEX QUERIES")
    results = []
    for q in COMPLEX_QUERIES:
        r = run_query(q, "complex")
        # Extract key terms for relevance
        terms = [w for w in q.lower().split() if len(w) > 3][:4]
        check_relevance(r, terms)
        print_result(r, verbose=True)
        results.append(r)
    print_stats(results, "Complex")
    return results


def test_single_constraint():
    print_header("SINGLE CONSTRAINT QUERIES")
    results = []
    for tc in SINGLE_CONSTRAINT_QUERIES:
        r = run_query(tc["q"], "single-constraint")
        check_relevance(r, tc.get("expect_positive", []))
        print_result(r, verbose=True, show_constraints=True)
        results.append(r)
    print_stats(results, "Single Constraint")
    return results


def test_multi_constraint():
    print_header("MULTI-CONSTRAINT QUERIES")
    results = []
    for tc in MULTI_CONSTRAINT_QUERIES:
        r = run_query(tc["q"], "multi-constraint")
        check_relevance(r, tc.get("expect_positive", []))
        print_result(r, verbose=True, show_constraints=True)
        results.append(r)
    print_stats(results, "Multi-Constraint")
    return results


def test_negative_constraints():
    print_header("NEGATIVE CONSTRAINT QUERIES")
    results = []
    for tc in NEGATIVE_CONSTRAINT_QUERIES:
        r = run_query(tc["q"], "negative")
        check_negative_violations(r, tc["neg"])
        terms = [w for w in tc["q"].lower().split() if len(w) > 3 and w not in tc["neg"]][:3]
        check_relevance(r, terms)
        print_result(r, verbose=True, show_constraints=True)
        results.append(r)

    # Summary stats
    total_violations = sum(len(r.negative_violations) for r in results if r.status_code == 200)
    successful = [r for r in results if r.status_code == 200]
    if successful:
        violation_rate = sum(1 for r in successful if r.negative_violations) / len(successful) * 100
        print(f"\n  NEGATIVE CONSTRAINT VIOLATION RATE: {violation_rate:.0f}% ({total_violations} total violations in top-10)")

    print_stats(results, "Negative Constraint")
    return results


def test_edge_cases():
    print_header("EDGE CASES")
    results = []
    for tc in EDGE_CASES:
        r = run_query(tc["q"], "edge", desc=tc["desc"])
        print_result(r)
        results.append(r)
    print_stats(results, "Edge Cases")
    return results


def stress_test_cached(concurrency_list=[5, 10, 20]):
    """Stress test with SAME query (cache hits expected after first)."""
    print_header("STRESS TEST: CACHED QUERIES")
    query = "python programming"

    # Prime the cache
    print("  Priming cache...")
    run_query(query, "prime")

    for concurrency in concurrency_list:
        num_requests = concurrency * 3
        print(f"\n  Cached burst: {concurrency} concurrent x {num_requests} total")
        latencies = []
        errors = 0

        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [executor.submit(run_query, query, f"cached-{i}") for i in range(num_requests)]
            for f in as_completed(futures):
                r = f.result()
                if r.status_code == 200 and not r.error:
                    latencies.append(r.response_time_ms)
                else:
                    errors += 1

        if latencies:
            ls = sorted(latencies)
            n = len(ls)
            total_time = sum(latencies) / 1000 / concurrency
            throughput = num_requests / total_time if total_time > 0 else 0
            print(f"    Throughput: {throughput:.0f} req/s")
            print(f"    p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")
        print(f"    Success: {len(latencies)}/{num_requests}, Errors: {errors}")


def stress_test_unique_concurrent(concurrency=20):
    """Stress test with UNIQUE queries (no cache, reveals serialization)."""
    print_header("STRESS TEST: UNIQUE CONCURRENT QUERIES")

    queries = [
        "python web scraping", "rust async runtime", "golang microservices",
        "kubernetes helm charts", "terraform aws modules", "react native navigation",
        "vue.js state management", "angular dependency injection", "svelte stores",
        "django rest framework", "flask blueprints", "fastapi middleware",
        "postgresql indexing", "redis caching strategies", "elasticsearch mapping",
        "docker multi-stage builds", "nginx reverse proxy", "linux kernel modules",
        "swift concurrency", "kotlin coroutines", "scala akka actors",
        "haskell monads", "elixir phoenix channels", "ruby metaprogramming",
        "perl regex advanced", "lua coroutines", "julia parallel computing",
        "R statistical modeling", "matlab simulink", "bash scripting advanced",
    ][:concurrency]

    print(f"  Firing {len(queries)} unique queries concurrently...")
    latencies = []
    errors = []
    start_wall = time.time()

    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = {executor.submit(run_query, q, f"unique-{i}"): q for i, q in enumerate(queries)}
        for f in as_completed(futures):
            r = f.result()
            if r.status_code == 200 and not r.error:
                latencies.append(r.response_time_ms)
            else:
                errors.append((futures[f], r.error))

    wall_time = (time.time() - start_wall) * 1000

    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        print(f"  Wall time: {wall_time/1000:.1f}s")
        print(f"  Throughput: {len(latencies) / (wall_time/1000):.1f} queries/s")
        print(f"  p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")
        print(f"  Contention ratio: {ls[-1]/ls[0]:.1f}x (max/min latency)")
    print(f"  Success: {len(latencies)}/{len(queries)}, Errors: {len(errors)}")
    if errors:
        for q, e in errors[:5]:
            print(f"    FAIL: {q[:50]} -> {e}")


def stress_test_unique_sequential(count=20):
    """Sequential unique queries — baseline latency without concurrency."""
    print_header("STRESS TEST: UNIQUE SEQUENTIAL QUERIES")

    queries = [
        "python data structures", "rust ownership model", "golang channels",
        "kubernetes pods", "terraform state", "react hooks",
        "vue composition api", "angular signals", "svelte reactivity",
        "django ORM", "flask middleware", "fastapi dependency injection",
        "postgresql vacuum", "redis pubsub", "elasticsearch queries",
        "docker networking", "nginx load balancing", "linux systemd",
        "swift protocols", "kotlin flows",
    ][:count]

    latencies = []
    for i, q in enumerate(queries):
        r = run_query(q, f"seq-{i}")
        latencies.append(r.response_time_ms)
        status = "\u2713" if r.status_code == 200 and not r.error else "\u2717"
        print(f"  {status} [{r.response_time_ms:7.0f}ms] results={r.num_results:3d} | {q}")

    ls = sorted(latencies)
    n = len(ls)
    print(f"\n  Baseline: avg={sum(ls)/n:.0f}ms  p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")


# ─── Main ───────────────────────────────────────────────────────────

def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("=" * 70)

    # Health check
    try:
        r = requests.get(f"{BASE_URL}/health", timeout=5)
        r.raise_for_status()
        print("  Health: OK")
    except Exception as e:
        print(f"  FATAL: Gateway not healthy: {e}")
        sys.exit(1)

    # Cache already flushed before running
    print("  Cache flush: skipped (done manually before run)")

    all_results = {}

    # 1. General queries
    all_results["general"] = test_general()

    # 2. Complex queries
    all_results["complex"] = test_complex()

    # 3. Single constraint
    all_results["single_constraint"] = test_single_constraint()

    # 4. Multi-constraint
    all_results["multi_constraint"] = test_multi_constraint()

    # 5. Negative constraints
    all_results["negative"] = test_negative_constraints()

    # 6. Edge cases
    all_results["edge_cases"] = test_edge_cases()

    # 7. Stress tests
    stress_test_unique_sequential(20)
    stress_test_cached([5, 10, 20])
    stress_test_unique_concurrent(20)

    # ─── Final Summary ──────────────────────────────────────────────
    print_header("OVERALL SUMMARY")

    total_queries = 0
    total_success = 0
    total_fail = 0
    all_latencies = []

    for category, results in all_results.items():
        successful = [r for r in results if r.status_code == 200 and not r.error]
        failed = [r for r in results if r.error]
        latencies = [r.response_time_ms for r in successful]
        total_queries += len(results)
        total_success += len(successful)
        total_fail += len(failed)
        all_latencies.extend(latencies)

        if latencies:
            avg = sum(latencies) / len(latencies)
            print(f"  {category:<20s}: {len(successful)}/{len(results)} ok, avg={avg:.0f}ms")
        else:
            print(f"  {category:<20s}: {len(successful)}/{len(results)} ok")

    if all_latencies:
        ls = sorted(all_latencies)
        n = len(ls)
        print(f"\n  TOTAL: {total_success}/{total_queries} success, {total_fail} failed")
        print(f"  OVERALL LATENCY: avg={sum(ls)/n:.0f}ms  p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")

    # Bottleneck analysis
    print(f"\n  BOTTLENECK ANALYSIS:")
    slow_queries = []
    for category, results in all_results.items():
        for r in results:
            if r.status_code == 200 and r.response_time_ms > 3000:
                slow_queries.append((r.response_time_ms, r.query, category))
    if slow_queries:
        slow_queries.sort(reverse=True)
        print(f"  Queries > 3s ({len(slow_queries)}):")
        for ms, q, cat in slow_queries[:10]:
            print(f"    {ms:.0f}ms [{cat}] {q[:60]}")
    else:
        print(f"  No queries exceeded 3s threshold.")

    # Negative constraint summary
    total_violations = 0
    for results in all_results.get("negative", []):
        total_violations += len(results.negative_violations)
    if total_violations > 0:
        print(f"\n  NEGATIVE CONSTRAINT VIOLATIONS: {total_violations} total across all negative tests")


if __name__ == "__main__":
    main()
