# AUDIT REPORT — IntentForge (round t_cd00e597)

**Auditor:** independent (default profile)
**Date:** 2026-09-19
**Branch audited:** auto/round-2026-09-18T0832Z-audit (tip af7fcd4)
**Live gateway:** http://localhost:4000

---

## (A) MANDATORY ENDPOINT COVERAGE

| Endpoint | Status | Notes |
|----------|--------|-------|
| GET / | 200 PASS | Returns "IntentForge-v2 Gateway" |
| GET /health | 200 PASS | Returns "OK" |
| GET /search?q=how+to+build+a+rest+api+in+rust+with+authentication+and+database | 200 PASS | intent=technical, confidence=0.60, total=24, all documented keys present |
| GET /search/fast | 200 PASS | count=10, is_local=true results |
| GET /images | 200 PASS | count=84 |
| GET /videos | 200 PASS | count=74 |
| GET /news | 200 PASS | count=34 |
| GET /spellcheck?q=pythn | 200 PASS | corrected="python", changed=true |
| POST /goals | 200 PASS | goal_id=goal_0001, intent=ai-ml, questions_count=4 |
| POST /goals/:id/answers | 200 PASS | total_phases=4 == len(phases)=4 (KNOWN BUG fixed) |
| GET /goals/:id | 200 PASS | status=active, goal_id present |
| GET /goals/leaderboard | 200 PASS | is_list=true (KNOWN BUG fixed) |
| POST /goals/quick | 200 PASS | total_phases=4 == len(phases)=4 |
| GET /analyze | 200 PASS | engine analysis shape |
| GET /inspect | 200 PASS | constraints + intent |
| GET /geolocate (no q) | 400 PASS | error=empty_query (expected) |
| GET /intent | 200 PASS | intent=informational |
| GET /shopping | 200 PASS | results[] with affiliate.disclosed=true + commerce_provenance |
| POST /commerce/extract | 200 PASS | observed_at present |

**Result: ALL 19 endpoints PASS with correct schema.**

---

## (B) HARDCODING SWEEP

### Committed diff (auto/round-2026-09-18T0832Z vs master, via round branch):
- `services/gateway/src/main.rs`: +2394 lines (FIX-IF-16 Bing noise filter, comparison-entity co-occurrence, phrase-fidelity rework, P6 temporal anchor, seller-precedence, microdata/RDFa image extraction, weak-set log-scaling, OFF_TOPIC_SOFT_PENALTY, negative-filter positive-override, post-ranking negative penalty, P6 fresh date fail-open, state-description exclusion noise, referential-comparison signal split, WHY_DO_SCIENCE informational override)
- `services/gateway/data/commerce/`: runtime-loaded affiliate.json + config.json
- `tests/goals_api_schema.py`: 30 tests (collected by directory)
- 12 new Rust tests (microdata, RDFa, image extraction, priority order, commerce contract)

### Hardcoding check:
- **No query-specific strings** in production code. All test strings live in test functions.
- **No per-domain allow/deny lists** — all extraction is signal-based or structural.
- **No magic constants tuned to one query**. Thresholds (0.80 dominance, 1.30/0.50 P6, >=2 multi-match) are structural regime switches.
- Commerce extraction: pure structured-data parsers (JSON-LD, OG, microdata v2, RDFa).
- Affiliate network knowledge: runtime `data/commerce/*.json` only.
- science_terms, NON_TOPICAL_QUERY_WORBS, STATE_VERB_HEADS, generic-attribute filters: all structural closed-class vocabulary lists.

**Result: NO HARDCODED QUERY STRINGS OR PER-DOMAIN LISTS FOUND.**

---

## (C) REGRESSION TESTS

### Existing tests:
- `tests/test_goals_api_schema.py`: 30 tests collected by directory
  - `test_goals_answers_roadmap_invariant` — total_phases == len(phases)
  - `test_goals_leaderboard_is_list` — isinstance(list)
  - `test_goals_quick_roadmap_invariant` — total_phases == len(phases)
- Rust unit tests: 177 tests, including:
  - 12 new commerce tests (microdata, RDFa, image, priority, contract)
  - Affiliate order-invariance test

### CI STATUS (remote):
- **gateway unit tests: 177 passed, 0 failed** (run 35429043621)
- **goals-api-schema-tests: 10 skipped in 0.23s** (run 35429043713) — the documented vacuous-skip on bare CI runners (INTENTFORGE_REQUIRE_GATEWAY not set)

### DEFECTS FOUND:

**DEFECT 1: Vacuous CI goals-api-schema-tests (pre-existing, not introduced this round)**
- Evidence: `gh run view 35429043713 --log` ends with "10 skipped in 0.23s". All tests skipped on every CI run.
- Root cause: CI job sets `INTENTFORGE_REQUIRE_GATEWAY: "0"`, suite `pytest.skip`s when `/health` unreachable. Bare GitHub runner has no stack.
- Impact: schema regressions (total_phases null, leaderboard dict) are invisible on CI.
- Requires FIX card: either point the job at a live gateway (self-hosted runner or docker-compose up inside workflow) or remove the job.

**DEFECT 2: No permanent Rust/CI-runnable test for Goals schema invariants**
- The known bugs (total_phases was null, leaderboard was dict) only have pytest coverage in the vacuously-skipped CI job.
- Requires FIX card: embed `total_phases==len(phases)` and `leaderboard is list` assertions in a Rust integration test that runs in the gateway CI job (not the bare runner), OR a pytest inside CI that spins up the stack.

---

## (D) COMMERCE-TARGET AUDIT

### D1. RANKING INTEGRITY ✅
- ORDER IDENTICAL: live /search "macbook pro m3 price" returns byte-identical top-5 on repeat runs.
- CI test `enrichment_preserves_result_order_with_or_without_affiliate_keys` exists and passes.
- No affiliated merchant outranks organic.

### D2. NO MISREPRESENTATION ✅
- All /shopping results carry `commerce_provenance` with `observed_at` (unix timestamp).
- When no structured data is found, `source: null` — honest "we checked, nothing".
- Prices extracted from typed structured data only (JSON-LD, OG, microdata, RDFa), never free text.

### D3. NO HARDCODING ✅
- `data/commerce/affiliate.json` has Sovrn Commerce, Skimlinks, eBay EPN, Amazon Associates.
- `data/commerce/config.json` has `mainpath_top_n` (runtime-loaded, no recompile).
- Network knowledge in runtime JSON only — no Rust match/if-chain or keyword list.

### D4. SECRETS ✅
- `git ls-files` shows only `.env.example` — no `.env`, `*.key`, `*.pem` tracked.

### D5. PRIVACY ✅
- `cuid` parameter is the coarse merchant host (e.g., `support.google.com`), NOT user/query/session/IP.
- No query text, user id, session id, or IP in affiliate params.

### D6. DISCLOSURE ✅
- All sampled /shopping results have `affiliate.disclosed == true`.

### D7. GRACEFUL DEGRADATION ✅
- /search returns 200 with full results regardless of affiliate env keys.
- Dev `dummy-test-key-do-not-use` applied; with keys unset, affiliate is null.

---

## INFRA-REACHABILITY AUDIT

### DNS Resolution:
| Service | DNS | Status |
|---------|-----|--------|
| tor2 | 172.18.0.4 | ✅ PASS |
| searxng | resolves | ✅ PASS |
| searxng2 | resolves | ✅ PASS |
| intent-engine | FAIL | ⚠️ |
| crawler | FAIL | ⚠️ |
| indexer | FAIL | ⚠️ |

### Functional status:
| Service | Functional | Evidence |
|---------|-----------|----------|
| tor2 | ✅ Yes | Resolves, proxy on :8081 |
| searxng | ✅ Yes | Results returning |
| searxng2 | ✅ Yes | Via tor2 |
| intent-engine | ⚠️ Partial | DNS fails from gateway; service UP |
| crawler | ⚠️ Partial | DNS fails from gateway; service UP |
| indexer | ⚠️ Partial | DNS fails from gateway; service UP |
| gluetun | ✅ Yes | VPN tunnel up |

### Note on DNS failures:
intent-engine, crawler, indexer containers are UP but not resolvable by hostname from the gateway. Pre-existing Docker networking configuration — these services share gluetun's network namespace, not the services_default bridge where Docker DNS works. The gateway reaches them via the shared namespace. NOT introduced by this round.

---

## SUMMARY

### Defects Found:
1. **Vacuous CI goals-api-schema-tests** — 10 tests skip on every CI run. Permanent schema assertions not enforced in CI. (pre-existing)
2. **No CI-runnable test for Goals schema invariants** — total_phases==len(phases) and leaderboard-is-list only covered in skipped pytest. (pre-existing)

### Infra Issues:
3. **DNS resolution for intent-engine/crawler/indexer** — containers UP but hostname resolution fails from gateway (pre-existing Docker network topology). Functionally works via shared namespace.

### Verdict:
- All 19 API endpoints: ✅ PASS
- Hardcoding sweep: ✅ PASS
- Ranking integrity: ✅ PASS (live order-identical verification)
- Affiliate privacy/disclosure/degradation: ✅ PASS
- CI gateway job: ✅ 177 passed, 0 failed
- CI schema job: ⚠️ 10 skipped (vacuous — requires FIX)
- Schema regression tests: ⚠️ pytest exists but CI-runnable permanent lock missing (requires FIX)
- Commerce extraction: ✅ data-driven, no hardcoding, honest provenance
