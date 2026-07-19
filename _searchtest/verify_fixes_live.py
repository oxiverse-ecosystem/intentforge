#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Verify the 3 fixes against live localhost:4000/search."""
import json, urllib.parse, urllib.request, re, time, os

BASE = "http://localhost:4000/search"

def ask(q, timeout=35):
    url = f"{BASE}?q=" + urllib.parse.quote(q)
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "verify/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read().decode()), r.status, None
    except Exception as e:
        return None, None, str(e)

def norm(s): return re.sub(r"[^a-z0-9]", "", (s or "").lower())

def leak_pct(q, d):
    res = d.get("results", []) or []
    cons = d.get("constraints", []) or []
    negs = [c[1:] for c in cons if c.startswith("-")]
    sc = d.get("structured_constraints") or {}
    negs += sc.get("negative", []) or []
    negs = [x for x in dict.fromkeys(negs) if x]
    if not res or not negs: return None, negs, res
    out = []
    for neg in negs:
        nh = norm(neg)
        if not nh: continue
        hits = [r for r in res if nh in norm((r.get("title","") or "")+" "+(r.get("content","") or ""))]
        if hits:
            out.append((neg, len(hits), round(len(hits)/len(res),3)))
    return out, negs, res

# P0: negation leak tests (small reword to bypass 30-min cache where useful,
# but also re-test the ORIGINAL failing queries to prove the fix)
p0 = [
    "best python ide not pycharm",
    "static site generator instead of jekyll",
    "best laptop for video editing not macbook",
    "browser not chrome not edge not firefox",
    "python web framework for beginners not django",
    "javascript framework not react",
    "python not java",
    "search engine alternative to google",   # should KEEP alt pages, drop google-centric
]
# P1-tech: should now be intent=technical (or at least not navigational)
p1t = [
    "kubernetes ingress tls configuration",
    "python asyncio event loop explained",
    "configure nginx reverse proxy with ssl",
    "postgresql connection pooling",
    "redis pub sub patterns",
]
# P1-compound: site+filetype should now recover results
p1c = [
    "python tutorial site:realpython.com filetype:pdf",
    "python tutorial site:docs.python.org filetype:pdf",
]

print("="*72); print("P0 — NEGATION LEAK (lower is better; 0 = clean)"); print("="*72)
worst=0
for q in p0:
    d, st, err = ask(q)
    if d is None: print(f"  ERR {q!r}: {err}"); continue
    leaks, negs, res = leak_pct(q, d)
    intent=d.get("intent")
    if leaks:
        for neg,h,p in leaks:
            worst=max(worst,p)
            print(f"  LEAK {q[:40]:<41} neg={neg:<10} {h}/{len(res)} ({p*100:.0f}%) intent={intent}")
    else:
        print(f"  OK   {q[:40]:<41} negs={negs} n={len(res)} intent={intent}")

print("\n"+"="*72); print("P1-TECH — intent label (want 'technical')"); print("="*72)
for q in p1t:
    d, st, err = ask(q)
    if d is None: print(f"  ERR {q!r}: {err}"); continue
    print(f"  {q[:40]:<41} intent={d.get('intent'):<13} conf={round(d.get('confidence',0),3)} n={len(d.get('results',[]) or [])}")

print("\n"+"="*72); print("P1-COMPOUND — site+filetype (want n>0)"); print("="*72)
for q in p1c:
    d, st, err = ask(q)
    if d is None: print(f"  ERR {q!r}: {err}"); continue
    n=len(d.get("results",[]) or [])
    print(f"  {q[:46]:<47} n={n} intent={d.get('intent')} msg={(d.get('message') or '')[:40]}")
