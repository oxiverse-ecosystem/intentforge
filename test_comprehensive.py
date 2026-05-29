#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general, complex, single-constraint, multi-constraint, negative-constraint,
       edge cases, stress (cached + unique + concurrent), bottleneck analysis.
"""

import requests
import json
import time
import sys
import statistics
import re
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import Optional

BASE_URL = "http://localhost:4000"
SEARCH_URL = f"{BASE_URL}/search"
TIMEOUT = 45

# ═══════════════════════════════════════════════════════════════════
# QUERY DEFINITIONS
# ═══════════════════════════════════════════════════════════════════

GENERAL_QUERIES = [
    "python programming",
    "machine learning tutorials",
    "how to learn guitar",
    "best restaurants in NYC",
    "climate change effects",
    "quantum computing basics",
    "healthy meal prep ideas",
    "javascript async await",
]

COMPLEX_QUERIES = [
    "rust vs go for systems programming performance comparison",
    "zero-knowledge proofs in blockchain scalability",
    "federated learning privacy preserving machine learning",
    "neuromorphic computing chip architectures 2026",
    "post-quantum cryptography NIST standards migration",
    "WebAssembly runtime performance browser vs serverless",
    "CRISPR gene editing ethical implications clinical trials",
    "large language model quantization techniques edge deployment",
]

SINGLE_CONSTRAINT_QUERIES = [
    {"q": "python web framework official documentation", "expect_terms": ["python", "web", "framework"]},
    {"q": "rust async runtime tokio tutorial", "expect_terms": ["rust", "tokio", "async"]},
    {"q": "kubernetes deployment best practices production", "expect_terms": ["kubernetes", "deployment"]},
    {"q": "react native mobile app development guide", "expect_terms": ["react", "mobile"]},
    {"q": "postgresql query optimization indexing", "expect_terms": ["postgresql", "query", "optimization"]},
]

MULTI_CONSTRAINT_QUERIES = [
    {"q": "python async web framework with websocket support not django", "expect_pos": ["python", "async", "web", "websocket"], "expect_neg": ["django"]},
    {"q": "rust web server framework with middleware and http2 support", "expect_pos": ["rust", "web", "middleware", "http2"]},
    {"q": "javascript frontend framework lightweight no virtual dom", "expect_pos": ["javascript", "frontend", "lightweight"], "expect_neg": ["virtual dom"]},
    {"q": "go database orm with migration support and connection pooling", "expect_pos": ["go", "database", "orm", "migration", "pooling"]},
    {"q": "python machine learning library for time series forecasting not tensorflow", "expect_pos": ["python", "machine learning", "time series"], "expect_neg": ["tensorflow"]},
    {"q": "linux container runtime lightweight alternative to docker for embedded", "expect_pos": ["linux", "container", "lightweight", "embedded"], "expect_neg": ["docker"]},
]

NEGATIVE_CONSTRAINT_QUERIES = [
    {"q": "python web framework not django not flask", "neg_terms": ["django", "flask"]},
    {"q": "javascript framework except react and angular", "neg_terms": ["react", "angular"]},
    {"q": "programming language for beginners not python", "neg_terms": ["python"]},
    {"q": "database for web app no sql not mongodb", "neg_terms": ["mongodb"]},
    {"q": "static site generator not jekyll not hugo", "neg_terms": ["jekyll", "hugo"]},
    {"q": "css framework without bootstrap not tailwind", "neg_terms": ["bootstrap", "tailwind"]},
]

EDGE_CASES = [
    {"q": "", "label": "empty string"},
    {"q": "a", "label": "single char"},
    {"q": "   ", "label": "whitespace only"},
    {"q": "the is a an of", "label": "stop words only"},
    {"q": "a]b[c{d}e@f!g#h", "label": "special chars"},
    {"q": "x" * 500, "label": "500-char query"},
    {"q": "query with emoji 🔥🚀💡", "label": "emoji"},
    {"q": "C++ programming", "label": "C++ (special chars)"},
    {"q": "node.js tutorial", "label": "node.js (dots)"},
    {"q": "what is the meaning of life the universe and everything", "label": "philosophical"},
    {"q": "SELECT * FROM users WHERE id=1", "label": "SQL injection attempt"},
    {"q": "<script>alert(1)</script>", "label": "XSS attempt"},
    {"q": "a" * 2000, "label": "2000-char query (extreme)"},
]

# ═══════════════════════════════════════════════════════════════════
# RESULT TRACKING
# ═══════════════════════════════════════════════════════════════════

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
    """Execute a single search query."""
    result = TestResult(name=name, query=query[:100])
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
    except requests.exceptions.ConnectionError as e:
        result.error = f"CONNECTION_ERROR: {str(e)[:100]}"
    except Exception as e:
        result.error = f"ERROR: {str(e)[:100]}"

    return result


def check_relevance(r: TestResult, expect_terms: list) -> dict:
    """Check top 5 results for term relevance."""
    if not r.raw_response:
        return {"score": 0, "matches": 0, "total": len(expect_terms), "violations": []}

    web = r.raw_response.get("web_results", [])[:5]
    if not web:
        return {"score": 0, "matches": 0, "total": len(expect_terms), "violations": []}

    total_matches = 0
    for w in web:
        text = (w.get("title", "") + " " + w.get("content", "") + " " + w.get("url", "")).lower()
        for term in expect_terms:
            if term.lower() in text:
                total_matches += 1
                break

    relevance = total_matches / len(web) if web else 0
    return {"score": relevance, "matches": total_matches, "total": len(web)}


def check_negative_violations(r: TestResult, neg_terms: list) -> list:
    """Check if top results violate negative constraints."""
    violations = []
    web = r.raw_response.get("web_results", [])[:10] if r.raw_response else []
    for w in web:
        text = (w.get("title", "") + " " + w.get("content", "")).lower()
        for term in neg_terms:
            if term.lower() in text:
                violations.append({"title": w.get("title", "")[:60], "violated_term": term})
                break
    return violations


def print_result(r: TestResult, verbose=False, indent="  "):
    status = "OK" if r.status_code == 200 and not r.error else "FAIL"
    marker = "✓" if status == "OK" else "✗"
    print(f"{indent}{marker} [{r.response_time_ms:7.0f}ms] {r.status_code} | "
          f"intent={r.intent:<15s} conf={r.confidence:.2f} | "
          f"web={r.num_web_results:2d} local={r.num_local_results:2d} | "
          f"{r.query[:70]}")
    if r.error:
        print(f"{indent}  ERROR: {r.error}")
    if verbose and r.raw_response:
        web = r.raw_response.get("web_results", [])[:3]
        for i, w in enumerate(web):
            sources = w.get("sources", [])
            src_str = ",".join(sources[:3]) if sources else "?"
            print(f"{indent}  [{i+1}] score={w.get('score',0):.3f} [{src_str}] {w.get('title','')[:60]}")
        if r.expanded_queries:
            print(f"{indent}  expanded: {r.expanded_queries[:3]}")


def print_separator(title):
    print(f"\n{'='*70}")
    print(f"  {title}")
    print(f"{'='*70}")


# ═══════════════════════════════════════════════════════════════════
# TEST CATEGORIES
# ═══════════════════════════════════════════════════════════════════

def test_general():
    print_separator("GENERAL QUERIES")
    results = []
    for q in GENERAL_QUERIES:
        r = run_query(q, "general")
        print_result(r, verbose=True)
        results.append(r)
        time.sleep(0.3)

    latencies = [r.response_time_ms for r in results if r.status_code == 200]
    total_web = sum(r.num_web_results for r in results)
    ok = sum(1 for r in results if r.status_code == 200 and r.num_web_results > 0)
    print(f"\n  SUMMARY: {ok}/{len(results)} returned results, "
          f"avg_latency={statistics.mean(latencies):.0f}ms, "
          f"total_web_results={total_web}")
    return results


def test_complex():
    print_separator("COMPLEX QUERIES")
    results = []
    for q in COMPLEX_QUERIES:
        r = run_query(q, "complex")
        print_result(r, verbose=True)
        results.append(r)
        time.sleep(0.3)

    latencies = [r.response_time_ms for r in results if r.status_code == 200]
    ok = sum(1 for r in results if r.status_code == 200 and r.num_web_results > 0)
    avg_conf = statistics.mean([r.confidence for r in results if r.status_code == 200])
    print(f"\n  SUMMARY: {ok}/{len(results)} returned results, "
          f"avg_latency={statistics.mean(latencies):.0f}ms, "
          f"avg_confidence={avg_conf:.3f}")
    return results


def test_single_constraints():
    print_separator("SINGLE CONSTRAINT QUERIES")
    results = []
    for spec in SINGLE_CONSTRAINT_QUERIES:
        r = run_query(spec["q"], "constraint")
        rel = check_relevance(r, spec["expect_terms"])
        print_result(r, verbose=False)
        print(f"    relevance: {rel['score']:.0%} ({rel['matches']}/{rel['total']} top results match)")
        results.append((r, rel))
        time.sleep(0.3)

    avg_rel = statistics.mean([rel["score"] for _, rel in results])
    print(f"\n  SUMMARY: avg_relevance={avg_rel:.0%}")
    return results


def test_multi_constraints():
    print_separator("MULTI-CONSTRAINT QUERIES")
    results = []
    for spec in MULTI_CONSTRAINT_QUERIES:
        r = run_query(spec["q"], "multi-constraint")
        rel = check_relevance(r, spec["expect_pos"])
        violations = check_negative_violations(r, spec.get("expect_neg", [])) if spec.get("expect_neg") else []
        print_result(r, verbose=True)
        print(f"    relevance: {rel['score']:.0%} | constraints_positive: {r.constraints_positive}")
        print(f"    constraints_negative: {r.constraints_negative}")
        if violations:
            print(f"    ⚠ NEGATIVE VIOLATIONS ({len(violations)}):")
            for v in violations[:3]:
                print(f"      - '{v['violated_term']}' found in: {v['title']}")
        else:
            print(f"    ✓ No negative constraint violations")
        results.append((r, rel, violations))
        time.sleep(0.3)

    avg_rel = statistics.mean([rel["score"] for _, rel, _ in results])
    total_violations = sum(len(v) for _, _, v in results)
    print(f"\n  SUMMARY: avg_relevance={avg_rel:.0%}, negative_violations={total_violations}")
    return results


def test_negative_constraints():
    print_separator("NEGATIVE CONSTRAINT QUERIES")
    results = []
    for spec in NEGATIVE_CONSTRAINT_QUERIES:
        r = run_query(spec["q"], "negative")
        violations = check_negative_violations(r, spec["neg_terms"])
        violation_rate = len(violations) / max(r.num_web_results, 1) * 100
        print_result(r, verbose=False)
        print(f"    neg_terms: {spec['neg_terms']} | "
              f"violations: {len(violations)}/{min(r.num_web_results, 10)} checked "
              f"({violation_rate:.0f}%)")
        if violations:
            for v in violations[:3]:
                print(f"      ⚠ '{v['violated_term']}' in: {v['title']}")
        results.append((r, violations))
        time.sleep(0.3)

    total_checked = sum(min(r.num_web_results, 10) for r, _ in results)
    total_violations = sum(len(v) for _, v in results)
    print(f"\n  SUMMARY: violations={total_violations}/{total_checked} "
          f"({total_violations/max(total_checked,1)*100:.1f}%)")
    return results


def test_edge_cases():
    print_separator("EDGE CASES")
    results = []
    for spec in EDGE_CASES:
        r = run_query(spec["q"], f"edge:{spec['label']}")
        status = "OK" if r.status_code == 200 and not r.error else "FAIL"
        print(f"  [{status}] [{r.response_time_ms:6.0f}ms] {r.status_code} | "
              f"web={r.num_web_results:2d} | {spec['label']}")
        if r.error:
            print(f"    ERROR: {r.error}")
        results.append((r, spec["label"]))

    ok = sum(1 for r, _ in results if r.status_code == 200)
    print(f"\n  SUMMARY: {ok}/{len(results)} handled gracefully")
    return results


# ═══════════════════════════════════════════════════════════════════
# STRESS TESTING
# ═══════════════════════════════════════════════════════════════════

def stress_cached_burst():
    """Same query repeated — tests cache + connection handling."""
    print_separator("STRESS: CACHED BURST (same query)")
    query = "python programming"
    for concurrency in [1, 5, 10, 20]:
        num_requests = concurrency * 3
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
            throughput = num_requests / (max(ls) / 1000) if max(ls) > 0 else 0
            print(f"  {concurrency:2d}x concurrent | {num_requests:3d} reqs | "
                  f"p50={ls[n//2]:6.0f}ms p95={ls[int(n*0.95)]:6.0f}ms max={ls[-1]:6.0f}ms | "
                  f"throughput={throughput:.0f} req/s | errors={errors}")
        else:
            print(f"  {concurrency:2d}x concurrent | ALL FAILED ({errors} errors)")


def stress_unique_sequential():
    """Unique queries, sequential — baseline per-query latency."""
    print_separator("STRESS: UNIQUE SEQUENTIAL (baseline)")
    queries = COMPLEX_QUERIES + GENERAL_QUERIES
    latencies = []
    errors = 0

    for i, q in enumerate(queries):
        r = run_query(q, f"seq-{i}")
        if r.status_code == 200 and not r.error:
            latencies.append(r.response_time_ms)
            print(f"  [{i+1:2d}/{len(queries)}] {r.response_time_ms:6.0f}ms | web={r.num_web_results:2d} | {q[:50]}")
        else:
            errors += 1
            print(f"  [{i+1:2d}/{len(queries)}] FAIL | {r.error or r.status_code} | {q[:50]}")
        time.sleep(0.2)

    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        print(f"\n  SUMMARY: n={n}, min={ls[0]:.0f}ms, p50={ls[n//2]:.0f}ms, "
              f"p90={ls[int(n*0.9)]:.0f}ms, p95={ls[int(n*0.95)]:.0f}ms, "
              f"max={ls[-1]:.0f}ms, mean={statistics.mean(ls):.0f}ms, "
              f"stdev={statistics.stdev(ls):.0f}ms")
        print(f"  Errors: {errors}")


def stress_unique_concurrent():
    """Unique queries, all at once — reveals serialization bottlenecks."""
    print_separator("STRESS: UNIQUE CONCURRENT (bottleneck test)")
    queries = COMPLEX_QUERIES + GENERAL_QUERIES + [q["q"] for q in SINGLE_CONSTRAINT_QUERIES]
    concurrency = min(len(queries), 20)
    queries = queries[:concurrency]

    latencies = []
    errors = 0
    completion_times = []

    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        start = time.time()
        futures = {executor.submit(run_query, q, f"conc-{i}"): (i, q) for i, q in enumerate(queries)}
        for f in as_completed(futures):
            r = f.result()
            elapsed = time.time() - start
            completion_times.append(elapsed)
            if r.status_code == 200 and not r.error:
                latencies.append(r.response_time_ms)
                print(f"  [{elapsed:5.1f}s] DONE {r.response_time_ms:6.0f}ms | web={r.num_web_results:2d} | {futures[f][1][:50]}")
            else:
                errors += 1
                print(f"  [{elapsed:5.1f}s] FAIL | {r.error or r.status_code} | {futures[f][1][:50]}")

    wall_time = max(completion_times) if completion_times else 0
    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        contention = wall_time / (statistics.mean(latencies) / 1000) if statistics.mean(latencies) > 0 else 0
        print(f"\n  SUMMARY: wall_time={wall_time:.1f}s, n={n}")
        print(f"  Latency: p50={ls[n//2]:.0f}ms, p90={ls[int(n*0.9)]:.0f}ms, p95={ls[int(n*0.95)]:.0f}ms, max={ls[-1]:.0f}ms")
        print(f"  Contention ratio: {contention:.1f}x (>2x = serialization bottleneck)")
        print(f"  Errors: {errors}")


def stress_ramp_up():
    """Gradually increase concurrency to find breaking point."""
    print_separator("STRESS: RAMP-UP (find breaking point)")
    query = "machine learning"
    for concurrency in [1, 2, 4, 8, 16, 25]:
        num_requests = concurrency * 2
        latencies = []
        errors = 0

        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            start = time.time()
            futures = [executor.submit(run_query, query, f"ramp-{i}") for i in range(num_requests)]
            for f in as_completed(futures):
                r = f.result()
                if r.status_code == 200 and not r.error:
                    latencies.append(r.response_time_ms)
                else:
                    errors += 1
            wall = time.time() - start

        if latencies:
            ls = sorted(latencies)
            n = len(ls)
            throughput = num_requests / wall
            print(f"  c={concurrency:2d} | p50={ls[n//2]:6.0f}ms p95={ls[int(n*0.95)]:6.0f}ms max={ls[-1]:6.0f}ms | "
                  f"throughput={throughput:.1f} r/s | wall={wall:.1f}s | err={errors}")
        else:
            print(f"  c={concurrency:2d} | ALL FAILED ({errors} errors)")


# ═══════════════════════════════════════════════════════════════════
# BOTTLENECK ANALYSIS
# ═══════════════════════════════════════════════════════════════════

def analyze_bottlenecks():
    """Identify bottlenecks from test data."""
    print_separator("BOTTLENECK ANALYSIS")

    # Test 1: Cache effectiveness
    print("\n  --- Cache Effectiveness ---")
    q = "cache test query unique xyzzy"
    r1 = run_query(q, "cache-1")
    r2 = run_query(q, "cache-2")
    r3 = run_query(q, "cache-3")
    print(f"  First call:  {r1.response_time_ms:.0f}ms")
    print(f"  Second call: {r2.response_time_ms:.0f}ms (cache hit expected)")
    print(f"  Third call:  {r3.response_time_ms:.0f}ms (cache hit expected)")
    if r1.response_time_ms > 0:
        cache_speedup = r1.response_time_ms / max(r2.response_time_ms, 1)
        print(f"  Cache speedup: {cache_speedup:.1f}x")
        if cache_speedup < 5:
            print(f"  ⚠ LOW CACHE SPEEDUP — possible cache miss or slow cache")

    # Test 2: 0-result retry penalty
    print("\n  --- Zero-Result Retry ---")
    r = run_query("zxqwkjfplm nonexistent thing 2026", "zero-result")
    print(f"  Query: nonexistent | time={r.response_time_ms:.0f}ms | results={r.num_web_results}")
    if r.response_time_ms > 5000:
        print(f"  ⚠ HIGH LATENCY for 0-result — likely retry sleep adding delay")

    # Test 3: Intent classification overhead
    print("\n  --- Intent Classification ---")
    intents = {}
    for q in GENERAL_QUERIES + COMPLEX_QUERIES:
        r = run_query(q, "intent-check")
        if r.status_code == 200:
            intents[r.intent] = intents.get(r.intent, 0) + 1
        time.sleep(0.2)
    print(f"  Intent distribution: {json.dumps(intents, indent=4)}")

    # Test 4: Response size
    print("\n  --- Response Size ---")
    r = run_query("python programming tutorial", "size-check")
    if r.raw_response:
        size = len(json.dumps(r.raw_response))
        web_count = len(r.raw_response.get("web_results", []))
        local_count = len(r.raw_response.get("local_results", []))
        print(f"  Response size: {size:,} bytes")
        print(f"  Web results: {web_count}, Local results: {local_count}")
        if size > 100000:
            print(f"  ⚠ LARGE RESPONSE — consider pagination or result limits")


# ═══════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════

def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print(f"  Target: {BASE_URL}")
    print(f"  Time: {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print("=" * 70)

    # Health check
    try:
        resp = requests.get(f"{BASE_URL}/health", timeout=5)
        print(f"  Health: {resp.status_code} | {resp.text[:100]}")
    except Exception as e:
        print(f"  FATAL: Gateway not reachable: {e}")
        sys.exit(1)

    # Run all test categories
    general_results = test_general()
    complex_results = test_complex()
    constraint_results = test_single_constraints()
    multi_results = test_multi_constraints()
    negative_results = test_negative_constraints()
    edge_results = test_edge_cases()

    # Stress tests
    stress_cached_burst()
    stress_unique_sequential()
    stress_unique_concurrent()
    stress_ramp_up()

    # Bottleneck analysis
    analyze_bottlenecks()

    # Final summary
    print_separator("FINAL SUMMARY")
    all_standard = general_results + complex_results
    ok_standard = sum(1 for r in all_standard if r.status_code == 200 and r.num_web_results > 0)
    latencies_standard = [r.response_time_ms for r in all_standard if r.status_code == 200]

    print(f"  Standard queries:  {ok_standard}/{len(all_standard)} returned results")
    if latencies_standard:
        ls = sorted(latencies_standard)
        n = len(ls)
        print(f"  Latency: min={ls[0]:.0f}ms p50={ls[n//2]:.0f}ms p90={ls[int(n*0.9)]:.0f}ms max={ls[-1]:.0f}ms mean={statistics.mean(ls):.0f}ms")

    total_neg_violations = sum(len(v) for _, v in negative_results)
    total_neg_checked = sum(min(r.num_web_results, 10) for r, _ in negative_results)
    print(f"  Negative violations: {total_neg_violations}/{total_neg_checked} ({total_neg_violations/max(total_neg_checked,1)*100:.1f}%)")

    edge_ok = sum(1 for r, _ in edge_results if r.status_code == 200)
    print(f"  Edge cases handled: {edge_ok}/{len(edge_results)}")

    print(f"\n  Done. {time.strftime('%H:%M:%S')}")


if __name__ == "__main__":
    main()
