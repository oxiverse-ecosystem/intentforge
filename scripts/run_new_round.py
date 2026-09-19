#!/usr/bin/env python3
"""Run 25 new unique NL queries against localhost:4000 and save results."""
import json
import subprocess
import sys
import time
import urllib.parse
import urllib.request

QUERIES = [
    # Multi-constraint, complex
    "best waterproof Bluetooth speaker for shower and outdoor use under 3000 rupees not jbl not boat",
    "i have been working as a mechanical engineer for 5 years and want to transition into data science what skills do i need to learn and how long will it take",
    "how to build a solar powered Raspberry Pi weather station with remote monitoring step by step",
    "difference between cognitive behavioral therapy and dialectical behavior therapy for anxiety",
    
    # Comparative
    "compare Nextcloud and Syncthing for self-hosted file sync with end-to-end encryption on a budget vps",
    "neovim vs helix vs kakoune which modal editor has the best LSP support for rust development in 2026",
    "compare Meilisearch and Typesense for a fast typo-tolerant search index for an e-commerce catalog with faceted filters",
    
    # Negated / exclusion
    "best email client for linux not thunderbird not evolution not geary with exchange support",
    "how to learn machine learning without a university degree or expensive bootcamp using only free resources",
    "electric toothbrush without bluetooth or app connectivity that just works well under 4000 rupees",
    "best note-taking app that does not require cloud sync or account signup for android",
    
    # Temporal / fresh
    "latest news about India's semiconductor fabrication plant construction progress",
    "recent advances in mRNA cancer vaccine trials and approval status 2026",
    "upcoming changes to the Indian tax regime for the upcoming budget session",
    
    # Transactional / price
    "iPhone 16 Pro 128GB price in India with bank discount and no-cost EMI options",
    "best budget 4K monitor with USB-C power delivery under 25000 rupees for macbook",
    "refurbished Dell XPS 15 price in India with warranty and original box",
    
    # Local / geo
    "best coffee shops in Indiranagar Bangalore with good WiFi power outlets and quiet atmosphere for working",
    "top rated dental clinics in Jubilee Hills Hyderabad with affordable root canal treatment",
    "where to buy fresh organic vegetables in Chennai early morning wholesale market near Koyambedu",
    
    # Ambiguous entity
    "tiger the animal habitat conservation vs the vodka brand history and product range",
    "amazon the rainforest deforestation vs Amazon the company quarterly earnings 2026",
    
    # Long conversational
    "i am a 35 year old homemaker who has not worked for 8 years and now wants to start a career in digital marketing what is the most realistic path for me given my age and gap in employment",
    "my 7 year old daughter wants to learn coding but i am not a programmer myself what are the best tools platforms and approaches to get her started without burning a hole in my pocket",
    
    # Non-English named entities
    "how to make authentic pad thai from scratch with tamarind paste and fish sauce at home",
    "traditional Japanese miso ramen recipe with chashu pork and ajitsuke tamago from scratch",
]

def search(q):
    """Run a search query and return the JSON response."""
    params = urllib.parse.urlencode({"q": q})
    url = f"http://localhost:4000/search?{params}"
    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            return json.loads(resp.read().decode())
    except Exception as e:
        return {"error": str(e)}

def main():
    results = []
    for i, q in enumerate(QUERIES):
        print(f"[{i+1}/{len(QUERIES)}] {q[:70]}")
        r = search(q)
        if "error" in r:
            print(f"  ERROR: {r['error']}")
            results.append({"query": q, "error": r["error"], "verdict": "ERROR"})
        else:
            total = r.get("total", 0)
            items = r.get("items", [])
            top5 = [
                {
                    "title": it.get("title", "")[:100],
                    "url": it.get("url", "")[:120],
                    "score": it.get("score", 0),
                    "sources": it.get("sources", []),
                    "is_local": it.get("is_local", False),
                }
                for it in items[:5]
            ]
            verdict = "PASS" if total >= 5 else ("PARTIAL" if total >= 1 else "FAIL")
            intent = r.get("intent", r.get("category", "?"))
            conf = r.get("confidence", 0)
            before = r.get("results_before_filter", 0)
            after = r.get("results_after_filter", total)
            constraints = r.get("constraints", [])
            rec = {
                "query": q,
                "intent": intent,
                "confidence": conf,
                "total": total,
                "before_filter": before,
                "after_filter": after,
                "constraints": constraints,
                "verdict": verdict,
                "top5": top5,
            }
            results.append(rec)
            print(f"  {total} results | intent={intent} | conf={conf:.2f} | verdict={verdict}")
            if total <= 2:
                print(f"  LOW RESULTS — investigating...")
        time.sleep(0.3)

    # Save results
    with open(".hermes-qa/reports/round_2026-09-18T0832Z_new.json", "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nSaved {len(results)} results to .hermes-qa/reports/round_2026-09-18T0832Z_new.json")
    
    # Summary
    pass_count = sum(1 for r in results if r.get("verdict") == "PASS")
    partial_count = sum(1 for r in results if r.get("verdict") == "PARTIAL")
    fail_count = sum(1 for r in results if r.get("verdict") == "FAIL")
    error_count = sum(1 for r in results if r.get("verdict") == "ERROR")
    print(f"\nSummary: {pass_count} PASS | {partial_count} PARTIAL | {fail_count} FAIL | {error_count} ERROR")
    
    # Queries with low results
    low = [r for r in results if r.get("total", 0) <= 3 and r.get("total", 0) > 0]
    if low:
        print("\nLOW RESULT queries:")
        for r in low:
            print(f"  [{r['total']}] {r['query']}")

if __name__ == "__main__":
    main()
