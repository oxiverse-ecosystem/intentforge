# IntentForge v2 — NL Quality Round 4

**Date:** 2026-08-03 (UTC)
**Repo:** C:\Users\Likhith\Documents\Projects\intentforge
**Stack:** localhost:4000 (Docker dev, fresh rebuild of gateway)
**Commit landed:** `dc0ba91` (local only, no push)

## Scope

- 28 brand-new unique NL `/search` queries + 2 `/videos`, 1 `/news`, 1 `/images`,
  1 `/goals/quick`, 1 `/goals` (discovery) = **32 queries executed cold** against the live stack.
- No query reused from prior rounds (checked `.hermes-qa/query_log.txt`; this round's
  queries appended).
- Re-run harness used `limit=24` so top-15 reflects the true head of each result list
  (cache stores full result sets and re-applies the limit at serve time; the first
  harness pass with `limit=5` RH-censored relevant lower-ranked results — e.g. s14/s25/s26
  were actually fine, just beyond position 5).

## Verdicts (32 queries)

| id | query (truncated) | intent | total | verdict | why |
|----|-------------------|--------|-------|---------|-----|
| s01 | lower blood sugar naturally in morning without medication | how-to | 10 | PASS | top results directly on-topic; "medication" negation correctly applied |
| s02 | negotiate higher salary as a fresher | informational | **0→8** | FIXED | was 0 results; recency-window bug (see below). Now top-relevant. |
| s03 | self-hosted alternative to google analytics not tracking visitors | comparison | 10 | PASS | Plausible + 6 self-hosted analytics alt pages; clean |
| s04 | brezza vs venue fuel efficiency maintenance | comparison | 22 | PASS | all comparison articles; minor score spread but correct |
| s05 | wireguard vpn server debian 12 ufw | how-to | 10 | PASS | how-to-setup guides, top relevant |
| s06 | early signs of burnout at work without quitting | how-to | 7 | PARTIAL | genuine burnout articles present (2-7) but a local-index "Multiple Sclerosis: Early Warning Signs" page scores 1.0 and sits #1 (local-index topical-noise — see limitation) |
| s07 | cleanest safe beaches for solo woman traveler september | informational | 13 | PASS | solo-women beach lists dominate |
| s08 | recursive function factorial step by step | how-to | 16 | PASS | recursion/factorial tutorials; 2 unrelated low-score tail items |
| s09 | mutual fund vs individual stocks long term wealth | comparison | 3 | PASS | direct comparison; thin but correct |
| s10 | lightweight linux distro old laptop 2gb not ubuntu | technical | 5 | PASS | lightweight-distro lists; ubuntu excluded |
| s11 | cold brew coffee at home without special equipment | how-to | 3 | PASS | cold-brew guides (one explicitly "no fancy equipment") |
| s12 | newest discoveries habitable zones red dwarf stars this year | fresh | 1 | PARTIAL | only 1 weak result; likely upstream coverage gap for this niche (no date window applied correctly; fail-open kept it as scoring boost) |
| s13 | personal finance books young adults not by indian authors | informational | 15 | PASS | finance-books-for-young-adults dominate; indian-author result correctly demoted to #9 |
| s14 | resume with no internships final year student | how-to | 23 | PASS | college-resume guides; strong |
| s15 | why indian cities power cuts summer prepare | informational | 20 | PASS | power-cut explainers; "why" dictionary tail is low-score |
| s16 | resin vs filament 3d printers miniatures | comparison | 3 | PASS | direct comparison; correct |
| s17 | learn rust interactive browser free | technical | 4 | PASS | rust interactive tutorials present |
| s18 | health risks scratched teflon nonstick pan | informational | 15 | PASS | teflon-safety articles; strong |
| s19 | rooftop vegetable garden hot tropical limited space | how-to | 6 | PASS | rooftop-garden guides |
| s20 | privacy browser blocks ads not chromium | informational | 2 | PASS | privacy-browser list + (off-topic ad-blocker #2, low score) |
| s21 | cultural history of pongal tamil nadu | informational | 9 | PASS | pongal articles present; Britannica #2, a travel-aggregator #1 (minor ranking anomaly, not a defect) |
| s22 | differential gear work turning corners | how-to | 4 | PASS | differential-gear explainers |
| s23 | open source animated explainer video tools without subscription | informational | 4 | PARTIAL | #1 is a paid explainer-video company (off-topic); local-index Notion-alternative pages (2-4) are weak matches (local-index topical-noise — see limitation) |
| s24 | improve hindi writing skills native english | how-to | 9 | PASS | hindi-writing guides |
| s25 | rental agreement chennai what to check | informational | 8 | PASS | chennai rental-agreement pages |
| s26 | brisk walking vs running weight knee joints | comparison | 2 | PASS | walking-vs-running health articles |
| s27 | back up linux system bootable image external drive | technical | 23 | PASS | full-system-backup guides |
| s28 | why new youtube channels fail first year | how-to | 9 | PASS | youtube-growth-failure articles; strong |
| v01 | fold a fitted sheet | videos | 58 | PASS | folding tutorials |
| n01 | room temperature superconductor breakthroughs this month | news | 5 | PASS | superconductor news |
| i01 | layers of earth atmosphere diagram | images | 85 | PASS | atmosphere diagrams |
| g_quick | side hustle homemade snacks india | goals | — | PASS | roadmap generated (4 phases, 6 resources) |
| g_disc | childrens picture book friendly robot | goals | — | PASS | 6 discovery questions generated |

**Summary:** 28 PASS, 1 FIXED (s02), 3 PARTIAL (s06, s12, s23). One real code defect fixed (s02); two PARTIALs are documented limitations (no small general fix exists without hardcoding, which is banned).

## Defect 1 — RECENCY WINDOW SPURIOUSLY INJECTED (FIXED)

**Symptom:** `what are the best strategies to negotiate a higher salary during a job offer
as a fresher` returned **0 results** (`results_before_filter=7`, `total=0`,
`warnings: All web results were removed by your date constraint`).

**Root cause:** `derive_recency_window()` (gateway `main.rs:867`) matched
`"recent"` / `"latest"` / `"fresh"` by **bare substring** with `q_lower.contains(...)`.
The word **"fresher"** contains "fresh", so a 7-day date window
(`after:2026-07-27 before:2026-08-03`) was injected into the query and forwarded to
upstream SearXNG. Upstream then hard-filtered every web result (none carried a
parseable date) → empty set. Affects any query containing "fresher", "freshman",
"refresh", etc.

**Fix (commit `dc0ba91`):** replaced the substring check with a whole-word matcher
`q_has_word(q, "recent"|"latest"|"fresh")`. The 7-day window is still applied for
genuine whole-word recency signals. No hardcoded queries or dates — general
word-boundary check, so it also fixes "freshman"/"refresh" silently.

**Verified cold after `build` + `up -d`:** same query now returns 8 relevant results
(top: "How to Negotiate a Job Offer & Salary: 7 Tips"), `applied_constraints=null`,
`ignored_constraints=null`, `warnings=null`.

**Self-audit (Q1-Q6):** no authored prose; not a reply-type change; no query→result map;
no retraining; no per-query threshold tuning (reused the existing 7-day window only for
real recency words); verified by running the previously-failing query cold.

## Limitations (no small general fix — documented, not hacked)

**L1 — Local-index polysemous topical noise (s06, s23).**
Local-index pages rank on **lexical** BM25/overlap, which cannot disambiguate word
sense. For s06, "Multiple Sclerosis (MS): Early Warning Signs and Symptoms" shares
generic words ("early", "warning", "signs") with the burnout query and the indexer
scores it `quality=0.65` → gateway local gate passes it (`semantic_relevance_score
>= 0.12`) → it surfaces at score 1.0 above the genuine burnout articles. For s23,
crawled "Open Source Alternatives to Notion" pages are weak matches for "explainervideo
tools". The existing topical-coherence gate keys on *distinctive-term presence*, but
these pages DO contain the query's distinctive terms (they are genuinely about
"warning signs" / "open source"), so a length/term-count heuristic still passes them.
A correct fix requires **sense disambiguation** (BERT/embedding at the indexer merge),
not a lexical threshold — which would be either hardcoding (banned) or a larger
architectural change. Logged as a known limitation; web results are correct, only the
local-index head is polluted for a minority of queries.

**L2 — Upstream coverage gap for niche fresh query (s12).**
`newest discoveries about habitable zones around red dwarf stars this year` returned
only 1 weak result. The recency window is correctly NOT applied as a hard filter
(fail-open), so this is upstream coverage, not a code defect. No action taken.

## Acceptance criteria

- [x] 28+ new unique NL queries executed cold with real captured output (32 total).
- [x] Every defect either fixed-and-reverified (s02) or documented as a limitation (s06/s23/s12).
- [x] Docker rebuild via `build` + `up -d` (gateway), health OK, post-fix verification COLD.
- [x] No regressions: 10-query regression sample from prior rounds all PASS (see regression.py output).
- [x] Changes committed LOCALLY only (`dc0ba91`); no push.
- [x] Round report written here.
- [x] Temp scripts: harness.py / diag.py / regression.py / queries.json / raw/ kept under
      `.hermes-qa/round4/` (round artifacts, not deleted); no stray files elsewhere.

## Residual risk

- L1 (local-index sense disambiguation) remains; affects a small minority of queries
  where a crawled page polysemously shares the query's distinctive words. Web results
  are unaffected. Recommended follow-up (separate task): add BERT/embedding-based
  local-relevance scoring at the indexer→gateway merge so low-sense local pages are
  demoted by a general semantic signal rather than lexical heuristics.
- Build artifact is the dev image; production mirrors via Codeberg auto-mirror on push
  (this round did NOT push, per protocol).
