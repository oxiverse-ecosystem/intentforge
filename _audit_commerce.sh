#!/bin/bash
set -e
cd /c/Users/Likhith/Documents/Projects/intentforge

echo "=== INFRA: Services reachable via localhost in gateway netns ==="
echo "--- searxng localhost:8080 ---"
docker exec if-dev-gateway sh -c 'curl -s -m 5 -o /dev/null -w "HTTP %{http_code}" http://localhost:8080/search?q=test&format=json' 2>&1
echo ""
echo "--- intent-engine localhost:8000 ---"
docker exec if-dev-gateway sh -c 'curl -s -m 5 -o /dev/null -w "HTTP %{http_code}" http://localhost:8000/health' 2>&1
echo ""
echo "--- indexer localhost:6000 ---"
docker exec if-dev-gateway sh -c 'curl -s -m 5 -o /dev/null -w "HTTP %{http_code}" http://localhost:6000/health' 2>&1
echo ""
echo "--- crawler localhost:8080 ---"
docker exec if-dev-gateway sh -c 'curl -s -m 5 -o /dev/null -w "HTTP %{http_code}" http://localhost:8080/health' 2>&1
echo ""
echo "--- tor2 (bridge) getent ---"
docker exec if-dev-gateway getent hosts tor2 2>&1
echo ""
echo "--- searxng2 (via tor2 bridge) ---"
docker exec if-dev-gateway sh -c 'curl -s -m 5 -o /dev/null -w "HTTP %{http_code}" http://if-dev-tor2:8081/' 2>&1
echo ""
echo ""
echo "=== COMMERCE AUDIT: Hardcoding sweep ==="
echo "--- data/commerce dir ---"
ls -la data/commerce/ 2>&1
echo ""
echo "--- commerce json files ---"
cat data/commerce/*.json 2>&1 | head -100
echo ""
echo "--- git diff master...HEAD (stat) ---"
git diff master...HEAD --stat 2>&1 | head -30
echo ""
echo "--- git diff master...HEAD (full diff, source) ---"
git diff master...HEAD -- '*.rs' 2>&1 | head -200
