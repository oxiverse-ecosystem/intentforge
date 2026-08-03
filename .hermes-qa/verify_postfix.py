#!/usr/bin/env python3
"""Post-fix COLD verification: re-run failing queries + regression sample."""
import json, time, urllib.parse, urllib.request

BASE = "http://localhost:4000"

# The two FAIL/PARTIAL queries we fixed (run COLD), plus extra negative-constraint
# variants to prove the soft-penalty fix generalizes.
FIXED = [
    "text editor without vim keybindings for people who hate modal editing",
    "static site generator instead of jekyll for a technical blog",
    "how does photosynthesis actually work at the molecular level",
    # extra negative-constraint checks (generalization proof):
    "python web framework not django",
    "text editor without vim",
    "search engine alternative to google that respects privacy",
    "linux distro no ubuntu",
    "css framework besides bootstrap for rapid ui prototyping",
    "javascript framework except react for building single page applications",
    "programming language other than java for building android apps",
    "static site generator instead of jekyll",
    # regression sample (previously PASSING, must stay good):
    "what is the best way to learn rust programming language as a complete beginner",
    "history of the world wide web and how it changed society",
    "restaurants serving authentic south indian food in hyderabad",
    "compare postgresql and mysql for a small web application",
    "latest developments in quantum computing research this year",
    "how much does it cost to buy a decent gaming laptop under 80000 rupees",
    "coffee shops with free wifi in tokyo shinjuku area",
    "best open source password manager that is not lastpass",
    "how to set up a wireguard vpn server on a cheap vps",
    "what is the meaning of the word biryani and its origin",
    "how to make a simple rest api with go and postgresql",
    "what happened in the field of artificial intelligence during the month of july 2026",
]

def get(q):
    url = BASE + "/search?" + urllib.parse.urlencode({"q": q})
    req = urllib.request.Request(url, headers={"User-Agent": "if-verify/1.0"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode("utf-8", "replace"))

out = []
for q in FIXED:
    try:
        t0 = time.time()
        j = get(q)
        dt = round(time.time() - t0, 2)
        rec = {
            "query": q, "intent": j.get("intent"), "conf": j.get("confidence"),
            "before": j.get("results_before_filter"), "after": j.get("results_after_filter"),
            "total": j.get("total"), "n": len(j.get("results", [])),
            "elapsed_s": dt,
            "top5": [{"t": r.get("title"), "u": r.get("url"), "s": r.get("score"),
                      "src": r.get("sources")} for r in j.get("results", [])[:5]],
            "warnings": j.get("warnings"),
        }
        out.append(rec)
        print(f"[{len(out):2}] before={rec['before']} after={rec['after']} n={rec['n']} "
              f"intent={rec['intent']} {dt}s  q={q!r}")
        for k, r in enumerate(rec["top5"]):
            print(f"      #{k+1} {r['t']!r} [{r['s']}] {r['src']}")
    except Exception as e:
        print(f"[{len(out)+1:2}] ERROR {e}  q={q!r}")
        out.append({"query": q, "error": str(e)})

with open(".hermes-qa/verify_postfix.json", "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2, ensure_ascii=False)
print(f"\nWrote .hermes-qa/verify_postfix.json ({len(out)} queries)")
