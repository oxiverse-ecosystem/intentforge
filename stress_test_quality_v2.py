#!/usr/bin/env python3
"""IntentForge Search API Stress Test — Quality + Performance
Tests intent classification AND result quality across all categories.
"""
import json
import time
import sys
import urllib.request
import urllib.parse
from collections import defaultdict

GW = "http://127.0.0.1:4000"
TIMEOUT = 15

# ── Test Queries ──────────────────────────────────────────────────
# Each entry: (query, expected_intent_category, expected_search_category, quality_keywords)
# quality_keywords: words that SHOULD appear in results for relevance scoring

QUERIES = [
    # Gateway uses 3 categories: navigational, informational, transactional
    # Intent engine has 7 but gateway collapses them

    # ── Navigational ──
    ("google", "navigational", "general", ["google", "search"]),
    ("youtube", "navigational", "general", ["youtube", "video"]),
    ("wikipedia", "navigational", "general", ["wikipedia"]),
    ("github", "navigational", "general", ["github", "code"]),
    ("amazon", "navigational", "general", ["amazon", "shop"]),
    ("netflix", "navigational", "general", ["netflix"]),
    ("reddit", "navigational", "general", ["reddit"]),

    # ── Informational ──
    ("what is quantum computing", "informational", "general", ["quantum", "computing", "qubit"]),
    ("how does photosynthesis work", "informational", "general", ["photosynthesis", "plant", "light"]),
    ("python asyncio tutorial", "informational", "general", ["python", "async", "await"]),
    ("rust programming language", "informational", "general", ["rust", "programming"]),
    ("machine learning explained", "informational", "general", ["machine learning", "model", "data"]),
    ("docker compose networking", "informational", "general", ["docker", "compose", "network"]),
    ("nginx reverse proxy setup", "informational", "general", ["nginx", "proxy", "server"]),
    ("kubernetes pod scheduling", "informational", "general", ["kubernetes", "pod", "schedule"]),
    ("linux cron job syntax", "informational", "general", ["cron", "linux", "schedule"]),
    ("ssh tunnel port forwarding", "informational", "general", ["ssh", "tunnel", "port"]),
    ("what is the capital of france", "informational", "general", ["paris", "france", "capital"]),
    ("difference between tcp and udp", "informational", "general", ["tcp", "udp", "protocol"]),

    # ── News / Fresh (gateway classifies as informational) ──
    ("latest tech news today", "informational", "news", ["tech", "news"]),
    ("nvidia stock price", "informational", "news", ["nvidia", "stock"]),
    ("spacex next launch", "informational", "news", ["spacex", "launch"]),
    ("bitcoin price today", "informational", "news", ["bitcoin", "price"]),
    ("ai regulation latest", "informational", "news", ["ai", "regulation"]),

    # ── Images (gateway classifies as navigational) ──
    ("sunset over ocean", "navigational", "images", ["sunset", "ocean"]),
    ("mountain landscape photography", "navigational", "images", ["mountain", "landscape"]),
    ("cute puppies", "navigational", "images", ["puppy", "dog"]),
    ("abstract digital art", "navigational", "images", ["abstract", "art"]),
    ("city skyline night", "navigational", "images", ["city", "skyline"]),

    # ── Videos (gateway classifies as informational) ──
    ("cooking pasta recipe", "informational", "videos", ["pasta", "cook", "recipe"]),
    ("python programming tutorial", "informational", "videos", ["python", "tutorial", "programming"]),
    ("guitar lesson beginner", "informational", "videos", ["guitar", "lesson", "beginner"]),
    ("workout routine home", "informational", "videos", ["workout", "exercise"]),
    ("3blue1brown linear algebra", "informational", "videos", ["linear algebra", "3blue1brown", "matrix"]),

    # ── Transactional ──
    ("buy iphone 16 pro", "transactional", "general", ["iphone", "buy", "price"]),
    ("book flight to tokyo", "transactional", "general", ["flight", "tokyo", "book"]),
    ("order pizza online", "transactional", "general", ["pizza", "order"]),
    ("subscribe to netflix", "transactional", "general", ["netflix", "subscribe", "plan"]),

    # ── Complex / Ambiguous ──
    ("apple", "navigational", "general", ["apple"]),
    ("bass", "informational", "general", ["bass"]),
    ("jaguar", "informational", "general", ["jaguar"]),
    ("python django rest framework cors", "informational", "general", ["django", "cors", "api"]),
    ("how to deploy react app to aws s3 cloudfront", "informational", "general", ["react", "aws", "deploy", "s3"]),
    ("best practices for securing nginx with letsencrypt ssl", "informational", "general", ["nginx", "ssl", "letsencrypt"]),
    ("compare postgresql vs mysql for high traffic web application", "informational", "general", ["postgresql", "mysql", "performance"]),
    ("rust async runtime tokio vs async-std benchmark", "informational", "general", ["tokio", "async-std", "rust"]),
    ("what happened in the 2024 us presidential election", "informational", "news", ["election", "2024", "president"]),
    ("latest developments in fusion energy research 2026", "informational", "news", ["fusion", "energy", "research"]),
]


def query_api(endpoint, q):
    """Query the gateway and return parsed JSON + latency."""
    url = f"{GW}/{endpoint}?q={urllib.parse.quote(q)}"
    start = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "stress-test/1.0"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            data = json.loads(resp.read().decode())
        latency = round((time.time() - start) * 1000)
        return data, latency, None
    except Exception as e:
        latency = round((time.time() - start) * 1000)
        return None, latency, str(e)


def score_relevance(results, keywords):
    """Score how many results contain quality keywords. Returns 0.0-1.0."""
    if not results or not keywords:
        return 0.0

    hits = 0
    for r in results[:10]:  # Check top 10 results
        text = " ".join([
            r.get("title", ""),
            r.get("description", ""),
            r.get("content", ""),
            r.get("url", ""),
        ]).lower()
        if any(kw.lower() in text for kw in keywords):
            hits += 1

    return round(hits / min(len(results), 10), 3)


def get_engines(results):
    """Extract engine distribution from results."""
    engines = defaultdict(int)
    for r in results:
        src = r.get("source", r.get("engine", "unknown"))
        engines[src] += 1
    return dict(engines)


def run_test():
    print(f"{'='*80}")
    print(f"  IntentForge Search API — Quality Stress Test")
    print(f"  {len(QUERIES)} queries across general/news/images/videos")
    print(f"{'='*80}\n")

    results_by_category = defaultdict(lambda: {"count": 0, "errors": 0, "latencies": [], "relevance": [], "result_counts": []})
    all_results = []
    intent_hits = 0
    intent_misses = 0
    category_hits = 0
    category_misses = 0

    for i, (query, expected_intent, expected_cat, quality_kw) in enumerate(QUERIES):
        sys.stdout.write(f"\r  [{i+1}/{len(QUERIES)}] {query[:50]:50s}")
        sys.stdout.flush()

        # Query the search endpoint
        data, latency, err = query_api("search", query)

        if err:
            results_by_category[expected_cat]["errors"] += 1
            all_results.append({
                "query": query, "error": err, "latency": latency,
                "expected_intent": expected_intent, "expected_cat": expected_cat,
            })
            continue

        # Extract intent
        actual_intent = data.get("category", "unknown")
        actual_cat = data.get("category", actual_intent)  # top-level category

        # Check intent match
        intent_match = actual_intent == expected_intent
        if intent_match:
            intent_hits += 1
        else:
            intent_misses += 1

        # Check if results came from the right search category
        search_results = data.get("results", [])
        result_engines = get_engines(search_results)

        # Determine actual search category from engine sources
        has_news = any(k in result_engines for k in ["bing news", "google news", "hackernews"])
        has_images = any(k in result_engines for k in ["bing images", "google images", "openverse"])
        has_videos = any(k in result_engines for k in ["bing videos", "google videos", "duckduckgo videos", "invidious"])
        has_general = any(k in result_engines for k in ["bing", "google", "duckduckgo", "brave", "startpage", "mojeek", "yandex", "wikipedia", "marginalia"])

        if expected_cat == "news" and has_news:
            category_hits += 1
        elif expected_cat == "images" and has_images:
            category_hits += 1
        elif expected_cat == "videos" and has_videos:
            category_hits += 1
        elif expected_cat == "general" and has_general:
            category_hits += 1
        else:
            category_misses += 1

        # Score relevance
        relevance = score_relevance(search_results, quality_kw)

        cat_stats = results_by_category[expected_cat]
        cat_stats["count"] += 1
        cat_stats["latencies"].append(latency)
        cat_stats["relevance"].append(relevance)
        cat_stats["result_counts"].append(len(search_results))

        all_results.append({
            "query": query,
            "expected_intent": expected_intent,
            "actual_intent": actual_intent,
            "intent_match": intent_match,
            "expected_cat": expected_cat,
            "result_count": len(search_results),
            "engines": result_engines,
            "relevance": relevance,
            "latency": latency,
        })

        time.sleep(0.3)  # Don't hammer too hard

    # ── Print Results ──────────────────────────────────────────────
    print(f"\n\n{'='*80}")
    print(f"  RESULTS SUMMARY")
    print(f"{'='*80}")

    # Intent accuracy
    total_intent = intent_hits + intent_misses
    print(f"\n  Intent Classification:")
    print(f"    Correct: {intent_hits}/{total_intent} ({100*intent_hits/max(total_intent,1):.1f}%)")

    # Category accuracy
    total_cat = category_hits + category_misses
    print(f"\n  Category Routing:")
    print(f"    Correct: {category_hits}/{total_cat} ({100*category_hits/max(total_cat,1):.1f}%)")

    # Per-category breakdown
    print(f"\n  Per-Category Breakdown:")
    print(f"  {'Category':12s} {'Count':>6s} {'Errors':>7s} {'Avg ms':>8s} {'P50 ms':>8s} {'P90 ms':>8s} {'Avg Results':>12s} {'Relevance':>10s}")
    print(f"  {'-'*75}")

    for cat in ["general", "news", "images", "videos"]:
        s = results_by_category[cat]
        if s["count"] == 0:
            continue
        lats = sorted(s["latencies"])
        avg_lat = sum(lats) // len(lats)
        p50 = lats[len(lats) // 2]
        p90 = lats[int(len(lats) * 0.9)]
        avg_results = sum(s["result_counts"]) // len(s["result_counts"])
        avg_rel = round(sum(s["relevance"]) / len(s["relevance"]) * 100, 1)
        print(f"  {cat:12s} {s['count']:>6d} {s['errors']:>7d} {avg_lat:>7d}ms {p50:>7d}ms {p90:>7d}ms {avg_results:>12d} {avg_rel:>9.1f}%")

    # Worst relevance queries
    print(f"\n  Lowest Relevance Queries:")
    by_relevance = sorted(all_results, key=lambda x: x.get("relevance", 0))
    for r in by_relevance[:5]:
        if "error" in r:
            print(f"    ERROR  {r['query'][:45]:45s}  {r['error'][:40]}")
        else:
            print(f"    {r['relevance']:.2f}   {r['query'][:45]:45s}  {r['result_count']} results  {r['latency']}ms")

    # Slowest queries
    print(f"\n  Slowest Queries:")
    by_latency = sorted([r for r in all_results if "error" not in r], key=lambda x: x["latency"], reverse=True)
    for r in by_latency[:5]:
        print(f"    {r['latency']:>5d}ms  {r['query'][:45]:45s}  {r['result_count']} results  relevance={r['relevance']:.2f}")

    # Intent mismatches
    print(f"\n  Intent Mismatches:")
    mismatches = [r for r in all_results if not r.get("intent_match") and "error" not in r]
    if mismatches:
        for r in mismatches[:10]:
            print(f"    {r['query'][:40]:40s}  expected={r['expected_intent']:15s} got={r.get('actual_intent','?')}")
    else:
        print(f"    None — all intents classified correctly")

    # Overall stats
    all_lats = [r["latency"] for r in all_results if "error" not in r]
    all_results_counts = [r["result_count"] for r in all_results if "error" not in r]
    all_relevance = [r["relevance"] for r in all_results if "error" not in r]

    if all_lats:
        print(f"\n  Overall:")
        print(f"    Queries:    {len(all_results)} ({sum(1 for r in all_results if 'error' in r)} errors)")
        print(f"    Latency:    avg={sum(all_lats)//len(all_lats)}ms  p50={sorted(all_lats)[len(all_lats)//2]}ms  p90={sorted(all_lats)[int(len(all_lats)*0.9)]}ms")
        print(f"    Results:    avg={sum(all_results_counts)//len(all_results_counts)} per query")
        print(f"    Relevance:  avg={round(sum(all_relevance)/len(all_relevance)*100,1)}%")

    print(f"\n{'='*80}\n")

    # Save detailed results
    with open("stress_test_results.json", "w") as f:
        json.dump({"queries": all_results, "summary": {
            "intent_accuracy": round(intent_hits/max(total_intent,1)*100, 1),
            "category_accuracy": round(category_hits/max(total_cat,1)*100, 1),
            "avg_latency_ms": sum(all_lats)//len(all_lats) if all_lats else 0,
            "avg_relevance_pct": round(sum(all_relevance)/len(all_relevance)*100, 1) if all_relevance else 0,
        }}, f, indent=2)
    print(f"  Detailed results saved to stress_test_results.json")


if __name__ == "__main__":
    run_test()
