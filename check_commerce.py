import json, urllib.request, urllib.parse, time

def check(q):
    url = f'http://localhost:4000/search?q={urllib.parse.quote(q)}'
    print(f'\n=== {q} ===')
    # Retry up to 3 times (cold cache can take 15-20s)
    for attempt in range(3):
        try:
            with urllib.request.urlopen(url, timeout=40) as r:
                d = json.loads(r.read())
            rs = d.get('results', [])
            wc = sum(1 for r in rs if r.get('commerce'))
            print(f'Total: {len(rs)}')
            print(f'Commerce: {wc}/{len(rs)}')
            print(f'Partial: {d.get("partial_results")}')
            for r in rs[:10]:
                c = r.get('commerce')
                tag = 'YES' if c else 'NO '
                print(f'  [{tag}] {r["url"][:80]}')
                if c and c.get('data'):
                    data = c['data']
                    price = data.get('price') or f'range {data.get("price_low")}-{data.get("price_high")}'
                    print(f'       price={price} cur={data.get("currency")} src={c.get("source")}')
            return
        except Exception as e:
            print(f'  attempt {attempt+1}: {e}')
            time.sleep(3)

check('iphone 16 pro max price')
check('buy sony wh-1000xm5 headphones')
check('rust ownership')
