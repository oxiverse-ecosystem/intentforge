#!/usr/bin/env python3
import json, subprocess, urllib.parse
BASE = "http://localhost:4000/search"
def run(q):
    enc = urllib.parse.quote(q, safe="")
    out = subprocess.run(["curl","-s","-m","30",f"{BASE}?q={enc}"],capture_output=True,text=True,timeout=40).stdout
    return json.loads(out)
for q in ["python filetype:pdf filetype:doc","python filetype:pdf filetype:docx",
          "python filetype:pdf OR filetype:doc","python filetype:pdf"]:
    d = run(q)
    sc = d.get("structured_constraints",{})
    print("Q:", q)
    print("  applied:", d.get("applied_constraints"), " file_types:", sc.get("file_types"))
    print("  results:", len(d.get("results",[])), " total:", d.get("total"), " warnings:", d.get("warnings"))
