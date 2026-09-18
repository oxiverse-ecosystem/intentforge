import json, urllib.request, urllib.parse

def search(q, limit=15):
    url = f"http://localhost:4000/search?q={urllib.parse.quote(q)}&limit={limit}"
    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            return json.loads(resp.read())
    except Exception as e:
        return {"error": str(e)}

print("=" * 70)
print("TEST 1: jaguar the car vs the animal top speed comparison")
print("=" * 70)
d = search("jaguar the car vs the animal top speed comparison", 15)
if "error" in d:
    print("ERROR:", d["error"])
else:
    print(f"Total: {d.get('total')}, Intent: {d.get('intent')}")
    print(f"Results ({len(d.get('results',[]))}):")
    for i, r in enumerate(d.get("results", [])):
        t = r.get("title", "")[:80]
        s = r.get("score", 0)
        print(f"  {i+1}. [{s:.4f}] {t}")

print()
print("=" * 70)
print("TEST 2: samsung galaxy s25 ultra camera specs compared to iphone 16 pro max in low light")
print("=" * 70)
d = search("samsung galaxy s25 ultra camera specs compared to iphone 16 pro max in low light", 10)
if "error" in d:
    print("ERROR:", d["error"])
else:
    print(f"Total: {d.get('total')}, Intent: {d.get('intent')}")
    print(f"Results ({len(d.get('results',[]))}):")
    for i, r in enumerate(d.get("results", [])):
        t = r.get("title", "")[:80]
        s = r.get("score", 0)
        print(f"  {i+1}. [{s:.4f}] {t}")

print()
print("=" * 70)
print("TEST 3: python vs rust performance comparison")
print("=" * 70)
d = search("python vs rust performance comparison", 10)
if "error" in d:
    print("ERROR:", d["error"])
else:
    print(f"Total: {d.get('total')}, Intent: {d.get('intent')}")
    print(f"Results ({len(d.get('results',[]))}):")
    for i, r in enumerate(d.get("results", [])):
        t = r.get("title", "")[:80]
        s = r.get("score", 0)
        print(f"  {i+1}. [{s:.4f}] {t}")
