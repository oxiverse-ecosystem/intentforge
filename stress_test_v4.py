#!/usr/bin/env python3
"""IntentForge Comprehensive Stress Test v4
Measures: latency, intent accuracy, result quality, domain diversity,
          title relevance, snippet quality, expansion quality.
"""

import json
import time
import urllib.request
import urllib.parse
import statistics
import re
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed

GATEWAY = "http://localhost:4000/search"
TIMEOUT = 15

# ─── Test queries with expected intent and quality expectations ───
TEST_QUERIES = [
    # ── Complex / multi-faceted queries ──
    ("how to implement oauth2 pkce flow in react with typescript", "how-to",
     {"min_results": 5, "must_contain": ["oauth2", "pkce"], "min_domains": 4}),
    ("difference between graphql federation and schema stitching in production", "comparison",
     {"min_results": 4, "must_contain": ["graphql", "federation"], "min_domains": 3}),
    ("best practices for postgresql connection pooling with pgbouncer vs pgcat", "comparison",
     {"min_results": 4, "must_contain": ["postgresql", "pooling"], "min_domains": 3}),
    ("debug memory leak in node.js worker threads under high concurrency", "how-to",
     {"min_results": 4, "must_contain": ["memory", "node"], "min_domains": 3}),
    ("set up kubernetes autoscaling with custom prometheus metrics", "how-to",
     {"min_results": 4, "must_contain": ["kubernetes", "autoscaling"], "min_domains": 3}),
    ("rust async runtime comparison tokio vs async-std performance benchmarks", "comparison",
     {"min_results": 3, "must_contain": ["rust", "async"], "min_domains": 3}),
    ("why does my docker container exit immediately after starting", "how-to",
     {"min_results": 5, "must_contain": ["docker", "container"], "min_domains": 4}),
    ("configure nginx reverse proxy with ssl termination and rate limiting", "how-to",
     {"min_results": 4, "must_contain": ["nginx", "ssl"], "min_domains": 3}),
    ("how does garbage collection work in golang vs java", "comparison",
     {"min_results": 4, "must_contain": ["garbage"], "min_domains": 3}),
    ("implement circuit breaker pattern in microservices with exponential backoff", "how-to",
     {"min_results": 3, "must_contain": ["circuit"], "min_domains": 3}),

    # ── General / broad queries ──
    ("quantum computing", "informational",
     {"min_results": 8, "must_contain": ["quantum"], "min_domains": 5}),
    ("machine learning", "informational",
     {"min_results": 8, "must_contain": ["machine", "learning"], "min_domains": 5}),
    ("blockchain", "informational",
     {"min_results": 6, "must_contain": ["blockchain"], "min_domains": 4}),
    ("artificial intelligence ethics", "informational",
     {"min_results": 5, "must_contain": ["artificial", "intelligence"], "min_domains": 4}),
    ("climate change solutions", "informational",
     {"min_results": 5, "must_contain": ["climate"], "min_domains": 4}),
    ("web assembly", "informational",
     {"min_results": 5, "must_contain": ["assembly"], "min_domains": 4}),
    ("linux kernel", "informational",
     {"min_results": 6, "must_contain": ["linux", "kernel"], "min_domains": 5}),
    ("python programming", "informational",
     {"min_results": 6, "must_contain": ["python"], "min_domains": 5}),
    ("cybersecurity", "informational",
     {"min_results": 5, "must_contain": ["cyber"], "min_domains": 4}),
    ("data structures and algorithms", "informational",
     {"min_results": 5, "must_contain": ["data"], "min_domains": 4}),

    # ── Technical / specific queries ──
    ("prometheus histogram quantile p99 calculation", "technical",
     {"min_results": 3, "must_contain": ["prometheus"], "min_domains": 2}),
    ("redis cluster vs sentinel for high availability", "comparison",
     {"min_results": 3, "must_contain": ["redis"], "min_domains": 2}),
    ("elasticsearch index mapping best practices for log data", "how-to",
     {"min_results": 3, "must_contain": ["elasticsearch"], "min_domains": 2}),
    ("terraform state locking with dynamodb backend", "how-to",
     {"min_results": 3, "must_contain": ["terraform"], "min_domains": 2}),
    ("grpc streaming vs rest api performance comparison", "comparison",
     {"min_results": 3, "must_contain": ["grpc"], "min_domains": 2}),
    ("jwt token refresh strategy with httpOnly cookies", "how-to",
     {"min_results": 3, "must_contain": ["jwt"], "min_domains": 2}),
    ("cors policy not working in express.js middleware", "how-to",
     {"min_results": 4, "must_contain": ["cors"], "min_domains": 3}),
    ("kafka consumer group rebalancing strategy", "technical",
     {"min_results": 3, "must_contain": ["kafka"], "min_domains": 2}),
    ("wireguard vpn split tunneling configuration", "how-to",
     {"min_results": 3, "must_contain": ["wireguard"], "min_domains": 2}),
    ("sqlite wal mode vs rollback journal performance", "comparison",
     {"min_results": 3, "must_contain": ["sqlite"], "min_domains": 2}),

    # ── How-does / explanatory queries ──
    ("how does a blockchain consensus mechanism work", "how-to",
     {"min_results": 4, "must_contain": ["blockchain", "consensus"], "min_domains": 3}),
    ("how does tcp congestion control work", "how-to",
     {"min_results": 4, "must_contain": ["tcp"], "min_domains": 3}),
    ("how does webassembly sandboxing work", "how-to",
     {"min_results": 3, "must_contain": ["webassembly"], "min_domains": 2}),
    ("explain raft consensus algorithm to a beginner", "informational",
     {"min_results": 3, "must_contain": ["raft"], "min_domains": 2}),
    ("what is the difference between http2 and http3", "comparison",
     {"min_results": 4, "must_contain": ["http"], "min_domains": 3}),

    # ── Fresh / news-oriented queries ──
    ("latest ai models released in 2026", "fresh",
     {"min_results": 4, "must_contain": ["ai"], "min_domains": 3}),
    ("new javascript features ecmascript 2026", "fresh",
     {"min_results": 3, "must_contain": ["javascript"], "min_domains": 2}),
    ("rust 2026 edition changes", "fresh",
     {"min_results": 3, "must_contain": ["rust"], "min_domains": 2}),

    # ── Transactional queries ──
    ("buy domain name cheap registrar", "transactional",
     {"min_results": 4, "must_contain": ["domain"], "min_domains": 3}),
    ("best cloud hosting provider for small projects", "transactional",
     {"min_results": 4, "must_contain": ["hosting"], "min_domains": 3}),
    ("download vscode extensions for python", "transactional",
     {"min_results": 4, "must_contain": ["vscode", "python"], "min_domains": 3}),
]


def query_gateway(query):
    """Hit the gateway and return raw response + latency."""
    params = urllib.parse.urlencode({"q": query, "format": "json"})
    url = f"{GATEWAY}?{params}"
    start = time.time()
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            data = json.loads(resp.read())
            latency = time.time() - start
            return {"ok": True, "data": data, "latency": latency, "query": query}
    except Exception as e:
        latency = time.time() - start
        return {"ok": False, "error": str(e), "latency": latency, "query": query}


def analyze_result_quality(query, results, expectations):
    """Score result quality on multiple dimensions."""
    scores = {}
    q_lower = query.lower()
    q_words = set(re.findall(r'[a-z]{3,}', q_lower))
    # Remove stopwords
    stopwords = {"the", "and", "for", "with", "how", "does", "what", "why",
                 "when", "where", "from", "that", "this", "does", "between"}
    q_keywords = q_words - stopwords

    # 1. Result count
    count = len(results)
    min_expected = expectations.get("min_results", 3)
    scores["result_count"] = count
    scores["result_count_adequate"] = count >= min_expected

    # 2. Domain diversity
    domains = set()
    for r in results:
        url = r.get("url", "")
        try:
            from urllib.parse import urlparse
            domains.add(urlparse(url).netloc)
        except:
            pass
    domain_count = len(domains)
    min_domains = expectations.get("min_domains", 2)
    scores["unique_domains"] = domain_count
    scores["domain_diversity_ok"] = domain_count >= min_domains
    scores["domain_ratio"] = round(domain_count / max(count, 1), 2)

    # 3. Title relevance — do titles contain query keywords?
    title_hits = 0
    for r in results:
        title = r.get("title", "").lower()
        if any(kw in title for kw in q_keywords):
            title_hits += 1
    scores["title_relevance"] = round(title_hits / max(count, 1), 2)

    # 4. Must-contain keywords in results
    must_contain = expectations.get("must_contain", [])
    if must_contain:
        keyword_found = 0
        for mc in must_contain:
            mc_lower = mc.lower()
            for r in results:
                combined = (r.get("title", "") + " " + r.get("content", "")).lower()
                if mc_lower in combined:
                    keyword_found += 1
                    break
        scores["must_contain_hits"] = f"{keyword_found}/{len(must_contain)}"
        scores["must_contain_ok"] = keyword_found == len(must_contain)
    else:
        scores["must_contain_ok"] = True

    # 5. Snippet quality
    snippet_lengths = []
    empty_snippets = 0
    for r in results:
        content = r.get("content", "")
        snippet_lengths.append(len(content))
        if not content.strip():
            empty_snippets += 1
    scores["avg_snippet_len"] = round(statistics.mean(snippet_lengths)) if snippet_lengths else 0
    scores["empty_snippets"] = empty_snippets
    scores["snippet_quality_ok"] = empty_snippets <= count * 0.2  # <20% empty

    # 6. Authority scores
    authorities = []
    for r in results:
        auth = r.get("authority")
        if auth is not None:
            try:
                authorities.append(float(auth))
            except:
                pass
    if authorities:
        scores["avg_authority"] = round(statistics.mean(authorities), 3)
        scores["min_authority"] = round(min(authorities), 3)
    else:
        scores["avg_authority"] = 0
        scores["min_authority"] = 0

    # 7. Result scores
    result_scores = []
    for r in results:
        sc = r.get("score")
        if sc is not None:
            try:
                result_scores.append(float(sc))
            except:
                pass
    if result_scores:
        scores["avg_score"] = round(statistics.mean(result_scores), 3)
        scores["top_score"] = round(max(result_scores), 3)
    else:
        scores["avg_score"] = 0
        scores["top_score"] = 0

    # 8. Source diversity (how many different engines contributed)
    all_sources = set()
    for r in results:
        srcs = r.get("sources", [])
        if isinstance(srcs, str):
            try:
                srcs = json.loads(srcs.replace("'", '"'))
            except:
                srcs = [srcs]
        for s in srcs:
            all_sources.add(s)
    scores["engine_sources"] = sorted(all_sources)
    scores["engine_count"] = len(all_sources)

    # Composite quality score (0-100)
    quality_points = 0
    quality_points += 20 if scores["result_count_adequate"] else (10 if count >= 2 else 0)
    quality_points += 15 if scores["domain_diversity_ok"] else (7 if domain_count >= 2 else 0)
    quality_points += min(20, int(scores["title_relevance"] * 25))
    quality_points += 15 if scores["must_contain_ok"] else 0
    quality_points += 15 if scores["snippet_quality_ok"] else (7 if empty_snippets <= count * 0.5 else 0)
    quality_points += 15 if scores["avg_authority"] > 0.3 else (10 if scores["avg_authority"] > 0.1 else 0)
    scores["quality_score"] = quality_points

    return scores


def main():
    print("=" * 70)
    print("  INTENTFORGE COMPREHENSIVE STRESS TEST v4")
    print(f"  {len(TEST_QUERIES)} queries | Latency + Intent + Result Quality")
    print("=" * 70)
    print()

    all_results = []
    errors = 0

    # Run queries sequentially with small delay to avoid hammering
    for i, (query, expected_intent, expectations) in enumerate(TEST_QUERIES):
        result = query_gateway(query)
        if not result["ok"]:
            print(f"  [{i+1:2d}/{len(TEST_QUERIES)}] ERROR: {query[:40]:40s} → {result['error'][:50]}")
            errors += 1
            all_results.append({
                "query": query, "expected_intent": expected_intent,
                "ok": False, "latency": result["latency"]
            })
            continue

        data = result["data"]
        actual_intent = data.get("intent", "?")
        confidence = data.get("confidence", 0)
        results_list = data.get("results", [])
        quality = analyze_result_quality(query, results_list, expectations)

        intent_match = actual_intent == expected_intent

        all_results.append({
            "query": query,
            "expected_intent": expected_intent,
            "actual_intent": actual_intent,
            "confidence": confidence,
            "intent_match": intent_match,
            "ok": True,
            "latency": result["latency"],
            "result_count": len(results_list),
            "quality": quality,
        })

        status = "✓" if intent_match else "✗"
        qscore = quality["quality_score"]
        print(f"  [{i+1:2d}/{len(TEST_QUERIES)}] {status} {query[:45]:45s} "
              f"intent={actual_intent:14s}({confidence:.2f}) "
              f"results={len(results_list):2d} quality={qscore:3d}/100 "
              f"latency={result['latency']:.2f}s")

        time.sleep(0.3)  # be nice to the server

    # ─── Aggregate Analysis ───
    print()
    print("=" * 70)
    print("  AGGREGATE RESULTS")
    print("=" * 70)

    successful = [r for r in all_results if r.get("ok")]
    if not successful:
        print("  No successful queries!")
        return

    # Latency
    latencies = [r["latency"] for r in successful]
    latencies.sort()
    print()
    print("  ── LATENCY ──")
    print(f"    p50: {latencies[len(latencies)//2]:.2f}s")
    print(f"    p90: {latencies[int(len(latencies)*0.9)]:.2f}s")
    print(f"    p95: {latencies[int(len(latencies)*0.95)]:.2f}s")
    print(f"    mean: {statistics.mean(latencies):.2f}s")
    print(f"    min: {min(latencies):.2f}s  max: {max(latencies):.2f}s")

    # Intent accuracy
    intent_matches = sum(1 for r in successful if r.get("intent_match"))
    intent_total = len(successful)
    print()
    print("  ── INTENT ACCURACY ──")
    print(f"    {intent_matches}/{intent_total} ({100*intent_matches/intent_total:.1f}%)")

    # Intent breakdown by category
    intent_by_expected = defaultdict(lambda: {"correct": 0, "total": 0})
    for r in successful:
        exp = r["expected_intent"]
        intent_by_expected[exp]["total"] += 1
        if r.get("intent_match"):
            intent_by_expected[exp]["correct"] += 1
    print("    By category:")
    for cat, counts in sorted(intent_by_expected.items()):
        pct = 100 * counts["correct"] / counts["total"]
        print(f"      {cat:16s} {counts['correct']}/{counts['total']} ({pct:.0f}%)")

    # Confidence distribution
    confidences = [r["confidence"] for r in successful]
    print()
    print("  ── CONFIDENCE ──")
    print(f"    mean: {statistics.mean(confidences):.3f}")
    print(f"    min:  {min(confidences):.3f}  max: {max(confidences):.3f}")
    low_conf = sum(1 for c in confidences if c < 0.4)
    print(f"    low confidence (<0.4): {low_conf}/{len(confidences)}")

    # Result quality
    quality_scores = [r["quality"]["quality_score"] for r in successful]
    result_counts = [r["result_count"] for r in successful]
    print()
    print("  ── RESULT QUALITY ──")
    print(f"    Quality score: mean={statistics.mean(quality_scores):.1f}/100  "
          f"min={min(quality_scores)}  max={max(quality_scores)}")
    print(f"    Result count:  mean={statistics.mean(result_counts):.1f}  "
          f"min={min(result_counts)}  max={max(result_counts)}")

    thin = sum(1 for c in result_counts if c < 3)
    print(f"    Thin results (<3): {thin}/{len(result_counts)}")

    # Domain diversity
    domain_ratios = [r["quality"]["domain_ratio"] for r in successful]
    print(f"    Domain ratio:  mean={statistics.mean(domain_ratios):.2f}")

    # Title relevance
    title_rels = [r["quality"]["title_relevance"] for r in successful]
    print(f"    Title relevance: mean={statistics.mean(title_rels):.2f}")

    # Snippet quality
    avg_snippets = [r["quality"]["avg_snippet_len"] for r in successful]
    empty_snippets = sum(r["quality"]["empty_snippets"] for r in successful)
    print(f"    Avg snippet len: {statistics.mean(avg_snippets):.0f} chars")
    print(f"    Empty snippets:  {empty_snippets}")

    # Authority
    avg_auths = [r["quality"]["avg_authority"] for r in successful]
    print(f"    Avg authority:   {statistics.mean(avg_auths):.3f}")

    # Engine sources
    all_engines = set()
    for r in successful:
        for e in r["quality"]["engine_sources"]:
            all_engines.add(e)
    print(f"    Engine sources:  {len(all_engines)} ({', '.join(sorted(all_engines))})")

    # ─── Quality breakdown: queries scoring < 60 ───
    low_quality = [(r["query"], r["quality"]["quality_score"], r["quality"])
                   for r in successful if r["quality"]["quality_score"] < 60]
    if low_quality:
        print()
        print("  ── LOW QUALITY QUERIES (<60/100) ──")
        for q, qs, qd in sorted(low_quality, key=lambda x: x[1]):
            print(f"    {qs:3d}/100  {q[:55]}")
            issues = []
            if not qd["result_count_adequate"]:
                issues.append(f"few results({qd['result_count']})")
            if not qd["domain_diversity_ok"]:
                issues.append(f"low diversity({qd['unique_domains']})")
            if qd["title_relevance"] < 0.5:
                issues.append(f"low title rel({qd['title_relevance']:.2f})")
            if not qd["must_contain_ok"]:
                issues.append(f"missing keywords({qd['must_contain_hits']})")
            if not qd["snippet_quality_ok"]:
                issues.append(f"empty snippets({qd['empty_snippets']})")
            print(f"           issues: {', '.join(issues)}")

    # ─── Intent failures detail ───
    intent_failures = [r for r in successful if not r.get("intent_match")]
    if intent_failures:
        print()
        print("  ── INTENT FAILURES ──")
        for r in intent_failures:
            print(f"    {r['query'][:50]:50s}  expected={r['expected_intent']:14s} "
                  f"got={r['actual_intent']:14s}({r['confidence']:.2f})")

    # ─── Overall score ───
    print()
    print("=" * 70)
    intent_pct = 100 * intent_matches / intent_total
    quality_avg = statistics.mean(quality_scores)
    latency_p90 = latencies[int(len(latencies)*0.9)]
    overall = (intent_pct * 0.3 + quality_avg * 0.5 + max(0, (5 - latency_p90) / 5 * 100) * 0.2)
    print(f"  OVERALL SCORE: {overall:.1f}/100")
    print(f"    Intent:  {intent_pct:.1f}% (weight 30%)")
    print(f"    Quality: {quality_avg:.1f}/100 (weight 50%)")
    print(f"    Latency: p90={latency_p90:.2f}s (weight 20%)")
    print(f"    Errors:  {errors}/{len(TEST_QUERIES)}")
    print("=" * 70)


if __name__ == "__main__":
    main()
