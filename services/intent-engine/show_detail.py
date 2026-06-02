"""Display detailed search results from a saved JSON response."""
import json, sys

def show_results(path, n=8):
    with open(path, encoding="utf-8") as f:
        d = json.load(f)
    print(f'Query:     {d.get("query","?")}')
    print(f'Intent:    {d.get("intent","?")}  (conf={d.get("confidence")})')
    print(f'Expanded:  {d.get("expanded_queries")}')
    print(f'Results:   {len(d.get("results",[]))}')
    print()
    for i, r in enumerate(d.get("results", [])[:n]):
        print(f'--- #{i+1} (score={r["score"]:.3f}, auth={r["authority"]:.3f}) ---')
        print(f'  Title:   {r["title"][:150]}')
        print(f'  URL:     {r["url"][:150]}')
        print(f'  Sources: {r.get("sources",[])}')
        c = r.get("content","")[:250]
        if c: print(f'  Snippet: {c}')
        print()

if __name__ == "__main__":
    show_results(sys.argv[1])
