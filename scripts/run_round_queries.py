#!/usr/bin/env python3
"""Run a batch of new unique NL queries against localhost:4000 and capture results."""
import json, urllib.request, urllib.parse, sys, os

# Read existing queries to avoid duplicates
existing = set()
if os.path.exists(".hermes-qa/query_log.txt"):
    with open(".hermes-qa/query_log.txt") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                existing.add(line.lower())

# 25 brand-new unique NL queries (not in query_log.txt)
QUERIES = [
    "how to build a privacy focused search engine from scratch using rust and python",
    "what are the health benefits of drinking green tea every morning on an empty stomach",
    "best noise cancelling headphones under 200 dollars with long battery life and comfort for airplane travel",
    "compare react and svelte for building a dashboard with real time data updates",
    "how to make sourdough bread at home without a dutch oven and with whole wheat flour",
    "latest news about space exploration missions launched in 2026",
    "what is the difference between machine learning and deep learning and when to use each",
    "quiet places to work remotely in chennai with good wifi and coffee near t nagar",
    "best budget smartphone under 25000 rupees with good camera and fast charging not from xiaomi",
    "how to remove stains from white clothes using household items like baking soda and vinegar",
    "alternative to spotify that respects user privacy and does not track listening habits",
    "step by step guide to setting up a home media server with jellyfin and tailscale",
    "what are the symptoms of vitamin d deficiency and how to fix it naturally through diet",
    "best science fiction novels by indian authors published in the last three years",
    "how to fix a leaking kitchen faucet without calling a plumber using basic tools",
    "compare golang and rust for building a high performance api server with database access",
    "where to buy authentic kerala spices online in india with reasonable shipping",
    "how to start a container garden on a small apartment balcony with limited sunlight",
    "what happened at apple wwdc 2026 and what were the major announcements",
    "best non fiction books about the history of science and technology in ancient india",
    "how to make a creamy pasta sauce without cream or cheese using cashews and nutritional yeast",
    "what are the rules for carrying lithium ion batteries on international flights in 2026",
    "best mechanical keyboard under 8000 rupees with hot swap switches and rgb lighting",
    "how to learn programming from scratch as an adult with only five hours per week",
    "compare nextjs and remix for building an ecommerce site with server side rendering",
    "what are the best practices for securing a linux server against brute force attacks",
    "how to make authentic punjabi chole bhature at home from dried chickpeas",
    "where to find free high quality stock photos for commercial use without attribution",
]

# Filter out any that accidentally match existing
new_queries = [q for q in QUERIES if q.lower() not in existing]
print(f"Running {len(new_queries)} new queries (skipped {len(QUERIES) - len(new_queries)} duplicates)")

results = []
for i, q in enumerate(new_queries):
    encoded = urllib.parse.quote(q)
    url = f"http://localhost:4000/search?q={encoded}"
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=60) as resp:
            body = json.loads(resp.read().decode())
            results.append({
                "query": q,
                "status": resp.status,
                "intent": body.get("intent"),
                "category": body.get("category"),
                "confidence": body.get("confidence"),
                "total": body.get("total"),
                "results_before_filter": body.get("results_before_filter"),
                "results_after_filter": body.get("results_after_filter"),
                "constraints": body.get("constraints"),
                "structured_constraints": body.get("structured_constraints"),
                "top5": [{"title": r.get("title","")[:100], "url": r.get("url","")[:100], "score": r.get("score"), "sources": r.get("sources")} for r in body.get("results",[])[:5]],
                "spell_corrected": body.get("spell_corrected_query"),
                "shopping_present": "shopping" in body,
            })
            print(f"  [{i+1}/{len(new_queries)}] {q[:60]}... -> intent={body.get('intent')} total={body.get('total')}")
    except Exception as e:
        results.append({"query": q, "error": str(e)})
        print(f"  [{i+1}/{len(new_queries)}] {q[:60]}... -> ERROR: {e}")

# Save results
outpath = ".hermes-qa/reports/round_20260902_newqueries.json"
os.makedirs(os.path.dirname(outpath), exist_ok=True)
with open(outpath, "w") as f:
    json.dump(results, f, indent=2)
print(f"\nSaved {len(results)} results to {outpath}")
