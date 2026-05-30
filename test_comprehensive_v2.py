#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general, complex, single-constraint, multi-constraint,
       negative constraints, edge cases, stress, deep quality audit.
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
TIMEOUT = 45

# ─── Test Data ───────────────────────────────────────────────────────

GENERAL_QUERIES = [
    "python programming",
    "machine learning tutorials",
    "best restaurants near me",
    "weather forecast today",
    "how to learn guitar",
    "javascript documentation",
    "linux kernel development",
    "blockchain technology explained",
]

COMPLEX_QUERIES = [
    "rust vs go for systems programming 2026",
    "zero-knowledge proofs in blockchain scalability",
    "federated learning privacy preserving machine learning",
    "building real-time collaborative editors with CRDTs",
    "quantum error correction surface codes implementation",
    "transformer architecture attention mechanism explained",
    "distributed consensus algorithms comparison paxos raft",
]

SINGLE_CONSTRAINT = [
    {"q": "python web framework", "expect_positive": ["python", "framework"]},
    {"q": "rust async runtime", "expect_positive": ["rust", "runtime"]},
    {"q": "kubernetes deployment best practices", "expect_positive": ["kubernetes", "deployment"]},
    {"q": "database alternative to postgresql", "expect_negative": ["postgresql"]},
    {"q": "linux container runtime lightweight alternative to docker for embedded", "expect_negative": ["docker"]},
]

MULTI_CONSTRAINT = [
    {"q": "python async web framework with websocket support", "expect_positive": ["python", "async", "websocket"]},
    {"q": "javascript frontend framework typescript SSR", "expect_positive": ["javascript", "typescript", "ssr"]},
    {"q": "rust web framework async database ORM", "expect_positive": ["rust", "async", "database", "orm"]},
    {"q": "go microservices gRPC protobuf kubernetes", "expect_positive": ["grpc", "protobuf", "kubernetes"]},
]

NEGATIVE_CONSTRAINT = [
    {"q": "python web framework not django", "expect_negative": ["django"], "expect_positive": ["python", "framework"]},
    {"q": "javascript framework except react", "expect_negative": ["react"], "expect_positive": ["javascript", "framework"]},
    {"q": "text editor without vim", "expect_negative": ["vim"], "expect_positive": ["editor"]},
    {"q": "css framework no bootstrap", "expect_negative": ["bootstrap"], "expect_positive": ["css", "framework"]},
    {"q": "programming language other than java", "expect_negative": ["java"], "expect_positive": ["programming"]},
    {"q": "linux distro not ubuntu", "expect_negative": ["ubuntu"], "expect_positive": ["linux", "distro"]},
    {"q": "search engine alternative to google", "expect_negative": ["google"], "expect_positive": ["search"]},
    {"q": "static site generator not jekyll", "expect_negative": ["jekyll"], "expect_positive": ["static", "site"]},
]

EDGE_CASES = [
    "",           # empty
    "a",          # single char
    "   ",        # whitespace only
    "the the the",  # stop words only
    "a]b[c{d}",   # special chars
    "x" * 500,    # very long
    "query with emoji \U0001f525",  # emoji
    "C++ programming",  # special language name
    "node.js tutorial",  # dots
    "machine learning & AI",  # ampersand
    "hello\x00world",  # null byte
    "react native vs flutter 2026",  # normal but tech-heavy
]

# ─── Data Classes ────────────────────────────────────────────────────

@dataclass
class TestResult:
    name: str
    query: str
    status_code: int = 0
    response_time_ms: float = 0.0
    intent: str = ""
    confidence: float = 0.0
    num_results: int = 0
    constraints_positive: list = field(default_factory=list)
    constraints_negative: list = field(default_factory=list)
    expanded_queries: list = field(default_factory=list)
    top5_scores: list = field(default_factory=list)
    error: Optional[str] = None
    raw_response: Optional[dict] = None


def run_query(query: str, name: str = "") -> TestResult:
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
            sc = data.get("structured_constraints", {})
            result.constraints_positive = sc.get("positive", [])
            result.constraints_negative = sc.get("negative", [])
            result.expanded_queries = data.get("expanded_queries", [])
            all_results = data.get("results", [])
            result.num_results = len(all_results)
            result.top5_scores = [round(r.get("score", 0), 4) for r in all_results[:5]]
        elif resp.status_code == 422:
            result.error = f"HTTP 422 (validation): {resp.text[:200]}"
        else:
            result.error = f"HTTP {resp.status_code}: {resp.text[:200]}"
    except requests.exceptions.Timeout:
        result.error = f"TIMEOUT (>{TIMEOUT}s)"
    except requests.exceptions.ConnectionError as e:
        result.error = f"CONNECTION_ERROR: {str(e)[:100]}"
    except Exception as e:
        result.error = f"ERROR: {str(e)[:100]}"

    return result


def print_result(r: TestResult, verbose=False, indent="  "):
    status = "OK" if r.status_code == 200 and not r.error else "FAIL"
    if r.status_code == 422:
        status = "422"
    print(f"{indent}{status:4s} [{r.response_time_ms:7.0f}ms] {r.status_code} | "
          f"intent={r.intent:<16s} conf={r.confidence:.2f} | "
          f"results={r.num_results:3d} | {r.query}")
    if r.error:
        print(f"{indent}     ERROR: {r.error}")
    if r.constraints_negative:
        print(f"{indent}     NEG: {r.constraints_negative}")
    if r.constraints_positive:
        print(f"{indent}     POS: {r.constraints_positive}")
    if verbose and r.top5_scores:
        print(f"{indent}     Top5 scores: {r.top5_scores}")


def check_negative_violations(results_list, neg_terms):
    """Check top-10 results for negative constraint violations (word-boundary)."""
    violations = []
    for i, r in enumerate(results_list[:10]):
        title = (r.get("title") or "").lower()
        content = (r.get("content") or "").lower()
        url = (r.get("url") or "").lower()
        combined = f"{title} {content} {url}"
        for neg in neg_terms:
            pattern = r'\b' + re.escape(neg.lower()) + r'\b'
            if re.search(pattern, combined):
                violations.append({"rank": i+1, "term": neg, "title": title[:80], "url": url[:80]})
                break  # one violation per result is enough
    return violations


def check_relevance(result, expected_terms):
    """Check a single result for term relevance."""
    title = (result.get("title") or "").lower()
    content = (result.get("content") or "").lower()
    url = (result.get("url") or "").lower()
    combined = f"{title} {content} {url}"
    found = 0
    for term in expected_terms:
        t = term.lower()
        if t in combined:
            found += 1
        else:
            stemmed = t.rstrip("s").rstrip("ed").rstrip("ing")
            if len(stemmed) > 3 and stemmed in combined:
                found += 1
    return round(found / max(len(expected_terms), 1), 2)


# ─── Test Runners ────────────────────────────────────────────────────

def test_category(name, queries, verbose=True):
    print(f"\n{'='*70}")
    print(f"  {name}")
    print(f"{'='*70}")
    results = []
    for q in queries:
        q_str = q["q"] if isinstance(q, dict) else q
        r = run_query(q_str, name)
        results.append(r)
        print_result(r, verbose=verbose)

        # Deep checks for constraint queries
        if isinstance(q, dict) and r.raw_response:
            all_res = r.raw_response.get("results", [])
            # Check negative constraint violations
            if q.get("expect_negative") and all_res:
                violations = check_negative_violations(all_res, q["expect_negative"])
                if violations:
                    print(f"     !! NEG VIOLATIONS ({len(violations)}):")
                    for v in violations[:3]:
                        print(f"        #{v['rank']} '{v['term']}' in: {v['title']}")
                else:
                    print(f"     OK: No negative constraint violations in top-10")

            # Check positive constraint presence
            if q.get("expect_positive") and all_res:
                rels = [check_relevance(r_item, q["expect_positive"]) for r_item in all_res[:5]]
                avg_rel = round(statistics.mean(rels), 2) if rels else 0
                print(f"     Relevance (top5 avg): {avg_rel} (expect >= 0.5)")

            # Check structured constraints extraction
            if q.get("expect_negative"):
                actual_neg = r.constraints_negative
                for expected in q["expect_negative"]:
                    found = any(expected.lower() in n.lower() for n in actual_neg)
                    if not found:
                        print(f"     !! Expected negative '{expected}' NOT extracted. Got: {actual_neg}")

        time.sleep(0.3)  # rate limit
    return results


def stress_test(concurrency, num_requests, query="python programming", label="cached"):
    """Stress test with same or unique queries."""
    print(f"\n{'='*70}")
    print(f"  STRESS TEST: {label} ({concurrency} concurrent x {num_requests} total)")
    print(f"{'='*70}")

    latencies = []
    errors = 0
    error_details = []

    def run_one(i):
        if label == "unique":
            q = f"{query} variant {i} topic {i*7 % 100}"
        else:
            q = query
        return run_query(q, f"stress-{i}")

    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(run_one, i) for i in range(num_requests)]
        for f in as_completed(futures):
            r = f.result()
            if r.status_code == 200 and not r.error:
                latencies.append(r.response_time_ms)
            else:
                errors += 1
                if len(error_details) < 3:
                    error_details.append(f"  {r.error}")

    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        wall_time = sum(latencies) / 1000 / concurrency
        throughput = num_requests / wall_time if wall_time > 0 else 0
        print(f"  Throughput: {throughput:.1f} req/s")
        print(f"  Latency: p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  p99={ls[int(n*0.99)]:.0f}ms  max={ls[-1]:.0f}ms")
        print(f"  Success: {len(latencies)}/{num_requests}  Errors: {errors}")
        if errors > 0:
            for ed in error_details:
                print(f"  {ed}")
    else:
        print(f"  ALL FAILED ({errors}/{num_requests})")
        for ed in error_details:
            print(f"  {ed}")


def stress_unique_sequential(count=15):
    """Sequential unique queries — baseline per-query latency."""
    print(f"\n{'='*70}")
    print(f"  SEQUENTIAL UNIQUE ({count} queries)")
    print(f"{'='*70}")

    queries = [
        "python web framework comparison",
        "rust ownership borrow checker",
        "kubernetes helm charts tutorial",
        "graphql vs rest api design",
        "elasticsearch full text search",
        "redis caching strategies",
        "terraform aws infrastructure",
        "react hooks useEffect patterns",
        "golang concurrency goroutines channels",
        "swift ios ui framework",
        "scala functional programming cats",
        "haskell monad transformer stack",
        "clojure spec generative testing",
        "elixir phoenix liveview tutorial",
        "zig comptime metaprogramming",
    ][:count]

    latencies = []
    errors = 0
    for i, q in enumerate(queries):
        r = run_query(q, f"seq-{i}")
        if r.status_code == 200 and not r.error:
            latencies.append(r.response_time_ms)
            print(f"  [{r.response_time_ms:6.0f}ms] intent={r.intent:<14s} results={r.num_results:3d} | {q}")
        else:
            errors += 1
            print(f"  [FAIL   ] {r.error} | {q}")
        time.sleep(0.2)

    if latencies:
        ls = sorted(latencies)
        n = len(ls)
        print(f"\n  Summary: avg={statistics.mean(ls):.0f}ms  p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")
        print(f"  Errors: {errors}/{count}")


def deep_quality_audit():
    """Check top-5 results per query for individual relevance."""
    print(f"\n{'='*70}")
    print(f"  DEEP QUALITY AUDIT (per-result relevance)")
    print(f"{'='*70}")

    audit_queries = [
        ("python web framework not django", ["python", "framework", "web"], ["django"]),
        ("javascript runtime not node", ["javascript", "runtime"], ["node"]),
        ("rust async web framework", ["rust", "async", "framework"], []),
        ("css framework no bootstrap", ["css", "framework"], ["bootstrap"]),
        ("linux distro not ubuntu", ["linux", "distro"], ["ubuntu"]),
    ]

    all_rels = []
    total_violations = 0

    for query, expected, negatives in audit_queries:
        r = run_query(query, "audit")
        if r.status_code != 200 or not r.raw_response:
            print(f"\n  FAIL: {query} — {r.error}")
            continue

        all_results = r.raw_response.get("results", [])
        print(f"\n  Query: \"{query}\"")
        print(f"  Intent: {r.intent} ({r.confidence:.2f}) | Results: {r.num_results} | {r.response_time_ms:.0f}ms")
        if r.constraints_negative:
            print(f"  Negative constraints: {r.constraints_negative}")

        violations = 0
        for i, res in enumerate(all_results[:5]):
            rel = check_relevance(res, expected)
            all_rels.append(rel)
            title = (res.get("title") or "")[:60]
            score = res.get("score", 0)
            url = (res.get("url") or "")[:80]

            # Check negative violation
            neg_violation = False
            if negatives:
                combined = f"{(res.get('title') or '')} {(res.get('content') or '')} {(res.get('url') or '')}".lower()
                for neg in negatives:
                    if re.search(r'\b' + re.escape(neg) + r'\b', combined):
                        neg_violation = True
                        violations += 1

            marker = "+++" if rel >= 0.6 else "++ " if rel >= 0.4 else "+  " if rel >= 0.2 else "   "
            neg_flag = " [NEG VIOL]" if neg_violation else ""
            print(f"    {marker} #{i+1} rel={rel:.2f} score={score:.3f} | {title}{neg_flag}")
            print(f"         {url}")

        total_violations += violations
        if violations == 0:
            print(f"  OK: No negative violations in top-5")
        time.sleep(0.3)

    # Summary
    if all_rels:
        print(f"\n{'='*70}")
        print(f"  AUDIT SUMMARY")
        print(f"{'='*70}")
        print(f"  Mean relevance: {statistics.mean(all_rels):.2f}")
        high = sum(1 for r in all_rels if r >= 0.6)
        med = sum(1 for r in all_rels if 0.4 <= r < 0.6)
        low = sum(1 for r in all_rels if r < 0.4)
        print(f"  High (>=0.6): {high}/{len(all_rels)} ({high*100//len(all_rels)}%)")
        print(f"  Med  (0.4-0.6): {med}/{len(all_rels)} ({med*100//len(all_rels)}%)")
        print(f"  Low  (<0.4): {low}/{len(all_rels)} ({low*100//len(all_rels)}%)")
        print(f"  Total negative violations: {total_violations}")


# ─── Main ────────────────────────────────────────────────────────────

def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("  " + time.strftime("%Y-%m-%d %H:%M:%S"))
    print("=" * 70)

    # Health check
    try:
        resp = requests.get(f"{BASE_URL}/health", timeout=5)
        resp.raise_for_status()
        print(f"  Gateway: OK")
    except Exception as e:
        print(f"  FATAL: Gateway not healthy: {e}")
        sys.exit(1)

    # 1. General queries
    gen_results = test_category("GENERAL QUERIES", GENERAL_QUERIES, verbose=True)

    # 2. Complex queries
    complex_results = test_category("COMPLEX QUERIES", COMPLEX_QUERIES, verbose=True)

    # 3. Single constraint
    single_results = test_category("SINGLE CONSTRAINT", SINGLE_CONSTRAINT, verbose=True)

    # 4. Multi-constraint
    multi_results = test_category("MULTI-CONSTRAINT", MULTI_CONSTRAINT, verbose=True)

    # 5. Negative constraints (most critical)
    neg_results = test_category("NEGATIVE CONSTRAINTS", NEGATIVE_CONSTRAINT, verbose=True)

    # 6. Edge cases
    print(f"\n{'='*70}")
    print(f"  EDGE CASES")
    print(f"{'='*70}")
    for q in EDGE_CASES:
        r = run_query(q, "edge")
        label = repr(q) if len(q) < 40 else repr(q[:30]) + "..."
        status = "OK" if r.status_code in (200, 422) else "FAIL"
        print(f"  {status:4s} [{r.response_time_ms:6.0f}ms] {r.status_code} | {label}")
        if r.error:
            print(f"       {r.error[:120]}")
        time.sleep(0.2)

    # 7. Stress tests
    stress_test(5, 15, label="cached")
    stress_test(10, 30, label="cached")
    stress_test(20, 40, label="cached")
    stress_test(10, 20, label="unique")

    # 8. Sequential unique (baseline)
    stress_unique_sequential(15)

    # 9. Deep quality audit
    deep_quality_audit()

    # Final summary
    print(f"\n{'='*70}")
    print(f"  OVERALL SUMMARY")
    print(f"{'='*70}")

    all_latencies = []
    all_intents = {}
    all_errors = 0
    for r in gen_results + complex_results + single_results + multi_results + neg_results:
        if r.status_code == 200 and not r.error:
            all_latencies.append(r.response_time_ms)
            all_intents[r.intent] = all_intents.get(r.intent, 0) + 1
        else:
            all_errors += 1

    if all_latencies:
        ls = sorted(all_latencies)
        n = len(ls)
        print(f"  Queries: {len(all_latencies)} OK, {all_errors} failed")
        print(f"  Latency: avg={statistics.mean(ls):.0f}ms  p50={ls[n//2]:.0f}ms  p95={ls[int(n*0.95)]:.0f}ms  max={ls[-1]:.0f}ms")
        print(f"  Intent distribution: {json.dumps(all_intents, indent=4)}")
    else:
        print(f"  NO SUCCESSFUL QUERIES")


if __name__ == "__main__":
    main()
