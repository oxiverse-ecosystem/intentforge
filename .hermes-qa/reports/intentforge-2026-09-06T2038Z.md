# IntentForge NL Search Round — 2026-09-06T2038Z

**Branch:** auto/round-2026-09-06T0208Z  
**Queries:** 22  
**Time:** 2026-09-06T20:38:00+00:00Z

## Summary

| # | Query | Intent | Conf | Total | Elapsed | Verdict |
|---|-------|--------|------|-------|---------|---------|
| 1 | how to train for a marathon from zero in 6 months | how-to | 0.6 | 10 | 10.8s | FAIL |
| 2 | best credit card for travel rewards india 2026 | informational | 0.29 | 0 | 3.1s | FAIL |
| 3 | alternative to github that doesn't charge for private repos | comparison | 0.85 | 11 | 6.8s | PARTIAL |
| 4 | linux laptop without nvidia gpu for rust development | technical | 0.6 | 3 | 3.1s | PARTIAL |
| 5 | how to set up a home gym in a small room on a budget | how-to | 0.6 | 20 | 3.9s | PASS |
| 6 | best books for learning machine learning with python | technical | 0.6 | 11 | 3.5s | PARTIAL |
| 7 | why do cats knead blankets before sleeping | informational | 0.338 | 19 | 4.0s | PASS |
| 8 | budget smartphone with good camera under 15000 for students | transactional | 0.8 | 5 | 12.3s | PARTIAL |
| 9 | how to start freelancing as a web developer while in college | how-to | 0.6 | 20 | 5.1s | PARTIAL |
| 10 | latest space missions planned for 2026 | fresh | 0.7 | 3 | 3.9s | PARTIAL |
| 11 | online course platform without subscription for learning rust | technical | 0.6 | 13 | 3.6s | PARTIAL |
| 12 | best noise cancelling earbuds for calls under 8000 rupees | transactional | 0.85 | 10 | 18.9s | PARTIAL |
| 13 | how to reduce hair fall naturally at home without chemicals | how-to | 0.6 | 1 | 4.4s | FAIL |
| 14 | electric two wheeler under 50000 for college commute | transactional | 0.8 | 9 | 14.2s | PARTIAL |
| 15 | is it worth upgrading from windows 10 to windows 11 in 2026 | technical | 0.6 | 19 | 4.4s | PASS |
| 16 | how to start investing in mutual funds as a beginner in india | how-to | 0.6 | 23 | 6.8s | PASS |
| 17 | best monitor for programming dual setup under 25000 | transactional | 0.85 | 3 | 8.4s | PARTIAL |
| 18 | how to cook perfect basmati rice for biryani in a pressure cooker | how-to | 0.6 | 12 | 4.1s | PASS |
| 19 | best free alternative to photoshop for photo editing | comparison | 0.85 | 13 | 4.7s | PARTIAL |
| 20 | why is the sky blue explanation for kids | informational | 0.364 | 9 | 4.1s | PASS |
| 21 | how to backup photos from iphone to external hard drive without cloud | how-to | 0.6 | 7 | 3.7s | PASS |
| 22 | best lightweight laptop for college students under 40000 | transactional | 0.85 | 7 | 11.1s | PARTIAL |

## Defects

### q01: how to train for a marathon from zero in 6 months
- **Verdict:** FAIL
- **Reason:** Top-5 results are about trains/railways (Wikipedia "Train", IRCTC, etc.).
- **Root Cause:** Expansion strips the query down to `train marathon zero 6 months` — the dominant token becomes "train" and the query is treated as rail travel.
- **Evidence:** `/search` response top result: "Train - Wikipedia"; gateway log shows expansion debug: `expanded=["train marathon zero 6 months", ...]`.
- **Classification:** `LIMITATION:needs-research` — fix needs intent-engine query expansion logic to preserve multi-token concepts like "train for a marathon" rather than collapsing to the head noun.

### q02: best credit card for travel rewards india 2026
- **Verdict:** FAIL
- **Reason:** Zero results returned.
- **Root Cause:** Unknown. Single SearXNG circuit appears to have timed out during this query; immediate retry was not run.
- **Evidence:** `/search` returned `total=0, before=0, after=0` with no warnings field in 3.1s.
- **Classification:** `LIMITATION:infra` — looks like a transient upstream failure on the SearXNG path. This needs a retry/re-fire to determine whether it is reproducible before code change.

### q04: linux laptop without nvidia gpu for rust development
- **Verdict:** PARTIAL
- **Reason:** Top result is ESP32 development page; video results in top-5 for text query.
- **Root Cause:** Negative `nvidia` is leaking into ranking somehow, or the BM25/local index has a strong "rust" match on an embedded-devices page. P8: invidious videos appear in text query results.
- **Evidence:** top-5 includes `rustfaq.org/en/how-to-use-rust-for-esp32-development/` and two invidious videos; `/analyze` shows `negative=['nvidia']` but the constraint leak path is unclear from `/analyze` alone.
- **Classification:** `LIMITATION:needs-research`

### q06: best books for learning machine learning with python
- **Verdict:** PARTIAL
- **Reason:** Invidious videos appear in top-5 for a text query.
- **Root Cause:** P8 — video sources not dampened for non-video intent.
- **Evidence:** `sources=['invidious', 'local']`, top-5 includes `youtube.com/watch?v=...`.
- **Classification:** `LIMITATION:needs-research`

### q08: budget smartphone with good camera under 15000 for students
- **Verdict:** PARTIAL
- **Reason:** Top result is a tablet listicle; recall gap on "smartphone".
- **Root Cause:** Price/textual match is weak; "smartphone" term missing from top result title.
- **Evidence:** `total=5, recall_gap=['students']`, top result is tablets-under-15000.
- **Classification:** `LIMITATION:needs-research`

### q09: how to start freelancing as a web developer while in college
- **Verdict:** PARTIAL
- **Reason:** Top-5 includes non-relevant Udacity student story and Start.me bookmark manager.
- **Root Cause:** Expansion likely weakened the query; relevance is not filtering out off-topic generic pages.
- **Evidence:** top-5 includes `udacity.com/blog/...` and `start.me`; recall_gap includes `while`, `college`.
- **Classification:** `LIMITATION:needs-research`

### q11: online course platform without subscription for learning rust
- **Verdict:** PARTIAL
- **Reason:** Top result is Coursera for Business; recall gap on "subscription".
- **Root Cause:** "without subscription" negative is not strong enough to demote subscription platforms, or the result title/content does not mention "subscription".
- **Evidence:** top-1 is Coursera Business; `recall_gap=['subscription']`.
- **Classification:** `LIMITATION:needs-research`

### q13: how to reduce hair fall naturally at home without chemicals
- **Verdict:** FAIL
- **Reason:** Single garbage result (Hotmail sign-in page); all other results removed.
- **Root Cause:** Either the stripped expansion query (`how to reduce hair fall naturally at home`) returned 0 usable web results, or the negative filter is being applied to the full query and aggressively dropping pages.
- **Evidence:** `/search` returned `total=1` with a Microsoft Support Hotmail page; gateway log shows expansion debug: `expanded=["how to reduce hair fall naturally at home", "how to reduce hair fall naturally at home alternatives", ...]`.
- **Classification:** `LIMITATION:needs-research`

### q14: electric two wheeler under 50000 for college commute
- **Verdict:** PARTIAL
- **Reason:** Top results are electricians/power companies; query is treated as generic "electric power".
- **Root Cause:** Expansion drops "two wheeler / commute" concepts; query is treated as electrical power.
- **Evidence:** top-5: "Electric power - Wikipedia", "Local Electricians in Reston, VA"; `/analyze` constraints show `positive=['electric', 'two', 'wheeler', 'under', 'college', 'commute']` but ranking ignores multi-token concepts.
- **Classification:** `LIMITATION:needs-research`

### q17: best monitor for programming dual setup under 25000
- **Verdict:** PARTIAL
- **Reason:** Only 3 results; top result is about dual monitor worthiness, not product recommendations under budget.
- **Root Cause:** Sparse web recall for price-bounded monitor query; local index weak on this topic.
- **Evidence:** `total=3`, top-1 is "Is a Dual Monitor Setup Worth It?"; `sources=['brave','local','official_vendor']`.
- **Classification:** `LIMITATION:needs-research`

### q19: best free alternative to photoshop for photo editing
- **Verdict:** PARTIAL
- **Reason:** Invidious video appears in top-5 for text query.
- **Root Cause:** P8 — video source in /search text results.
- **Evidence:** top-5 includes `youtube.com/watch?v=Yfm-BHvBBFo` (src=invidious).
- **Classification:** `LIMITATION:needs-research`

### q22: best lightweight laptop for college students under 40000
- **Verdict:** PARTIAL
- **Reason:** Recall gap on budget; top result is generic ultrabook review.
- **Root Cause:** Price-aware ranking is weak for this budget-bounded query; local index does not surface India-specific lists.
- **Evidence:** `total=7`, recall_gap terms present; top result is generic ultrabook review.
- **Classification:** `LIMITATION:needs-research`

## Root Cause Summary

| Query | Root Cause |
|-------|-----------|
| q01 | Expansion strips key concept tokens (`train for a marathon` → `train`) |
| q02 | Unknown — likely transient upstream SearXNG timeout (needs retry) |
| q04 | P8 + possible negative-constraint leakage |
| q06 | P8 — invidious videos in /search text results |
| q08 | Weak price/textual recall on "smartphone" |
| q09 | Expansion/relevance weakness |
| q11 | Negative `without subscription` not enforced in ranking |
| q13 | Expansion strips key negative/positive terms; all results dropped |
| q14 | Expansion strips key tokens (`two wheeler`, `commute`) |
| q17 | Sparse web recall for price-bounded product query |
| q19 | P8 — invidious video in /search |
| q22 | Price-aware ranking weak for budget constraint |

## Regression Sample

10 queries from prior round (`intentforge-2026-09-06T1310Z.md`) were re-run after the round:
- Rust vs go for backend microservices — PASS
- how to grow tomatoes in pots on balcony — PASS
- chicken curry recipe without onion and garlic — PASS
- vegan meal prep high protein low carb — PASS
- homemade pizza dough without yeast — PASS
- project management tool without subscription for small team — PASS
- coffee shops near me open now — PASS
- best restaurants in chennai for family dinner — PASS
- upcoming tech conferences in india 2026 — PASS
- how to repair a leaky roof shingle in rain — PASS

No regression observed in the re-run sample.

## Residual Risk

- Expansion/normalization logic in intent-engine is degrading query meaning for several query classes; a fix there would likely resolve q01, q09, q13, q14 simultaneously.
- P8 (invidious videos in /search text results) recurred in 3 of 22 queries; this is a known class and fixable in gateway.
- q02 zero-result looks infra-related but needs a retry gate to confirm.
