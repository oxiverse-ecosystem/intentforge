import json, time, urllib.request, urllib.parse

BASE = "http://localhost:4000/search"

def call(q, timeout=40):
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            data = json.loads(r.read().decode())
        return data, None
    except Exception as e:
        return None, str(e)

def summ(q):
    data, err = call(q)
    if err:
        print(f"  ERR {q!r}: {err}")
        return
    res = data.get("results", [])
    warns = data.get("warnings") or []
    api_err = data.get("error")
    print(f"  q={q!r}  n={len(res)}  warnings={warns}  api_error={api_err}")
    if res:
        for i, r in enumerate(res[:3], 1):
            print(f"     {i}. {r.get('title','')[:70]}  [{r.get('score'):.3f}|{r.get('authority')}|{r.get('source','')}]  {r.get('url','')[:55]}")
    else:
        # dump top-level keys to see what the API reports on empty
        print(f"     top-level keys: {list(data.keys())}")

if __name__ == "__main__":
    # 1) boilerplate site:arxiv.org repeated -> intermittency check
    print("=== boilerplate site:arxiv.org x4 (intermittency) ===")
    for _ in range(4):
        summ("boilerplate site:arxiv.org")
        print("  ---")
        time.sleep(1.5)

    # 2) controls: other site:arxiv.org queries (do they return?)
    print("=== controls: other site:arxiv.org ===")
    summ("function words site:arxiv.org")
    summ("predictive coding site:arxiv.org")
    summ("attention mechanism site:arxiv.org")

    # 3) baseline: boilerplate alone (should be ~7)
    print("=== baseline: boilerplate (no site) ===")
    summ("boilerplate")

    # 4) broad arxiv presence: does 'boilerplate' ever surface arxiv in normal search?
    print("=== boilerplate detection (no site op) x2 ===")
    summ("boilerplate detection arxiv")
    summ("boilerplate code arxiv")
