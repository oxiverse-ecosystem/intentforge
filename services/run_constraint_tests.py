import json
import re
import subprocess
import urllib.parse
from datetime import datetime

BASE = 'http://localhost:4000/search'
queries = [
    'distributed task queue for python not celery',
    'linux firewall gui with vlan support excluding ufw',
    'self-hosted analytics alternative to google analytics privacy-focused excluding matomo',
    'rust async web framework without actix deadline-first pricing model',
    'open source cms without wordpress multilingual',
    'vector database for ai not pinecone managed',
    'javascript framework besides react typescript',
]

stop = {'a','an','the','and','or','for','to','in','on','of','with','support','model','vs','like','than','other','instead','is','no'}

def norm(s):
    return re.sub(r'[^a-z0-9]+', ' ', s.lower()).strip()

def toks(s):
    return [t for t in re.split(r'\s+', norm(s)) if t and t not in stop]

def normalize_constraints(src):
    positives = [norm(t) for t in src.get('positive', []) if norm(t)]
    negatives = [norm(t) for t in src.get('negative', []) if norm(t)]
    seen=[]
    for t in positives:
        if t not in seen: seen.append(t)
    positives=seen
    seen=[]
    for t in negatives:
        if t not in seen: seen.append(t)
    negatives=seen
    return positives, negatives

def blob(r):
    return norm(r.get('title','')) + ' ' + norm(r.get('content','')) + ' ' + norm(r.get('url',''))

def verify(constraints, results):
    positives, negatives = normalize_constraints(constraints)
    rows = []
    violations = []
    hits = {'title':0,'url':0,'content':0,'none':0}
    for r in results:
        tokens = set(re.split(r'\s+', blob(r)))
        matched_neg = [n for n in negatives if n in tokens]
        matched_pos_t = [p for p in positives if p in norm(r.get('title',''))]
        matched_pos_u = [p for p in positives if p in norm(r.get('url',''))]
        matched_pos_c = [p for p in positives if p in norm(r.get('content',''))]
        row = {
            'title': r.get('title',''),
            'url': r.get('url',''),
            'score': r.get('score'),
            'negative_match': matched_neg,
            'positive_title': matched_pos_t,
            'positive_url': matched_pos_u,
            'positive_content': matched_pos_c,
        }
        if matched_neg:
            violations.append(row)
            row['verdict'] = ' VIOLATION'
        elif matched_pos_t or matched_pos_u:
            row['verdict'] = 'OK'
            if matched_pos_t: hits['title'] += 1
            elif matched_pos_u: hits['url'] += 1
            else: hits['content'] += 1
        else:
            row['verdict'] = 'NO_POSITIVE_SIGNAL'
            hits['none'] += 1
        rows.append(row)
    return {'rows':rows,'violations':violations,'hits':hits,'positives':positives,'negatives':negatives,'count':len(rows)}

report = {'timestamp': datetime.utcnow().isoformat()+'Z', 'queries':[]}

for q in queries:
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    r = subprocess.run(['curl','-fsS', url], capture_output=True, text=True)
    entry = {'query': q, 'http_status': None, 'error': None}
    if r.returncode != 0:
        entry['http_status'] = 'connect_failed'
        entry['error'] = r.stderr[:500]
        report['queries'].append(entry)
        continue
    try:
        data = json.loads(r.stdout)
    except Exception as e:
        entry['http_status'] = 'json_failed'
        entry['error'] = str(e)
        entry['body'] = r.stdout[:400]
        report['queries'].append(entry)
        continue
    constraints = data.get('structured_constraints') or {
        'positive': [norm(t) for t in data.get('constraints', []) if t.startswith('+')],
        'negative': [norm(t.split('+',1)[0]) for t in data.get('constraints', []) if t.startswith('-')],
    }
    v = verify(constraints, data.get('results', []))
    entry['status'] = 200
    entry['intent'] = data.get('intent')
    entry['confidence'] = data.get('confidence')
    entry['constraints'] = constraints
    entry['verification'] = {
        'count': v['count'],
        'hits': v['hits'],
        'violations': v['violations'][:10],
        'included_examples': v['rows'][:8],
    }
    report['queries'].append(entry)

out = r'C:\Users\Likhith\Documents\projects\intentforge-v2\services\constraint_report.json'
with open(out, 'w', encoding='utf-8') as f:
    json.dump(report, f, indent=2)
print(out)
