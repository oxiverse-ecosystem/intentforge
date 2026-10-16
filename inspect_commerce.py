#!/usr/bin/env python3
import json, urllib.request, urllib.parse
q = "best wireless earbuds under 50"
r = urllib.request.urlopen(f"http://localhost:4000/shopping?q={urllib.parse.quote(q)}&count=3", timeout=30)
d = json.loads(r.read())
for i, res in enumerate(d.get("results", [])[:3]):
    print(f"--- result[{i}] ---")
    print("  keys:", sorted(res.keys()))
    prov = res.get("commerce_provenance")
    if prov:
        print("  commerce_provenance:", json.dumps(prov, indent=4))
    else:
        print("  commerce_provenance: NONE")
    comm = res.get("commerce")
    if comm:
        print("  commerce:", json.dumps(comm, indent=4))
    aff = res.get("affiliate")
    if aff:
        print("  affiliate:", json.dumps(aff, indent=4))
