#!/bin/bash
BASE="http://localhost:4000/search?q="
PASS=0; FAIL=0; TOTAL_MS=0; COUNT=0

run() {
  local query="$1" expected="$2"
  local encoded=$(echo "$query" | sed 's/ /+/g')
  local start_ms=$(date +%s%N)
  local resp=$(curl -s "${BASE}${encoded}")
  local end_ms=$(date +%s%N)
  local elapsed_ms=$(( (end_ms - start_ms) / 1000000 ))
  TOTAL_MS=$((TOTAL_MS + elapsed_ms))
  COUNT=$((COUNT + 1))

  local intent=$(echo "$resp" | sed -n 's/.*"intent":"\([^"]*\)".*/\1/p')
  local n=$(echo "$resp" | sed -n 's/.*"count":\([0-9]*\).*/\1/p')
  [ -z "$n" ] && n=$(echo "$resp" | grep -o '"results":\[' | wc -l)
  local rcount=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('results',[])))" 2>/dev/null)
  [ -z "$rcount" ] && rcount=0

  local sources=$(echo "$resp" | python3 -c "
import sys,json
d=json.load(sys.stdin)
s=set()
for r in d.get('results',[]):
    for x in r.get('sources',[]): s.add(x)
print(','.join(sorted(s))[:40])
" 2>/dev/null)
  [ -z "$sources" ] && sources="none"

  local top=$(echo "$resp" | python3 -c "
import sys,json
d=json.load(sys.stdin)
r=d.get('results',[])
print(r[0]['title'][:55] if r else 'none')
" 2>/dev/null)
  [ -z "$top" ] && top="none"

  if [ "$intent" = "$expected" ]; then
    printf "  PASS  %-42s → %-14s %3d results %4dms  [%s]\n" "\"$query\"" "$intent" "$rcount" "$elapsed_ms" "$sources"
    printf "        top: %s\n" "$top"
    PASS=$((PASS + 1))
  else
    printf "  FAIL  %-42s → %-14s (exp: %-14s) %3d results %4dms\n" "\"$query\"" "$intent" "$expected" "$rcount" "$elapsed_ms"
    printf "        top: %s\n" "$top"
    FAIL=$((FAIL + 1))
  fi
}

echo "════════════════════════════════════════════════════════════════════════════════════════════════════════"
echo "  INTENTFORGE v2 — STRESS TEST (50 unique queries)"
echo "════════════════════════════════════════════════════════════════════════════════════════════════════════"

echo ""
echo "── NAVIGATIONAL ──"
run "stack overflow" "navigational"
run "linear app" "navigational"
run "hugging face" "navigational"
run "crates.io" "navigational"
run "mdn web docs" "navigational"
run "tailwind css" "navigational"
run "next.js" "navigational"
run "deno land" "navigational"

echo ""
echo "── INFORMATIONAL ──"
run "what is a transformer model" "informational"
run "explain distributed systems" "informational"
run "what does a load balancer do" "informational"
run "define microservices architecture" "informational"
run "what is event driven programming" "informational"
run "meaning of idempotent" "informational"

echo ""
echo "── TECHNICAL ──"
run "grpc protocol buffers" "technical"
run "kubernetes ingress controller" "technical"
run "postgres query optimization" "technical"
run "rust lifetime annotations" "technical"
run "react server components" "technical"
run "oauth2 authorization code flow" "technical"
run "webassembly simd" "technical"
run "graphql subscriptions" "technical"

echo ""
echo "── HOW-TO ──"
run "how to set up a reverse proxy" "how-to"
run "how to implement rate limiting" "how-to"
run "how to configure ssl certificates" "how-to"
run "how to build a rest api in rust" "how-to"
run "steps to deploy a docker container" "how-to"
run "guide to learning system design" "how-to"
run "how to use git rebase interactive" "how-to"
run "how to optimize postgres queries" "how-to"

echo ""
echo "── COMPARISON ──"
run "grpc vs rest api" "comparison"
run "monolith vs microservices" "comparison"
run "tailwind vs bootstrap" "comparison"
run "deno vs node" "comparison"
run "best database for real time apps" "comparison"
run "which message queue to use" "comparison"
run "terraform vs pulumi" "comparison"
run "svelte vs react performance" "comparison"

echo ""
echo "── TRANSACTIONAL ──"
run "buy domain name cheap" "transactional"
run "download rust toolchain" "transactional"
run "sign up for cloudflare" "transactional"
run "purchase vps hosting" "transactional"
run "install ubuntu server" "transactional"
run "subscribe to pluralsight" "transactional"

echo ""
echo "── FRESH ──"
run "latest rust release notes" "fresh"
run "new features in react 19" "fresh"
run "cve 2026 critical vulnerabilities" "fresh"
run "recent breakthroughs in quantum computing" "fresh"
run "latest typescript update" "fresh"

echo ""
echo "── COMPLEX / MULTI-INTENT ──"
run "how to use grpc with rust and deploy to kubernetes" "how-to"
run "best framework for building rest api in python 2026" "comparison"
run "what is the difference between graphql and grpc" "comparison"
run "install docker on ubuntu and configure networking" "how-to"
run "latest postgres vs mysql benchmark results" "comparison"
run "how does oauth2 work with react and nextjs" "how-to"
run "buy mechanical keyboard for programming" "transactional"
run "what is webassembly and why should i use it" "informational"

echo ""
echo "════════════════════════════════════════════════════════════════════════════════════════════════════════"
AVG_MS=$((TOTAL_MS / COUNT))
echo "  RESULTS: $PASS passed, $FAIL failed out of $COUNT queries"
echo "  ACCURACY: $((PASS * 100 / COUNT))%"
echo "  LATENCY: total=${TOTAL_MS}ms  avg=${AVG_MS}ms per query"
echo "════════════════════════════════════════════════════════════════════════════════════════════════════════"
