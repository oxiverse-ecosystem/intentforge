# IntentForge v2 — Stress Test & Quality Audit
**Date:** 2026-05-31
**Target:** http://localhost:4000

---

## SCORECARD

| Metric | Result |
|---|---|
| Intent accuracy (strict) | 14/24 (58%) |
| Intent accuracy (effective*) | 21/24 (88%) |
| Negative constraints | 0 violations / 150 checked (0.0%) |
| Cache speedup | 137x |
| Concurrent throughput | 2.09 req/s (15/15 succeeded) |
| Avg latency (uncached) | ~2.1s |
| Privacy | CLEAN — no tracking, no cookies, no query echo |

*Effective accuracy accounts for the API using label "comparison" where the test expected "comparative" — same semantic meaning, different label string.

---

## INTENT ACCURACY BREAKDOWN

### Label Naming Difference (not real errors)
The API uses `comparison` not `comparative`. 7 of 10 "mismatches" are just this label difference:
- best laptops 2026, react vs vue, best open source LLM, compare rust vs go,
  nvidia rtx 5090 vs amd, best vpn for torrenting, github copilot vs cursor

### Real Mismatches (3)
| Query | Expected | Got | Notes |
|---|---|---|---|
| cheapest flights to london | transactional | informational | "cheapest" should signal buying intent |
| how does CRISPR gene editing work step by step | how-to | informational | "how does...work" reads as explain, not tutorial |
| cheapest way to send money internationally | how-to | transactional | "cheapest way" is transactional — API got this right, test label was wrong |

**Real error rate: 2/24 (8%)** — "cheapest flights" and "CRISPR step by step".

---

## NEGATIVE CONSTRAINTS: 15 queries × top-10 = 150 results checked

All 15 queries passed. Zero violations. The constraint system is solid.

Examples tested: python not java, linux not windows, electric car not tesla, AI model not GPT, browser not chrome, music streaming not spotify...

---

## RESULT QUALITY (spot-check)

All 5 audited queries returned directly relevant top-5 results from multiple engines (duckduckgo, bing, whoogle, startpage, mojeek, yandex, brave). No spam, no dead links, no off-topic results in top-5.

---

## EDGE CASES

| Case | Status | Notes |
|---|---|---|
| Single char ("a") | OK (31 results) | Navigational intent, reasonable |
| C++ programming | OK (20 results) | Special chars handled |
| <html> tag | OK (37 results) | HTML injection harmless |
| 100+ char query | OK (20 results) | Long queries work |
| Japanese (日本語テスト) | OK (5 results) | Limited but works |
| SQL injection attempt | OK (26 results) | Safely handled |
| Whitespace only | 400 Bad Request | Correct rejection |
| Empty query | 400 Bad Request | Correct rejection |

---

## CACHE PERFORMANCE

| Hit | Latency | Results |
|---|---|---|
| 1 (cold) | 1.511s | 34 |
| 2 | 0.008s | 34 |
| 3 | 0.007s | 34 |
| 4 | 0.006s | 34 |
| 5 | 0.024s | 34 |

**Speedup: 137x.** Cache is working perfectly.

---

## CONCURRENT PERFORMANCE

- 15/15 queries succeeded
- Wall time: 7.19s
- Latency p50: 5.04s
- Latency p95: 7.16s
- Throughput: 2.09 req/s

Note: p50/p95 elevated due to upstream SearXNG engine latency under concurrent load — gateway itself handles concurrency fine.

---

## PRIVACY AUDIT

- All analytics endpoints return 404 (expected)
- No cookies set
- No tracking fields in response
- No query echoed back in response
- Clean

---

## RESPONSE STRUCTURE

All expected fields present: results, intent, confidence, constraints, category.
Result objects: authority, content, is_local, score, sources, title, url.
No tracking/analytics fields. No user/session identifiers.

---

## RECOMMENDATIONS

1. **Consider aliasing "comparative" → "comparison"** in the intent label, or document the canonical label set. External consumers may expect "comparative".
2. **"cheapest flights to london" → informational** — queries with price-comparison verbs + travel destinations may need a transactional boost.
3. **"how does X work step by step"** — the "step by step" suffix should push toward how-to, currently classifies as informational.
4. **Concurrent throughput (2 req/s)** is acceptable for current scale but may need tuning if traffic grows — Granian workers or SearXNG parallelism could be increased.
