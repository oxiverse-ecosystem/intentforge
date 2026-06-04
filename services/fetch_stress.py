import urllib.request, urllib.parse, json
queries = [
    'static site generator without react/vue/nextjs',
    'vector database without Pinecone without Weaviate without Qdrant',
    'FoundationDB vs Spanner vs CockroachDB',
    'Linux kernel drop TCP packets SO_REUSEPORT',
    'eBPF and io_uring',
]
for q in queries:
    url = 'http://localhost:4000/search?q=' + urllib.parse.quote(q, safe='')
    try:
        with urllib.request.urlopen(url, timeout=120) as r:
            data = r.read().decode('utf-8', 'replace')
    except Exception as e:
        data = json.dumps({'error': str(e)})
    path = '/tmp/stress_' + q.replace(' ', '_').replace('/', '_') + '.json'
    open(path, 'w', encoding='utf-8').write(data)
    print(path, len(data))
