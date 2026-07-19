#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Deep battery 2: confirm negative enforcement depth, technical->navigational
misclassification, freshness date honesty, multi-operator chains, and
date-staleness of 'fresh' queries."""
import json, urllib.parse, urllib.request, time, re, os

BASE = "http://localhost:4000/search"
OUT = os.path.join(os.path.dirname(__file__), "ux_probe2_results.json")

DATE_RE = re.compile(r"(20\d{2}[-/]\d{1,2}[-/]\d{1,2})|((?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s*20\d{2})", re.I)

def ask(q, timeout=35):
    url = f"{BASE}?q=" + urllib.parse.quote(q)
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "uxprobe2/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read().decode()), r.status, None, time.time() - t0
    except Exception as e:
        return None, None, str(e), time.time() - t0

def norm(s): return re.sub(r"[^a-z0-9]", "", (s or "").lower())

# Strip year from date to bucket recency
def year_of(text):
    for m in DATE_RE.finditer(text or ""):
        ym = re.search(r"20\d{2}", m.group(0))
        if ym: return int(ym.group(0))
    return None

battery = [
    # negative enforcement DEPTH (is a page ABOUT the excluded term dropped?)
    ("best python ide not pycharm", "neg-depth"),
    ("linux distro not windows", "neg-depth"),
    ("javascript framework not react", "neg-depth"),
    ("python not java", "neg-depth"),
    # multi-operator chain
    ("rust web framework with async not actix after:2024", "chain"),
    ("python tutorial filetype:pdf site:realpython.com", "chain"),
    ("gaming laptop under 1500 rtx 4070 not asus", "chain"),
    # technical->navigational misclassification sweep
    ("kubernetes", "brand-nav"),
    ("docker", "brand-nav"),
    ("nginx", "brand-nav"),
    ("postgresql", "brand-nav"),
    ("terraform", "brand-nav"),
    ("redis", "brand-nav"),
    # freshness date honesty
    ("latest ai news 2026", "fresh-dates"),
    ("recent rust releases", "fresh-dates"),
    ("breaking news today", "fresh-dates"),
    # complex natural-language
    ("how do i convince my boss to let me work remotely without sounding lazy", "complex-nl"),
    ("whats a good cheap reliable used car for a college student under 8000 not a mustang", "complex-nl"),
]

if __name__ == "__main__":
    results = []
    start = time.time()
    for q, label in battery:
        d, status, err, lat = ask(q)
        rec = {"q": q, "label": label, "status": status, "latency_s": round(lat, 2), "error": err}
        if d is not None:
            rec["intent"] = d.get("intent")
            rec["category"] = d.get("category")
            rec["confidence"] = d.get("confidence")
            cons = d.get("constraints", []) or []
            rec["constraints"] = cons
            rec["neg"] = [c[1:] for c in cons if c.startswith("-")]
            res = d.get("results", []) or []
            rec["n_results"] = len(res)
            # leak check
            leaks = []
            for neg in rec["neg"]:
                nh = norm(neg)
                if not nh: continue
                hits = [r for r in res if nh in norm((r.get("title","") or "")+" "+(r.get("content","") or ""))]
                if hits:
                    leaks.append({"term": neg, "n": len(hits), "pct": round(len(hits)/len(res),3) if res else 0})
            rec["leaks"] = leaks
            # freshness date analysis
            if label in ("fresh-dates",):
                years = []
                for r in res:
                    blob = (r.get("title","") or "")+" "+(r.get("content","") or "")[:300]+" "+(r.get("url","") or "")
                    y = year_of(blob)
                    if y: years.append(y)
                rec["result_years"] = years
                rec["fresh_2026_or_2025_frac"] = round(sum(1 for y in years if y>=2025)/len(years),3) if years else 0
            # show top results
            rec["top"] = []
            for r in res[:5]:
                rec["top"].append({"score": round(float(r.get("score",0) or 0),3),
                                   "title": (r.get("title","") or "")[:80],
                                   "url": (r.get("url","") or "")[:80]})
        results.append(rec)
        n = len(results)
        if d is None:
            print(f"[{n}] ERR {q!r} {err}")
        else:
            leakstr = " LEAK" if rec.get("leaks") else ""
            yr = rec.get("fresh_2026_or_2025_frac")
            yrstr = f" y25+={yr}" if yr is not None else ""
            print(f"[{n}/{len(battery)}] {label:<12} intent={str(rec.get('intent')):<13} n={rec.get('n_results'):>2} "
                  f"neg={rec.get('neg')}{leakstr}{yrstr} lat={lat:.1f}s")
    out = {"generated": time.strftime("%Y-%m-%dT%H:%M:%S"), "queries": len(battery),
           "total_time_s": round(time.time()-start,1), "results": results}
    json.dump(out, open(OUT,"w"), indent=2)
    print(f"\nWrote {OUT}")
    # detailed dumps
    for rec in results:
        if rec["label"] in ("neg-depth","chain") and rec.get("leaks"):
            print(f"\n  DEEP LEAK: {rec['q']!r} neg={rec['neg']} leaks={rec['leaks']}")
            for t in rec["top"][:5]:
                print(f"     [{t['score']}] {t['title']}")
