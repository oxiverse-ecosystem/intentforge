#!/usr/bin/env python3
"""
IntentForge v2 — Comprehensive API Test Suite
Tests: general, constraints, multi-constraints, negative constraints, stress, bottleneck analysis
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
HEALTH_URL = f"{BASE_URL}/health"
TIMEOUT = 40

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
    result = TestResult(name=name, query=query[:120])
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
    print(f"\n{'='*72}")
    print(f"  {title}")
    print(f"{'='*72}")

def check_neg_violations(r: TestResult, top_n=10):
    """Check top-N results for negative constraint violations (strict)."""
    if not r.raw or not r.constraints_negative:
        return 0, 0, []
    results = r.raw.get("results", [])[:top_n]
    violations = []
    for i, res in enumerate(results):
        title = (res.get("title", "") + " " + res.get("content", "") + " " + res.get("url", "")).lower()
        for neg in r.constraints_negative:
            if neg.lower() in title:
                violations.append((i+1, neg, res.get("title","")[:60]))
                break
    return len(violations), len(results), violations

def check_top5_relevance(r: TestResult, topic_keywords):
    """Check top-5 results for topical relevance."""
    if not r.raw:
        return 0, 0
    results = r.raw.get("results", [])[:5]
    relevant = 0
    for res in results:
        text = (res.get("title", "") + " " + res.get("content", "") + " " + res.get("url", "")).lower()
        if any(kw.lower() in text for kw in topic_keywords):
            relevant += 1
    return relevant, len(results)

# ═══════════════════════════════════════════════════════════════
# TEST DATA
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
    ("what is quantum computing", "informational"),
    ("best laptop for programming 2026", "comparison"),
    ("how to set up a home server", "how-to"),
]

CONSTRAINT_QUERIES = [
    ("fast web framework for python", ["python", "web"], None),
    ("lightweight javascript bundler", ["javascript"], None),
    ("modern css framework responsive", ["css", "responsive"], None),
    ("async python web framework", ["python", "web"], None),
    ("secure authentication library nodejs", ["nodejs"], None),
    ("rust async web framework", ["rust", "web"], None),
    ("typescript SSR framework", ["typescript"], None),
    ("cross-platform mobile framework flutter", ["flutter"], None),
    ("minimalist python testing library", ["python", "testing"], None),
    ("high-performance database sqlite alternative", ["database"], None),
]

MULTI_CONSTRAINT_QUERIES = [
    ("fast python web framework not django", ["python", "web"], ["django"]),
    ("lightweight javascript framework not react", ["javascript"], ["react"]),
    ("modern css framework no bootstrap responsive", ["css", "responsive"], ["bootstrap"]),
    ("rust web framework async not actix", ["rust", "web"], ["actix"]),
    ("python ORM lightweight not sqlalchemy", ["python"], ["sqlalchemy"]),
    ("javascript bundler fast not webpack", ["javascript", "bundler"], ["webpack"]),
    ("linux distro lightweight not ubuntu", ["linux"], ["ubuntu"]),
    ("java web framework modern not spring", ["java", "web"], ["spring"]),
    ("database fast not mysql", ["database", "fast"], ["mysql"]),
    ("frontend framework not react not angular", ["frontend"], ["react", "angular"]),
]

NEGATIVE_QUERIES = [
    ("python web framework not django", ["django"], "not"),
    ("javascript framework except react", ["react"], "except"),
    ("text editor without vim", ["vim"], "without"),
    ("css framework no bootstrap", ["bootstrap"], "no"),
    ("programming language other than java", ["java"], "other than"),
    ("search engine alternative to google", ["google"], "alternative to"),
    ("static site generator instead of jekyll", ["jekyll"], "instead of"),
    ("javascript framework besides react", ["react"], "besides"),
    ("database minus mongodb", ["mongodb"], "minus"),
    ("python web framework excluding django", ["django"], "excluding"),
    ("os but not windows", ["windows"], "but not"),
    ("frontend framework minus angular", ["angular"], "minus"),
]

COMPLEX_QUERIES = [
    # Complex multi-constraint with specificity
    ("fast lightweight python web framework with async support not django not flask", ["python", "web", "async"], ["django", "flask"]),
    ("modern react alternative with server side rendering not nextjs", ["react", "server"], ["nextjs"]),
    ("rust web framework with websocket support not actix not warp", ["rust", "web", "websocket"], ["actix", "warp"]),
    ("python machine learning library for nlp not tensorflow not pytorch", ["python", "machine learning", "nlp"], ["tensorflow", "pytorch"]),
    ("lightweight linux distro for old laptops not ubuntu not debian", ["linux", "lightweight"], ["ubuntu", "debian"]),
    ("javascript typescript build tool fast not webpack not rollup not vite", ["javascript", "build"], ["webpack", "rollup", "vite"]),
    ("database for time series data fast not influxdb not prometheus", ["database", "time series"], ["influxdb", "prometheus"]),
    ("golang web framework with middleware not gin not echo", ["golang", "web"], ["gin", "echo"]),
]

EDGE_CASES = [
    ("", "empty"),
    ("a", "single-char"),
    ("   ", "whitespace-only"),
    ("the the the", "stopwords-only"),
    ("a]b[c{d}e", "special-chars"),
    ("x" * 500, "500-char-query"),
    ("x" * 1000, "1000-char-query"),
    ("python 🔥 programming", "emoji"),
    ("PYTHON WEB FRAMEWORK", "ALL-CAPS"),
    ("C++ programming language", "c++"),
    ("Node.js vs Deno vs Bun", "dots-in-names"),
    ("what is the best way to learn python?", "question-form"),
    ("how does a neural network work?", "how-question"),
    ("SELECT * FROM users WHERE id=1", "sql-injection-attempt"),
    ("<script>alert(1)</script>", "xss-attempt"),
    ("python\x00null\x00byte", "null-bytes"),
    ("framework for Python web apps", "natural-english"),
    ("q" * 50, "repeated-char"),
]

# ═══════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════

def main():
    print("=" * 72)
    print("  IntentForge v2 — Comprehensive API Test Suite")
    print("  " + time.strftime("%Y-%m-%d %H:%M:%S"))
    print("=" * 72)

    # Health check
    try:
        r = requests.get(HEALTH_URL, timeout=5)
        assert r.status_code == 200
        print(f"  Health: OK (status {r.status_code})")
    except Exception as e:
        print(f"  FATAL: Gateway not healthy: {e}")
        sys.exit(1)

    total_pass = 0
    total_fail = 0
    all_latencies = []
    all_issues = []

    # ─── 1. GENERAL QUERIES ──────────────────────────────────
    print_header("1. GENERAL QUERIES (intent detection)")
    intent_matches = 0
    intent_mismatches = 0
    for q, expected_intent in GENERAL_QUERIES:
        r = run_query(q, "general")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        intent_ok = r.intent == expected_intent
        if intent_ok:
            intent_matches += 1
        else:
            intent_mismatches += 1
            all_issues.append(f"General: '{q}' — got '{r.intent}', expected '{expected_intent}'")
        status = "PASS" if ok else "FAIL"
        match_mark = "  " if intent_ok else "!!"
        print(f"  [{status}] {match_mark} {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"res={r.num_results:3d} | {q}")
        if not intent_ok:
            print(f"         Intent mismatch: got '{r.intent}', expected '{expected_intent}'")
    print(f"\n  Intent accuracy: {intent_matches}/{intent_matches+intent_mismatches} ({intent_matches/(intent_matches+intent_mismatches)*100:.0f}%)")

    # ─── 2. POSITIVE CONSTRAINT QUERIES ─────────────────────
    print_header("2. POSITIVE CONSTRAINT QUERIES (constraint extraction)")
    pos_ok = 0
    pos_total = 0
    for q, expect_pos, _ in CONSTRAINT_QUERIES:
        r = run_query(q, "pos-constraint")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        # Check expected positive constraints
        if ok and expect_pos:
            for ep in expect_pos:
                pos_total += 1
                found = any(ep.lower() in p.lower() for p in r.constraints_positive)
                if found:
                    pos_ok += 1
                else:
                    all_issues.append(f"Pos: '{q}' — expected positive '{ep}' not found in {r.constraints_positive}")
        status = "PASS" if ok else "FAIL"
        pos_str = ",".join(r.constraints_positive[:5]) if r.constraints_positive else "-"
        neg_str = ",".join(r.constraints_negative[:3]) if r.constraints_negative else "-"
        print(f"  [{status}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} | pos=[{pos_str:30s}] neg=[{neg_str:10s}] | {q[:60]}")
    if pos_total > 0:
        print(f"\n  Positive constraint recall: {pos_ok}/{pos_total} ({pos_ok/pos_total*100:.0f}%)")

    # ─── 3. MULTI-CONSTRAINT QUERIES (pos + neg combined) ───
    print_header("3. MULTI-CONSTRAINT QUERIES (positive + negative)")
    multi_violations_total = 0
    multi_checked_total = 0
    for q, expect_pos, expect_neg in MULTI_CONSTRAINT_QUERIES:
        r = run_query(q, "multi")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        status = "PASS" if ok else "FAIL"
        neg_str = ",".join(r.constraints_negative) if r.constraints_negative else "-"
        pos_str = ",".join(r.constraints_positive[:5]) if r.constraints_positive else "-"
        print(f"  [{status}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"pos=[{pos_str:30s}] neg=[{neg_str:15s}] | {q[:55]}")

        # Deep check: expected negatives
        if ok and expect_neg:
            for en in expect_neg:
                found = any(en.lower() in n.lower() for n in r.constraints_negative)
                if not found:
                    all_issues.append(f"Multi: '{q}' — expected negative '{en}' not found in {r.constraints_negative}")
                    print(f"         *** Expected negative '{en}' NOT detected")

            # Check violations in top-10
            violations, total_checked, v_details = check_neg_violations(r)
            multi_violations_total += violations
            multi_checked_total += total_checked
            if violations > 0:
                print(f"         *** {violations}/{total_checked} results violate neg constraints:")
                for vi, vneg, vtitle in v_details[:3]:
                    print(f"             [{vi}] contains '{vneg}': {vtitle}")

    if multi_checked_total > 0:
        vr = multi_violations_total / multi_checked_total * 100
        print(f"\n  Multi-constraint violation rate: {multi_violations_total}/{multi_checked_total} = {vr:.1f}%")

    # ─── 4. NEGATIVE CONSTRAINT VARIETY ─────────────────────
    print_header("4. NEGATIVE CONSTRAINT VARIETY (12 marker types)")
    neg_violations_total = 0
    neg_checked_total = 0
    neg_detected = 0
    neg_missed = 0
    for q, expect_neg, marker in NEGATIVE_QUERIES:
        r = run_query(q, f"neg-{marker}")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        status = "PASS" if ok else "FAIL"
        neg_str = ",".join(r.constraints_negative) if r.constraints_negative else "NONE"
        print(f"  [{status}] {r.response_time_ms:6.0f}ms | neg=[{neg_str:20s}] marker={marker:12s} | {q[:55]}")

        if ok and expect_neg:
            for en in expect_neg:
                found = any(en.lower() in n.lower() for n in r.constraints_negative)
                if found:
                    neg_detected += 1
                else:
                    neg_missed += 1
                    all_issues.append(f"Neg-{marker}: '{q}' — '{en}' NOT detected (got: {r.constraints_negative})")
                    print(f"         *** '{en}' NOT detected as negative!")

            violations, total_checked, v_details = check_neg_violations(r)
            neg_violations_total += violations
            neg_checked_total += total_checked
            if violations > 0:
                print(f"         *** {violations}/{total_checked} violations in top-10")

    neg_total_checks = neg_detected + neg_missed
    if neg_total_checks > 0:
        print(f"\n  Negative detection rate: {neg_detected}/{neg_total_checks} ({neg_detected/neg_total_checks*100:.0f}%)")
    if neg_checked_total > 0:
        vr = neg_violations_total / neg_checked_total * 100
        print(f"  Negative violation rate: {neg_violations_total}/{neg_checked_total} = {vr:.1f}%")

    # ─── 5. COMPLEX MULTI-NEGATIVE QUERIES ──────────────────
    print_header("5. COMPLEX MULTI-NEGATIVE QUERIES (multiple exclusions)")
    for q, expect_pos, expect_neg in COMPLEX_QUERIES:
        r = run_query(q, "complex")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        status = "PASS" if ok else "FAIL"
        neg_str = ",".join(r.constraints_negative) if r.constraints_negative else "NONE"
        pos_str = ",".join(r.constraints_positive[:4]) if r.constraints_positive else "-"
        print(f"  [{status}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"pos=[{pos_str:25s}] neg=[{neg_str:25s}] | {q[:50]}")

        if ok and expect_neg:
            for en in expect_neg:
                found = any(en.lower() in n.lower() for n in r.constraints_negative)
                if not found:
                    all_issues.append(f"Complex: '{q}' — neg '{en}' not detected")
                    print(f"         *** Expected neg '{en}' NOT detected")

            violations, total_checked, v_details = check_neg_violations(r)
            if violations > 0:
                print(f"         *** {violations}/{total_checked} violations:")
                for vi, vneg, vtitle in v_details[:3]:
                    print(f"             [{vi}] '{vneg}': {vtitle}")

    # ─── 6. DEEP QUALITY AUDIT (top-5 per result) ───────────
    print_header("6. DEEP QUALITY AUDIT (top-5 relevance per query)")
    audit_queries = [
        ("python web framework not django", ["python", "web", "framework"], ["django"]),
        ("rust async web framework", ["rust", "web"], []),
        ("javascript runtime not node", ["javascript", "runtime"], ["node"]),
        ("css framework no bootstrap", ["css", "framework"], ["bootstrap"]),
        ("linux distro not ubuntu", ["linux", "distro"], ["ubuntu"]),
        ("fast python web framework not django not flask", ["python", "web"], ["django", "flask"]),
        ("lightweight javascript bundler", ["javascript", "bundler"], []),
        ("database for time series not influxdb", ["database", "time series"], ["influxdb"]),
    ]
    total_relevant = 0
    total_audited = 0
    total_audit_violations = 0
    for q, topic_kw, expect_neg in audit_queries:
        r = run_query(q, "audit")
        if r.error:
            print(f"  FAIL: {q} — {r.error}")
            total_fail += 1
            continue
        total_pass += 1
        all_latencies.append(r.response_time_ms)
        results = r.raw.get("results", [])[:5]

        # Topical relevance
        relevant, count = check_top5_relevance(r, topic_kw)
        total_relevant += relevant
        total_audited += count

        # Neg violations
        violations, _, v_details = check_neg_violations(r, top_n=5)
        total_audit_violations += violations

        print(f"\n  Query: {q}")
        print(f"  Intent: {r.intent} (conf={r.confidence:.2f}) | Neg: {r.constraints_negative}")
        print(f"  Relevance: {relevant}/{count} | Violations: {violations}/{count}")
        for i, res in enumerate(results):
            title = res.get("title", "")[:70]
            score = res.get("score", 0)
            url = res.get("url", "")[:80]
            # Check if violates any negative
            text = (title + " " + res.get("content","") + " " + url).lower()
            neg_hit = any(neg.lower() in text for neg in expect_neg) if expect_neg else False
            rel_hit = any(kw.lower() in text for kw in topic_kw)
            marks = ""
            if neg_hit:
                marks += " [NEG-VIOLATION]"
            if not rel_hit:
                marks += " [OFF-TOPIC]"
            print(f"    [{i+1}] score={score:.3f} | {title}{marks}")
            print(f"         {url}")

    if total_audited > 0:
        print(f"\n  Top-5 relevance: {total_relevant}/{total_audited} ({total_relevant/total_audited*100:.0f}%)")
        print(f"  Top-5 violation rate: {total_audit_violations}/{total_audited} ({total_audit_violations/total_audited*100:.0f}%)")

    # ─── 7. EDGE CASES ──────────────────────────────────────
    print_header("7. EDGE CASES (stability under unusual input)")
    for q, desc in EDGE_CASES:
        r = run_query(q, f"edge-{desc}")
        ok = r.status_code == 200 and not r.error
        if ok:
            total_pass += 1
            all_latencies.append(r.response_time_ms)
        else:
            total_fail += 1
        status = "PASS" if ok else "FAIL"
        q_display = q[:60] if len(q) < 60 else q[:30] + "..." + q[-27:]
        q_display = q_display.replace("\n","\\n").replace("\r","\\r").replace("\x00","\\0")
        print(f"  [{status}] {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"res={r.num_results:3d} | {desc:18s} | {q_display}")
        if r.error:
            print(f"         ERROR: {r.error}")

    # ─── 8. STRESS TEST — CACHED (same query) ───────────────
    print_header("8. STRESS TEST — CACHED (same query, ramping concurrency)")
    # Warm cache first
    run_query("python programming", "warmup")
    time.sleep(0.5)
    for concurrency in [5, 10, 20, 30]:
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
            print(f"  c={concurrency:2d}: throughput={throughput:5.1f} req/s | "
                  f"p50={ls[n//2]:.0f}ms p95={ls[int(n*0.95)]:.0f}ms p99={ls[min(int(n*0.99),n-1)]:.0f}ms max={ls[-1]:.0f}ms | "
                  f"ok={len(latencies)}/{num_requests} err={errors}")
        else:
            print(f"  c={concurrency:2d}: ALL FAILED ({errors} errors)")

    # ─── 9. STRESS TEST — UNIQUE (cache-busting) ────────────
    print_header("9. STRESS TEST — UNIQUE QUERIES (no cache benefit)")
    for concurrency in [3, 5, 10, 15]:
        queries = [f"unique stress test {i} about programming {time.time()}" for i in range(concurrency)]
        latencies = []
        errors = 0
        start_wall = time.time()
        with ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [executor.submit(run_query, q, f"stress-u-{i}") for i, q in enumerate(queries)]
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
            throughput = len(queries) / wall_time
            contention = ls[-1] / ls[0] if ls[0] > 0 else 0
            print(f"  c={concurrency:2d}: throughput={throughput:.1f} req/s | "
                  f"p50={ls[n//2]:.0f}ms p95={ls[min(int(n*0.95),n-1)]:.0f}ms max={ls[-1]:.0f}ms | "
                  f"contention={contention:.1f}x | ok={len(latencies)}/{len(queries)} err={errors}")
        else:
            print(f"  c={concurrency:2d}: ALL FAILED ({errors} errors)")

    # ─── 10. STRESS TEST — BURST (rapid sequential) ─────────
    print_header("10. STRESS TEST — BURST (30 rapid sequential)")
    burst_latencies = []
    burst_errors = 0
    for i in range(30):
        q = f"stress burst query {i} about {['python','rust','golang','java','javascript'][i%5]} programming {time.time()}"
        r = run_query(q, f"burst-{i}")
        if r.status_code == 200 and not r.error:
            burst_latencies.append(r.response_time_ms)
        else:
            burst_errors += 1
    if burst_latencies:
        ls = sorted(burst_latencies)
        n = len(ls)
        print(f"  Sequential burst ({n}/30 ok, {burst_errors} err):")
        print(f"    avg={statistics.mean(ls):.0f}ms | p50={ls[n//2]:.0f}ms | "
              f"p95={ls[int(n*0.95)]:.0f}ms | p99={ls[min(int(n*0.99),n-1)]:.0f}ms | max={ls[-1]:.0f}ms")
        # Detect degradation pattern
        first_10 = statistics.mean(ls[:10]) if n >= 10 else 0
        last_10 = statistics.mean(ls[-10:]) if n >= 10 else 0
        if first_10 > 0:
            degradation = last_10 / first_10
            print(f"    First-10 avg: {first_10:.0f}ms | Last-10 avg: {last_10:.0f}ms | Degradation: {degradation:.2f}x")
            if degradation > 1.5:
                print(f"    *** LATENCY DEGRADATION DETECTED over burst!")
                all_issues.append(f"Burst degradation: {degradation:.2f}x ({first_10:.0f}ms -> {last_10:.0f}ms)")

    # ─── 11. BOTTLENECK ANALYSIS — Latency Breakdown ────────
    print_header("11. BOTTLENECK ANALYSIS")
    print("\n  Testing sequential queries with timing analysis...")
    bottleneck_queries = [
        "simple query",
        "python web framework not django",
        "fast lightweight python web framework with async support not django not flask",
        "rust web framework async not actix",
        "x" * 200,
    ]
    for q in bottleneck_queries:
        r = run_query(q, "bottleneck")
        if r.error:
            print(f"  FAIL: {q[:50]} — {r.error}")
            continue
        results = r.raw.get("results", [])
        print(f"  {r.response_time_ms:6.0f}ms | {r.intent:<14s} conf={r.confidence:.2f} | "
              f"results={len(results):3d} | expanded={len(r.expanded_queries)} | "
              f"pos={len(r.constraints_positive)} neg={len(r.constraints_negative)} | {q[:50]}")
        all_latencies.append(r.response_time_ms)

    # Concurrent degradation test
    print("\n  Concurrent degradation curve (1->2->5->10->20 unique queries):")
    for c in [1, 2, 5, 10, 20]:
        queries = [f"degradation test {i} about {['python','rust','javascript','golang','java'][i%5]} {time.time()}" for i in range(c)]
        lats = []
        start_wall = time.time()
        with ThreadPoolExecutor(max_workers=c) as executor:
            futures = [executor.submit(run_query, q, f"deg-{i}") for i, q in enumerate(queries)]
            for f in as_completed(futures):
                r = f.result()
                if r.status_code == 200 and not r.error:
                    lats.append(r.response_time_ms)
        wall_time = time.time() - start_wall
        if lats:
            ls = sorted(lats)
            n = len(ls)
            tp = len(lats) / wall_time
            print(f"    c={c:2d}: p50={ls[n//2]:.0f}ms p95={ls[min(int(n*0.95),n-1)]:.0f}ms max={ls[-1]:.0f}ms | "
                  f"throughput={tp:.1f} req/s | ok={len(lats)}/{c}")

    # ─── 12. EXPANDED QUERY QUALITY ─────────────────────────
    print_header("12. EXPANDED QUERY ANALYSIS")
    expand_queries = [
        "python web framework",
        "how to learn machine learning",
        "best javascript bundler 2026",
        "rust vs go performance",
    ]
    for q in expand_queries:
        r = run_query(q, "expand")
        if r.error:
            print(f"  FAIL: {q} — {r.error}")
            continue
        all_latencies.append(r.response_time_ms)
        expanded = r.expanded_queries
        print(f"  Query: {q}")
        print(f"  Intent: {r.intent} (conf={r.confidence:.2f})")
        print(f"  Expanded ({len(expanded)}): {expanded}")
        print()

    # ─── SUMMARY ─────────────────────────────────────────────
    print_header("SUMMARY")
    print(f"  Total PASS: {total_pass}")
    print(f"  Total FAIL: {total_fail}")
    print(f"  Pass rate:  {total_pass/(total_pass+total_fail)*100:.1f}%")
    if all_latencies:
        ls = sorted(all_latencies)
        n = len(ls)
        print(f"\n  Latency (all {n} queries):")
        print(f"    avg={statistics.mean(ls):.0f}ms | p50={ls[n//2]:.0f}ms | "
              f"p95={ls[int(n*0.95)]:.0f}ms | p99={ls[min(int(n*0.99),n-1)]:.0f}ms | max={ls[-1]:.0f}ms")
        print(f"    min={ls[0]:.0f}ms | stdev={statistics.stdev(ls):.0f}ms" if n > 1 else "")

    if all_issues:
        print(f"\n  ISSUES FOUND ({len(all_issues)}):")
        for i, iss in enumerate(all_issues, 1):
            print(f"    {i:2d}. {iss}")
    else:
        print(f"\n  NO ISSUES FOUND — all checks passed!")

    print(f"\n{'='*72}")

if __name__ == "__main__":
    main()
