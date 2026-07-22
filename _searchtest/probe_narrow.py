#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Reproduce the 5 narrow/technical queries the user flagged. Capture full JSON,
intent, constraints, expanded_queries, distribution, top results + sources + latency.
Also test the user's own mitigations: site:arxiv.org and quoted phrases."""
import json, urllib.parse, urllib.request, time, sys

BASE = "http://localhost:4000"

def ask(q, endpoint="search", timeout=60):
    url = f"{BASE}/{endpoint}?q=" + urllib.parse.quote(q)
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "probe/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            body = r.read().decode()
            return json.loads(body), r.status, None, time.time()-t0
    except urllib.error.HTTPError as e:
        return None, e.code, f"HTTP {e.code}", time.time()-t0
    except Exception as e:
        return None, None, str(e), time.time()-t0

def show(q, endpoint="search", label=""):
    d, status, err, dt = ask(q, endpoint)
    print("=" * 90)
    print(f"Q: {q!r}  [{label}]  endpoint=/{endpoint}")
    if err:
        print(f"  ERROR: {err}   ({dt:.2f}s)")
        return
    if not d:
        print(f"  EMPTY/NONE  ({dt:.2f}s)")
        return
    res = d.get("results", [])
    print(f"  http={status}  latency={dt:.2f}s  intent={d.get('intent')!r}  conf={d.get('confidence')}")
    print(f"  category={d.get('category','-')}  n_results={len(res)}")
    cons = d.get("constraints")
    if cons: print(f"  constraints={cons}")
    sc = d.get("structured_constraints")
    if sc: print(f"  structured_constraints={sc}")
    eq = d.get("expanded_queries")
    if eq: print(f"  expanded={eq}")
    dist = d.get("distribution")
    if dist:
        top = sorted(dist.items(), key=lambda x: -x[1])[:5]
        print("  dist=" + ", ".join(f"{k}:{v:.2f}" for k, v in top))
    errf = d.get("error"); warn = d.get("warnings")
    if errf: print(f"  API error field: {errf!r}")
    if warn: print(f"  warnings: {warn!r}")
    if not res:
        print("  *** NO RESULTS ***")
        return
    for i, r in enumerate(res[:7]):
        src = ",".join(r.get("sources", [])) or r.get("source", "?")
        loc = "LOCAL" if r.get("is_local") else "web"
        print(f"  {i+1}. [{r.get('score',0):.3f}|auth={r.get('authority',0):.2f}|{loc}|{src}] {r.get('title','')[:70]}")
        print(f"       {r.get('url','')[:95]}")

# The 5 flagged queries + the user's own mitigations (site:arxiv.org, quoted phrases)
queries = [
    # --- Lancaster norms ---
    ("Lancaster norms", "FLAG: Lancaster norms (timeout reported)"),
    ('"Lancaster norms"', "Lancaster norms quoted"),
    ("Lancaster norms site:arxiv.org", "Lancaster norms + site:arxiv.org"),
    # --- boilerplate ---
    ("boilerplate", "FLAG: boilerplate (drifted to 'Why' dictionary)"),
    ('"boilerplate" arxiv', "boilerplate quoted + arxiv"),
    ("boilerplate site:arxiv.org", "boilerplate + site:arxiv.org"),
    ("boilerplate code", "boilerplate (disambiguate to code?)"),
    # --- predictive coding ---
    ("predictive coding", "FLAG: predictive coding (drifted to football)"),
    ('"predictive coding"', "predictive coding quoted"),
    ("predictive coding neuroscience", "predictive coding + neuroscience"),
    ("predictive coding site:arxiv.org", "predictive coding + site:arxiv.org"),
    # --- function words ---
    ("function words", "function words (good arxiv reported)"),
    ("function words site:arxiv.org", "function words + site:arxiv.org"),
    # --- semantic memory ---
    ("semantic memory", "semantic memory (solid PMC/local reported)"),
    ("semantic memory PMC", "semantic memory + PMC"),
    # extra narrow technical controls
    ("bert attention", "control narrow technical"),
    ("transformer explainability survey", "control narrow technical"),
]

# To reduce upstream flakiness, run serially with a short gap.
if __name__ == "__main__":
    for q, label in queries:
        try:
            show(q, "search", label)
        except Exception as e:
            print(f"Q={q!r} CRASHED: {e}")
        time.sleep(0.4)
    print("=" * 90)
    print("DONE")
