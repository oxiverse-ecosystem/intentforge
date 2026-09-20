# AUDIT REPORT — IntentForge (round auto/round-2026-09-16T0750Z-audit)

**Auditor:** independent (default profile)
**Date:** 2026-09-19
**Branch:** auto/round-2026-09-16T0750Z-audit
**Live gateway:** http://localhost:4000

---

## (A) MANDATORY ENDPOINT COVERAGE

| Endpoint | Status | Notes |
|----------|--------|-------|
| GET / | 200 PASS | Returns "IntentForge-v2 Gateway" |
| GET /health | 200 PASS | Returns "OK" |
| GET /search?q=complex | PASS (with defects) | 21 results for complex query; price_lt missing for "under $N"; intent misclassified for model-number queries |
| GET /search/fast | PASS | 10 results for "rust programming tutorial" |
| GET /images | PASS | 147 results |
| GET /videos | PASS | 127 results |
| GET /news | PASS | 38 results |
| GET /spellcheck | PASS | pythn→python, biryani preserved, embaras→embarrass, ngnix→nginx |
| POST /goals | PASS | goal_id + 4 questions returned |
| POST /goals/:id/answers | PASS | Full 4-phase roadmap, total_phases=4 == len(phases)=4 |
| GET /goals/:id | PASS | status "pending_answers" present |
| GET /goals/leaderboard | PASS | Returns a LIST (1 item) |
| POST /goals/quick | PASS | total_phases=4 == len(phases)=4 |

### Spellcheck verification (field name: `corrected`, not `corrected_query`)
- pythn → python (in_dictionary=true)
- biryani → biryani (unchanged, absent-word guard working)
- embaras → embarrass (known-misspelling exception working)
- ngnix → nginx (known-misspelling working)

---

## (B) HARDCODING SWEEP

- **No query-specific strings** in production code
- **No per-domain allow/deny lists**
- **No keyword→reply tables**
- **No capability lists**
- **No one-test-tuned thresholds**
- Brand names (sony, apple, samsung, bose, logitech, nike) appear only in dictionary.rs frequency tables — legitimate linguistic data, not routing logic

**Result: CLEAN**

---

## (C) REGRESSION TESTS

30 tests collected by directory. **2 failed, 28 passed**:
1. `test_api_schema.py::test_other_brand_negatives_applied_only` — bose missing from applied_constraints (DEFECT D3)
2. `test_goals_api_schema.py::test_negation_full_suite_with_price` — price_lt not parsed from "under $N" (DEFECT D2)

---

## (D) COMMERCE-TARGET AUDIT

| Invariant | Status | Evidence |
|---|---|---|
| Ranking integrity | PASS | order-invariance test exists (commerce_contract_tests.rs:281); decoration strictly post-ranking |
| No misrepresentation | PARTIAL | commerce_provenance.observed_at present; most results have null price (no fabricated facts) |
| No hardcoding | PASS | affiliate.json/config.json runtime-loaded; no Rust match/if-chain |
| Secrets from env only | PASS | git status shows no .env/*.key/*.pem modified |
| Privacy (no PII in affiliate params) | PASS | subid = merchant host only (main.rs:3918-3919) |
| Disclosure | PASS | Every decorated result has affiliate.disclosed=true |
| Graceful degradation | PASS | With keys unset, search returns 200 with affiliate=null |

### /search vs /shopping affiliate discrepancy
- /shopping endpoint decorates 9/9 results with affiliate links
- /search endpoint decorates 0/19 results
- ROOT CAUSE: /search does not call affiliate decoration code at all; only /shopping does
- IMPACT: Commercial queries on main search page never show affiliate links → breaks monetization
- FIX CARD: t_3fa833cf

---

## INFRA-REACHABILITY AUDIT

### DNS Resolution:
| Service | DNS | Status |
|---------|-----|--------|
| tor2 | 172.18.0.4 | PASS |
| searxng | NOT FOUND | FAIL |
| intent-engine | NOT FOUND | FAIL |
| crawler | NOT FOUND | FAIL |
| indexer | NOT FOUND | FAIL |

ROOT CAUSE: Gateway shares gluetun's network namespace. Docker embedded DNS (127.0.0.11) only resolves containers on the SAME bridge. resolv-gateway.conf correctly lists 127.0.0.11 first but it can't cross bridge boundaries.

FIX CARD: t_1d869149

### Tor Circuit:
- tor2:8081 reachable, returns valid SearXNG HTML
- Gateway logs: "tor2 circuit rotated successfully", "tor2 cache warmed after NEWNYM"
- ZERO "Circuit OPEN (connection failure)" in gateway logs
- PASS

---

## CI GATE

| Job | Conclusion | Non-vacuous? |
|---|---|---|
| gateway unit tests (ci.yml) | success | YES — 174 passed, 0 failed |
| Non-Goals schema tests (goals-api-schema.yml) | success | NO — 10 skipped, 0 ran |
| Goals schema tests (goals-api-schema.yml) | success | NO — 20 skipped, 0 ran |

Schema test green-washing confirmed. Fix card t_8f55b79b.

---

## DEFECT INVENTORY

| ID | Title | Card | Status |
|---|---|---|---|
| D1 | DNS resolution for searxng/intent-engine/crawler/indexer | t_1d869149 | todo |
| D2 | price_lt extraction from "under $N" natural language | t_3f2b35f2 | todo (dup: t_10f9c8c7) |
| D3 | Negated brands missing from applied_constraints (bose) | t_1096847a | todo |
| D4 | CI schema tests green-washing (30 skip on CI) | t_8f55b79b | todo |
| D2a | Intent classifier misclassifies exact-model queries | t_8112c44c | todo (dup: t_58cd0b08) |
| D2b | /search endpoint missing affiliate decoration | t_3fa833cf | todo |
| MERGE | Land verified round on master | t_bf18acfd | todo |

## Duplicate Cards Note

Two fix cards were spawned twice each due to parallel tool calls:
- t_8112c44c + t_58cd0b08 (intent classifier)
- t_3f2b35f2 + t_10f9c8c7 (price_lt)

The dispatcher should work one and delete the other.

## Residual Risk

When the intent-classification fix ships (making model-number queries "transactional"), the /search affiliate-decoration fix must also land in the same round — otherwise commercial queries will correctly classify as transactional but still show no affiliate links on /search while /shopping works. The two fixes are coupled.
