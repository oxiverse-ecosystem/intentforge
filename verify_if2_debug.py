import json, urllib.request

url = 'http://localhost:4000/search?q=what+are+the+long+term+health+effects+of+intermittent+fasting&debug=true'
req = urllib.request.Request(url)
with urllib.request.urlopen(req) as resp:
    data = json.loads(resp.read().decode())

# Print all fields
for key in data:
    if key != 'results':
        print(f"{key}: {data[key]}")

print("\n--- All results ---")
for i, r in enumerate(data.get('results', [])):
    print(f"{i+1}. score={r['score']:.4f} src={r.get('sources',[])} is_local={r.get('is_local')}")
    print(f"   title={r['title'][:80]}")
    print(f"   url={r['url'][:80]}")
