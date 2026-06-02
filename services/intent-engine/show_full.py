import json, sys

path = sys.argv[1]
d = json.load(open(path, encoding="utf-8"))
print("Query:    ", d.get("query","?"))
print("Intent:   ", d["intent"], "(conf="+str(d["confidence"])+")")
print("Expanded: ", d.get("expanded_queries"))
print("Results:  ", len(d.get("results",[])))
sc = d.get("structured_constraints",{})
print("Positive: ", sc.get("positive",[]))
print("Negative: ", sc.get("negative",[]))
print("Entities: ", json.dumps(sc.get("entities",[]), indent=2))
print()
for i, r in enumerate(d.get("results",[])[:3]):
    print("--- #"+str(i+1)+" (score="+str(round(r["score"],3))+") ---")
    print("  Title:   "+r["title"][:120])
    print("  URL:     "+r["url"][:120])
    print("  Sources: "+str(r.get("sources",[])))
    print()
