import json, urllib.request, urllib.parse

url = f'http://localhost:4000/search?q=iphone+16+pro+max+price'
with urllib.request.urlopen(url, timeout=30) as r:
    d = json.loads(r.read())
print('shopping field present:', 'shopping' in d)
print('shopping value:', d.get('shopping'))
print('intent:', d.get('intent'))
print('total results:', d.get('total'))

# check commerce_provenance on results
rs = d.get('results', [])
for i, r in enumerate(rs[:5]):
    cp = r.get('commerce_provenance')
    c = r.get('commerce')
    print(f'\n--- result {i} ---')
    print(f'  url: {r["url"][:80]}')
    print(f'  commerce_provenance: {json.dumps(cp, indent=4) if cp else "MISSING"}')
    print(f'  commerce: {json.dumps(c, indent=4) if c else "MISSING"}')
