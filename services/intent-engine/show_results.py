import json, sys

path = sys.argv[1]
d = json.load(open(path, encoding='utf-8'))
print(f'Query: {d.get("query", "?")}')
print(f'Intent: {d.get("intent")} (conf={d.get("confidence")})')
print(f'Expanded: {d.get("expanded_queries")}')
print(f'Total results: {len(d.get("results", []))}')
print()

for i, r in enumerate(d.get('results', [])[:5]):
    print(f'--- Result {i+1} ---')
    print(f'  Title:   {r.get("title","")[:120]}')
    print(f'  URL:     {r.get("url","")[:120]}')
    print(f'  Score:   {r.get("score", 0):.3f}  Authority: {r.get("authority", 0):.3f}')
    content = r.get('content', '')
    print(f'  Content: {content[:200]}...' if content else '  Content: [empty]')
    print(f'  Sources: {r.get("sources", [])}')
    print()
