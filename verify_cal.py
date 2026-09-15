import json, urllib.parse, urllib.request, sys

BASE = "http://localhost:4000"

def get(path, q):
    u = f"{BASE}{path}?q=" + urllib.parse.quote(q)
    return json.load(urllib.request.urlopen(urllib.request.Request(u, headers={"User-Agent": "v/1"}), timeout=120))

QUERIES = [
    "latest developments in lab grown meat and cultured protein 2026",
    "how to create a passive income stream from writing technical documentation",
    "best laptop for programming 2026",
    "what is quantum computing",
    "macbook pro m3 price in india",
    "rust vs go performance comparison 2026",
    "iphone 16 pro max price",
]

print("=" * 80)
print("SCORING CALIBRATION VERIFICATION (IFIX-B)")
print("=" * 80)

for q in QUERIES:
    try:
        r = get("/search", q)
        n = len(r.get("results", []))
        before = r.get("results_before_filter", "?")
        after = r.get("results_after_filter", "?")
        top = r.get("results", [{}])[0]
        score = top.get("score", 0)
        title = top.get("title", "")[:60]
        dist_terms = r.get("distinctive_terms_count", r.get("distinctive_terms", "?"))
        print(f"\n[{score:.3f}] {title}")
        print(f"  Query: {q}")
        print(f"  Results: {n} (before={before}, after={after})")
        print(f"  Distinctive terms: {dist_terms}")
    except Exception as e:
        print(f"\n[ERROR] {q}: {e}")

print("\n" + "=" * 80)
