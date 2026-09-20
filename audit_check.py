import json, urllib.request, sys

base = 'http://localhost:4000'

def get(path):
    r = urllib.request.urlopen(base + path, timeout=20)
    return r.status, json.loads(r.read())

def post(path, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, headers={'Content-Type':'application/json'})
    r = urllib.request.urlopen(req, timeout=20)
    return r.status, json.loads(r.read())

# Search
try:
    s, j = get('/search?q=best+laptop+for+programming+2026')
    print(f'/search => {s} results={len(j.get("results",[]))} intent={j.get("intent",{}).get("primary")} before={j.get("results_before_filter")} after={j.get("results_after_filter")}')
    print(f'  constraints={j.get("constraints")}')
    print(f'  response keys={sorted(j.keys())}')
    if j.get('results'):
        r0 = j['results'][0]
        print(f'  result keys={sorted(r0.keys())}')
        print(f'  first_url={r0.get("url","")[:100]}')
        print(f'  first_score={r0.get("score")}')
        print(f'  first_affiliate={r0.get("affiliate")}')
except Exception as e:
    print(f'/search ERROR: {e}')

# Search Fast
try:
    s, j = get('/search/fast?q=rust+programming')
    print(f'/search/fast => {s} results={len(j.get("results",[]))}')
except Exception as e:
    print(f'/search/fast ERROR: {e}')

# Images
try:
    s, j = get('/images?q=cat')
    print(f'/images => {s} results={len(j.get("results",[]))}')
except Exception as e:
    print(f'/images ERROR: {e}')

# Videos
try:
    s, j = get('/videos?q=rust+tutorial')
    print(f'/videos => {s} results={len(j.get("results",[]))}')
except Exception as e:
    print(f'/videos ERROR: {e}')

# News
try:
    s, j = get('/news?q=latest+ai')
    print(f'/news => {s} results={len(j.get("results",[]))}')
except Exception as e:
    print(f'/news ERROR: {e}')

# Spellcheck
try:
    s, j = get('/spellcheck?q=pythn')
    print(f'/spellcheck => {s} changed={j.get("changed")} corrected={j.get("corrected")}')
except Exception as e:
    print(f'/spellcheck ERROR: {e}')

# Intent
try:
    s, j = get('/intent?q=rust+vs+go')
    print(f'/intent => {s} primary={j.get("primary")} conf={j.get("confidence")}')
except Exception as e:
    print(f'/intent ERROR: {e}')

# Shopping (affiliate)
try:
    s, j = get('/shopping?q=iphone+16+pro+max')
    print(f'/shopping => {s} results={len(j.get("results",[]))}')
    if j.get('results'):
        r0 = j['results'][0]
        print(f'  first keys={sorted(r0.keys())}')
        print(f'  first.affiliate={r0.get("affiliate")}')
except Exception as e:
    print(f'/shopping ERROR: {e}')

# Goals - POST create
try:
    s, j = post('/goals', {'goal': 'learn machine learning'})
    print(f'POST /goals => {s} goal_id={j.get("goal_id")} questions={len(j.get("questions",[]))}')
    print(f'  keys={sorted(j.keys())}')
except Exception as e:
    print(f'POST /goals ERROR: {e}')
