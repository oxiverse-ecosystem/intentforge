import urllib.request
import urllib.parse
import json

BASE = "http://localhost:4000"

# Test the exact failing queries from CI
queries = [
    # From test_other_brand_negatives_applied_only (test_api_schema.py)
    "wireless headphones price:<100 not bose after:2025-01-01",
    "wireless headphones price:<100 not logitech after:2025-01-01",
    "wireless headphones price:<100 not nike after:2025-01-01",
    # From test_negation_full_suite_with_price
    "best noise cancelling headphones not from sony under $200 for travel with good battery life",
]

for q in queries:
    url = f"{BASE}/search?" + urllib.parse.urlencode({"q": q})
    with urllib.request.urlopen(url, timeout=60) as r:
        d = json.loads(r.read().decode())
    print(f"Query: {q}")
    print(f"  applied_constraints: {d.get('applied_constraints')}")
    sc = d.get('structured_constraints', {})
    print(f"  negative: {sc.get('negative')}")
    print(f"  positive: {sc.get('positive')}")
    print(f"  price_lt: {sc.get('price_lt')}")
    print(f"  price_verified: {d.get('price_verified')}")
    print()

# Test media endpoints
for endpoint in ['/images', '/videos', '/news']:
    url = f"{BASE}{endpoint}?" + urllib.parse.urlencode({"q": "rust programming"})
    with urllib.request.urlopen(url, timeout=60) as r:
        d = json.loads(r.read().decode())
    results = d.get('results', [])
    print(f"GET {endpoint}: count={d.get('count')}, results_len={len(results)}")
    if results:
        print(f"  first result keys: {sorted(results[0].keys())}")
    print()
