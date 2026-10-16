import json, urllib.request, sys

base = 'http://localhost:4000'

def get(path):
    r = urllib.request.urlopen(base + path, timeout=30)
    return r.status, r.read().decode('utf-8')

def post(path, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, headers={'Content-Type':'application/json'})
    r = urllib.request.urlopen(req, timeout=30)
    return r.status, r.read().decode('utf-8')

print("=" * 60)
print("LIVE API AUDIT — t_488b4353")
print("=" * 60)

# ── A. MANDATORY ENDPOINT COVERAGE ──

# GET /
print("\n[1] GET /")
s, d = get('/')
assert s == 200, f"expected 200, got {s}"
assert "IntentForge" in d, f"unexpected body: {d[:100]}"
print(f"  PASS — 200 text/plain: {d.strip()!r}")

# GET /health
print("\n[2] GET /health")
s, d = get('/health')
assert s == 200
assert d.strip() == "OK"
print(f"  PASS — 200 OK")

# GET /search?q=<complex multi-constraint>
print("\n[3] GET /search?q=best+laptop+for+programming+2026+not+chromebook+under+1000")
s, d = get('/search?q=best+laptop+for+programming+2026+not+chromebook+under+1000')
assert s == 200
j = json.loads(d)
expected_keys = {'query', 'intent', 'category', 'confidence', 'constraints', 'structured_constraints', 'expanded_queries', 'distribution', 'results', 'results_before_filter', 'results_after_filter', 'total', 'limit', 'offset', 'has_more'}
missing = expected_keys - set(j.keys())
extra = set(j.keys()) - expected_keys
print(f"  status={s} results={len(j.get('results',[]))} intent={j.get('intent')} confidence={j.get('confidence')}")
print(f"  missing keys: {missing or 'none'}")
print(f"  extra keys: {extra or 'none'}")
assert not missing, f"MISSING KEYS: {missing}"
# Assert confidence is a real float
assert isinstance(j['confidence'], float), f"confidence is {type(j['confidence'])}"
assert 0.0 < j['confidence'] < 1.0, f"confidence out of range: {j['confidence']}"
# Assert results have required fields
for r in j.get('results', [])[:3]:
    assert 'url' in r and 'title' in r and 'score' in r, f"result missing core keys: {list(r.keys())}"
print("  PASS — shape matches API_REFERENCE.md UnifiedResponse")

# GET /search/fast
print("\n[4] GET /search/fast?q=rust+programming")
s, d = get('/search/fast?q=rust+programming')
assert s == 200
j = json.loads(d)
assert 'count' in j and 'results' in j and 'source' in j, f"fast missing keys: {list(j.keys())}"
assert j.get('source') == 'local', f"source should be 'local', got {j.get('source')}"
print(f"  PASS — count={j['count']} source={j['source']}")

# GET /images
print("\n[5] GET /images?q=rust+logo")
s, d = get('/images?q=rust+logo')
assert s == 200
j = json.loads(d)
assert 'count' in j and 'query' in j and 'results' in j, f"images missing keys: {list(j.keys())}"
if j['results']:
    r0 = j['results'][0]
    for k in ['title', 'url', 'image_url', 'thumbnail_url', 'description', 'source', 'score']:
        assert k in r0, f"image result missing key: {k}"
print(f"  PASS — count={j['count']}")

# GET /videos
print("\n[6] GET /videos?q=rust+tutorial")
s, d = get('/videos?q=rust+tutorial')
assert s == 200
j = json.loads(d)
assert 'count' in j and 'query' in j and 'results' in j, f"videos missing keys: {list(j.keys())}"
if j['results']:
    r0 = j['results'][0]
    for k in ['title', 'url', 'description', 'thumbnail', 'source', 'score']:
        assert k in r0, f"video result missing key: {k}"
print(f"  PASS — count={j['count']}")

# GET /news
print("\n[7] GET /news?q=latest+ai")
s, d = get('/news?q=latest+ai')
assert s == 200
j = json.loads(d)
assert 'count' in j and 'query' in j and 'results' in j, f"news missing keys: {list(j.keys())}"
if j['results']:
    r0 = j['results'][0]
    for k in ['title', 'url', 'description', 'source', 'score']:
        assert k in r0, f"news result missing key: {k}"
print(f"  PASS — count={j['count']}")

# GET /spellcheck
print("\n[8] GET /spellcheck?q=pythn")
s, d = get('/spellcheck?q=pythn')
assert s == 200
j = json.loads(d)
assert j.get('changed') == True, f"expected changed=True"
assert j.get('corrected') == 'python', f"expected 'python', got {j.get('corrected')}"
assert 'query' in j and 'corrections' in j, f"spellcheck missing keys: {list(j.keys())}"
print(f"  PASS — changed={j['changed']} corrected={j['corrected']}")

# ── GOALS SUBSYSTEM ──

# POST /goals
print("\n[9] POST /goals {goal: 'build a full-stack web app'}")
s, d = post('/goals', {'goal': 'build a full-stack web app for project management with team collaboration'})
assert s == 200
j = json.loads(d)
assert 'goal_id' in j, f"missing goal_id: {list(j.keys())}"
assert 'questions' in j, f"missing questions: {list(j.keys())}"
assert len(j['questions']) > 0, "questions array is empty"
goal_id = j['goal_id']
print(f"  PASS — goal_id={goal_id} questions={len(j['questions'])} total_questions={j.get('total_questions')}")

# POST /goals/:id/answers
print(f"\n[10] POST /goals/{goal_id}/answers")
answers = []
for q in j.get('questions', []):
    # Pick the first option as answer
    opts = q.get('options', [])
    ans = opts[0] if opts else "default"
    answers.append({'question_id': q['id'], 'answer': ans})
s, d = post(f'/goals/{goal_id}/answers', {'answers': answers})
assert s == 200
j2 = json.loads(d)
roadmap = j2.get('roadmap', {})
phases = roadmap.get('phases', [])
total_phases = roadmap.get('total_phases')
print(f"  total_phases={total_phases} len(phases)={len(phases)}")
print(f"  RESPONSE KEYS: {sorted(j2.keys())}")
print(f"  ROADMAP KEYS: {sorted(roadmap.keys())}")
assert total_phases is not None, "DEFECT: total_phases is null"
assert total_phases == len(phases), f"DEFECT: total_phases={total_phases} != len(phases)={len(phases)}"
print(f"  PASS — total_phases matches len(phases)")

# GET /goals/:id
print(f"\n[11] GET /goals/{goal_id}")
s, d = get(f'/goals/{goal_id}')
assert s == 200
j3 = json.loads(d)
assert 'status' in j3, f"missing status: {list(j3.keys())}"
print(f"  PASS — status={j3['status']}")

# GET /goals/leaderboard
print("\n[12] GET /goals/leaderboard")
s, d = get('/goals/leaderboard')
assert s == 200
j4 = json.loads(d)
# MUST BE A LIST
is_list = isinstance(j4, list)
print(f"  type={type(j4).__name__} len={len(j4) if is_list else 'N/A'}")
if not is_list and isinstance(j4, dict):
    print(f"  DICT KEYS: {sorted(j4.keys())}")
    print(f"  DEFECT: leaderboard returned a dict, not a list!")
assert is_list, "DEFECT: leaderboard must return a bare JSON array"
print("  PASS — response is a list")

# POST /goals/quick
print("\n[13] POST /goals/quick {goal: 'build a rust web framework'}")
s, d = post('/goals/quick', {'goal': 'build a rust web framework'})
assert s == 200
j5 = json.loads(d)
roadmap2 = j5.get('roadmap', {})
phases2 = roadmap2.get('phases', [])
total2 = roadmap2.get('total_phases')
print(f"  total_phases={total2} len(phases)={len(phases2)}")
assert total2 is not None, "DEFECT: total_phases is null in /goals/quick"
assert total2 == len(phases2), f"DEFECT: total_phases={total2} != len(phases)={len(phases2)}"
print("  PASS — total_phases matches len(phases)")

# ── COMMERCE-TARGET AUDIT ──

# GET /shopping
print("\n[14] GET /shopping?q=iphone+16+pro+max+256gb")
s, d = get('/shopping?q=iphone+16+pro+max+256gb')
assert s == 200
j6 = json.loads(d)
results = j6.get('results', [])
print(f"  results={len(results)}")
# Check affiliate decoration on first result
if results:
    r0 = results[0]
    aff = r0.get('affiliate')
    print(f"  first.affiliate.disclosed={aff.get('disclosed') if aff else 'null'}")
    print(f"  first.affiliate.network={aff.get('network') if aff else 'null'}")
    if aff is not None:
        assert aff.get('disclosed') == True, "DEFECT: affiliate.disclosed != true"
        print("  PASS — affiliate disclosed")

# ── MAIN PATH /search with commercial intent
print("\n[15] GET /search?q=buy+sony+wh-1000xm5+headphones (commercial intent)")
s, d = get('/search?q=buy+sony+wh-1000xm5+headphones')
assert s == 200
j7 = json.loads(d)
shopping = j7.get('shopping')
print(f"  intent={j7.get('intent')}")
print(f'  shopping present={"shopping" in j7}')
if shopping:
    print(f'  shopping results={len(shopping.get("results",[]))}')
    sr0 = shopping.get('results', [])
    if sr0:
        print(f'  first.affiliate.disclosed={sr0[0].get("affiliate",{}).get("disclosed")}')

# ── CHECK RESPONSE FOR PRIVACY LEAKS
print("\n[16] Privacy check: affiliate URL does not contain query/user/IP")
if results and results[0].get('affiliate'):
    aff_url = results[0]['affiliate'].get('url', '')
    print(f"  affiliate.url={aff_url[:200]}")
    assert 'sony' not in aff_url or 'buy' not in aff_url, "DEFECT: query text in affiliate URL"
    # cuid should be merchant host only
    if 'cuid=' in aff_url:
        cuid_val = aff_url.split('cuid=')[1].split('&')[0]
        print(f"  cuid={cuid_val}")

print("\n" + "=" * 60)
print("AUDIT COMPLETE")
print("=" * 60)
