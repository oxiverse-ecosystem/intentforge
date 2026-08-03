#!/usr/bin/env python3
import json, urllib.request, urllib.parse, time
BASE="http://localhost:4000"
reg=[
 ("r3-s01","best noise cancelling headphones under 200 dollars with long battery life and good call quality for remote work"),
 ("r3-s02","open source password manager that works on linux windows and android and supports self hosting"),
 ("r3-s03","compare postgresql and mysql for a small web application that expects moderate traffic"),
 ("r3-s04","which is better for learning to code a tablet or a laptop for a teenager"),
 ("r3-s05","a productivity app that helps with time tracking but does not require a subscription"),
 ("r3-s06","a python web framework other than django and flask for building apis"),
 ("r3-s07","books on stoicism that are not written by ryan holiday"),
 ("r3-s09","what is the latest stable release of the linux kernel and what changed in it this year"),
 ("r3-s11","where can i buy an affordable standing desk under 15000 rupees in hyderabad"),
 ("r3-s21","how is the festival of diwali celebrated differently across north and south india"),
]
for tag,q in reg:
    t0=time.time()
    try:
        with urllib.request.urlopen(BASE+"/search?q="+urllib.parse.quote(q)+"&limit=3",timeout=60) as r:
            j=json.loads(r.read().decode())
        res=j.get("results",[])
        top=res[0]["title"][:75] if res else "(none)"
        print("[%s] int=%s total=%s top1: %s"%(tag,j.get("intent"),j.get("total"),top))
    except Exception as e:
        print("[%s] ERR %r"%(tag,e))
