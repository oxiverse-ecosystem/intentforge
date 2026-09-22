import json, urllib.request, urllib.parse

# Check the full /shopping response to see how many results have commerce
url = f'http://localhost:4000/shopping?q=iphone+16+pro+max+price'
with urllib.request.urlopen(url, timeout=30) as r:
    d = json.loads(r.read())

rs = d.get('results', [])
print(f'Total results: {len(rs)}')
with_commerce = 0
for r in rs:
    c = r.get('commerce')
    if c and c.get('data'):
        with_commerce += 1
        data = c['data']
        price = data.get('price') or f'range {data.get("price_low")}-{data.get("price_high")}'
        print(f'  ✓ {r["url"][:60]} | price={price} cur={data.get("currency")} src={c.get("source")}')
    else:
        cp = r.get('commerce_provenance', {})
        print(f'  ✗ {r["url"][:60]} | provenance_src={cp.get("source")}')

print(f'\nWith commerce: {with_commerce}/{len(rs)}')
