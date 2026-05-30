#!/usr/bin/env python3
"""Quick re-test of negative constraints after fix."""
import requests, json, time

BASE_URL = "http://localhost:4000"
SEARCH_URL = f"{BASE_URL}/search"

NEGATIVE_QUERIES = [
    {"q": "python web framework not django not flask", "neg_terms": ["django", "flask"]},
    {"q": "javascript framework except react and angular", "neg_terms": ["react", "angular"]},
    {"q": "programming language for beginners not python", "neg_terms": ["python"]},
    {"q": "database for web app no sql not mongodb", "neg_terms": ["mongodb"]},
    {"q": "static site generator not jekyll not hugo", "neg_terms": ["jekyll", "hugo"]},
    {"q": "css framework without bootstrap not tailwind", "neg_terms": ["bootstrap", "tailwind"]},
]

MULTI_QUERIES = [
    {"q": "python async web framework with websocket support not django", "neg": ["django"]},
    {"q": "linux container runtime lightweight alternative to docker for embedded", "neg": ["docker"]},
    {"q": "python machine learning library for time series forecasting not tensorflow", "neg": ["tensorflow"]},
]

print("=" * 70)
print("  NEGATIVE CONSTRAINT RE-TEST (post-fix)")
print("=" * 70)

total_checked = 0
total_violations = 0

for spec in NEGATIVE_QUERIES:
    resp = requests.get(SEARCH_URL, params={"q": spec["q"]}, timeout=30)
    data = resp.json()
    web = data.get("web_results", [])
    neg_constraints = data.get("structured_constraints", {}).get("negative", [])
    
    # Check top 10 for violations
    violations = []
    for w in web[:10]:
        text = (w.get("title", "") + " " + w.get("content", "")).lower()
        for term in spec["neg_terms"]:
            if term.lower() in text:
                violations.append({"title": w.get("title", "")[:60], "term": term})
                break
    
    checked = min(len(web), 10)
    total_checked += checked
    total_violations += len(violations)
    rate = len(violations) / max(checked, 1) * 100
    marker = "✓" if rate <= 10 else "⚠" if rate <= 25 else "✗"
    
    print(f"\n  {marker} [{resp.elapsed.total_seconds()*1000:.0f}ms] web={len(web)} | {spec['q'][:60]}")
    print(f"    neg_constraints: {neg_constraints}")
    print(f"    violations: {len(violations)}/{checked} ({rate:.0f}%)")
    for v in violations[:3]:
        print(f"      ⚠ '{v['term']}' in: {v['title']}")

print(f"\n{'='*70}")
print(f"  MULTI-CONSTRAINT NEGATIVE RE-TEST")
print(f"{'='*70}")

for spec in MULTI_QUERIES:
    resp = requests.get(SEARCH_URL, params={"q": spec["q"]}, timeout=30)
    data = resp.json()
    web = data.get("web_results", [])
    neg_constraints = data.get("structured_constraints", {}).get("negative", [])
    
    violations = []
    for w in web[:10]:
        text = (w.get("title", "") + " " + w.get("content", "")).lower()
        for term in spec["neg"]:
            if term.lower() in text:
                violations.append({"title": w.get("title", "")[:60], "term": term})
                break
    
    checked = min(len(web), 10)
    total_checked += checked
    total_violations += len(violations)
    rate = len(violations) / max(checked, 1) * 100
    marker = "✓" if rate <= 10 else "⚠" if rate <= 25 else "✗"
    
    print(f"\n  {marker} [{resp.elapsed.total_seconds()*1000:.0f}ms] web={len(web)} | {spec['q'][:60]}")
    print(f"    neg_constraints: {neg_constraints}")
    print(f"    violations: {len(violations)}/{checked} ({rate:.0f}%)")
    for v in violations[:3]:
        print(f"      ⚠ '{v['term']}' in: {v['title']}")

print(f"\n{'='*70}")
overall_rate = total_violations / max(total_checked, 1) * 100
marker = "✓" if overall_rate <= 10 else "⚠" if overall_rate <= 25 else "✗"
print(f"  {marker} OVERALL: violations={total_violations}/{total_checked} ({overall_rate:.1f}%)")
print(f"  (Before fix: 40.0%)")
print(f"{'='*70}")
