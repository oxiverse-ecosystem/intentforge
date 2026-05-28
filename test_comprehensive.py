#!/usr/bin/env python3
"""Comprehensive IntentForge API test suite — timing, constraints, stress."""

import requests
import time
import json
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

API = "http://localhost:4000/search"

# ── Test categories ──────────────────────────────────────────────────────────

GENERAL_QUERIES = [
    "python programming",
    "machine learning tutorials",
    "rust lang documentation",
    "javascript async await",
    "golang concurrency patterns",
]

COMPLEX_QUERIES = [
    "how to implement distributed consensus in a fault tolerant system",
    "best practices for microservices observability with open telemetry",
    "comparing rust vs go for systems programming 2026",
    "building real time collaborative editors with crdts",
    "zero trust architecture implementation guide for enterprises",
]

POSITIVE_CONSTRAINT_QUERIES = [
    {"query": "python -tutorial -beginner", "name": "single_negative"},
    {"query": "rust -cargo -crate", "name": "single_negative2"},
    {"query": "machine learning -tensorflow -pytorch", "name": "double_negative"},
    {"query": "docker -container -kubernetes -helm", "name": "triple_negative"},
    {"query": "java -coffee -island", "name": "ambiguous_negative"},
    {"query": "ruby -gem -jewelry", "name": "ambiguous_negative2"},
    {"query": "swift -bird -taylor", "name": "ambiguous_negative3"},
    {"query": "python -snake -monty", "name": "ambiguous_negative4"},
    {"query": "rust -oxidize -corrosion", "name": "ambiguous_negative5"},
    {"query": "go -golang -board -game", "name": "mixed_negative"},
]

MULTI_CONSTRAINT_QUERIES = [
    {"query": "python django REST framework -flask -fastapi", "name": "framework_exclude"},
    {"query": "react hooks typescript -class -javascript", "name": "multi_constraint"},
    {"query": "kubernetes deployment -helm -ship -minikube", "name": "k8s_negative3"},
    {"query": "git rebase merge -svn -perforce", "name": "vcs_negative"},
    {"query": "nginx reverse proxy -apache -caddy -traefik", "name": "webserver_exclude"},
]

STRESS_QUERIES = [
    "a",
    "the",
    "x",
    "test",
    "how to",
    "what is",
    "best",
    "tutorial",
    "programming",
    "examples",
    "a" * 200,
    "python " * 50,
    "special chars: @#$%^&*()_+{}|:<>?",
    "unicode: 你好世界 مرحبا",
    "",
]


def run_query(query, timeout=30):
    """Run a single query and return timing + result count."""
    start = time.time()
    try:
        resp = requests.get(API, params={"q": query}, timeout=timeout)
        elapsed = time.time() - start
        if resp.status_code == 200:
            data = resp.json()
            count = len(data.get("web_results", data.get("results", [])))
            return {"ok": True, "count": count, "time": elapsed, "status": 200, "query": query}
        else:
            return {"ok": False, "count": 0, "time": elapsed, "status": resp.status_code, "query": query, "error": resp.text[:200]}
    except requests.Timeout:
        return {"ok": False, "count": 0, "time": time.time() - start, "status": "TIMEOUT", "query": query}
    except Exception as e:
        return {"ok": False, "count": 0, "time": time.time() - start, "status": "ERROR", "query": query, "error": str(e)[:200]}


def print_result(r, prefix=""):
    status = "OK" if r["ok"] else f"FAIL({r['status']})"
    print(f"  {prefix}[{status}] {r['time']:.2f}s | {r['count']:>3} results | {r['query'][:60]}")
    if not r["ok"] and "error" in r:
        print(f"         ERROR: {r['error'][:120]}")


def run_section(name, queries, timeout=30):
    print(f"\n{'='*70}")
    print(f"  {name}")
    print(f"{'='*70}")
    results = []
    for q in queries:
        if isinstance(q, dict):
            r = run_query(q["query"], timeout)
            r["test_name"] = q["name"]
            print_result(r, f"[{q['name']}] ")
        else:
            r = run_query(q, timeout)
            print_result(r)
        results.append(r)
    return results


def run_stress_test(concurrency=5):
    print(f"\n{'='*70}")
    print(f"  STRESS TEST ({concurrency} concurrent)")
    print(f"{'='*70}")
    # Warmup
    run_query("warmup")
    
    start = time.time()
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = {pool.submit(run_query, q, 30): q for q in STRESS_QUERIES}
        results = []
        for f in as_completed(futures):
            r = f.result()
            print_result(r)
            results.append(r)
    total = time.time() - start
    
    ok = [r for r in results if r["ok"]]
    fail = [r for r in results if not r["ok"]]
    times = [r["time"] for r in ok]
    
    print(f"\n  Stress Summary:")
    print(f"    Total time: {total:.2f}s")
    print(f"    OK: {len(ok)} / FAIL: {len(fail)}")
    if times:
        print(f"    Avg: {sum(times)/len(times):.2f}s | Min: {min(times):.2f}s | Max: {max(times):.2f}s")
        print(f"    P50: {sorted(times)[len(times)//2]:.2f}s | P95: {sorted(times)[int(len(times)*0.95)]:.2f}s")
    return results


def main():
    print("IntentForge Comprehensive API Test Suite")
    print(f"API: {API}")
    print(f"Time: {time.strftime('%Y-%m-%d %H:%M:%S')}")
    
    # Flush Redis cache first
    try:
        import subprocess
        subprocess.run(["docker", "exec", "redis", "redis-cli", "FLUSHALL"], 
                       capture_output=True, timeout=5)
        print("Redis cache flushed.")
    except:
        print("Warning: Could not flush Redis")
    
    all_results = []
    
    # 1. General queries
    all_results += run_section("GENERAL QUERIES", GENERAL_QUERIES)
    
    # Flush between sections
    try:
        import subprocess
        subprocess.run(["docker", "exec", "redis", "redis-cli", "FLUSHALL"], capture_output=True, timeout=5)
    except:
        pass
    
    # 2. Complex queries
    all_results += run_section("COMPLEX QUERIES", COMPLEX_QUERIES)
    
    try:
        import subprocess
        subprocess.run(["docker", "exec", "redis", "redis-cli", "FLUSHALL"], capture_output=True, timeout=5)
    except:
        pass
    
    # 3. Positive/negative constraints
    all_results += run_section("CONSTRAINT QUERIES (positive + negative)", POSITIVE_CONSTRAINT_QUERIES)
    
    try:
        import subprocess
        subprocess.run(["docker", "exec", "redis", "redis-cli", "FLUSHALL"], capture_output=True, timeout=5)
    except:
        pass
    
    # 4. Multi-constraint queries
    all_results += run_section("MULTI-CONSTRAINT QUERIES", MULTI_CONSTRAINT_QUERIES)
    
    try:
        import subprocess
        subprocess.run(["docker", "exec", "redis", "redis-cli", "FLUSHALL"], capture_output=True, timeout=5)
    except:
        pass
    
    # 5. Stress test
    stress_results = run_stress_test(concurrency=5)
    
    # ── Summary ──────────────────────────────────────────────────────────────
    print(f"\n{'='*70}")
    print("  OVERALL SUMMARY")
    print(f"{'='*70}")
    
    ok = [r for r in all_results if r["ok"]]
    fail = [r for r in all_results if not r["ok"]]
    times = [r["time"] for r in all_results]
    
    print(f"  Total queries: {len(all_results)}")
    print(f"  OK: {len(ok)} | FAIL: {len(fail)}")
    if times:
        print(f"  Avg time: {sum(times)/len(times):.2f}s")
        print(f"  Min/Max: {min(times):.2f}s / {max(times):.2f}s")
    
    # Bottleneck analysis
    slow = [r for r in all_results if r["time"] > 5.0]
    very_slow = [r for r in all_results if r["time"] > 10.0]
    zero_results = [r for r in all_results if r["ok"] and r["count"] == 0]
    
    print(f"\n  BOTTLENECK ANALYSIS:")
    print(f"    >5s queries: {len(slow)}")
    print(f"    >10s queries: {len(very_slow)}")
    print(f"    0-result queries: {len(zero_results)}")
    
    if very_slow:
        print(f"\n  SLOWEST QUERIES:")
        for r in sorted(very_slow, key=lambda x: x["time"], reverse=True)[:5]:
            print(f"    {r['time']:.2f}s | {r['query'][:60]}")
    
    if zero_results:
        print(f"\n  ZERO-RESULT QUERIES:")
        for r in zero_results:
            print(f"    {r['query'][:60]}")
    
    if fail:
        print(f"\n  FAILED QUERIES:")
        for r in fail:
            print(f"    [{r['status']}] {r['query'][:60]}")


if __name__ == "__main__":
    main()
