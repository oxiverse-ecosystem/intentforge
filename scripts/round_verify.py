import json, sys, subprocess, time

QUERIES = [
    # Technical / how-to
    "how to build a rest api in rust",
    "how does async await work in javascript",
    "what is the difference between tcp and udp",
    "how to implement oauth 2.0 in a web application step by step",
    "best practices for writing unit tests in python",
    
    # Comparison / multi-constraint
    "react vs vue vs angular for large scale enterprise applications",
    "postgres vs mysql for high concurrency write heavy workloads",
    "rust vs go for building microservices in 2026",
    
    # Negated / alternative
    "best noise cancelling headphones without bluetooth for airplane travel",
    "content management system alternative to wordpress that is headless",
    "programming language for systems programming other than c or rust",
    
    # Transactional / price / commercial
    "samsung galaxy s25 ultra 256gb price in india with exchange offer",
    "best budget mechanical keyboard with hot swap switches under 8000 rupees",
    "refurbished macbook air m3 price in usa with student discount",
    
    # Temporal / fresh / news
    "latest developments in quantum computing hardware 2026",
    "new movies released this week streaming on netflix and prime video",
    "recent changes to rust edition 2024 migration guidelines",
    
    # Local / geo / regional
    "best coffee shops in bangalore with wifi and quiet workspace",
    "how to get a driving license in hyderabad step by step process",
    "top engineering colleges in telangana with good placement record",
    
    # Ambiguous entity / disambiguation
    "python the programming language vs the snake habitat and diet",
    "jaguar the car vs the animal top speed and acceleration",
    "apple the company vs the fruit nutrition facts and health benefits",
    
    # Long conversational
    "i have been learning web development for 6 months and built a few projects with html css and javascript what should i focus on next to get my first job as a frontend developer",
    "im a college student from india interested in machine learning but dont know where to start can you suggest a roadmap from beginner to job ready",
    
    # Non-English named entities
    "authentic hyderabadi biryani recipe at home with mutton",
    "best traditional handicrafts to buy from varanasi silk weavers",
    "history and significance of konark sun temple in odisha",
    
    # Gibberish / edge
    "asdfghjkl qwertyuiop zxcvbnm nonsense random",
    
    # Fresh + specific
    "latest ai news today from major labs openai anthropic google",
    
    # Additional unique
    "how to set up ci cd pipeline for a rust project using github actions",
    "difference between rest graphql and grpc when to use each",
    "best ide for python development with good debugger and linter integration",
    "how to protect privacy online from data brokers and trackers",
]

print(f"Total queries: {len(QUERIES)}")
print("=" * 80)

# Collect results
report_data = []

for i, q in enumerate(QUERIES):
    encoded = q.replace(" ", "+")
    try:
        r = subprocess.run(
            ["curl", "-s", f"http://localhost:4000/search?q={encoded}&limit=24"],
            capture_output=True, text=True, timeout=60
        )
        d = json.loads(r.stdout)
        intent = d.get("intent", "?")
        category = d.get("category", "?")
        conf = d.get("confidence", 0)
        total = d.get("total", 0)
        before_f = d.get("results_before_filter", 0)
        after_f = d.get("results_after_filter", 0)
        results = d.get("results", [])
        local_c = sum(1 for r in results if r.get("is_local"))
        web_c = sum(1 for r in results if not r.get("is_local"))
        all_local = all(r.get("is_local") for r in results) if results else True
        has_web = any(not r.get("is_local") for r in results)
        
        top3 = results[:3]
        top_titles = [r.get("title", "?")[:60] for r in top3]
        
        report_data.append({
            "query": q,
            "intent": intent,
            "category": category,
            "confidence": conf,
            "total": total,
            "before_filter": before_f,
            "after_filter": after_f,
            "local_count": local_c,
            "web_count": web_c,
            "all_local": all_local,
            "has_web": has_web,
            "top_titles": top_titles,
        })
        
        flag = "⚠️ ALL-LOCAL" if all_local and not has_web else "✅ mixed" if has_web else "❌ no-web"
        print(f"\n[{i+1:2d}] {q[:70]}")
        print(f"     intent={intent}({conf:.2f}) category={category} total={total} before={before_f} after={after_f}")
        print(f"     local={local_c} web={web_c} {flag}")
        for j, t in enumerate(top_titles):
            print(f"       [{j}] {t}")
    except Exception as e:
        print(f"\n[{i+1:2d}] {q[:70]} — ERROR: {e}")
        report_data.append({"query": q, "error": str(e)})

# Summary
print("\n" + "=" * 80)
print("SUMMARY")
print("=" * 80)
all_local_count = sum(1 for r in report_data if r.get("all_local") and not r.get("has_web") and "error" not in r)
web_count = sum(1 for r in report_data if r.get("has_web") and "error" not in r)
err_count = sum(1 for r in report_data if "error" in r)
print(f"Queries run: {len(QUERIES)}")
print(f"All-local (no web): {all_local_count}")
print(f"Has web results: {web_count}")
print(f"Errors: {err_count}")

# Check for known defects
print("\n--- Known defect patterns ---")
for r in report_data:
    if "error" in r:
        continue
    q = r["query"]
    if "rust" in q.lower() and r["all_local"]:
        print(f"  P0? all-local rust query: {q[:60]}")
    if "how to build a rest api" in q.lower() and r["local_count"] > 0 and r["web_count"] == 0:
        print(f"  P0? REST API in Rust all-local ({r['local_count']} local, {r['web_count']} web)")
    if "asdfghjkl" in q.lower() and r["total"] > 0:
        print(f"  gibberish query returned {r['total']} results")

# Save report data
with open("scratch/round_query_results.json", "w") as f:
    json.dump(report_data, f, indent=2)
print("\nSaved to scratch/round_query_results.json")
