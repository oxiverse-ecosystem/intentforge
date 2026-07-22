import json, time, urllib.request, urllib.parse, statistics

BASE = "http://localhost:4000/search"

def call(q, timeout=12):
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    t0 = time.time()
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            data = json.loads(r.read().decode())
        dt = time.time() - t0
        return data, dt, None
    except Exception as e:
        return None, time.time() - t0, str(e)

def run(q, n=6):
    lats, ns, errs, upstream = [], [], 0, 0
    for _ in range(n):
        data, dt, err = call(q)
        if err:
            errs += 1; lats.append(dt); continue
        lats.append(dt)
        ns.append(len(data.get("results", [])))
        if data.get("error") == "upstream_unavailable":
            upstream += 1
    ok = [x for x in lats]
    return {
        "q": q, "n": n, "errs": errs, "upstream": upstream,
        "n_results_avg": round(statistics.mean(ns), 1) if ns else 0,
        "lat_p50": round(statistics.median(ok), 2),
        "lat_p95": round(sorted(ok)[int(len(ok)*0.95)-1] if len(ok) > 1 else ok[0], 2),
        "lat_max": round(max(ok), 2),
        "under5s": all(x < 5.0 for x in ok),
    }

if __name__ == "__main__":
    queries = [
        "boilerplate site:arxiv.org",
        "function words site:arxiv.org",
        "predictive coding site:arxiv.org",
        "attention mechanism site:arxiv.org",
        "semantic memory site:arxiv.org",
        "Lancaster norms",          # non-site control (should stay 5.5s budget, fast early-return)
    ]
    print(f"{'query':42} n  res  p50   p95   max   <5s  upstream")
    for q in queries:
        r = run(q, n=6)
        print(f"{r['q']:42} {r['n']:2} {r['n_results_avg']:4.0f} {r['lat_p50']:4.2f} {r['lat_p95']:4.2f} {r['lat_max']:4.2f}  {str(r['under5s']):5} {r['upstream']}")
