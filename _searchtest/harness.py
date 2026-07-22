#!/usr/bin/env python3
"""Empirical UX/quality test harness for localhost:4000/search (IntentForge gateway)."""
import json, time, urllib.parse, urllib.request, sys
from collections import Counter

BASE = "http://localhost:4000/search"
N = {0: "NONE", 1: "LOW", 2: "MED", 3: "HIGH"}

def call(q, timeout=40):
    url = f"{BASE}?q={urllib.parse.quote(q)}"
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "harness/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            body = r.read().decode("utf-8", "replace")
            status = r.status
        dt = time.time() - t0
        try:
            data = json.loads(body)
        except Exception:
            return {"_error": "json_parse", "_raw": body[:300], "latency": dt, "status": status}
        data["_latency"] = dt
        data["_status"] = status
        return data
    except urllib.error.HTTPError as e:
        return {"_error": f"http_{e.code}", "_raw": e.read().decode("utf-8","replace")[:300], "latency": time.time()-t0}
    except Exception as e:
        return {"_error": f"{type(e).__name__}: {e}", "latency": time.time()-t0}

def path_host(u):
    try:
        from urllib.parse import urlparse
        return urlparse(u).netloc.lower()
    except Exception:
        return ""

def url_ext(u):
    try:
        from urllib.parse import urlparse
        p = urlparse(u).path.lower()
        return p.rsplit(".", 1)[-1] if "." in p else ""
    except Exception:
        return ""

# ---- TEST BATTERY ----
# Each test: (label, query, checks)
tests = []

# 1) site: — does it filter at all? doc.rust-lang.org is known in index.
tests.append(("site: known-host rust", "rust site:rust-lang.org"))
tests.append(("site: different-host", "rust site:github.com"))
tests.append(("site: wrong-host still returns nothing?", "rust site:example.com"))

# 2) filetype — verify returned urls actually end in that ext
tests.append(("filetype pdf", "report filetype:pdf"))
tests.append(("filetype pdf generic", "tutorial filetype:pdf"))
tests.append(("filetype doc", "manual filetype:doc"))

# 3) exact phrase — is phrase constraint preserved & results contain phrase?
tests.append(("phrase", '"continuous integration"'))
tests.append(("phrase complex", '"how to deploy a kubernetes cluster"'))
tests.append(("phrase two words", '"cargo test"'))

# 4) exclusion — does -term actually reduce/remove matches?
tests.append(("exclude", "python -language"))
tests.append(("exclude vs base", "java -script"))  # "java" vs "javascript"
tests.append(("double exclusion", "apple -fruit -phone"))

# 5) date constraints
tests.append(("after year", "news after:2024"))
tests.append(("before year", "release before:2020"))
tests.append(("after date", "security advisory after:2025-01-01"))

# 6) complex multi-constraint
tests.append(("combo", "rust async tutorial site:rust-lang.org filetype:html"))
tests.append(("combo2", 'python web framework "fastapi" -django after:2023'))

# 7) intent sanity
tests.append(("navigational", "github"))
tests.append(("howto", "how to bake sourdough bread"))
tests.append(("transactional", "buy wireless headphones"))
tests.append(("comparison", "rust vs go performance"))
tests.append(("local", "coffee shop near me"))
tests.append(("fresh/news", "latest ai news"))

# 8) natural-language complex question (the "should handle complex queries" ask)
tests.append(("NL question", "what are the best practices for securing a postgres database in production"))
tests.append(("NL multi-constraint", "show me rust crates for parsing json that are actively maintained and have >1000 stars"))
tests.append(("NL comparison", "compare tokio and async-std for building high throughput network servers"))

# 9) edge / error handling
tests.append(("empty", ""))
tests.append(("single char", "a"))
tests.append(("stopwords only", "the and of"))
tests.append(("gibberish", "zxqw flarp qwibble"))
tests.append(("very long", "how " * 60 + "to do things"))
tests.append(("unicode", "北京 美食 推荐"))
tests.append(("special chars", "c++ && && operator precedence"))

results = {}
for label, q in tests:
    r = call(q)
    results[label] = (q, r)
    sys.stderr.write(f"[done] {label:32s} q={q[:40]!r}\n")
    sys.stderr.flush()

# ---- ANALYSIS ----
report = []
def W(s=""): report.append(s)

W("="*80)
W("INTENTFORGE SEARCH API — EMPIRICAL UX/QUALITY AUDIT")
W(f"endpoint: {BASE}   tests: {len(tests)}")
W("="*80)

# A) site: filtering verification
W("\n### A. SITE: CONSTRAINT — does it actually filter? ###")
for label, q in [("site: known-host rust","rust site:rust-lang.org"),
                 ("site: different-host","rust site:github.com"),
                 ("site: wrong-host still returns nothing?","rust site:example.com")]:
    q2, r = results[label]
    res = r.get("results", [])
    hosts = Counter(path_host(x.get("url","")) for x in res)
    cons = r.get("constraints")
    W(f"\n  q={q!r}")
    W(f"    constraints={cons}")
    W(f"    num_results={len(res)}  hosts={dict(hosts)}")
    if "site:" in q:
        host_filter = q.split("site:")[1].strip()
        if res and not any(host_filter in h for h in hosts):
            W(f"    >>> BUG: site:{host_filter} set but NO result host contains it: {list(hosts)[:5]}")

# B) filetype verification
W("\n### B. FILETYPE: CONSTRAINT — are returned urls actually that type? ###")
for label in ["filetype pdf","filetype pdf generic","filetype doc"]:
    q, r = results[label]
    res = r.get("results", [])
    exts = Counter(url_ext(x.get("url","")) for x in res)
    W(f"\n  q={q!r}")
    W(f"    num_results={len(res)}  ext_distribution={dict(exts)}")
    want = q.split("filetype:")[1].strip().split()[0]
    if res and exts.get(want,0) == 0:
        W(f"    >>> BUG: filetype:{want} requested but 0/{len(res)} urls have .{want} ext. sample={list(exts)[:6]}")

# C) phrase verification
W("\n### C. EXACT PHRASE — preserved & results contain it? ###")
for label in ['phrase', 'phrase complex', 'phrase two words']:
    q, r = results[label]
    cons = r.get("constraints")
    res = r.get("results", [])
    phrase = q.strip('"')
    contains = sum(1 for x in res if phrase.lower() in (x.get("title","")+x.get("content","")).lower())
    W(f"\n  q={q!r}")
    W(f"    constraints={cons}  (None => phrase DROPPED)")
    W(f"    num_results={len(res)}  results_containing_phrase={contains}/{len(res)}")
    if cons is None:
        W(f"    >>> BUG: quoted phrase silently dropped from constraints")

# D) exclusion verification
W("\n### D. EXCLUSION (-term) — does it actually exclude? ###")
def base_count(term, n=10):
    r = call(term)
    return len(r.get("results", []))
for label in ["exclude","exclude vs base","double exclusion"]:
    q, r = results[label]
    res = r.get("results", [])
    cons = r.get("constraints")
    # find negative terms
    negs = [c for c in (cons or []) if c.startswith("-")]
    W(f"\n  q={q!r}")
    W(f"    constraints={cons}")
    W(f"    num_results={len(res)}  negatives={negs}")
    if negs and any(c.startswith("+-") for c in cons):
        W(f"    >>> SMELL: negation appears both as '+-x' and '-x' (double entry)")

# E) latency
W("\n### E. LATENCY (p50 / max) ###")
lats = sorted([r.get("_latency",0) for _,r in results.values()])
if lats:
    W(f"    min={lats[0]:.2f}s  median={lats[len(lats)//2]:.2f}s  max={lats[-1]:.2f}s")
    slow = [(lbl, results[lbl][1].get("_latency",0)) for lbl in results if results[lbl][1].get("_latency",0)>15]
    if slow:
        W(f"    >>> SLOW (>{15}s): {slow}")

# F) errors / edge
W("\n### F. EDGE / ERROR HANDLING ###")
for label in ["empty","single char","stopwords only","gibberish","very long","unicode","special chars"]:
    q, r = results[label]
    if "_error" in r:
        W(f"  q={q!r:30s} -> ERROR {r['_error']}")
        continue
    res = r.get("results", [])
    intent = r.get("intent")
    dist = r.get("distribution", {})
    topint = max(dist, key=dist.get) if dist else None
    W(f"  q={q!r:30s} -> status={r.get('_status')} results={len(res)} intent={intent}({r.get('confidence')}) topdist={topint}")

# G) intent sanity
W("\n### G. INTENT CLASSIFICATION SANITY ###")
expect = {
    "navigational":"navigational","howto":"how-to","transactional":"transactional",
    "comparison":"comparison","local":"local","fresh/news":"fresh",
}
for label, exp in expect.items():
    q, r = results[label]
    got = r.get("intent")
    ok = "OK " if got==exp else "MISS"
    W(f"  {ok} q={q!r:30s} expected={exp:12s} got={got} conf={r.get('confidence'):.2f}")

# H) relevance: does NL complex question return on-topic results?
W("\n### H. COMPLEX NATURAL-LANGUAGE QUERY RELEVANCE ###")
for label in ["NL question","NL multi-constraint","NL comparison","combo2"]:
    q, r = results[label]
    res = r.get("results", [])
    # crude relevance: does title/content share >=1 significant query token
    qtokens = set(t for t in q.lower().replace('"','').split() if len(t)>3 and t not in ("show","me","that","have","with","best","for","the","and"))
    hits = 0
    for x in res[:5]:
        txt = (x.get("title","")+" "+x.get("content","")).lower()
        if qtokens and any(t in txt for t in qtokens):
            hits += 1
    W(f"  q={q!r}")
    W(f"    intent={r.get('intent')} results={len(res)} top5_relevant~{hits}/5  constraints={r.get('constraints')}")
    if res:
        W(f"    top1: {res[0].get('title','')[:70]}")
        W(f"          {res[0].get('url','')}")

# I) duplicate / source diversity
W("\n### I. RESULT SOURCE DIVERSITY (local vs web) ###")
allsrc = Counter()
for _,(_,r) in results.items():
    for x in r.get("results", []):
        for s in x.get("sources",[]) or ["<none>"]:
            allsrc[s]+=1
W(f"    source token counts across all tests: {dict(allsrc)}")
if allsrc and allsrc.get("local",0) and allsrc.get("bing",0) is None and allsrc.get("brave",0) is None:
    W("    >>> NOTE: only 'local' results returned in entire run — web upstreams (bing/brave/tor) may be down/absent.")

W("\n"+"="*80)
out = "\n".join(report)
print(out)

# dump raw for deeper inspection
with open("_searchtest/raw.json","w") as f:
    json.dump({k:{"q":v[0],"r":v[1]} for k,v in results.items()}, f, default=str)
W("\n[raw dumped to _searchtest/raw.json]")
