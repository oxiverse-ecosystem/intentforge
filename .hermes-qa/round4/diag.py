#!/usr/bin/env python3
import json, urllib.request, urllib.parse, urllib.error, time, sys
BASE="http://localhost:4000"
items=json.load(open("queries.json"))
watch={"s02","s14","s03","s05","s06","s10","s11","s13","s20","s23","s25","s26"}
for it in items:
    if it["id"] not in watch: continue
    q=it["q"]; t0=time.time()
    try:
        url=BASE+"/search?q="+urllib.parse.quote(q)+"&limit=24"
        with urllib.request.urlopen(url,timeout=60) as r:
            j=json.loads(r.read().decode())
        neg=(j.get("structured_constraints") or {}).get("negative")
        print("==== %s | total=%s before=%s neg=%s ms=%d"%(it["id"],j.get("total"),j.get("results_before_filter"),neg,int((time.time()-t0)*1000)))
        for i,res in enumerate(j.get("results",[])):
            print("  %2d  sc=%.3f  auth=%.2f  src=%s  %s"%(i+1,res.get("score",0),res.get("authority",0),",".join(res.get("sources",[])),res.get("title","")[:90]))
    except Exception as e:
        print(it["id"],"ERR",repr(e))
