# IntentForge Audit Report — Round auto/round-2026-09-18T1242Z

**Auditor:** independent (t_488b4353)
**Branch:** auto/round-2026-09-18T1242Z-audit (from auto/round-2026-09-18T1242Z)
**Live API:** http://localhost:4000 (health: OK)
**Date:** 2026-09-20

---

## (A) ENDPOINT COVERAGE — LIVE VERIFICATION

| Endpoint | Status | Result |
|----------|--------|--------|
| GET / | 200 | PASS — "IntentForge-v2 Gateway" |
| GET /health | 200 | PASS — "OK" |
| GET /search?q=complex+multi-constraint | 200 | PASS — all documented keys present |
| GET /search/fast | 200 | PASS — {count, source, results} |
| GET /images | 200 | PASS — {count, query, results}, result keys: title, url, image_url, thumbnail_url, description, source, score |
| GET /videos | 200 | PASS — {count, query, results}, result keys: title, url, description, thumbnail, video_id, source, score |
| GET /news | 200 | PASS — {count, query, results}, result keys: title, url, description, published_at, source, score |
| GET /spellcheck?q=pythn+programing+langauge | 200 | PASS — {query, corrected, changed, corrections} |
| GET /analyze?q=javascript+not+java+not+typescript | 200 | PASS — {query, contrastive_framing, exclusions, declined, manner_qualifiers, decisions} |
| GET /inspect?q=python+web+framework+not+django | 200 | PASS — {query, spelling, negation, intent, constraints, recency, quality} |
| GET /geolocate?q=quiet+places+to+study+near+chennai | 200 | PASS — {query, resolved, source, explicit_location, local_intent} |
| GET /intent?q=violin+vs+viola+for+beginner | 200 | PASS — {query, intent, category, confidence, contrastive_framing, local_intent, structured_constraints, expanded_queries} |
| **GET /video?q=rust+vs+go** | **404** | **FAIL — route missing (REGRESSION)** |
| POST /goals {goal} | 200 | PASS — goal_id present, questions[] non-empty (4 questions) |
| POST /goals/:id/answers {answers} | 200 | PASS — total_phases=4 == len(phases)=4 |
| GET /goals/:id | 200 | PASS — status=active |
| GET /goals/leaderboard | 200 | PASS — returns JSON ARRAY (len=1) |
| POST /goals/quick {goal} | 200 | PASS — total_phases=4 == len(phases)=4 |
| GET /shopping?q=best+wireless+earbuds+under+50 | 200 | PASS — commerce, affiliate, commerce_provenance present |
| GET /search?q=buy+wireless+earbuds | 200 | PASS — shopping block present for commercial query |
| GET /search?q=laptop (non-commercial) | 200 | PASS — no shopping field |

Error envelopes (400): all endpoints return correct JSON envelope shape.

---

## (B) HARDCODING SWEEP

### DEFECT 1 (P0 REGRESSION): /video endpoint deleted

**Evidence:** `GET /video?q=rust+vs+go+high+concurrency+servers` → HTTP 404 (empty body).

On master: `.route("/video", get(handle_video))` at line 12751, `fn handle_video` at line 13379, `fn build_video` at line 13399, `video_endpoint_tests` module with 6 tests.

On round branch: route gone, `handle_video` gone, `build_video` gone, `video_endpoint_tests` gone. Only `handle_videos` (plural) remains.

The `/video` endpoint is documented in API_REFERENCE.md and was working on master. Its removal is a regression with no migration or deprecation.

**Fix card:** SPAWNED — t_488b4353-fix-video-endpoint

### DEFECT 2 (HARDCODED): tx_keywords hardcoded in merge_local_and_web

**Evidence:** `services/gateway/src/main.rs:9999`:
```rust
let tx_keywords = ["buy", "price", "pricing", "cheap", "purchase", "shop", "store", "discount", "coupon"];
let has_tx = tx_keywords.iter().any(|k| q_lower_check.contains(k));
```

The `CommerceConfig` struct has `transactional_keywords: Vec<String>` loaded from `data/commerce/config.json` / `data/commerce/signals.json`. But this hardcoded local shadows it — the config field is never read in the merge path. Editing the JSON files has zero effect on merge behavior.

Additionally, `data/commerce/config.json` had `transactional_keywords` removed, and `data/commerce/signals.json` was deleted entirely in this round. The config-loading code still reads these paths but the field is always empty, so the hardcoded local is the only thing that works — making the config mechanism dead code.

**Fix card:** SPAWNED — t_488b4353-fix-tx-keywords-hardcoded

### DEFECT 3 (HARDCODED): Second tx_keywords hardcoded at line 14173

**Evidence:** `services/gateway/src/main.rs:14173`:
```rust
let tx_keywords = ["buy ", "price ", "pricing", "cheap ", "purchase ", "shop ", "store ", "discount ", "coupon ", "under "];
let has_tx_signal = tx_keywords.iter().any(|k| q_lower.starts_with(k) || q_lower.contains(k));
```

This is the main-path shopping detection. It correctly reads `state.commerce_config.transactional_keywords` at line 14211, but line 14173 is a separate hardcoded set used in a different code path. The two sets diverge (one has "under ", the other doesn't).

**Fix card:** SPAWNED — t_488b4353-fix-tx-keywords-hardcoded (same card — both are the same root cause: hardcoded keywords instead of runtime-loaded config)

### Non-issue: Academic domain list

`is_academic` check at line 9973 hardcodes `arxiv.org`, `crossref.org`, `ncbi.nlm.nih.gov`. This is a general structural signal (academic repository TLDs), not a per-query literal. Acceptable.

### Non-issue: Test fixture URLs

Test code uses `https://www.amazon.com/dp/B0EXAMPLE` and `https://ebay.com/itm/123?foo=bar` as fixture URLs in unit tests. These are test fixtures, not production routing. Acceptable.

---

## (C) REGRESSION TEST ASSESSMENT

### Existing tests

| File | Tests | Status |
|------|-------|--------|
| tests/test_goals_api_schema.py | 18+ | PASS — comprehensive, uses stdlib urllib, gateway guard |
| tests/test_api_schema.py | 10+ | PASS — covers all endpoints |
| tests/test_recall_gap_endpoint.py | 3 | PASS |
| tests/goals_api_schema.py | 18+ | PASS — duplicate of test_goals_api_schema.py (consolidated) |

### Missing tests

| Endpoint | Test | Status |
|----------|------|--------|
| GET /video | video_endpoint_tests (6 tests) | **DELETED — needs restoration** |
| GET /commerce/bid | None | No test |
| GET /commerce/extract | None | No test |

The `/video` endpoint had 6 unit tests on master (`video_endpoint_tests`). They were deleted along with the endpoint. The fix card for DEFECT 1 must include restoring these tests.

---

## (D) COMMERCE-TARGET AUDIT

### RANKING INTEGRITY: PASS

**Order invariance verified live:** `/search?q=wireless+earbuds+under+50` vs `/shopping?q=wireless+earbuds+under+50` → 21 results each, URL order byte-identical.

CI test exists: `enrichment_preserves_result_order_with_or_without_affiliate_keys` and `decoration_preserves_ranking_order` in gateway suite.

### NO MISREPRESENTATION: PASS

`commerce_provenance` present on every result with `{url, observed_at, source, data:null}`. `commerce` only present when page exposed structured product data. `affiliate.url` destination matches the result URL.

### NO HARDCODING: FAIL → DEFECT 2 & 3

See above. `tx_keywords` hardcoded in two places instead of using runtime-loaded `CommerceConfig.transactional_keywords`.

### SECRETS: PASS

No `.env`, `*.key`, `*.pem` files created or modified. Affiliate keys come from env vars (`SOVRN_COMMERCE_KEY`, `AMAZON_ASSOCIATES_TAG`, etc.).

### PRIVACY: PASS

Affiliate URL params: `key`, `u` (destination), `cuid` (coarse merchant host), `bf` (bid floor), `fbu` (fallback). No user id, query text, session id, or IP in any parameter.

### DISCLOSURE: PASS

All affiliate-decorated results carry `affiliate.disclosed == true`.

### GRACEFUL DEGRADATION: PASS

With `SOVRN_COMMERCE_KEY=dummy-test-key-do-not-use` (dev), search returns 200 with affiliate block present. If key were unset, the affiliate block would be omitted (code path exists). Non-commercial queries return no `shopping` field.

---

## INFRA-REACHABILITY AUDIT

| Service | getent hosts | Status |
|---------|-------------|--------|
| tor2 | 172.18.0.4 | PASS |
| searxng | 127.0.0.1 | PASS |
| intent-engine | 127.0.0.1 | PASS |
| indexer | 127.0.0.1 | PASS |

`resolv-gateway.conf` correctly lists `nameserver 127.0.0.11` FIRST. Live search shows ZERO 'Circuit OPEN' warnings.

---

## SUMMARY

| Category | Finding |
|----------|---------|
| Endpoints | 19/20 pass — `/video` 404 (REGRESSION) |
| Hardcoding | 2 defects — `tx_keywords` hardcoded in 2 places |
| Commerce | Ranking integrity PASS, privacy PASS, disclosure PASS |
| Infra | All services reachable |
| Regression tests | `/video` tests deleted with endpoint |

**Verdict:** 3 defects found, 3 fix cards spawned. Do NOT merge until fix cards land and CI is green.
