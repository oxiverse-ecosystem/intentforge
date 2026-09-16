import urllib.request, json, urllib.parse

BASE='http://localhost:4000'

# Test 1: bose negative applied
q='wireless headphones price:<100 not bose after:2025-01-01'
r=json.load(urllib.request.urlopen(f'{BASE}/search?q='+urllib.parse.quote(q), timeout=60))
ac=r.get('applied_constraints',[])
ic=r.get('ignored_constraints',[])
neg_app=set(e[4:].split('—')[0].strip().split()[0] for e in ac if isinstance(e,str) and e.startswith('not:') and e[4:].strip())
neg_ig=set(e[4:].split('—')[0].strip() for e in ic if isinstance(e,str) and e.startswith('not:'))
print('TEST1 bose: applied_neg=',neg_app,'ignored_neg=',neg_ig)
print('  raw applied_constraints=',ac[:5])
print('  raw ignored_constraints=',ic[:5])

# Test 2: images
d=json.load(urllib.request.urlopen(f'{BASE}/images?q=rust+programming', timeout=30))
print('TEST2 images: count=',d.get('count'),'results_len=',len(d.get('results',[])))
if d.get('results'):
    print('  first=',list(d['results'][0].keys()))

# Test 3: videos
d=json.load(urllib.request.urlopen(f'{BASE}/videos?q=rust+tutorial', timeout=30))
print('TEST3 videos: count=',d.get('count'),'results_len=',len(d.get('results',[])))
if d.get('results'):
    print('  first=',list(d['results'][0].keys()))

# Test 4: news
d=json.load(urllib.request.urlopen(f'{BASE}/news?q=artificial+intelligence', timeout=30))
print('TEST4 news: count=',d.get('count'),'results_len=',len(d.get('results',[])))
if d.get('results'):
    print('  first=',list(d['results'][0].keys()))

# Test 5: negation + price
q='best noise cancelling headphones not from sony under $200 for travel with good battery life'
r=json.load(urllib.request.urlopen(f'{BASE}/search?q='+urllib.parse.quote(q), timeout=60))
sc=r.get('structured_constraints',{})
neg=sc.get('negative',[])
pos=sc.get('positive',[])
price_lt=sc.get('price_lt')
print('TEST5 price: negative=',neg,'positive=',pos[:5],'price_lt=',price_lt,'sc=',sc)
