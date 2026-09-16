import urllib.request
import urllib.parse
import json

BASE = "http://localhost:4000"

# Test 1: "not bose" - should have bose in applied_constraints
q = "wireless headphones price:<100 not bose after:2025-01-01"
url = f"{BASE}/search?" + urllib.parse.urlencode({"q": q})
with urllib.request.urlopen(url, timeout=60) as r:
    d = json.loads(r.read().decode())

print("Query:", q)
print("applied_constraints:", d.get("applied_constraints"))
print("ignored_constraints:", d.get("ignored_constraints"))
print()

# Test 2: /images endpoint
url2 = f"{BASE}/images?" + urllib.parse.urlencode({"q": "rust programming"})
with urllib.request.urlopen(url2, timeout=60) as r:
    d2 = json.loads(r.read().decode())

print("GET /images?q=rust programming")
print("keys:", sorted(d2.keys()))
print("count:", d2.get("count"))
results = d2.get("results", [])
print("len(results):", len(results))
if results:
    print("first result keys:", sorted(results[0].keys()))

# Test 3: /videos endpoint
url3 = f"{BASE}/videos?" + urllib.parse.urlencode({"q": "lofi hip hop beats"})
with urllib.request.urlopen(url3, timeout=60) as r:
    d3 = json.loads(r.read().decode())

print()
print("GET /videos?q=lofi hip hop beats")
print("keys:", sorted(d3.keys()))
results3 = d3.get("results", [])
print("len(results):", len(results3))
if results3:
    print("first result keys:", sorted(results3[0].keys()))

# Test 4: /news endpoint
url4 = f"{BASE}/news?" + urllib.parse.urlencode({"q": "latest ai news"})
with urllib.request.urlopen(url4, timeout=60) as r:
    d4 = json.loads(r.read().decode())

print()
print("GET /news?q=latest ai news")
print("keys:", sorted(d4.keys()))
results4 = d4.get("results", [])
print("len(results):", len(results4))
if results4:
    print("first result keys:", sorted(results4[0].keys()))
