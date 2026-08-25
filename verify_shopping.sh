#!/usr/bin/env bash
set -e
echo "=== /health ==="
curl -s -m 5 localhost:4000/health; echo ""
echo "=== /shopping (transactional) ==="
curl -s -m 25 "localhost:4000/shopping?q=best%20wireless%20earbuds%20under%2050&count=5" -o /tmp/shop.json -w "HTTP %{http_code}\n"
echo "--- first result (shopping endpoint) ---"
python - <<'PY'
import json
d=json.load(open('/tmp/shop.json'))
print('num_results', len(d.get('results',[])))
r=d['results'][0] if d.get('results') else {}
for k in ['url','title','commerce_provenance','commerce','affiliate']:
    v=r.get(k)
    print('---',k,'---')
    print(json.dumps(v,indent=2)[:800])
PY
echo ""
echo "=== compare /shopping vs /search order (should be identical URLs) ==="
curl -s -m 25 "localhost:4000/search?q=best%20wireless%20earbuds%20under%2050&count=5" -o /tmp/search.json -w "HTTP %{http_code}\n"
python - <<'PY'
import json
a=json.load(open('/tmp/shop.json')).get('results',[])
b=json.load(open('/tmp/search.json')).get('results',[])
sa=[r.get('url') for r in a]
sb=[r.get('url') for r in b]
print('shopping order:', sa)
print('search   order:', sb)
print('IDENTICAL ORDER:', sa==sb)
PY
