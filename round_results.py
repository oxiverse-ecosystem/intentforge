import json, subprocess, time, sys

queries = [
    # Historical/cultural (not in log)
    "what caused the fall of the roman empire according to modern historians",
    "how did the silk road trade network shape medieval european cuisine",
    "why did the mayan civilization collapse archaeological evidence",
    "what are the origins of the viking runic alphabet and how was it used",
    "how did the industrial revolution change family structures in england",
    
    # Science/nature (not in log)
    "why do some animals hibernate while others migrate in winter",
    "how do honeybees communicate the location of flowers to each other",
    "what is the role of mycorrhizal networks in forest ecosystems",
    "how do chameleons change color at the cellular level",
    "what causes bioluminescence in deep sea creatures evolutionary advantage",
    
    # Technical (different angles)
    "how does a bloom filter work in distributed databases like cassandra",
    "what is the difference between rest and graphql for mobile app backends",
    "how to implement rate limiting in a microservices architecture with redis",
    "what are zero knowledge proofs and how do they enable private transactions",
    "how does the linux kernel scheduler handle real time processes",
    
    # Commerce (different products)
    "buy sony a7 iv mirrorless camera body only best price india with warranty",
    "refurbished macbook pro m3 max 16 inch price in india with original box",
    "best budget robot vacuum under 25000 rupees with mop and self empty dock",
    "samsung galaxy tab s9 fe price in india with s pen and keyboard cover",
    "automatic espresso machine with built in grinder under 30000 rupees india",
    
    # Local/geo (different cities)
    "best street food in varanasi near kashi vishwanath temple for first time visitors",
    "quiet co working spaces in bengaluru koramangala with 24 7 access and meeting rooms",
    "top rated ayurvedic treatment centers in kerala for chronic back pain",
    "best places to watch sunset in goa south beach not crowded with shacks",
    "weekend getaway hill stations near pune within 150 km for solo travelers",
    
    # Health/wellness (different angles)
    "what are the symptoms of vitamin d deficiency in indian adults and how to correct",
    "how does intermittent fasting affect insulin sensitivity in women over 40",
    "what is the gut brain axis and how does it influence anxiety and mood",
    "how to improve sleep quality without medication for shift workers",
    "what are the early signs of thyroid dysfunction in young men and natural management",
    
    # Food/recipe (different dishes)
    "how to make authentic rajasthani dal baati churma at home step by step",
    "traditional kerala fish curry recipe with coconut milk and kokum",
    "how to ferment homemade kimchi without fish sauce vegan version",
    "best way to cook tender lamb chops on a cast iron skillet without oven",
    "how to make soft and fluffy naan bread at home without tandoor oven",
    
    # Career/education (different angles)
    "how to transition from mechanical engineering to data science without a masters degree",
    "what are the career prospects after bsc physics in india for research oriented students",
    "how to prepare for cat 2026 while working full time in a tech company",
    "what is the scope of ethical hacking as a career in india with ceh certification",
    "how to build a portfolio for ui ux design jobs without formal design education",
    
    # Entertainment/media
    "best underrated sci fi movies on netflix 2026 that most people missed",
    "how to start a successful tech youtube channel with zero subscribers in 2026",
    "what are the best podcasts for learning about behavioral economics and psychology",
    
    # Sports/fitness
    "how to prevent shin splints when starting to run on concrete roads as a beginner",
    "best yoga sequence for desk workers with lower back pain and tight hip flexors",
]

print(f"Total queries: {len(queries)}")

results = []
for i, q in enumerate(queries):
    url = f"http://localhost:4000/search?q={q.replace(' ', '+')}&limit=10"
    try:
        r = subprocess.run(["curl", "-s", "--max-time", "30", url], capture_output=True, text=True, timeout=35)
        data = json.loads(r.stdout)
        total = data.get("total", 0)
        intent = data.get("intent", "?")
        conf = data.get("confidence", 0)
        sources = set()
        for res in data.get("results", [])[:5]:
            for s in res.get("sources", []):
                sources.add(s)
        top_titles = [r.get("title", "")[:60] for r in data.get("results", [])[:3]]
        results.append({
            "query": q,
            "total": total,
            "intent": intent,
            "confidence": conf,
            "sources": list(sources),
            "top_titles": top_titles,
            "before_filter": data.get("results_before_filter", 0),
            "after_filter": data.get("results_after_filter", 0),
        })
        print(f"[{i+1}/{len(queries)}] {q[:50]}... -> total={total} intent={intent} conf={conf:.2f} sources={list(sources)[:3]}")
    except Exception as e:
        print(f"[{i+1}/{len(queries)}] {q[:50]}... -> ERROR: {e}")
        results.append({"query": q, "error": str(e)})

with open("round_results.json", "w") as f:
    json.dump(results, f, indent=2)

print(f"\nDone. {len(results)} queries processed. Saved to round_results.json")
