#!/usr/bin/env bash
set -u
BASE="http://localhost:4000"
OUT="./docs/_generated/_round_v2_raw.md"
BODY="./docs/_generated/_round_v2_body.txt"
META="./docs/_generated/_round_v2_meta.txt"
: > "$OUT"

emit() { printf '%s\n' "$1" >> "$OUT"; }

get_call() {
  local label="$1" path="$2"
  emit "### ${label}"
  emit "REQ: GET ${path}"
  curl -s -m 30 -o "$BODY" -w '%{http_code}\n%{time_total}\n' "${BASE}${path}" > "$META"
  local http time
  http=$(sed -n '1p' "$META")
  time=$(sed -n '2p' "$META")
  emit "HTTP ${http}  time ${time}s"
  emit '```json'
  python -c "import json; d=open('${BODY}').read();
try:
    print(json.dumps(json.loads(d), indent=2, ensure_ascii=False)[:6000])
except Exception as e:
    print(d[:4000])" >> "$OUT"
  emit '```'
  emit ""
}

post_call() {
  local label="$1" path="$2" json="$3"
  emit "### ${label}"
  emit "REQ: POST ${path}"
  emit "BODY: ${json}"
  curl -s -m 60 -X POST "${BASE}${path}" \
    -H 'Content-Type: application/json' \
    -d "${json}" \
    -o "$BODY" -w '%{http_code}\n%{time_total}\n' > "$META"
  local http time
  http=$(sed -n '1p' "$META")
  time=$(sed -n '2p' "$META")
  emit "HTTP ${http}  time ${time}s"
  emit '```json'
  python -c "import json; d=open('${BODY}').read();
try:
    print(json.dumps(json.loads(d), indent=2, ensure_ascii=False)[:9000])
except Exception as e:
    print(d[:6000])" >> "$OUT"
  emit '```'
  python -c "import json; d=open('${BODY}').read();
try:
    o=json.loads(d); idv=o.get('goal_id'); print(idv if idv else '')
except: print('')" > /tmp/if_id.txt
  emit ""
}

echo "=== round v2 exercise start $(date -u +%FT%TZ) ===" >> "$OUT"
emit ""

get_call "ROOT /" "/"
get_call "HEALTH /health" "/health"
get_call "SEARCH informational: what causes aurora borealis" "/search?q=what%20causes%20aurora%20borealis"
get_call "SEARCH comparison: react vs vue vs svelte" "/search?q=react%20vs%20vue%20vs%20svelte"
get_call "SEARCH transactional: buy mechanical keyboard" "/search?q=buy%20mechanical%20keyboard"
get_call "SEARCH fresh: latest rust releases 2026" "/search?q=latest%20rust%20releases%202026"
get_call "SEARCH how-to: how to make sourdough bread" "/search?q=how%20to%20make%20sourdough%20bread"
get_call "SEARCH/FAST rust web framework" "/search/fast?q=rust%20web%20framework&limit=3"
get_call "EDGE empty q" "/search?q="
get_call "EDGE missing q" "/search"
get_call "EDGE single char a" "/search?q=a"
get_call "EDGE protected single word go" "/search?q=go"
get_call "EDGE unicode" "/search?q=%E0%A4%B9%E0%A4%BF%E0%A4%A8%E0%A5%8D%E0%A4%A6%E0%A5%80%20%E0%A4%95%E0%A4%BE%20%E0%A4%B9%E0%A5%88"
get_call "EDGE very long q" "/search?q=$(python -c "import urllib.parse;print(urllib.parse.quote('quantum ' * 60))")"
get_call "IMAGES rust programming" "/images?q=rust%20programming"
get_call "VIDEOS rust tutorial" "/videos?q=rust%20tutorial"
get_call "NEWS artificial intelligence" "/news?q=artificial%20intelligence"
post_call "GOALS quick one-shot" "/goals/quick" '{"goal":"learn to build a privacy-first search engine using Rust"}'
post_call "GOALS create (get questions)" "/goals" '{"goal":"write a novel in 6 months"}'
GOAL_ID=$(cat /tmp/if_id.txt)
emit "GOAL_ID_FROM_CREATE=${GOAL_ID}"
emit ""
if [ -n "$GOAL_ID" ]; then
  post_call "GOALS submit answers for ${GOAL_ID}" "/goals/${GOAL_ID}/answers" '{"answers":[{"question_id":0,"answer":"intermediate"},{"question_id":1,"answer":"2 hours per day"},{"question_id":2,"answer":"fiction"}]}'
  get_call "GOALS get ${GOAL_ID}" "/goals/${GOAL_ID}"
fi
get_call "GOALS leaderboard" "/goals/leaderboard"
if [ -n "$GOAL_ID" ]; then
  post_call "GOALS update progress ${GOAL_ID}" "/goals/${GOAL_ID}/progress" '{"phase_id":0,"is_completed":true}'
  post_call "GOALS complete phase 0 of ${GOAL_ID}" "/goals/${GOAL_ID}/phases/0/complete" '{}'
fi
echo "=== round v2 exercise end $(date -u +%FT%TZ) ===" >> "$OUT"
emit ""
echo "DONE"
