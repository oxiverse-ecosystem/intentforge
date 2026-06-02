#!/usr/bin/env python3
"""
IntentForge v2 Stress Test - Latency + Result Quality
"""

import json
import time
import statistics
import sys
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError
from urllib.parse import quote_plus

BASE_URL = "http://localhost:4000/search"

QUERIES = [
    # SIMPLE / GENERAL
    ("weather in new york today", "informational", {
        "min_results": 3, "title_keywords": ["weather", "new york", "nyc"], "should_have_content": True,
    }),
    ("who is the president of france", "informational", {
        "min_results": 3, "title_keywords": ["president", "france"],
    }),
    ("population of tokyo", "informational", {
        "min_results": 2, "title_keywords": ["population", "tokyo"],
    }),
    ("what is quantum computing", "informational", {
        "min_results": 5, "title_keywords": ["quantum", "computing"], "should_have_content": True,
    }),
    ("translate hello to japanese", "informational", {
        "min_results": 2,
    }),

    # COMPARISON / DECISION
    ("react vs vue vs angular performance benchmark 2026", "comparison", {
        "min_results": 5, "title_keywords": ["react", "vue", "angular", "performance"], "should_have_content": True,
    }),
    ("best laptop for machine learning under 2000 dollars", "comparison", {
        "min_results": 5, "title_keywords": ["laptop", "machine learning"],
    }),
    ("postgres vs mysql vs mongodb for high traffic saas", "comparison", {
        "min_results": 4, "title_keywords": ["postgres", "mysql", "mongodb"],
    }),
    ("claude opus 4 vs gpt-5 vs gemini 2.5 pro coding ability", "comparison", {
        "min_results": 3,
    }),
    ("cheapest way to deploy kubernetes cluster production", "comparison", {
        "min_results": 4,
    }),

    # HOW-TO / PROCEDURAL
    ("how to set up nginx reverse proxy with ssl certificate", "how-to", {
        "min_results": 5, "title_keywords": ["nginx", "ssl", "reverse proxy"], "should_have_content": True,
    }),
    ("step by step guide docker compose multi container app", "how-to", {
        "min_results": 4, "title_keywords": ["docker", "compose"],
    }),
    ("how to implement rate limiting in fastapi with redis", "how-to", {
        "min_results": 3, "title_keywords": ["rate limit", "fastapi"],
    }),
    ("tutorial building rust webassembly frontend application", "how-to", {
        "min_results": 3,
    }),
    ("how to migrate postgresql database without downtime", "how-to", {
        "min_results": 3, "title_keywords": ["migrate", "postgresql"],
    }),

    # TECHNICAL / DEEP
    ("kubernetes pod crashloopbackoff debugging etcd connection refused", "technical", {
        "min_results": 3, "title_keywords": ["kubernetes", "crashloopbackoff"],
    }),
    ("nginx 502 bad gateway upstream timed out gunicorn unix socket", "technical", {
        "min_results": 3,
    }),
    ("rust lifetime elision rules async fn returning impl future", "technical", {
        "min_results": 2,
    }),
    ("tensorflow cuda out of memory despite small batch size a100", "technical", {
        "min_results": 3,
    }),
    ("webpack 5 module federation shared dependency version mismatch error", "technical", {
        "min_results": 2,
    }),

    # TRANSACTIONAL / PURCHASE
    ("buy domain name cheap privacy protection included", "transactional", {
        "min_results": 4, "title_keywords": ["domain", "buy"],
    }),
    ("order custom mechanical keyboard pcb online", "transactional", {
        "min_results": 3,
    }),
    ("cheapest flights tokyo to seoul september 2026", "transactional", {
        "min_results": 3,
    }),
    ("hire freelance solidity smart contract auditor", "transactional", {
        "min_results": 3,
    }),

    # FRESH / TIME-SENSITIVE
    ("latest ai research papers june 2026", "fresh", {
        "min_results": 4, "should_have_content": True,
    }),
    ("breaking news artificial intelligence regulation europe", "fresh", {
        "min_results": 4,
    }),
    ("new javascript framework released 2026", "fresh", {
        "min_results": 3,
    }),

    # NAVIGATIONAL
    ("github copilot official documentation", "navigational", {
        "min_results": 3, "title_keywords": ["github", "copilot"],
    }),
    ("stackoverflow create account sign up", "navigational", {
        "min_results": 2,
    }),
    ("aws lambda pricing calculator", "navigational", {
        "min_results": 3, "title_keywords": ["aws", "lambda"],
    }),

    # AMBIGUOUS / EDGE CASES
    ("python", "informational", {"min_results": 5}),
    ("apple", "informational", {"min_results": 5}),
    ("bass fishing techniques cold water", "informational", {"min_results": 3}),

    # COMPLEX MULTI-CONSTRAINT
    ("lightweight fast python web framework async support no ORM minimal boilerplate", "comparison", {
        "min_results": 3,
    }),
    ("free open source self-hosted alternative to notion with markdown export", "comparison", {
        "min_results": 4,
    }),
    ("how to build real-time chat application with websockets postgresql and react scaling tips", "how-to", {
        "min_results": 3,
    }),
    ("best static site generator for documentation with full-text search and versioning", "comparison", {
        "min_results": 4,
    }),
    ("monitoring stack prometheus grafana alertmanager kubernetes helm chart production ready", "technical", {
        "min_results": 3,
    }),
]


def run_query(query, timeout=15):
    url = f"{BASE_URL}?q={quote_plus(query)}"
    req = Request(url, headers={"Accept": "application/json"})
    result = {"query": query, "error": None, "ttfb_ms": None, "total_ms": None, "status_code": None, "response": None}
    try:
        start = time.perf_counter()
        with urlopen(req, timeout=timeout) as resp:
            ttfb = time.perf_counter()
            result["ttfb_ms"] = (ttfb - start) * 1000
            result["status_code"] = resp.status
            raw = resp.read()
            end = time.perf_counter()
            result["total_ms"] = (end - start) * 1000
            result["response"] = json.loads(raw)
    except HTTPError as e:
        result["error"] = f"HTTP {e.code}: {e.reason}"
        result["status_code"] = e.code
    except URLError as e:
        result["error"] = f"URL Error: {e.reason}"
    except Exception as e:
        result["error"] = f"{type(e).__name__}: {e}"
    return result


def evaluate_quality(result, expected_intent, checks):
    scores = {
        "intent_match": 0.0, "result_count": 0.0, "content_quality": 0.0,
        "title_relevance": 0.0, "constraint_quality": 0.0, "confidence": 0.0, "expansion_diversity": 0.0,
    }
    details = []
    resp = result.get("response")
    if not resp:
        return scores, ["NO RESPONSE"]

    intent = resp.get("intent", "")
    category = resp.get("category", "")
    dist = resp.get("distribution", {})
    sorted_intents = sorted(dist.items(), key=lambda x: x[1], reverse=True)
    top2 = [i[0] for i in sorted_intents[:2]]
    # Score against the actual intent field (which matches distribution top) and distribution
    if expected_intent == intent:
        scores["intent_match"] = 1.0
    elif expected_intent in top2:
        scores["intent_match"] = 0.8
    elif expected_intent in dist and dist[expected_intent] > 0.1:
        scores["intent_match"] = 0.4
    elif expected_intent in dist:
        scores["intent_match"] = 0.2
    details.append(f"intent: got={intent} cat={category} expected={expected_intent} top2={top2}")

    results = resp.get("results", [])
    n_results = len(results)
    min_expected = checks.get("min_results", 3)
    if n_results >= min_expected * 2:
        scores["result_count"] = 1.0
    elif n_results >= min_expected:
        scores["result_count"] = 0.8
    elif n_results > 0:
        scores["result_count"] = 0.4
    details.append(f"results: {n_results} (min expected {min_expected})")

    if checks.get("should_have_content"):
        contentful = sum(1 for r in results[:10] if r.get("content") and len(r["content"]) > 30)
        ratio = contentful / min(len(results), 10) if results else 0
        scores["content_quality"] = min(ratio, 1.0)
        details.append(f"contentful: {contentful}/{min(len(results), 10)}")
    else:
        scores["content_quality"] = 0.5

    title_kws = checks.get("title_keywords", [])
    if title_kws and results:
        hits = 0
        for r in results[:5]:
            title_lower = r.get("title", "").lower()
            if any(kw.lower() in title_lower for kw in title_kws):
                hits += 1
        scores["title_relevance"] = hits / min(5, len(results))
        details.append(f"title hits: {hits}/{min(5, len(results))}")
    else:
        scores["title_relevance"] = 0.5

    constraints = resp.get("constraints", [])
    structured = resp.get("structured_constraints", {})
    if constraints:
        scores["constraint_quality"] = 1.0 if structured else 0.8
        details.append(f"constraints: {constraints[:3]}")
    else:
        scores["constraint_quality"] = 0.5
        details.append("constraints: none")

    conf = resp.get("confidence", 0)
    if conf >= 0.6:
        scores["confidence"] = 1.0
    elif conf >= 0.4:
        scores["confidence"] = 0.7
    elif conf >= 0.25:
        scores["confidence"] = 0.4
    else:
        scores["confidence"] = 0.2
    details.append(f"confidence: {conf:.3f}")

    expanded = resp.get("expanded_queries", [])
    if len(expanded) >= 3:
        unique_ratio = len(set(expanded)) / len(expanded)
        scores["expansion_diversity"] = unique_ratio
        details.append(f"expanded: {len(expanded)} queries, {len(set(expanded))} unique")
    elif len(expanded) >= 1:
        scores["expansion_diversity"] = 0.5
        details.append(f"expanded: {len(expanded)} queries")
    else:
        scores["expansion_diversity"] = 0.0
        details.append("expanded: none")

    return scores, details


def print_bar(label, value, width=30):
    filled = int(value * width)
    bar = "#" * filled + "." * (width - filled)
    return f"  {label:<22s} [{bar}] {value:.3f}"


def main():
    print("=" * 80)
    print("  IntentForge v2 STRESS TEST - Latency + Result Quality")
    print(f"  Target: {BASE_URL}")
    print(f"  Queries: {len(QUERIES)}")
    print("=" * 80)
    print()

    all_results = []
    all_latencies = []
    all_scores = {k: [] for k in [
        "intent_match", "result_count", "content_quality",
        "title_relevance", "constraint_quality", "confidence", "expansion_diversity"
    ]}
    category_stats = {}

    for i, (query, expected_intent, checks) in enumerate(QUERIES):
        sys.stdout.write(f"\r  [{i+1:02d}/{len(QUERIES)}] Running: {query[:60]:<60s}")
        sys.stdout.flush()

        result = run_query(query)
        all_results.append(result)

        if result["error"]:
            all_latencies.append(result.get("total_ms", 15000))
            print(f"\r  [{i+1:02d}/{len(QUERIES)}] ERROR: {query[:50]:<50s} -> {result['error']}")
            continue

        latency = result["total_ms"]
        all_latencies.append(latency)
        scores, details = evaluate_quality(result, expected_intent, checks)

        for k, v in scores.items():
            all_scores[k].append(v)

        cat = result["response"].get("category", "unknown")
        if cat not in category_stats:
            category_stats[cat] = {"count": 0, "latencies": [], "result_counts": [], "quality_scores": []}
        category_stats[cat]["count"] += 1
        category_stats[cat]["latencies"].append(latency)
        category_stats[cat]["result_counts"].append(len(result["response"].get("results", [])))
        overall_q = statistics.mean(scores.values())
        category_stats[cat]["quality_scores"].append(overall_q)

        overall = statistics.mean(scores.values())
        quality_label = "EXCELLENT" if overall >= 0.8 else "GOOD" if overall >= 0.6 else "FAIR" if overall >= 0.4 else "POOR"
        n_results = len(result["response"].get("results", []))
        print(f"\r  [{i+1:02d}/{len(QUERIES)}] {latency:>7.0f}ms | {n_results:>3}r | {quality_label:>8s} | {query[:50]}")

        if overall < 0.5:
            for d in details:
                print(f"           -> {d}")

    # SUMMARY
    print()
    print("=" * 80)
    print("  LATENCY REPORT")
    print("=" * 80)
    valid_latencies = [l for l in all_latencies if l < 15000]
    if valid_latencies:
        print(f"  Queries completed:  {len(valid_latencies)}/{len(QUERIES)}")
        print(f"  Mean latency:       {statistics.mean(valid_latencies):>8.0f} ms")
        print(f"  Median (p50):       {statistics.median(valid_latencies):>8.0f} ms")
        sorted_l = sorted(valid_latencies)
        p90_idx = int(len(sorted_l) * 0.9)
        p99_idx = int(len(sorted_l) * 0.99)
        print(f"  p90:                {sorted_l[min(p90_idx, len(sorted_l)-1)]:>8.0f} ms")
        print(f"  p99:                {sorted_l[min(p99_idx, len(sorted_l)-1)]:>8.0f} ms")
        print(f"  Min:                {min(valid_latencies):>8.0f} ms")
        print(f"  Max:                {max(valid_latencies):>8.0f} ms")
        if len(valid_latencies) > 1:
            print(f"  Stdev:              {statistics.stdev(valid_latencies):>8.0f} ms")
    else:
        print("  NO SUCCESSFUL QUERIES")
    errors = sum(1 for r in all_results if r["error"])
    print(f"  Errors:             {errors}/{len(QUERIES)}")

    print()
    print("=" * 80)
    print("  RESULT QUALITY REPORT (scored 0.0-1.0)")
    print("=" * 80)
    overall_quality = []
    for metric, values in all_scores.items():
        if values:
            avg = statistics.mean(values)
            overall_quality.append(avg)
            label = metric.replace("_", " ").title()
            print(print_bar(label, avg))
    if overall_quality:
        print()
        combined = statistics.mean(overall_quality)
        grade = "A+" if combined >= 0.85 else "A" if combined >= 0.75 else "B" if combined >= 0.65 else "C" if combined >= 0.55 else "D" if combined >= 0.45 else "F"
        print(f"  OVERALL QUALITY SCORE: {combined:.3f}  (Grade: {grade})")

    print()
    print("=" * 80)
    print("  PER-CATEGORY BREAKDOWN")
    print("=" * 80)
    print(f"  {'Category':<16s} {'Count':>5s} {'Avg ms':>8s} {'Avg Results':>11s} {'Quality':>8s}")
    print(f"  {'-'*16} {'-'*5} {'-'*8} {'-'*11} {'-'*8}")
    for cat in sorted(category_stats.keys()):
        s = category_stats[cat]
        avg_lat = statistics.mean(s["latencies"])
        avg_res = statistics.mean(s["result_counts"])
        avg_q = statistics.mean(s["quality_scores"])
        print(f"  {cat:<16s} {s['count']:>5d} {avg_lat:>8.0f} {avg_res:>11.1f} {avg_q:>8.3f}")

    print()
    print("=" * 80)
    print("  WORST QUERIES (quality < 0.5)")
    print("=" * 80)
    worst = []
    for i, (query, expected, checks) in enumerate(QUERIES):
        result = all_results[i]
        if result["error"]:
            worst.append((0.0, query, result["error"]))
            continue
        if result["response"]:
            scores, _ = evaluate_quality(result, expected, checks)
            avg = statistics.mean(scores.values())
            if avg < 0.5:
                worst.append((avg, query, f"intent={result['response'].get('category')} results={len(result['response'].get('results',[]))}"))
    worst.sort(key=lambda x: x[0])
    for score, q, note in worst[:10]:
        print(f"  {score:.3f} | {q[:55]:<55s} | {note}")

    print()
    print("=" * 80)
    print("  BEST QUERIES (quality >= 0.75)")
    print("=" * 80)
    best = []
    for i, (query, expected, checks) in enumerate(QUERIES):
        result = all_results[i]
        if result["response"] and not result["error"]:
            scores, _ = evaluate_quality(result, expected, checks)
            avg = statistics.mean(scores.values())
            if avg >= 0.75:
                best.append((avg, query, len(result["response"].get("results", []))))
    best.sort(key=lambda x: -x[0])
    for score, q, nres in best[:10]:
        print(f"  {score:.3f} | {q[:55]:<55s} | {nres} results")

    print()
    print("=" * 80)
    print("  STRESS TEST COMPLETE")
    print("=" * 80)


if __name__ == "__main__":
    main()
