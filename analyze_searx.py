import json
import urllib.request

url = "http://localhost:8080/search?q=rust+lang+latest+features+2026&format=json"
try:
    with urllib.request.urlopen(url) as response:
        data = json.loads(response.read().decode('utf-8'))
    
    print("="*60)
    print("SEARCH QUALITY REPORT")
    print("="*60)
    results = data.get('results', [])
    engines = set(r.get('engine') for r in results)
    print(f"Total Results: {len(results)}")
    print(f"Engines Participating: {engines}")
    print("\nTop 5 Results:")
    for i, r in enumerate(results[:5]):
        print(f"\n{i+1}. {r.get('title')}")
        print(f"   URL: {r.get('url')}")
        print(f"   Engine: {r.get('engine')}")
        content = r.get('content', '')
        print(f"   Snippet: {content[:200]}...")
except Exception as e:
    print(f"Error: {e}")
