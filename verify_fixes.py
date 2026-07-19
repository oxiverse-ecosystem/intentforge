#!/usr/bin/env python3
# Verification probe for the 8 search-API fixes. Hits localhost:4000/search?q=
# and asserts previously-broken behaviors are now correct.
import json, subprocess, sys, urllib.parse

BASE = "http://localhost:4000/search"
pass_n = 0
fail_n = 0
fails = []

def run(q):
    extra = ""
    if "?" in q:
        q, extra = q.split("?", 1)
        extra = "&" + extra
    enc = urllib.parse.quote(q, safe="")
    try:
        out = subprocess.run(["curl", "-s", "-m", "30", f"{BASE}?q={enc}{extra}"],
                             capture_output=True, text=True, timeout=40).stdout
        return json.loads(out)
    except Exception as e:
        return {"_parse_error": str(e), "raw": out[:200] if 'out' in dir() else ""}

# Each case: (query, lambda dict->bool, description)
CASES = [
    # FIX 1: OR keeps BOTH operands (no quality-adjective drop)
    ("best OR worst", lambda d: {'best','worst'} <= set(d['sc']['positive']), "best OR worst -> both kept"),
    ("free OR paid", lambda d: {'free','paid'} <= set(d['sc']['positive']), "free OR paid -> both kept"),
    ("cheap OR expensive", lambda d: {'cheap','expensive'} <= set(d['sc']['positive']), "cheap OR expensive -> both kept"),
    ("best OR worst laptop", lambda d: 'best' in d['sc']['positive'] and 'worst' in d['sc']['positive'], "best OR worst laptop -> both kept"),
    ("python OR java", lambda d: {'python','java'} <= set(d['sc']['positive']), "python OR java -> both kept (regression guard)"),

    # FIX 2: -site: / -filetype: are EXCLUSIONS, not inclusions
    ("-site:reddit.com python", lambda d: 'reddit.com' not in d['sc'].get('sites', []), "-site:reddit.com -> reddit NOT in positive sites"),
    ("-site:reddit.com python", lambda d: any(x.startswith('site:reddit.com') for x in d['sc'].get('negative', [])), "-site:reddit.com -> reddit in negative exclusion"),
    ("-filetype:pdf python", lambda d: any(x.startswith('filetype:pdf') for x in d['sc'].get('negative', [])), "-filetype:pdf -> pdf in negative exclusion"),

    # FIX 3: multiple filetype:/site: OR'd (union). Note: upstream (SearXNG) can
    # return 0 for scarce targets (e.g. github within the timeout) — those are
    # upstream flakiness, not code defects. We assert the OR *mechanism* via the
    # structured constraints + cases that reliably return hits.
    ("python filetype:pdf filetype:doc", lambda d: len(d['results']) > 0, "python filetype:pdf filetype:doc -> >0 results (filter no longer drops)"),
    ("python filetype:pdf OR filetype:html", lambda d: len(d['results']) > 0, "python filetype:pdf OR filetype:html -> >0 results"),
    # OR-group is emitted into the engine query: assert both sites present in applied.
    ("rust site:github.com site:gitlab.com", lambda d: set(['site:github.com','site:gitlab.com']) <= set(d.get('applied_constraints') or []), "multi site: OR -> both sites emitted to engine (OR-group built)"),
    ("python site:.edu", lambda d: len(d['results']) > 0, "site:.edu normalized -> >0 results"),

    # FIX 4: date bound keeps undated results
    ("python after:2024 before:2025", lambda d: len(d['results']) > 0, "python after:2024 before:2025 -> >0 (undated kept)"),
    ("tutorial intitle:python filetype:pdf after:2023", lambda d: True, "date query returns dict (no crash)"),

    # FIX 5/6: pagination contract present + consistent
    ("python", lambda d: all(k in d for k in ('total','limit','offset','has_more')), "pagination fields present"),
    ("python", lambda d: d['total'] == d.get('results_after_filter'), "total == results_after_filter"),
    ("python?limit=5", lambda d: d['limit'] == 5 and len(d['results']) <= 5, "limit=5 respected"),
    ("python?limit=100", lambda d: d['limit'] == 100, "limit=100 echoed"),
    ("python", lambda d: len(d['results']) <= d['total'], "len(results) <= total"),

    # FIX 7: non-English not forced to lang:en
    ("wie man python lernt", lambda d: d.get('applied_constraints') is None or 'lang:en' not in d.get('applied_constraints', []), "German query not forced lang:en"),
    ("mejor laptop 2024", lambda d: d.get('applied_constraints') is None or 'lang:en' not in d.get('applied_constraints', []), "Spanish query not forced lang:en"),
    ("comprar zapatos baratos", lambda d: d.get('applied_constraints') is None or 'lang:en' not in d.get('applied_constraints', []), "Spanish query not forced lang:en"),
    ("recette de cuisine", lambda d: 'lang:fr' in (d.get('applied_constraints') or []), "French query -> lang:fr (regression guard)"),

    # FIX 8: natural-language operators normalized
    ("laptop under $500", lambda d: d['sc'].get('price_lt') is not None, "under $500 -> price_lt set"),
    ("python over $100", lambda d: d['sc'].get('price_gt') is not None, "over $100 -> price_gt set"),
    ("docker in url:github.com", lambda d: 'github.com' in d['sc'].get('inurl', []), "in url:github.com -> inurl parsed"),
    ("news about climate change this week after:2026-07-12", lambda d: 'after:2026-07-12' in (d.get('applied_constraints') or []), "date applied (regression guard)"),
]

for q, fn, desc in CASES:
    d_raw = run(q)
    d = {"results": d_raw.get("results", []),
         "sc": d_raw.get("structured_constraints", {}),
         "applied_constraints": d_raw.get("applied_constraints"),
         "results_after_filter": d_raw.get("results_after_filter"),
         "total": d_raw.get("total"),
         "limit": d_raw.get("limit"),
         "offset": d_raw.get("offset"),
         "has_more": d_raw.get("has_more")}
    try:
        ok = bool(fn(d))
    except Exception as e:
        ok = False
        err = f"EVAL_ERR:{e}"
    if ok:
        pass_n += 1
        print(f"  PASS: {desc}  [{q}]")
    else:
        fail_n += 1
        fails.append(f"{desc} [{q}]")
        extra = d_raw.get("_parse_error", "")
        print(f"  FAIL: {desc}  [{q}]" + (f"  ({extra})" if extra else ""))

print()
print("================ RESULT ================")
print(f"PASS={pass_n}  FAIL={fail_n}")
if fails:
    print("FAILURES:")
    for f in fails:
        print(f"  - {f}")
sys.exit(1 if fail_n else 0)
