#!/bin/bash
set -e
cd /c/Users/Likhith/Documents/Projects/intentforge

echo "=== A1: GET / ==="
curl -s -w "\nHTTP %{http_code}" http://localhost:4000/
echo ""
echo ""
echo "=== A2: GET /health ==="
curl -s http://localhost:4000/health
echo ""
echo ""
echo "=== A3: GET /search (complex query) ==="
curl -s "http://localhost:4000/search?q=rust+async+web+framework+not+django" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['query','intent','category','confidence','results_before_filter','results_after_filter','total','limit','offset','has_more']}, indent=2)); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[])))" 2>&1 | head -40
echo ""
echo "=== A4: GET /search/fast ==="
curl -s "http://localhost:4000/search/fast?q=rust+programming" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['count','source']}, indent=2)); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[])))" 2>&1
echo ""
echo "=== A5: GET /images ==="
curl -s "http://localhost:4000/images?q=rust+programming" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['count','query']}, indent=2)); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[]))); print('first_result_keys:', sorted(d['results'][0].keys()) if d.get('results') else 'none')" 2>&1
echo ""
echo "=== A6: GET /videos ==="
curl -s "http://localhost:4000/videos?q=rust+tutorial" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['count','query']}, indent=2)); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[]))); print('first_result_keys:', sorted(d['results'][0].keys()) if d.get('results') else 'none')" 2>&1
echo ""
echo "=== A7: GET /news ==="
curl -s "http://localhost:4000/news?q=latest+AI" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['count','query']}, indent=2)); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[]))); print('first_result_keys:', sorted(d['results'][0].keys()) if d.get('results') else 'none')" 2>&1
echo ""
echo "=== A8: GET /spellcheck (typo) ==="
curl -s "http://localhost:4000/spellcheck?q=ngnix+tutoriaal" | py -3 -m json.tool 2>&1 | head -20
echo ""
echo "=== A9: GET /analyze ==="
curl -s "http://localhost:4000/analyze?q=best+laptop+for+programming" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['intent','category','confidence','query']}, indent=2))" 2>&1 | head -20
echo ""
echo "=== A10: GET /inspect ==="
curl -s "http://localhost:4000/inspect?q=macbook+pro+m3+price+in+india" | py -3 -c "import sys,json; d=json.load(sys.stdin); print('keys:', sorted(d.keys())); print('query:', d.get('query','MISSING'))" 2>&1
echo ""
echo "=== A11: GET /intent ==="
curl -s "http://localhost:4000/intent?q=iphone+16+pro+max+price" | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['intent','category','confidence','query']}, indent=2))" 2>&1
echo ""
echo "=== A12: GET /geolocate ==="
curl -s "http://localhost:4000/geolocate" | py -3 -m json.tool 2>&1 | head -15
echo ""
echo "=== A13: GET /shopping ==="
curl -s "http://localhost:4000/shopping?q=iphone+16+pro+max+256gb" | py -3 -c "import sys,json; d=json.load(sys.stdin); print('keys:', sorted(d.keys())); print('result_count:', len(d.get('results',[]))); print('first_affiliate:', d['results'][0].get('affiliate') if d.get('results') else 'none')" 2>&1
echo ""
echo "=== A14: POST /goals ==="
curl -s -X POST http://localhost:4000/goals -H "Content-Type: application/json" -d '{"goal":"learn machine learning"}' | py -3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps({k:d.get(k) for k in ['goal_id','questions','status','total_phases'] if k in d}, indent=2)); print('has goal_id:', 'goal_id' in d); print('questions_count:', len(d.get('questions',[])) if isinstance(d.get('questions'), list) else 'NOT A LIST')" 2>&1
echo ""
echo "=== A15: POST /goals/quick ==="
curl -s -X POST http://localhost:4000/goals/quick -H "Content-Type: application/json" -d '{"goal":"learn rust programming"}' | py -3 -c "import sys,json; d=json.load(sys.stdin); rp=d.get('roadmap',{}); print('roadmap_keys:', sorted(rp.keys())); print('total_phases:', rp.get('total_phases')); print('len(phases):', len(rp.get('phases',[]))); print('total_phases==len(phases):', rp.get('total_phases') == len(rp.get('phases',[])) if rp.get('total_phases') is not None else 'NULL total_phases = BUG')" 2>&1
echo ""
echo "=== A16: GET /goals/leaderboard ==="
curl -s "http://localhost:4000/goals/leaderboard" | py -3 -c "import sys,json; d=json.load(sys.stdin); print('type:', type(d).__name__); print('is_list:', isinstance(d, list)); print('len:', len(d) if isinstance(d, list) else 'N/A'); print('first_item_keys:', sorted(d[0].keys()) if isinstance(d, list) and d else (sorted(d.keys()) if isinstance(d, dict) else 'unknown'))" 2>&1
