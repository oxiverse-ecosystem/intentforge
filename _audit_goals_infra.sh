#!/bin/bash
set -e
cd /c/Users/Likhith/Documents/Projects/intentforge

echo "=== A17: POST /goals/:id/answers ==="
# First create a goal to get an id
GOAL_ID=$(curl -s -X POST http://localhost:4000/goals -H "Content-Type: application/json" -d '{"goal":"learn rust programming"}' | py -3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('goal_id',''))")
echo "Created goal_id: $GOAL_ID"

curl -s -X POST "http://localhost:4000/goals/$GOAL_ID/answers" -H "Content-Type: application/json" -d '{"answers":{"1":"3 months — Quarter project","2":"10-20 hours — Half-time commitment","3":"I want to build a web API with axum","4":"A working deployed project"}}' | py -3 -c "import sys,json; d=json.load(sys.stdin); rp=d.get('roadmap',{}); print('roadmap_keys:', sorted(rp.keys())); print('total_phases:', rp.get('total_phases')); print('len(phases):', len(rp.get('phases',[]))); print('total_phases==len(phases):', rp.get('total_phases') == len(rp.get('phases',[])) if rp.get('total_phases') is not None else 'NULL total_phases = BUG')" 2>&1
echo ""
echo "=== A18: GET /goals/:id ==="
curl -s "http://localhost:4000/goals/$GOAL_ID" | py -3 -c "import sys,json; d=json.load(sys.stdin); print('keys:', sorted(d.keys())); print('status:', d.get('status','MISSING'))" 2>&1
echo ""
echo "=== INFRA AUDIT ==="
echo "--- tor2 reachability ---"
docker exec if-dev-gateway getent hosts tor2 2>&1 || echo "FAILED: tor2 not reachable"
echo ""
echo "--- searxng reachability ---"
docker exec if-dev-gateway getent hosts searxng 2>&1 || echo "FAILED: searxng not reachable"
echo ""
echo "--- intent-engine reachability ---"
docker exec if-dev-gateway getent hosts intent-engine 2>&1 || echo "FAILED: intent-engine not reachable"
echo ""
echo "--- crawler reachability ---"
docker exec if-dev-gateway getent hosts crawler 2>&1 || echo "FAILED: crawler not reachable"
echo ""
echo "--- indexer reachability ---"
docker exec if-dev-gateway getent hosts indexer 2>&1 || echo "FAILED: indexer not reachable"
echo ""
echo "--- tor2 circuit check (live search) ---"
curl -s "http://localhost:4000/search?q=test+query+for+tor+check" | py -3 -c "import sys,json; d=json.load(sys.stdin); print('tor_circuit_open:', 'Circuit OPEN' in json.dumps(d))" 2>&1
