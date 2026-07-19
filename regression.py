#!/usr/bin/env python3
import json, subprocess, urllib.parse
BASE="http://localhost:4000/search"
def run(q, extra=""):
    enc=urllib.parse.quote(q,safe="")
    out=subprocess.run(["curl","-s","-m","30",f"{BASE}?q={enc}{extra}"],capture_output=True,text=True,timeout=40).stdout
    try: return json.loads(out)
    except: return {"_raw":out[:200]}
qs=[
 ("python","basic"),
 ("best laptop 2024","quality+noun+year"),
 ("tutorial intitle:python filetype:pdf","operators"),
 ("rust OR go OR python","multi-OR"),
 ("news about ai this week","news intent"),
 ("resepice crepe","spelling (fr)"),
 ("how to learn guitar","how-to"),
 ("python -site:reddit.com","negation"),
 ("machine learning course free OR paid","OR quality"),
 ("docker inurl:github.com","inurl NL"),
 ("laptop under $500 site:amazon.com","price+site NL"),
]
ok=0;bad=0
for q,tag in qs:
    d=run(q)
    n=len(d.get("results",[]))
    total=d.get("total")
    intent=d.get("intent")
    has_more=d.get("has_more")
    status="OK" if n>0 else "EMPTY"
    if n>0: ok+=1
    else: bad+=1
    print(f"[{status}] ({tag}) {q!r} -> n={n} total={total} has_more={has_more} intent={intent}")
    # sanity: total>=n, limit present
    if total is not None and n>total:
        print("   !! n>total inconsistent")
print(f"\nRegression: {ok} ok, {bad} empty")
