#!/usr/bin/env python3
"""Validate all 4 bottleneck fixes against the live API."""
import requests, json, time, sys

BASE = 'http://localhost:4000/search'

def test(q, label=''):
    t0 = time.perf_counter()
    r = requests.get(BASE, params={'q': q}, timeout=60)
    ms = (time.perf_counter() - t0) * 1000
    d = r.json()
    sc = d.get('structured_constraints', {})
    res = d.get('results', [])
    return {'ms': ms, 'sc': sc, 'res': res, 'count': len(res)}

all_passed = True

def check(condition, msg):
    global all_passed
    if condition:
        print(f'  ✅ PASS: {msg}')
    else:
        print(f'  ❌ FAIL: {msg}')
        all_passed = False

print('=' * 70)
print('COMPREHENSIVE FIX VALIDATION')
print('=' * 70)

# ── P0 TEST: Post-merge hard negative filter ──
print('\n--- P0: Post-merge negative filter (local + web) ---')
t = test('python async web framework not django not flask')
neg = [n.lower() for n in t['sc'].get('negative', [])]
print(f'  Negatives: {neg}')
print(f'  Results: {t["count"]} ({t["ms"]:.0f}ms)')
violations = []
for i, r in enumerate(t['res'][:15]):
    text = (r.get('title', '') + ' ' + r.get('content', '')[:300]).lower()
    for n in neg:
        if n in text:
            violations.append((i+1, r.get('title', '')[:50], n, r.get('is_local', False)))
            break
check(len(violations) == 0, f'No negative violations in top 15 (violations: {len(violations)})')
for pos, title, n, isl in violations[:3]:
    print(f'    #{pos} (local={isl}): "{title}" contains "{n}"')

# ── P0b: Dual negatives ──
print('\n--- P0b: Dual negative constraints ---')
t = test('distributed cache without redis without memcached')
neg = [n.lower() for n in t['sc'].get('negative', [])]
print(f'  Negatives: {neg}')
print(f'  Results: {t["count"]} ({t["ms"]:.0f}ms)')
violations = []
for i, r in enumerate(t['res'][:10]):
    text = (r.get('title', '') + ' ' + r.get('content', '')[:200]).lower()
    for n in neg:
        if n in text:
            violations.append((i+1, r.get('title', '')[:50], n))
            break
check(len(violations) == 0, f'No violations in top 10 ({len(violations)} found)')
for pos, title, n in violations[:3]:
    print(f'    #{pos}: "{title}" contains "{n}"')

# ── P4 TEST: Swift disambiguation ──
print('\n--- P4: Swift disambiguation (OpenStack Swift) ---')
t = test('swift object storage openstack')
lang = t['sc'].get('language')
print(f'  Query: swift object storage openstack')
print(f'  Language: {lang}')
check(lang is None, 'Swift in storage context should NOT trigger language detection')

# P4b: Swift as programming language should still work
print('\n--- P4b: Swift programming language (should still detect) ---')
# Use a query that's clearly about the Swift programming language
t = test('swift async http server framework')
lang = t['sc'].get('language')
print(f'  Query: swift async http server framework')
print(f'  Language: {lang}')
check(lang == 'swift', 'Swift in dev context SHOULD trigger language detection')

# ── P3 TEST: Basic functionality (early-exit for empty content) ──
print('\n--- P3: Semantic relevance score (basic functionality) ---')
t = test('rust tokio async runtime')
print(f'  Results: {t["count"]} ({t["ms"]:.0f}ms)')
check(t['count'] > 5, f'Got {t["count"]} results (expected > 5)')
if t['res']:
    top = t['res'][0]
    print(f'  Top: {top.get("title","")[:70]}')
    check('rust' in top.get('title','').lower() or 'tokio' in top.get('title','').lower(),
          'Top result is relevant to query')

# ── Latency check ──
print('\n--- Performance: Latency comparison ---')
queries = [
    ('python async web framework', 'simple'),
    ('machine learning framework for edge devices not tensorflow not pytorch', 'complex+neg'),
    ('swift object storage openstack', 'storage'),
    ('distributed cache without redis without memcached', 'dual neg'),
]
for q, label in queries:
    t = test(q, label)
    status = 'OK' if t['ms'] < 5000 else 'SLOW'
    print(f'  [{status}] {label:20s}: {t["count"]:2d} results in {t["ms"]:.0f}ms')

print(f'\n{"=" * 70}')
if all_passed:
    print('✅ ALL TESTS PASSED')
else:
    print('❌ SOME TESTS FAILED')
print(f'{"=" * 70}')
sys.exit(0 if all_passed else 1)
