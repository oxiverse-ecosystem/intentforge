#!/usr/bin/env python3
"""
IntentForge v2 — HARD STRESS TEST
Hits ALL API endpoints concurrently with diverse queries.
Measures latency distributions, throughput, error rates, and bottleneck detection.
"""
import urllib.request, urllib.parse, urllib.error
import json, sys, time, threading, statistics, math, os, socket

BASE = "http://localhost:4000"
TIMEOUT = 120  # generous timeout for slow queries
CONCURRENCY = [1, 2, 4, 8, 16]  # concurrency levels to test
QUERIES_PER_BATCH = 10  # total queries per endpoint per concurrency level

# Diverse query pool — mixes intents, constraints, lengths
SEARCH_QUERIES = [
    # Short navigational
    "python docs",
    "github",
    "rust programming",
    "docker",
    "kubernetes",
    "sqlite",
    "postgresql",
    "nginx",
    "redis",
    "linux kernel",
    # Technical with constraints
    "python web framework not django",
    "javascript framework without react",
    "static site generator without nextjs vue",
    "rust async web framework",
    "go web framework",
    "c++ build system",
    "typescript orm",
    "rust testing framework",
    # Comparisons
    "react vs vue vs angular 2026",
    "postgresql vs mysql performance",
    "aws lambda vs google cloud functions",
    "docker vs podman",
    "traefik vs nginx ingress",
    "rust vs go vs python performance",
    # How-to / informational
    "how to deploy docker compose",
    "how to configure nginx reverse proxy",
    "how to use redis with python",
    "what is kubernetes",
    "explain tcp congestion control",
    "machine learning tutorial python",
    # Multi-constraint complex
    "vector database without pinecone without weaviate without qdrant",
    "container registry without docker without ecr without gcr",
    "monitoring stack without prometheus without grafana without datadog",
    "task queue without redis without rabbitmq without celery",
    "log aggregation without elasticsearch without opensearch without splunk",
    "browser engine without blink without gecko without webkit",
    "backend framework without node without django without flask",
    "message broker without kafka without nats without pulsar",
    "object storage without s3 without minio without wasabi",
    "password manager without bitwarden without lastpass without 1password",
    # Complex long queries
    "what monitoring stack should a small startup use for kubernetes microservices running on aws",
    "how to build a real time chat application with websockets and redis",
    "best free static site generator for documentation without react",
    "distributed sql database with postgres compatibility and horizontal scaling",
    "lightweight container orchestration alternative to kubernetes for small teams",
    # Fresh/news
    "latest AI news",
    "new javascript frameworks 2026",
    "rust 2026 release",
    "cloud computing trends 2026",
    "cybersecurity vulnerabilities 2026",
    # Transactional
    "buy raspberry pi 5",
    "download postgresql",
    "best laptop for programming",
    "cheap cloud hosting",
    "buy domain name",
    # Edge cases
    "not django",
    "without react",
    "minus vim",
    "alternative to notion",
    "text editor without vim without emacs",
    "lightweight javascript bundler without webpack without vite",
    "search engine alternative to google",
    "css framework besides bootstrap",
    "programming language other than java",
]

IMAGE_QUERIES = [
    "rust logo",
    "python programming",
    "docker containers",
    "cloud architecture diagram",
    "machine learning",
    "kubernetes logo",
    "linux kernel",
    "web development",
    "database schema",
    "api gateway",
]

VIDEO_QUERIES = [
    "rust tutorial",
    "kubernetes tutorial",
    "python machine learning",
    "docker compose tutorial",
    "linux system administration",
    "go programming tutorial",
    "javascript tutorial",
    "devops tutorial",
    "database design",
    "web development tutorial",
]

NEWS_QUERIES = [
    "AI news",
    "cybersecurity",
    "cloud computing",
    "programming languages",
    "open source",
    "technology news",
    "startups",
    "data science",
    "web development",
    "devops",
]


def fetch_url(url, timeout=TIMEOUT):
    """Fetch a URL and return (status, duration_ms, size, error)."""
    start = time.time()
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = resp.read()
            elapsed = (time.time() - start) * 1000
            return (resp.status, elapsed, len(data), None)
    except urllib.error.HTTPError as e:
        elapsed = (time.time() - start) * 1000
        return (e.code, elapsed, 0, str(e))
    except Exception as e:
        elapsed = (time.time() - start) * 1000
        return (0, elapsed, 0, str(e))


def run_concurrent(endpoint, queries, concurrency, label):
    """Run queries at a given concurrency level."""
    results = []
    lock = threading.Lock()
    errors = []
    latencies = []
    response_sizes = []
    status_codes = {}

    def worker(q):
        url = f"{BASE}{endpoint}{urllib.parse.urlencode({'q': q})}"
        status, dur, size, err = fetch_url(url)
        with lock:
            results.append((q, status, dur, size, err))
            if err:
                errors.append((q, err))
            else:
                latencies.append(dur)
                response_sizes.append(size)
                status_codes[status] = status_codes.get(status, 0) + 1

    # Create threaded pool
    for i in range(0, len(queries), concurrency):
        batch = queries[i:i+concurrency]
        threads = [threading.Thread(target=worker, args=(q,)) for q in batch]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

    n = len(latencies)
    if n == 0:
        return {
            "label": f"{label} (concurrency={concurrency})",
            "count": len(queries),
            "ok": 0,
            "errors": len(errors),
            "error_rate": 1.0,
            "latency_ms": {"min": 0, "max": 0, "mean": 0, "median": 0,
                          "p90": 0, "p95": 0, "p99": 0, "stddev": 0},
            "throughput_qps": 0,
            "total_duration_ms": 0,
            "status_codes": status_codes,
        }

    latencies.sort()
    mean = statistics.mean(latencies)
    total_time = sum(latencies)
    throughput = n / (total_time / 1000) if total_time > 0 else 0

    return {
        "label": f"{label} (concurrency={concurrency})",
        "count": len(queries),
        "ok": n,
        "errors": len(errors),
        "error_rate": len(errors) / len(queries) if queries else 0,
        "latency_ms": {
            "min": min(latencies),
            "max": max(latencies),
            "mean": round(mean, 1),
            "median": latencies[n // 2] if n % 2 else (latencies[n//2-1] + latencies[n//2]) / 2,
            "p90": latencies[int(n * 0.9)],
            "p95": latencies[int(n * 0.95)],
            "p99": latencies[int(n * 0.99)],
            "stddev": round(statistics.stdev(latencies), 1) if n > 1 else 0,
        },
        "throughput_qps": round(throughput, 1),
        "total_duration_ms": round(total_time, 1),
        "status_codes": status_codes,
    }


def print_results(results, title):
    print(f"\n{'='*80}")
    print(f"  {title}")
    print(f"{'='*80}")
    print(f"  {'Endpoint':<50} {'OK':>4} {'Err':>4}  {'P50(ms)':>8} {'P95(ms)':>8} {'P99(ms)':>8}  {'QPS':>7}")
    print(f"  {'-'*50} {'-'*4} {'-'*4}  {'-'*8} {'-'*8} {'-'*8}  {'-'*7}")
    for r in results:
        l = r['latency_ms']
        ok = r['ok']
        errs = r['errors']
        if ok == 0:
            print(f"  {r['label']:<50} {ok:>4} {errs:>4}  {'N/A':>8} {'N/A':>8} {'N/A':>8}  {'N/A':>7}")
        else:
            print(f"  {r['label']:<50} {ok:>4} {errs:>4}  {l['median']:>7.0f} {l['p95']:>7.0f} {l['p99']:>7.0f}  {r['throughput_qps']:>6.0f}")


if __name__ == "__main__":
    print("=" * 80)
    print("  INTENTFORGE v2 — HARD STRESS TEST")
    print(f"  Started: {time.strftime('%Y-%m-%dT%H:%M:%S')}")
    print(f"  Base URL: {BASE}")
    print(f"  Queries: {len(SEARCH_QUERIES)} search + {len(IMAGE_QUERIES)} image + {len(VIDEO_QUERIES)} video + {len(NEWS_QUERIES)} news")
    print(f"  Concurrency levels: {CONCURRENCY}")
    print("=" * 80)

    all_results = []

    # ── Test 1: Health check burst ──
    print("\n\n>>> STAGE 1: Rapid health check burst (100 requests)")
    health_results = run_concurrent("/health?", ["ok"] * 100, 50, "GET /health")
    all_results.append(health_results)

    # ── Test 2: Search endpoint ──
    print("\n\n>>> STAGE 2: /search endpoint — full stack")
    for c in CONCURRENCY:
        r = run_concurrent("/search?", SEARCH_QUERIES[:QUERIES_PER_BATCH], c, f"GET /search (q={QUERIES_PER_BATCH})")
        all_results.append(r)

    # ── Test 3: Full search with ALL queries at optimal concurrency ──
    print("\n\n>>> STAGE 3: /search — ALL queries at concurrency=4")
    r = run_concurrent("/search?", SEARCH_QUERIES, 4, "GET /search ALL")
    all_results.append(r)

    # ── Test 4: /search/fast endpoint ──
    print("\n\n>>> STAGE 4: /search/fast — local index only")
    for c in CONCURRENCY:
        r = run_concurrent("/search/fast?", SEARCH_QUERIES[:QUERIES_PER_BATCH], c, f"GET /search/fast (q={QUERIES_PER_BATCH})")
        all_results.append(r)

    # ── Test 5: Images endpoint ──
    print("\n\n>>> STAGE 5: /images endpoint")
    for c in [1, 2, 4, 8]:
        r = run_concurrent("/images?", IMAGE_QUERIES, c, f"GET /images")
        all_results.append(r)

    # ── Test 6: Videos endpoint ──
    print("\n\n>>> STAGE 6: /videos endpoint")
    for c in [1, 2, 4]:
        r = run_concurrent("/videos?", VIDEO_QUERIES, c, f"GET /videos")
        all_results.append(r)

    # ── Test 7: News endpoint ──
    print("\n\n>>> STAGE 7: /news endpoint")
    for c in [1, 2, 4]:
        r = run_concurrent("/news?", NEWS_QUERIES, c, f"GET /news")
        all_results.append(r)

    # ── Test 8: Mixed endpoint burst (real-world pattern) ──
    print("\n\n>>> STAGE 8: Mixed endpoint burst (simulating real-world traffic)")
    mixed_queries = ([f"/search?q={urllib.parse.quote(q)}" for q in SEARCH_QUERIES[:5]] +
                     [f"/images?q={urllib.parse.quote(q)}" for q in IMAGE_QUERIES[:5]] +
                     [f"/videos?q={urllib.parse.quote(q)}" for q in VIDEO_QUERIES[:5]] +
                     [f"/news?q={urllib.parse.quote(q)}" for q in NEWS_QUERIES[:5]])
    # Convert to simple q params for run_concurrent
    # We'll test this differently
    mixed_results = []
    lock = threading.Lock()
    def mixed_worker(path):
        url = f"{BASE}{path}"
        status, dur, size, err = fetch_url(url)
        with lock:
            mixed_results.append((path, status, dur, size, err))
    
    threads = [threading.Thread(target=mixed_worker, args=(path,)) for path in mixed_queries]
    for t in threads: t.start()
    for t in threads: t.join()
    
    ok = sum(1 for r in mixed_results if r[1] == 200)
    errs = sum(1 for r in mixed_results if r[1] != 200)
    latencies = [r[2] for r in mixed_results if r[1] == 200]
    if latencies:
        latencies.sort()
        print(f"\n  Mixed burst ({len(mixed_queries)} req): {ok} ok, {errs} err")
        print(f"  Latency: median={latencies[len(latencies)//2]:.0f}ms mean={statistics.mean(latencies):.0f}ms")

    # ── Combined Summary ──
    print("\n\n" + "=" * 80)
    print("  COMBINED RESULTS SUMMARY")
    print("=" * 80)
    print_results(all_results, "All Endpoints")

    # ── Bottleneck Analysis ──
    print("\n\n" + "=" * 80)
    print("  BOTTLENECK ANALYSIS")
    print("=" * 80)

    # Aggregate by endpoint
    endpoints = {}
    for r in all_results:
        label = r['label']
        if 'search' in label.lower() and 'fast' not in label.lower():
            key = '/search'
        elif 'search/fast' in label.lower() or 'fast' in label.lower():
            key = '/search/fast'
        elif 'images' in label.lower():
            key = '/images'
        elif 'videos' in label.lower():
            key = '/videos'
        elif 'news' in label.lower():
            key = '/news'
        elif 'health' in label.lower():
            key = '/health'
        else:
            key = 'other'
        if key not in endpoints:
            endpoints[key] = []
        endpoints[key].append(r)

    for ep, results_list in endpoints.items():
        # Find best throughput
        best = max(results_list, key=lambda x: x['throughput_qps'])
        # Find lowest p95
        best_latency = min(results_list, key=lambda x: x['latency_ms'].get('p95', float('inf')))
        print(f"\n  {ep}:")
        print(f"    Best throughput: {best['throughput_qps']} qps at concurrency={best['label'].split('=')[-1].rstrip(')')}")
        print(f"    Best latency:    P95={best_latency['latency_ms']['p95']:.0f}ms at concurrency={best_latency['label'].split('=')[-1].rstrip(')')}")
        print(f"    Error rate:      {best['error_rate']*100:.1f}%")

    # Bottleneck detection
    print("\n\n>>> POTENTIAL BOTTLENECKS:\n")
    for ep, results_list in endpoints.items():
        # Check if throughput degrades with concurrency
        throughputs = [(int(r['label'].split('=')[-1].rstrip(')')), r['throughput_qps']) 
                      for r in results_list if r['throughput_qps'] > 0]
        if len(throughputs) >= 2:
            throughputs.sort()
            first_tp = throughputs[0][1]
            last_tp = throughputs[-1][1]
            # If throughput doesn't scale with concurrency, it's a bottleneck
            concurrency_ratio = throughputs[-1][0] / max(throughputs[0][0], 1)
            tp_ratio = last_tp / max(first_tp, 0.1)
            if concurrency_ratio > 2 and tp_ratio < 1.3:
                print(f"  ⚠  {ep}: Throughput scales poorly with concurrency ({throughputs[0][0]}→{throughputs[-1][0]} conc, {first_tp:.0f}→{last_tp:.0f} qps)")
            elif concurrency_ratio > 1 and tp_ratio < 1.1:
                print(f"  ⚠  {ep}: Throughput SATURATED — adding concurrency doesn't help")
            elif tp_ratio > 1.5:
                print(f"  ✓  {ep}: Throughput scales well ({first_tp:.0f}→{last_tp:.0f} qps)")
            else:
                print(f"  ~  {ep}: Moderate scaling ({first_tp:.0f}→{last_tp:.0f} qps @ {throughputs[0][0]}→{throughputs[-1][0]} conc)")

    # High-error endpoints
    for ep, results_list in endpoints.items():
        for r in results_list:
            if r['error_rate'] > 0.1:
                print(f"  🔴 {r['label']}: {r['errors']} errors ({r['error_rate']*100:.0f}% error rate)")

    # Slowest queries
    print("\n\n>>> LATENCY DISTRIBUTION BY ENDPOINT:\n")
    for ep, results_list in endpoints.items():
        p95s = [r['latency_ms']['p95'] for r in results_list if r['ok'] > 0]
        if p95s:
            print(f"  {ep:15s}: P95 range = {min(p95s):.0f}ms — {max(p95s):.0f}ms")

    print(f"\n\n  Test finished: {time.strftime('%Y-%m-%dT%H:%M:%S')}")
    print("=" * 80)
