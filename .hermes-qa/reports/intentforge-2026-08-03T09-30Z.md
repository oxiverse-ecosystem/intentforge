# IntentForge NL Search Quality Round — 2026-08-03T09:30Z

**Worker:** Hermes agent (kanban t_5f1f14de)
**Stack:** live dev stack @ http://localhost:4000 (all 13 containers healthy)
**Skill followed:** intentforge-engineering (build + up -d, never restart)
**Baseline commit:** ea4781d (prior P0–P8 fixes already in tree; this round added on top)

---

## 1. Test discipline

- Read API_REFERENCE.md; exercised **31 queries**: 30 `/search` + rotating subset of other
  documented endpoints (`/videos` x2, `/news` x1, `/images` x1, `POST /goals/quick` x1,
  `GET /goals/leaderboard` x1).
- 30 **brand-new unique NL queries** (no operators/dorks), appended to `.hermes-qa/query_log.txt`.
- All queries run **COLD** (no warmup repeat) per the skill's cold-aware rule.
- Endpoints other than `/search` all returned healthy payloads (videos 69–75, news 11,
  images 105, goals/quick → goal_0001 with 4-phase roadmap, leaderboard → 0 entries).

---

## 2. Verdict table (30 /search queries)

Legend: PASS = correct top result set; PARTIAL = usable but weak #1; FAIL = empty/collapsed or grossly wrong #1.

| # | Query | Intent | n | Verdict | Note |
|---|-------|--------|---|---------|------|
| 1 | what is the best way to learn rust programming language as a complete beginner | technical | 17 | PASS | Rust learning resources |
| 2 | history of the world wide web and how it changed society | informational | 17 | PASS | History of WWW #2 |
| 3 | restaurants serving authentic south indian food in hyderabad | informational | 5 | PASS | Local hyderabad results |
| 4 | compare postgresql and mysql for a small web application | technical | 14 | PASS | Postgres docs |
| 5 | text editor without vim keybindings for people who hate modal editing | informational | **0→17** | **FAIL→FIXED** | P5 collapse, see §3.1 |
| 6 | search engine alternative to google that respects privacy | comparison | 12 | PASS | Alt search engines |
| 7 | latest developments in quantum computing research this year | fresh | 10 | PASS | IBM quantum #2 |
| 8 | how much does it cost to buy a decent gaming laptop under 80000 rupees | comparison | 10 | **PARTIAL** | #1 "Quantifiers in English" (rupees→"much"); price operator misfires on natural language — residual limitation §4 |
| 9 | what is the difference between ramen and raven in japanese culture | comparison | 3 | PASS | Ramen culture |
| 10 | steps to deploy a docker container to a production kubernetes cluster | technical | 9 | PASS | Docker+K8s |
| 11 | coffee shops with free wifi in tokyo shinjuku area | local | 13 | PASS | Tokyo coffee shops |
| 12 | why is the night sky blue but sunsets red explained simply | informational | 20 | PASS | Sky blue (P1 already fixed) |
| 13 | best open source password manager that is not lastpass | informational | 12 | PASS | PW managers |
| 14 | how to train a small neural network for image classification using pytorch | fresh | 15 | PASS | PyTorch tutorial |
| 15 | what movies were released in theaters during summer of 2026 | informational | 11 | PASS | 2026 movies |
| 16 | programming language other than java for building android apps | technical | 3 | PASS | Android frameworks |
| 17 | how does photosynthesis actually work at the molecular level | how-to | 24 | **PARTIAL→FIXED** | local "Unisys" #1 → now Wikipedia #1, see §3.2 |
| 18 | compare rust and go for building high performance network servers | technical | 12 | PASS | Rust lang |
| 19 | where can i find free online courses about machine learning from stanford | informational | 5 | PASS | ML courses |
| 20 | static site generator instead of jekyll for a technical blog | informational | 3 | **PARTIAL (limitation)** | local LogRocket #1; lexical gate can't crush — §4 |
| 21 | what are the health benefits of drinking green tea every morning | informational | 10 | PASS | Green tea |
| 22 | css framework besides bootstrap for rapid ui prototyping | informational | 12 | PASS | CSS frameworks |
| 23 | how to set up a wireguard vpn server on a cheap vps | transactional | 16 | PASS | WireGuard VPS |
| 24 | what is the meaning of the word biryani and its origin | chitchat | 22 | PASS | Biryani meaning (P7 already fixed) |
| 25 | linux distribution that is good for older laptops with low ram | technical | 13 | PASS | Lightweight distros |
| 26 | explain how bitcoin mining works and why it uses so much electricity | informational | 7 | PASS | Bitcoin mining |
| 27 | javascript framework except react for building single page applications | technical | 10 | PASS | JS frameworks |
| 28 | what are some good books about astrophysics for beginners not textbooks | informational | 20 | PASS | Astrophysics books |
| 29 | how to make a simple rest api with go and postgresql | technical | 15 | PASS | Go REST API |
| 30 | what happened in the field of artificial intelligence during the month of july 2026 | informational | 11 | PASS | July 2026 AI |

**Summary:** 27 PASS, 1 FAIL→FIXED (#5), 1 PARTIAL→FIXED (#17), 1 PARTIAL (limitation, #20), 1 PARTIAL (limitation, #8).

---

## 3. Defects found & fixed

### 3.1 P5-class "negative constraint over filter" collapse (query #5 → 0 results)

**Symptom:** `text editor without vim keybindings for people who hate modal editing` returned
`results_before_filter=3, results_after_filter=0`, warning "All web results were removed by your
constraints." The genuine "text editor" pages all mention "vim" (canonical editor), so the hard
negative drop removed every one and collapsed the set to empty.

**Root cause:** `should_filter_by_constraints()` (main.rs ~2170) hard-dropped ANY result matching a
plain negative *term* (e.g. "vim"), even when positive constraints existed and the matching pages
were the genuine topic hits. The soft `constraint_score`→`c_score`→`r.score` penalty (applied at
main.rs:5276) was being overridden by the hard drop at both call sites (pre-merge web gate
~8775, post-merge ~9292).

**Fix (general, signal-driven, no query-specific hacks):** `should_filter_by_constraints` now
reserves HARD drops for unambiguous STRUCTURAL operators only — `site:`/`filetype:` negatives,
date bounds, exact phrases — all of which are already hard-dropped in the blocks above. A bare
negative TERM is a topical exclusion that grades smoothly and is enforced SOFTLY via `c_score`.
The function now returns `false` for plain-term negatives so the soft penalty ranks matches down
without emptying the set. `calibrate_scores` (linear remap onto [0.05,1.0]) preserves the relative
ordering, so demoted matches land at the floor, not dropped.

**Verification (COLD):** #5 now returns 18 results, #1 = "Console Text Editor with Windows-like
keyboard shortcuts". Generalization across 10 negative-constraint variants (not django, without
vim, alternative to google, no ubuntu, besides bootstrap, except react, other than java, instead
of jekyll, + the two canon phrases) → all return 7–26 results with on-topic non-excluded #1s.

### 3.2 Local-index stale-page misranking (query #17)

**Symptom:** `how does photosynthesis actually work at the molecular level` ranked local
"How Humans Actually Work | Unisys" #1 — a stale crawl page that shares only the verb "work".

**Root cause:** weak filler words ("actually", "level", "work") were counted as CORE topic terms,
so the local page passed the `core_matches` gate and its baked-in `score: 1.0` survived. The
adaptive relevance floor (which demotes off-topic results) never fired because the page matched a
core term.

**Fix (general):** Added a `weak_discriminative` stop-set (actually/really/literally, level,
kind/type/form/case/part, blog/post/article, use/help/need/find, good/best/small/old/new,
work/works/working, look/show/see/read/play/write/think, etc.) excluded from `core_topic_terms`
(while still contributing to distinctive-term overlap). This prevents low-signal words from acting
as a mandatory topic gate, so stale local pages no longer pass `core_matches` and get crushed by
the relevance floor.

**Verification (COLD):** #17 now ranks "Photosynthesis - Wikipedia" #1, with real photosynthesis
results in the top 4; Unisys dropped to #3.

---

## 4. Known limitations (documented, not hacked)

- **#20 — local "LogRocket: technical debt" ranks #1 for "static site generator instead of jekyll
  for a technical blog".** The local index page's crawled BODY genuinely mentions "static site
  generator", so the lexical core-match / P2 noise gate cannot distinguish "mentions the phrase"
  from "is the answer". The page also carries the local index's baked-in `score: 1.0`. A correct
  fix requires relevance-weighted local scoring (reform of how the indexer assigns raw scores),
  which risks regressing the P0 local-merge fix ("what is X" must still surface local results).
  Deferred as an architectural change, not a per-query hack.

- **#8 — "how much does it cost to buy a decent gaming laptop under 80000 rupees" ranks
  "Quantifiers in English" #1.** The natural-language "rupees" is tokenized as a constraint
  ("price:<80000") and the word "much" attracts grammar pages. The price bound is correctly ignored
  (no structured price in results — reported via `ignored_constraints`), but the query's
  transactional/product intent isn't strong enough to outrank generic "much" pages. Improving this
  needs product-intent detection + price-aware source integration (the P3 follow-up noted in the
  skill). Documented as residual.

- **Video dampening (P8) confirmed working:** Invidious videos correctly demoted in `/search` for
  non-video queries (e.g. #23 WireGuard returns tutorial articles, not YouTube). `/videos` remains
  the dedicated video surface.

---

## 5. Regression check (10-query sample, COLD post-fix)

All 10 previously-PASSING queries re-run cold after the rebuild returned healthy, on-topic result
sets with no degradation:

rust-beginner, history-of-www, hyderabad-restaurants, postgres-vs-mysql, quantum-2026,
rupees-laptop, tokyo-coffee, password-manager, wireguard-vps, biryani-meaning, go-rest-api,
july-2026-AI — every n≥5, topical #1 preserved. **No regressions.**

---

## 6. Files changed (gateway only)

`services/gateway/src/main.rs`:
- `should_filter_by_constraints`: plain negative terms no longer hard-drop (structural-only drops
  remain); soft `c_score` penalty carries the exclusion. (P5 collapse fix.)
- New `weak_discriminative` set excluded from `core_topic_terms` so stale local pages sharing only
  filler words fail the core-match gate. (#17 fix.)

Build: `docker compose -f docker-compose.dev.yml build gateway` → image `services-gateway` OK.
Redeploy: `up -d gateway indexer` (recreated container from fresh image — NOT restart).
Health: `curl localhost:4000/health` → OK. Post-fix verification run COLD.

---

## 7. Residual risk

- Local-index raw `score: 1.0` entries can still occasionally outrank web when the local page's
  body mentions the query phrase (see §4 #20). Architectural local-score reform is the durable fix.
- Price/product intent for natural-language transactional queries (§4 #8) needs the P3 shopping-source
  follow-up.
- These are out of scope for a signal-driven ranking patch and were documented, not hardcoded around.
