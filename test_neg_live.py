import urllib.request
import urllib.parse
import json

BASE = "http://localhost:4000"

queries = [
    "wireless headphones price:<100 not bose after:2025-01-01",
    "wireless headphones not from bose",
    "best headphones not sony under $200",
    "learn python without watching videos",
]

for q in queries:
    url = f"{BASE}/search?" + urllib.parse.urlencode({"q": q})
    with urllib.request.urlopen(url, timeout=60) as r:
        d = json.loads(r.read().decode())
    print(f"Query: {q}")
    print(f"  applied_constraints: {d.get('applied_constraints')}")
    print(f"  ignored_constraints: {d.get('ignored_constraints')}")
    sc = d.get('structured_constraints', {})
    print(f"  negative: {sc.get('negative')}")
    print(f"  positive: {sc.get('positive')}")
    print(f"  price_lt: {sc.get('price_lt')}")
    print()
