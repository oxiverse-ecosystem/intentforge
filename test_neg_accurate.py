#!/usr/bin/env python3
"""
IntentForge v2 — Accurate violation checker using word-boundary matching.
Matches the gateway's constraint_score logic exactly.
"""
import requests, json, time, re

BASE_URL = "http://localhost:4000"
SEARCH_URL = f"{BASE_URL}/search"

def word_boundary_match(text: str, term: str) -> bool:
    """Check if term appears as a whole word in text (matching constraint_score logic)."""
    term_lower = term.lower()
    term_normalized = ''.join(c for c in term_lower if c.isalnum())
    
    for word in text.lower().split():
        # Exact match
        if word == term_lower:
            return True
        # Trim leading/trailing non-alnum
        trimmed = word.strip(''.join(c for c in [chr(i) for i in range(128)] if not c.isalnum()))
        if trimmed == term_lower:
            return True
        # Alnum-only match
        word_clean = ''.join(c for c in word if c.isalnum())
        if word_clean == term_normalized:
            return True
        # Substring match for compound terms (like "nodejs" containing "node")
        if len(term_normalized) >= 3 and term_normalized in word_clean:
            return True
    return False

def check_violations(web_results, neg_terms, check_count=10):
    """Check top N results for negative constraint violations using word-boundary matching."""
    violations = []
    for w in web_results[:check_count]:
        title = w.get("title", "")
        content = w.get("content", "")[:500]  # first 500 chars, matching constraint_score
        text = title + " " + content
        
        for term in neg_terms:
            if word_boundary_match(text, term):
                violations.append({"title": title[:70], "term": term})
                break  # one violation per result
    return violations

# ═══════════════════════════════════════════════════════════════
# TEST CASES
# ═══════════════════════════════════════════════════════════════

NEGATIVE_QUERIES = [
    {"q": "python web framework not django not flask", "neg": ["django", "flask"]},
    {"q": "javascript framework except react and angular", "neg": ["react", "angular"]},
    {"q": "programming language for beginners not python", "neg": ["python"]},
    {"q": "database for web app no sql not mongodb", "neg": ["mongodb"]},
    {"q": "static site generator not jekyll not hugo", "neg": ["jekyll", "hugo"]},
    {"q": "css framework without bootstrap not tailwind", "neg": ["bootstrap", "tailwind"]},
]

MULTI_QUERIES = [
    {"q": "python async web framework with websocket support not django", "neg": ["django"]},
    {"q": "linux container runtime lightweight alternative to docker for embedded", "neg": ["docker"]},
    {"q": "python machine learning library for time series forecasting not tensorflow", "neg": ["tensorflow"]},
    {"q": "javascript frontend framework lightweight no virtual dom", "neg": ["virtual dom"]},
    {"q": "go database orm with migration support and connection pooling", "neg": []},
]

print("=" * 70)
print("  NEGATIVE CONSTRAINT ACCURACY TEST (word-boundary matching)")
print("=" * 70)

total_checked = 0
total_violations = 0

for spec in NEGATIVE_QUERIES:
    resp = requests.get(SEARCH_URL, params={"q": spec["q"]}, timeout=30)
    data = resp.json()
    web = data.get("web_results", [])
    neg_constraints = data.get("structured_constraints", {}).get("negative", [])
    
    violations = check_violations(web, spec["neg"])
    checked = min(len(web), 10)
    total_checked += checked
    total_violations += len(violations)
    rate = len(violations) / max(checked, 1) * 100
    marker = "✓" if rate == 0 else "⚠" if rate <= 10 else "✗"
    
    print(f"\n  {marker} [{resp.elapsed.total_seconds()*1000:.0f}ms] web={len(web)} | {spec['q'][:65]}")
    print(f"    extracted neg: {neg_constraints}")
    print(f"    violations: {len(violations)}/{checked} ({rate:.0f}%)")
    for v in violations[:3]:
        print(f"      ⚠ '{v['term']}' in: {v['title']}")

print(f"\n{'='*70}")
print(f"  MULTI-CONSTRAINT QUERIES")
print(f"{'='*70}")

for spec in MULTI_QUERIES:
    resp = requests.get(SEARCH_URL, params={"q": spec["q"]}, timeout=30)
    data = resp.json()
    web = data.get("web_results", [])
    neg_constraints = data.get("structured_constraints", {}).get("negative", [])
    
    violations = check_violations(web, spec["neg"]) if spec["neg"] else []
    checked = min(len(web), 10) if spec["neg"] else 0
    total_checked += checked
    total_violations += len(violations)
    rate = len(violations) / max(checked, 1) * 100 if checked else 0
    marker = "✓" if rate == 0 else "⚠" if rate <= 10 else "✗"
    
    print(f"\n  {marker} [{resp.elapsed.total_seconds()*1000:.0f}ms] web={len(web)} | {spec['q'][:65]}")
    print(f"    extracted neg: {neg_constraints}")
    if spec["neg"]:
        print(f"    violations: {len(violations)}/{checked} ({rate:.0f}%)")
    else:
        print(f"    (no negative constraints expected)")
    for v in violations[:3]:
        print(f"      ⚠ '{v['term']}' in: {v['title']}")

print(f"\n{'='*70}")
overall_rate = total_violations / max(total_checked, 1) * 100
marker = "✓" if overall_rate <= 2 else "⚠" if overall_rate <= 10 else "✗"
print(f"  {marker} OVERALL: violations={total_violations}/{total_checked} ({overall_rate:.1f}%)")
print(f"{'='*70}")
