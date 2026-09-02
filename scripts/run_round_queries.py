#!/usr/bin/env python3
"""Run IntentForge NL queries and capture results."""
import json
import subprocess
import sys
import time
import urllib.parse

OUTFILE = ".hermes-qa/round_20260902T0245Z_new.json"

queries = [
    "best running shoes for flat pronation under 8000 rupees not nike or adidas",
    "compare postgresql and mysql for a high write throughput application in 2026",
    "how to make sourdough bread at home without a dutch oven or banneton",
    "what are the latest advances in solid state battery technology this month",
    "best budget wireless mouse with silent clicks and long battery life under 30 dollars",
    "quiet study cafes with power outlets in hyderabad near madhapur for remote work",
    "how to remove red wine stains from white cotton fabric without bleach",
    "rust vs go for building a microservices backend which is better for a small team",
    "authentic mughlai biryani recipe from old delhi like my nani used to make",
    "best noise cancelling earbuds under 5000 rupees with anc and multipoint",
    "where to buy organic cold pressed coconut oil near me that is not adulterated",
    "how to set up a home media server with jellyfin on ubuntu for streaming to tv",
    "best indian authors who wrote mythological fiction in the last decade",
    "what are the health effects of drinking green tea on an empty stomach daily",
    "compare tmux and screen for terminal multiplexing which should i use in 2026",
    "how to grow tomatoes on a balcony in mumbai during monsoon season",
    "best free alternatives to microsoft excel that work offline and support macros",
    "where to find authentic korean bibimbap in bangalore that is not a chain",
    "how to fix a leaking tap washer without calling a plumber step by step",
    "best lightweight linux distribution for an old laptop with 2gb ram and pentium processor",
    "why do some developers prefer vim over vscode for code editing",
    "how to make authentic kerala fish curry without coconut milk",
    "best budget smartphone under 20000 rupees with clean android and fast charging",
    "explain like im thirty how a blockchain works without using technical jargon",
    "where to buy fresh sourdough bread in bangalore early morning near indiranagar",
    # Extra queries for goals endpoint testing
    "best online course for learning machine learning with python for beginners",
    "how to start a small organic farming business in india with less than 1 lakh investment",
    "what are the best practices for securing a rest api in production",
    "compare kubernetes and docker swarm for container orchestration in 2026",
    "how to make authentic hyderabadi haleim at home from scratch",
]

results = []
total = len(queries)

for i, q in enumerate(queries):
    encoded = urllib.parse.quote(q)
    url = f"http://localhost:4000/search?q={encoded}&limit=5"
    
    try:
        # Use subprocess to call curl
        proc = subprocess.run(
            ["curl", "-s", "--max-time", "15", url],
            capture_output=True, text=True, timeout=20
        )
        raw = proc.stdout
        
        if raw.strip():
            d = json.loads(raw)
            entry = {
                'query': q,
                'status': 200,
                'intent': d.get('intent', ''),
                'category': d.get('category', ''),
                'confidence': d.get('confidence', 0),
                'total': d.get('total', 0),
                'before': d.get('results_before_filter', 0),
                'after': d.get('results_after_filter', 0),
                'top5': [{'title': r.get('title', ''), 'url': r.get('url', ''), 'score': r.get('score', 0), 'sources': r.get('sources', [])} for r in d.get('results', [])[:5]],
                'constraints': d.get('constraints', []),
                'structured': d.get('structured_constraints', {}),
                'spell': d.get('spell_corrected_query'),
                'warnings': d.get('warnings', []),
                'shopping': 'shopping' in d,
            }
            print(f"  [{i+1}/{total}] OK intent={entry['intent']} conf={entry['confidence']:.2f} total={entry['total']}")
        else:
            entry = {'query': q, 'status': 0, 'error': 'empty'}
            print(f"  [{i+1}/{total}] EMPTY")
    except Exception as e:
        entry = {'query': q, 'status': 0, 'error': str(e)}
        print(f"  [{i+1}/{total}] FAIL: {e}")
    
    results.append(entry)
    time.sleep(0.1)

# Write JSON
with open(OUTFILE, 'w') as f:
    json.dump(results, f, indent=2)

print(f"\nDone. {len(results)} queries saved to {OUTFILE}")
