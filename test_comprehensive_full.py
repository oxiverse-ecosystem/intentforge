#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general, constraints, multi-constraints, negative constraints, stress, edge cases
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
TIMEOUT = 35

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
    error: Optional[str] = None
    raw: Optional[dict] = None

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
            result.raw = data
            result.intent = data.get("intent", "unknown")
            result.confidence = data.get("confidence", 0.0)
            sc = data.get("structured_constraints", {})
            result.constraints_positive = sc.get("positive", [])
            result.constraints_negative = sc.get("negative", [])
            result.expanded_queries = data.get("expanded_queries", [])
            result.num_results = len(data.get("results", []))
        else:
            result.error = f"HTTP {resp.status_code}: {resp.text[:200]}"
    except requests.exceptions.Timeout:
        result.error = "TIMEOUT"
    except requests.exceptions.ConnectionError as e:
        result.error = f"CONN_ERR: {str(e)[:80]}"
    except Exception as e:
        result.error = f"ERROR: {str(e)[:80]}"
    return result

def print_header(title):
    print(f"\n{'='*70}")
    print(f"  {title}")
    print(f"{'='*70}")

def print_result(r: TestResult, verbose=False, check_neg=None, check_pos=None):
    ok = r.status_code == 200 and not r.error
    status = "PASS" if ok else "FAIL"
    neg_str = ",".join(r.constraints_negative) if r.constraints_negative else "-"
    pos_str = ",".join(r.constraints_positive[:5]) if r.constraints_positive else "-"
    print(f"  [{status:4s}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
          f"res={r.num_results:3d} | neg=[{neg_str:20s}] pos=[{pos_str:20s}] | {r.query}")
    if r.error:
        print(f"         ERROR: {r.error}")

    # Validation checks
    issues = []
    if check_neg:
        for neg in check_neg:
            found = any(neg.lower() in n.lower() for n in r.constraints_negative)
            if not found:
                issues.append(f"Expected negative constraint '{neg}' not found")
    if check_pos:
        for pos in check_pos:
            found = any(pos.lower() in p.lower() for p in r.constraints_positive)
            if not found:
                issues.append(f"Expected positive constraint '{pos}' not found")
    if issues:
        for iss in issues:
            print(f"         *** {iss}")
    return issues

def check_neg_violations(r: TestResult, top_n=10):
    """Check top-N results for negative constraint violations."""
    if not r.raw or not r.constraints_negative:
        return 0, 0
    results = r.raw.get("results", [])[:top_n]
    violations = 0
    for res in results:
        title = (res.get("title", "") + " " + res.get("content", "") + " " + res.get("url", "")).lower()
        for neg in r.constraints_negative:
            if neg.lower() in title:
                violations += 1
                break
    return violations, len(results)

# ═══════════════════════════════════════════════════════════════
# TEST CATEGORIES
# ═══════════════════════════════════════════════════════════════

GENERAL_QUERIES = [
    ("python programming", "technical"),
    ("machine learning tutorials", "informational"),
    ("best restaurants near me", "comparison"),
    ("weather forecast today", "fresh"),
    ("how to learn guitar", "how-to"),
    ("javascript documentation", "technical"),
    ("linux kernel development", "technical"),
    ("blockchain technology explained", "informational"),
    ("react vs vue vs angular", "comparison"),
    ("how to deploy docker containers", "how-to"),
    ("latest AI news today", "fresh"),
    ("data structures and algorithms", "technical"),
]

CONSTRAINT_QUERIES = [
    # Positive constraints
    ("fast web framework for python", ["python", "web"], None, "positive"),
    ("lightweight javascript bundler", ["javascript"], None, "positive"),
    ("modern css framework responsive", ["css", "responsive"], None, "positive"),
    ("async python web framework", ["python", "async", "web"], None, "positive"),
    ("secure authentication library nodejs", ["authentication", "nodejs"], None, "positive"),
    ("rust async web framework", ["rust", "async", "web"], None, "positive"),
    ("typescript SSR framework", ["typescript"], None, "positive"),
]

MULTI_CONSTRAINT_QUERIES = [
    # Multiple positive + negative combined
    ("fast python web framework not django", ["python", "web", "fast"], ["django"], "multi"),
    ("lightweight javascript framework not react", ["javascript", "lightweight"], ["react"], "multi"),
    ("modern css framework no bootstrap responsive", ["css", "responsive"], ["bootstrap"], "multi"),
    ("rust web framework async not actix", ["rust", "web", "async"], ["actix"], "multi"),
    ("python ORM lightweight not sqlalchemy", ["python", "lightweight"], ["sqlalchemy"], "multi"),
    ("javascript typescript bundler fast not webpack", ["javascript", "bundler", "fast"], ["webpack"], "multi"),
    ("linux distro lightweight not ubuntu", ["linux", "lightweight"], ["ubuntu"], "multi"),
    ("java web framework modern not spring", ["java", "web", "modern"], ["spring"], "multi"),
]

NEGATIVE_QUERIES = [
    # Various negative markers
    ("python web framework not django", None, ["django"], "not"),
    ("javascript framework except react", None, ["react"], "except"),
    ("text editor without vim", None, ["vim"], "without"),
    ("css framework no bootstrap", None, ["bootstrap"], "no"),
    ("programming language other than java", None, ["java"], "other than"),
    ("search engine alternative to google", None, ["google"], "alternative to"),
    ("static site generator instead of jekyll", None, ["jekyll"], "instead of"),
    ("javascript framework besides react", None, ["react"], "besides"),
    ("database minus mongodb", None, ["mongodb"], "minus"),
    ("python web framework excluding django", None, ["django"], "excluding"),
    ("os but not windows", None, ["windows"], "but not"),
    ("frontend framework minus angular", None, ["angular"], "minus"),
]

EDGE_CASES = [
    ("", "empty"),
    ("a", "single-char"),
    ("   ", "whitespace"),
    ("the the the", "stop-words-only"),
    ("a]b[c{d}e", "special-chars"),
    ("x" * 500, "very-long"),
    ("python 🔥 programming", "emoji"),
    ("PYTHON WEB FRAMEWORK", "all-caps"),
    ("python web framework", "repeated-1"),
    ("python web framework", "repeated-2"),
    ("C++ programming language", "c++"),
    ("Node.js vs Deno vs Bun", "dots-in-names"),
    ("what is the best way to learn python?", "question-form"),
    ("how does a neural network work?", "how-question"),
]

# ═══════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════

def main():
    print("=" * 70)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("=" * 70)

    # Health check
    try:
        r = requests.get(f"{BASE_URL}/health", timeout=5)
        assert r.status_code == 200
        print("  Health: OK")
    except Exception as e:
        print(f"  FATAL: Gateway not healthy: {e}")
        sys.exit(1)

    total_pass = 0
    total_fail = 0
    all_latencies = []
    all_issues = []

    # ─── 1. GENERAL QUERIES ──────────────────────────────────
    print_header("1. GENERAL QUERIES (intent detection + result count)")
    for q, expected_intent in GENERAL_QUERIES:
        r = run_query(q, "general")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        intent_match = "  " if r.intent == expected_intent else f" (expected {expected_intent})"
        print_result(r)
        if intent_match.strip():
            print(f"         Intent mismatch: got '{r.intent}', expected '{expected_intent}'")
            all_issues.append(f"General: '{q}' — intent '{r.intent}' != '{expected_intent}'")

    # ─── 2. CONSTRAINT QUERIES (positive extraction) ─────────
    print_header("2. POSITIVE CONSTRAINT QUERIES")
    for q, expect_pos, expect_neg, cat in CONSTRAINT_QUERIES:
        r = run_query(q, "constraint")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        issues = print_result(r, check_pos=expect_pos, check_neg=expect_neg)
        all_issues.extend(issues)

    # ─── 3. MULTI-CONSTRAINT QUERIES (positive + negative) ───
    print_header("3. MULTI-CONSTRAINT QUERIES (positive + negative combined)")
    for q, expect_pos, expect_neg, cat in MULTI_CONSTRAINT_QUERIES:
        r = run_query(q, "multi-constraint")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        issues = print_result(r, check_pos=expect_pos, check_neg=expect_neg)
        all_issues.extend(issues)
        # Check negative violations in top-10
        if ok and expect_neg:
            violations, total_checked = check_neg_violations(r)
            if violations > 0:
                print(f"         *** {violations}/{total_checked} top-10 results violate negative constraints")
                all_issues.append(f"Multi: '{q}' — {violations}/{total_checked} neg violations")

    # ─── 4. NEGATIVE CONSTRAINT QUERIES (various markers) ────
    print_header("4. NEGATIVE CONSTRAINT QUERIES (various markers)")
    neg_total_violations = 0
    neg_total_checked = 0
    for q, expect_pos, expect_neg, marker_type in NEGATIVE_QUERIES:
        r = run_query(q, f"neg-{marker_type}")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        issues = print_result(r, check_neg=expect_neg)
        all_issues.extend(issues)
        if ok and expect_neg:
            violations, total_checked = check_neg_violations(r)
            neg_total_violations += violations
            neg_total_checked += total_checked
            if violations > 0:
                print(f"         *** {violations}/{total_checked} neg violations in top-10")

    if neg_total_checked > 0:
        violation_rate = neg_total_violations / neg_total_checked * 100
        print(f"\n  NEGATIVE CONSTRAINT VIOLATION RATE: {neg_total_violations}/{neg_total_checked} = {violation_rate:.1f}%")

    # ─── 5. DEEP QUALITY AUDIT ────────────────────────────────
    print_header("5. DEEP QUALITY AUDIT (top-5 result relevance)")
    audit_queries = [
        ("python web framework not django", "technical"),
        ("javascript runtime not node", "technical"),
        ("rust async web framework", "technical"),
        ("css framework no bootstrap", "technical"),
        ("linux distro not ubuntu", "technical"),
        ("fast python web framework not django", "technical"),
        ("lightweight javascript bundler", "technical"),
        ("modern css framework responsive", "technical"),
    ]
    for q, expected_intent in audit_queries:
        r = run_query(q, "audit")
        if r.error:
            print(f"  FAIL: {q} — {r.error}")
            total_fail += 1
            continue
        total_pass += 1
        all_latencies.append(r.response_time_ms)
        results = r.raw.get("results", [])[:5]
        print(f"\n  Query: {q}")
        print(f"  Intent: {r.intent} (conf={r.confidence:.2f}) | Neg: {r.constraints_negative}")
        for i, res in enumerate(results):
            title = res.get("title", "")[:70]
            score = res.get("score", 0)
            url = res.get("url", "")[:80]
            print(f"    [{i+1}] score={score:.3f} | {title}")
            print(f"         {url}")

    # ─── 6. EDGE CASES ───────────────────────────────────────
    print_header("6. EDGE CASES")
    for q, desc in EDGE_CASES:
        r = run_query(q, f"edge-{desc}")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        status = "PASS" if ok else "FAIL"
        print(f"  [{status:4s}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"res={r.num_results:3d} | {desc}: '{q[:60]}'")
        if r.error:
            print(f"         ERROR: {r.error}")

    # ─── 7. STRESS TEST — CACHED (same query) ────────────────
    print_header("7. STRESS TEST — CACHED (same query, high concurrency)")
    for concurrency in [5, 10, 20]:
        num_requests = concurrency * 2
        query = "python programming"
        latencies = []
        errors = 0
        start_wall = time.time()
        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [executor.submit(run_query, query, f"stress-c-{i}") for i in range(num_requests)]
            for f in as_completed(futures):
                r = f.result()
                if r.status_code == 200 and not r.error:
                    latencies.append(r.response_time_ms)
                else:
                    errors += 1
        wall_time = time.time() - start_wall
        if latencies:
            ls = sorted(latencies)
            n = len(ls)
            throughput = num_requests / wall_time
            print(f"  Concurrency={concurrency:2d}: throughput={throughput:.0f} req/s | "
                  f"p50={ls[n//2]:.0f}ms p95={ls[int(n*0.95)]:.0f}ms max={ls[-1]:.0f}ms | "
                  f"ok={len(latencies)}/{num_requests} err={errors}")
        else:
            print(f"  Concurrency={concurrency:2d}: ALL FAILED ({errors} errors)")

    # ─── 8. STRESS TEST — UNIQUE (no cache, concurrent) ──────
    print_header("8. STRESS TEST — UNIQUE QUERIES (no cache, concurrent)")
    unique_queries = [
        "python async web framework 2026",
        "javascript bundler comparison",
        "rust programming getting started",
        "machine learning for beginners",
        "docker compose best practices",
        "kubernetes vs docker swarm",
        "typescript strict mode guide",
        "golang concurrency patterns",
        "react server components",
        "nextjs app router tutorial",
    ]
    for concurrency in [3, 5, 10]:
        subset = unique_queries[:concurrency]
        latencies = []
        errors = 0
        start_wall = time.time()
        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [executor.submit(run_query, q, f"stress-u-{i}") for i, q in enumerate(subset)]
            for f in as_completed(futures):
                r = f.result()
                if r.status_code == 200 and not r.error:
                    latencies.append(r.response_time_ms)
                else:
                    errors += 1
        wall_time = time.time() - start_wall
        if latencies:
            ls = sorted(latencies)
            n = len(ls)
            throughput = len(subset) / wall_time
            contention = ls[-1] / ls[0] if ls[0] > 0 else 0
            print(f"  Concurrency={concurrency:2d}: throughput={throughput:.1f} req/s | "
                  f"p50={ls[n//2]:.0f}ms p95={ls[int(n*0.95)]:.0f}ms max={ls[-1]:.0f}ms | "
                  f"contention={contention:.1f}x | ok={len(latencies)}/{len(subset)} err={errors}")
        else:
            print(f"  Concurrency={concurrency:2d}: ALL FAILED ({errors} errors)")

    # ─── 9. STRESS TEST — BURST (rapid fire) ─────────────────
    print_header("9. STRESS TEST — BURST (20 rapid sequential)")
    burst_latencies = []
    burst_errors = 0
    for i in range(20):
        q = f"test query number {i} about programming"
        r = run_query(q, f"burst-{i}")
        if r.status_code == 200 and not r.error:
            burst_latencies.append(r.response_time_ms)
        else:
            burst_errors += 1
    if burst_latencies:
        ls = sorted(burst_latencies)
        n = len(ls)
        print(f"  Sequential burst: avg={statistics.mean(ls):.0f}ms | "
              f"p50={ls[n//2]:.0f}ms p95={ls[int(n*0.95)]:.0f}ms max={ls[-1]:.0f}ms | "
              f"ok={len(burst_latencies)}/20 err={burst_errors}")

    # ─── SUMMARY ──────────────────────────────────────────────
    print_header("SUMMARY")
    print(f"  Total PASS: {total_pass}")
    print(f"  Total FAIL: {total_fail}")
    if all_latencies:
        ls = sorted(all_latencies)
        n = len(ls)
        print(f"  Latency (all queries): avg={statistics.mean(ls):.0f}ms | "
              f"p50={ls[n//2]:.0f}ms p95={ls[int(n*0.95)]:.0f}ms p99={ls[int(n*0.99)]:.0f}ms max={ls[-1]:.0f}ms")
    if all_issues:
        print(f"\n  ISSUES FOUND ({len(all_issues)}):")
        for iss in all_issues:
            print(f"    - {iss}")
    else:
        print(f"\n  NO ISSUES FOUND — all checks passed!")

    print(f"\n{'='*70}")

if __name__ == "__main__":
    main()
