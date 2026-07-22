#!/usr/bin/env python3
"""Confirmatory A/B tests for IntentForge search API."""
import json, time, urllib.parse, urllib.request
from collections import Counter

BASE = "http://localhost:4000/search"

def call(q, timeout=40):
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent":"harness/2.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            body = r.read().decode("utf-8","replace")
        data = json.loads(body); data["_latency"]=time.time()-t0; data["_status"]=r.status
        return data
    except Exception as e:
        return {"_error": f"{type(e).__name__}: {e}", "results":[], "_latency":time.time()-t0}

def urls(r): return [x.get("url","") for x in r.get("results",[])]
def hosts(r):
    from urllib.parse import urlparse
    return [urlparse(u).netloc.lower() for u in urls(r)]
def exts(r):
    from urllib.parse import urlparse
    out=[]
    for u in urls(r):
        p=urlparse(u).path.lower()
        out.append(p.rsplit(".",1)[-1] if "." in p else "")
    return out

def eqset(a,b): return set(a)==set(b)

print("="*78)
print("CONFIRMATORY A/B TESTS")
print("="*78)

# 1) filetype cosmetic? compare BASE vs +filetype:pdf and +filetype:doc
for base in ["report","tutorial","manual","rust guide"]:
    b = call(base); p = call(base+" filetype:pdf"); d = call(base+" filetype:doc")
    sb, sp, sd = urls(b), urls(p), urls(d)
    same_p = eqset(sb, sp); same_d = eqset(sb, sd)
    ep = Counter(exts(p)); ed = Counter(exts(d))
    print(f"\n[filetype] base={base!r}")
    print(f"   base_results={len(sb)}  +pdf_results={len(sp)}  identical_set_to_base={same_p}")
    print(f"   +doc_results={len(sd)}  identical_set_to_base={same_d}")
    print(f"   pdf-set exts={dict(ep)}  doc-set exts={dict(ed)}")
    if same_p and same_d:
        print("   >>> CONFIRMED: filetype: changes NOTHING (purely cosmetic).")

# 2) exclusion effect: base vs -term. Does -term remove the term's pages?
for base, neg in [("java","-script"), ("python","-language"), ("apple","-fruit")]:
    b = call(base); n = call(base+neg)
    ub, un = urls(b), urls(n)
    # are all base urls still present after exclusion?
    removed = set(ub)-set(un)
    added = set(un)-set(ub)
    # crude: do remaining results still mention the negated word?
    from urllib.parse import urlparse
    negword = neg.lstrip("-").lower()
    still_has = [u for u in un if negword in (urlparse(u).netloc+urlparse(u).path).lower()]
    print(f"\n[exclusion] base={base!r} neg={neg}")
    print(f"   base={len(ub)}  excluded={len(un)}  removed={len(removed)} added={len(added)}")
    print(f"   note: constraints for excluded={n.get('constraints')}")
    neg_present = any(negword in (x.get('title','')+x.get('content','')).lower() for x in n.get('results',[]))
    print(f"   any remaining result still textually mentions '{negword}' = {neg_present}")

# 3) site: hard filter? does off-site leak in?
for q in ["rust site:rust-lang.org","python site:docs.python.org","go site:golang.org"]:
    r = call(q)
    host_filter = q.split("site:")[1].strip().lower()
    hs = hosts(r)
    on = [h for h in hs if host_filter in h]
    off = [h for h in hs if host_filter not in h]
    print(f"\n[site:] q={q!r}")
    print(f"   host_filter={host_filter!r}  on_site={len(on)}  off_site={len(off)}")
    print(f"   off_site_hosts={sorted(set(off))[:8]}")
    if off:
        print(f"   >>> CONFIRMED: site: NOT a hard filter — off-domain results leak in.")

# 4) date constraint effect? compare base 'news' vs 'news after:2024' vs 'news before:2020'
for q in ["news","news after:2024","news before:2020","releases after:2023"]:
    r = call(q)
    has_date_field = any("published" in k.lower() or "date" in k.lower() for x in r.get("results",[]) for k in x.keys())
    print(f"\n[date] q={q!r}  results={len(r.get('results',[]))}  any_date_field_in_output={has_date_field}")
    cons = r.get("constraints")
    print(f"   constraints={cons}")

# 5) tokio/Tokyo false friend
r = call("compare tokio and async-std for building high throughput network servers")
print(f"\n[tokio] top5 hosts/titles:")
for x in r.get("results",[])[:5]:
    print(f"   - {x.get('title','')[:60]}  [{x.get('url','')[:50]}]")
    print(f"       contains 'tokyo'={ 'tokyo' in x.get('title','').lower() or 'tokyo' in x.get('content','').lower()[:200]}")

# 6) relevance of a clean multi-constraint that SHOULD work, to show ceiling
for q in ["rust web framework axum vs actix performance","best python http client for async requests"]:
    r = call(q)
    print(f"\n[ceiling] q={q!r} results={len(r.get('results',[]))} intent={r.get('intent')}")
    for x in r.get("results",[])[:3]:
        print(f"   - {x.get('title','')[:60]}")
