import json, urllib.request, sys

base = 'http://localhost:4000'

def get(path):
    r = urllib.request.urlopen(base + path, timeout=20)
    return r.read().decode('utf-8')

def post(path, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, headers={'Content-Type':'application/json'})
    r = urllib.request.urlopen(req, timeout=20)
    return r.read().decode('utf-8')

# /search - raw inspection
raw = get('/search?q=best+laptop+for+programming+2026')
print(f'/search RAW (first 1000):')
print(raw[:1000])
print('...')
print(f'/search RAW (last 500):')
print(raw[-500:])
print(f'Total len={len(raw)}')

# Try parsing
try:
    j = json.loads(raw)
    print(f'Parsed type: {type(j).__name__}')
    if isinstance(j, dict):
        print(f'Keys: {sorted(j.keys())}')
except Exception as e:
    print(f'Parse error: {e}')

print('\n--- /search with simple query ---')
raw2 = get('/search?q=laptop')
print(f'RAW: {raw2[:500]}')

# Check /intent
print('\n--- /intent ---')
raw3 = get('/intent?q=rust+vs+go')
print(f'RESPONSE: {raw3[:500]}')

# Check /analyze
print('\n--- /analyze ---')
raw4 = get('/analyze?q=best+laptop')
print(f'RESPONSE: {raw4[:500]}')

# Check /inspect
print('\n--- /inspect ---')
raw5 = get('/inspect?q=best+laptop')
print(f'RESPONSE: {raw5[:500]}')
