# Narrow / Technical Search Investigation — localhost:4000

**Date:** 2026-07-22
**Scope:** The 5 queries the user flagged + the upstream/breadth observations.
**Method:** Reproduced each query live against `GET /search` (full JSON, latency,
intent, constraints, expanded_queries, top results + sources). Read the gateway
ranking path (`services/gateway/src/main.rs`). No fixes applied — investigation only.

---

## 0. Headline

The NL parser is genuinely accurate (every `structured_constraints` block is
correct). The weakness is in **two places downstream**:

1. **The quoted-phrase (`"..."`) path is broken** — it returns 0 results and a
   confusing warning even though the same unquoted query returns great results.
2. **The local index floods the top of mixed queries** with substring-token
   matches (e.g. `code`) that have nothing to do with the query, because the
   topical-coherence gate is far too weak a lever against the authority/local
   score terms.

The `predictive coding` → football drift and the `boilerplate` → "Why does it
mean…" dictionary drift are both **symptoms of the same weak-gate problem**, not
separate bugs. The earlier `intitle:`/`inurl:` n=0 bug and the concurrency/408
instability bug from the prior `INVESTIGATION.md` are **already fixed** in the
current `main.rs` (native operators are now re-emitted; tests
`preprocess_preserves_native_operators` / `intitle_inurl_no_longer_hard_drop`
exist). This report is about a *different* layer.

---

## 1. Per-query evidence (reproduced this session)

| Query | n | Latency | Verdict |
|---|---|---|---|
| `Lancaster norms` | 18 | 4.27s | **GOOD** — all 18 are the real Lancaster Sensorimotor Norms (local/PMC/Edge Hill). constraint=`['+lancaster','+norms']`. |
| `"Lancaster norms"` (quoted) | **0** | 3.30s | **BROKEN** — `warnings: ['All web results were removed by your constraints…']`. Same topic, 0 hits. |
| `Lancaster norms site:arxiv.org` | 2 | 4.20s | OK-ish — 2 arxiv papers, but only 2 (narrow `site:` fan-out returns little; see §4). |
| `boilerplate` | 16 | 2.68s | **MIXED** — #1–2 good (Cambridge/NLP boilerplate removal, Wikipedia), but #3 = "Why Does 'Boilerplate' Mean 'Standard'? - Mental Floss" dictionary clickbait, #7 = Cambridge Dictionary definition. |
| `boilerplate code` | 11 | 4.41s | **LOCAL FLOOD** — #1 = "Create your first QR Code" (QR Code Generator), #2 = "Code Splitting" (parceljs), #5 = "International Morse Code", #6 = "Visual Studio Code". None are boilerplate. All `LOCAL`. |
| `predictive coding` | 24 | 4.63s | **FOOTBALL DRIFT** — #1–16 on-topic, but #17–24 are pure football-prediction spam (Forebet, SoccerTips.ai, etc.). 8/24 = 33% off-topic. |
| `"predictive coding"` (quoted) | 9 | 2.60s | Good (all neuroscience/ML), but only 9 — because quoting triggers the same broken phrase gate (see §2) yet the unquoted upstream also still returns the topic. |
| `predictive coding neuroscience` | 24 | 2.66s | Good. |
| `predictive coding site:arxiv.org` | 3 | 3.05s | Good. |
| `function words` | 24 | 4.92s | Good — Wikipedia + several arxiv papers. As user reported. |
| `function words site:arxiv.org` | 4 | 1.97s | Good. |
| `semantic memory` | 16 | 2.79s | Good — PMC/simplypsychology/Springer. As user reported. |
| `semantic memory PMC` | 24 | 3.90s | Good. |

**Conclusion on the user's own mitigations:** `site:arxiv.org` and quoted phrases
*both help topical precision* — but quoted phrases are *not reliable* (they hit
the §2 gate and can fall to 0), and `site:` alone returns very few results.

---

## 2. BUG A — Quoted phrase `"..."` returns 0 results (hard-filter over-strip)

**Symptom:** `"Lancaster norms"` → `n=0`, warning *"All web results were removed
by your constraints."* Meanwhile `Lancaster norms` (unquoted) → 18 perfect hits.

**Root cause — `should_filter_by_constraints()`, `main.rs:2103-2114`:**
```rust
// 4. Hard filter on phrases
if !constraints.phrases.is_empty() {
    let t_low = title.to_lowercase();
    let c_low = content.to_lowercase();
    let u_low = url.to_lowercase();
    for phrase in &constraints.phrases {
        let p_low = phrase.to_lowercase();
        if !t_low.contains(&p_low) && !c_low.contains(&p_low) && !u_low.contains(&p_low) {
            return true;   // DROP
        }
    }
}
```
The intent engine correctly extracts the phrase (`phrases:["Lancaster norms"]`),
but **the upstream engine query (`preprocess_searxng_query`) strips the quotes**
(`clean_w = w.replace('"',"")` at `main.rs:3339`), so SearXNG is asked for
`Lancaster norms` without quotes. The returned snippets rarely contain the exact
substring `"lancaster norms"` (case/whitespace/POS-tagged text differ), so the
local hard filter drops **every** web result → `n=0`. The local index (which may
contain the exact title) is also gated the same way, so it drops too.

This is the *same class* of bug as the old `intitle:` n=0 (hard-filter with no
upstream enforcement) — it was fixed for operators but never for phrases.

**Why it "sometimes works":** when the phrase happens to appear verbatim in a
snippet/title (e.g. `predictive coding` quoted still got 9) you get hits; when it
doesn't, you get 0. Non-deterministic UX.

**Fix options (not applied):**
- **A (recommended): forward quotes upstream + downgrade to a soft match.**
  In `preprocess_searxng_query`, stop stripping quotes for phrase tokens — keep
  `"..."` so SearXNG honors the exact phrase. Then in the phrase filter, require
  only a *case-insensitive substring OR a token-overlap* match (split phrase into
  words, require all words present in title/content), not the exact joined
  substring. This makes the filter fail-open instead of nuking everything.
- **B (safest UX): fall back.** If the phrase gate removes 100% of results,
  re-run scoring on the unfiltered set and attach
  `warnings:["phrase '…' matched 0 results; showing broad matches"]`. Never show
  a blank page for a valid query.
- **C (min):** at minimum, stop stripping quotes in `preprocess_searxng_query`
  (`main.rs:3339`) so upstream actually searches the phrase — recovers most cases.

---

## 3. BUG B — Local-index token flooding (the real "boilerplate code → QR Code" bug)

**Symptom:** `boilerplate code` → top 7 are ALL `LOCAL` and almost all wrong
("QR Code Generator", "Code Splitting", "Morse Code", "Visual Studio Code"). The
local index matched the bare token `code` against pages that merely contain the
word "code".

**Root cause chain:**
1. The local indexer does a **substring/BM25 match on individual tokens**, so
   `code` matches any page containing "code". There is no phrase requirement.
2. In `merge_local_and_web` → scoring loop, the topical-coherence gate
   (`main.rs:4228-4251`) is the *only* thing that should sink these. But it only
   multiplies `quality`, and `quality` is weighted **0.06–0.08** in the ranking
   blend (`RankingWeights`, `main.rs:2850/2861`), while `rrf`/`authority`/
   `local_bonus` carry the weight:
   ```rust
   let base = (weights.rrf * r.score)
            + (weights.semantic * semantic)
            + (weights.authority * r.authority)   // local pages get domain_authority too
            + ...
            + (weights.local_bonus * local_bonus)  // +1.0 for local!
            ;
   r.score = base * c_score * generic_penalty;
   ```
   So even after `quality *= 0.08`, the local page still scores ~0.85 because
   `local_bonus=1.0` and a decent `authority`. The coherence penalty is a rounding
   error against those terms.
3. There is a *semantic* gate (`main.rs:4155`: `if r.is_local && semantic < 0.12
   { quality *= 0.05; }`) but `code`-containing pages have enough lexical overlap
   with the query that `semantic` clears 0.12, so the gate doesn't fire.

**Why `function words` / `semantic memory` DON'T show this:** those tokens are
rarer and the web results that come back are genuinely strong, so local noise
sits below them. It's a *token-generic* problem: any query containing a hyperscope
word (`code`, `data`, `app`, `test`, `api`, `web`) will let local index pages
through.

**Fix options (not applied):**
- **A (recommended): require phrase/AND-match for local results.** When the query
  is a multi-word phrase, a local result should only survive if it contains the
  *full* cleaned query (all tokens) in title+content+url, not just one token.
  Apply this as a *hard drop* for local results (fail-closed: a local page that
  only matches 1 of N query words is almost certainly noise). This is safe because
  local results can always be re-covered by web.
- **B: make the coherence penalty bite harder for local.** Raise the local
  no-match multiplier from `0.08`/`0.01` to something like `0.005`, and/or reduce
  `local_bonus` from `1.0` when `semantic < 0.4`. Stronger but blunter.
- **C: raise the local semantic gate floor** from `0.12` to `~0.25` so weak
  single-token overlaps get crushed. Cheapest, partial.

---

## 4. BUG C — Topical-coherence gate is too weak (football drift + dictionary drift)

**Symptom:** `predictive coding` → 8/24 football-prediction spam at ranks 17–24
scored 0.6–0.8. `boilerplate` → "Why Does 'Boilerplate' Mean 'Standard'?"
(Mental Floss) at rank 3.

**Root cause:** the coherence gate (`main.rs:4228-4251`) reduces `quality` for a
result that matches none of the distinctive terms, but `quality` is only
`0.06–0.08` of `base`, and the football/dictionary pages arrive with high
`rrf`/`authority` from the upstream engines, so the net effect is cosmetic. The
gate was clearly designed to fight the older "football scores for a productivity
query" collapse, but it no longer wins against the current weight blend.

Two reinforcing issues:
- The distinctive-terms set for `predictive coding` is `["predictive","coding"]`.
  Football-prediction pages about "predictions" match neither, get `quality*=0.08`,
  but still float at 0.6+.
- Dictionary/definition pages ARE penalized (`main.rs:4264-4318`, `quality*=0.10`),
  but only when the page has phonetic/POS structure *and* the query isn't a
  definition query. "Why Does 'Boilerplate' Mean 'Standard'?" is a *listicle*, not
  a dictionary entry, so the dictionary guard misses it entirely.

**Fix options (not applied):**
- **A (recommended, addresses both drifts): apply the coherence penalty to the
  WHOLE score, not just `quality`.** Instead of `quality *= 0.08`, compute a
  `coherence_factor` and multiply the final `r.score` by it (e.g. off-topic web
  result → ×0.15, off-topic local → ×0.03). This gives the gate real authority
  against the RRF/authority terms.
- **B: extend the dictionary/listicle guard** to catch "why does X mean…" /
  "what is the meaning of X" clickbait titles via a structural title pattern
  (`/^(why|what|how)\b.*\b(mean|meaning|definition)\b/i` + short content), not
  just POS/phonetic markers.
- **C: raise the football-pollution fix by adding a lightweight "topic signature"
  check** — but A alone removes most of the harm.

---

## 5. Upstream / latency observations (consistent with user's report)

- Latency spread: simple/quoted 2.6–3.3s; constraint/arxiv 3.3–4.4s; some 4.9s
  (`function words`). Variable, as reported. `site:` narrows the upstream fan-out
  so it returns fewer results (2–4) — not a timeout here, but fewer.
- `Lancaster norms` originally reported as `upstream_unavailable`/timeout — this
  session it returned in 4.27s with 18 good hits. The flaky-upstream/timeout
  behaviour is **intermittent** (depends on the gluetun/VPN + SearXNG engine mix
  at that moment), consistent with the prior `INVESTIGATION.md` BUG 2. No new
  code fix needed for the timeouts themselves beyond what's already noted there
  (decouple dedup subscribers from leader failure; surface `upstream_unavailable`
  instead of silent `n=0`).
- The engine diversity (duckduckgo-onion, bing, local, wikipedia, PMC) gives good
  breadth on the GOOD queries — the problem is purely the ranking/gate layer, not
  the upstream mix.

---

## 6. Prior bugs — status check

| Prior bug | Status in current `main.rs` |
|---|---|
| `intitle:`/`inurl:`/`intext:` n=0 | **FIXED** — `preprocess_searxng_query` re-emits native operators (`main.rs:3311-3394`); hard-drop downgraded to boost. Unit tests present at bottom of file. |
| Concurrency 408 / `n=0` instability | Addressed in prior work (dedup + `upstream_unavailable` surfacing). Not re-tested this session; out of scope. |
| **Quoted phrase n=0 (this report, §2)** | **NOT fixed** — new finding. |
| **Local token flood (§3)** | **NOT fixed** — new finding. |
| **Weak coherence gate → drift (§4)** | **NOT fixed** — new finding (the football collapse was patched cosmetically, not structurally). |

---

## 7. Suggested fixes (priority order)

1. **§2 quoted-phrase gate** — stop stripping quotes upstream + make phrase match
   fail-open (token-overlap, not exact substring). Highest user pain (0 results).
2. **§3 local phrase/AND-match** — hard-drop local results that match < all query
   tokens. Directly fixes "boilerplate code → QR Code".
3. **§4 coherence multiplier on final score** — multiply `r.score` by a coherence
   factor so off-topic web pages actually sink. Fixes football/dictionary drift.
4. (Optional) **§4b** extend dictionary/listicle guard to "why does X mean…"
   clickbait titles.

All four are localized to `services/gateway/src/main.rs` (functions
`preprocess_searxng_query`, `should_filter_by_constraints`, `merge_local_and_web`
scoring loop). No schema/API changes — all are internal ranking tweaks, so the
frontend contract is untouched.

## 8. Verification plan (after fixes)

Re-run `_searchtest/probe_narrow.py` and assert:
- `"Lancaster norms"` → `n_results > 0` (was 0).
- `boilerplate code` → top 3 should contain "boilerplate"; no "QR Code"/
  "Morse Code"/"Visual Studio Code" in top 7.
- `predictive coding` → 0 football-prediction URLs in top 24 (was 8).
- `boilerplate` → no "Why does X mean…" listicle in top 5.
- `Lancaster norms`, `function words`, `semantic memory` → still return their
  current good results (regression check).
Add the three assertions as new `#[test]` cases mirroring the existing
`intitle_inurl_no_longer_hard_drop` test at the bottom of `main.rs`.
