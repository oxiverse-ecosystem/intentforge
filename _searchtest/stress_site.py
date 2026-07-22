import json, time, urllib.request, urllib.parse, statistics, sys

BASE = "http://localhost:4000/search"

def call(q, timeout=12):
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    t0 = time.time()
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            data = json.loads(r.read().decode())
        return data, time.time() - t0, None
    except Exception as e:
        return None, time.time() - t0, str(e)

# Distinct site:-constrained queries (the failure-prone class) — variety so we
# don't get fooled by engine/circuit warming on a single query.
SITE_QUERIES = [
    "boilerplate site:arxiv.org",
    "function words site:arxiv.org",
    "predictive coding site:arxiv.org",
    "attention mechanism site:arxiv.org",
    "semantic memory site:arxiv.org",
    "transformer explainability site:arxiv.org",
    "bayesian inference site:arxiv.org",
    "word embedding site:arxiv.org",
    "reinforcement learning site:arxiv.org",
    "knowledge graph site:arxiv.org",
]

PER_QUERY = int(sys.argv[1]) if len(sys.argv) > 1 else 10
GAP = float(sys.argv[2]) if len(sys.argv) > 2 else 0.4  # seconds between calls

total = 0
zero_runs = 0
upstream_runs = 0
err_runs = 0
lats = []
per_query_stats = {}

for q in SITE_QUERIES:
    q_zero = 0
    q_n = []
    for i in range(PER_QUERY):
        data, dt, err = call(q)
        total += 1
        lats.append(dt)
        if err:
            err_runs += 1; q_zero += 1
            print(f"  ERR  {q!r} #{i+1}: {err} ({dt:.2f}s)")
            continue
        n = len(data.get("results", []))
        q_n.append(n)
        if data.get("error") == "upstream_unavailable":
            upstream_runs += 1; q_zero += 1
            print(f"  ZERO {q!r} #{i+1}: upstream_unavailable ({dt:.2f}s)")
        elif n == 0:
            q_zero += 1
            print(f"  ZERO {q!r} #{i+1}: 0 results but no upstream flag ({dt:.2f}s)")
        time.sleep(GAP)
    per_query_stats[q] = (q_zero, PER_QUERY, round(statistics.mean(q_n), 1) if q_n else 0)
    zero_runs += q_zero

lats_sorted = sorted(lats)
def pct(p):
    if not lats_sorted: return 0
    return lats_sorted[min(len(lats_sorted)-1, int(len(lats_sorted)*p))]

print("\n================ STRESS TEST SUMMARY ================")
print(f"queries            : {len(SITE_QUERIES)} distinct site: queries")
print(f"runs total         : {total}")
print(f"zero-result runs   : {zero_runs}   ({100*zero_runs/total:.1f}%)")
print(f"  of which upstream: {upstream_runs}")
print(f"  of which errors  : {err_runs}")
print(f"latency p50/p95/max: {pct(0.5):.2f}s / {pct(0.95):.2f}s / {max(lats):.2f}s")
print(f"under 5s          : {all(x < 5.0 for x in lats)}")
print("\nper-query zero-rate (expect 0/0):")
for q,(z,t,avg) in per_query_stats.items():
    flag = "OK" if z == 0 else "FAIL"
    print(f"  [{flag}] {q:42} zero={z}/{t}  avg_results={avg}")
print("====================================================")
sys.exit(1 if zero_runs > 0 else 0)
