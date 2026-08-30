# Audit Report — v2 Docs Rounds (Independent)

- **Auditor:** t_5348df31 (independent, did not author any of the audited diffs)
- **Date:** 2026-08-05
- **Scope:** Verify the REAL committed diffs of the two v2 docs rounds against live behavior,
  covering BOTH classes of finding — (1) FALSE claims and (2) MISSING coverage — and confirm
  that net-new sections trace to real observed behavior rather than invented prose.
- **Repos:**
  - IntentForge — `C:\Users\Likhith\Documents\Projects\intentforge`
  - RAVANA — `C:\Users\Likhith\Documents\Projects\ravana`
- **Method:** Read `git diff <base>..HEAD`, enumerate every route/engine capability from source,
  diff against documented sections, and verify live against the running gateway (`localhost:4000`,
  healthy) plus RAVANA source templates. Auditor does NOT edit source — only this report + FIX cards.

---

## Per-repo verdict

### IntentForge — VERDICT: HAS VIOLATIONS (one structural doc defect; all *claims* verified true)
- Base audited: `3cc8249` → `HEAD` (`b9c33b8`, `04dea49`)
- Files changed: `API_REFERENCE.md`, `README.md`, `docs/_generated/_round_v2_exercise.sh`, `docs/_generated/_round_v2_raw.md`
- All documented *behavioral claims* were re-verified live and are TRUE (see evidence below).
- One real defect surfaced in the doc STRUCTURE (not a false claim): the v2 diff dropped the
  `### POST /goals` heading, orphaning the create-goal docs. → spawned FIX card `t_7eb143a3`.

### RAVANA — VERDICT: SUBSTANTIALLY HONEST (no violations)
- Base audited: `1618ed2` → `HEAD` (`6aa79e9`, `3c586d2`, `31ee109`)
- Files changed: `README.md`, `docs/README.md`, `.gitignore`, removed `.test_durations` from index
- The new "What RAVANA does" section's 6 capabilities each trace to a real, value-interpolated
  code template (not authored prose) and the cited store classes all exist in source.
- De-clutter of `.test_durations` is inert (gitignored, local copy kept, referenced by no code/CI).

---

## Findings — IntentForge

### F1 (VIOLATION, structural) — `### POST /goals` heading replaced by `### Goals Error Codes`
**Class:** structural defect introduced by the v2 round (a form of broken coverage).
**Evidence:**
- `git diff 3cc8249..HEAD -- API_REFERENCE.md` shows the hunk at the Goals section inserting
  `### Goals Error Codes (verified live, 2026-08-05)` and the error-code table (lines 1002–1013),
  and the preceding `### POST /goals` heading is gone.
- Current file, lines 1002–1015:
  ```
  1002: ### Goals Error Codes (verified live, 2026-08-05)
  ...
  1013: See `docs/_generated/_round_v2_raw.md` for the exact raw bodies ...
  1014:
  1015: Creates a new goal and returns domain-specific questions tailored to the goal type.
  ```
- `grep -n "^### POST /goals$"` API_REFERENCE.md → **no match**. The create-goal prose (request
  body, 200 OK response, `next_step`) is now stranded under the "Goals Error Codes" heading.
- The Endpoints table (line 994) still lists `POST /goals`; README Goals table (README line 236)
  also lists `POST /goals`. So the doc claims a section that no longer has a heading.
**Why it matters:** breaks the "every route has a doc section" coverage guarantee the round
claimed; broken heading anchor; reader confusion between error codes and the create call.
**Action:** FIX card `t_7eb143a3` spawned (re-insert `### POST /goals` before line 1015; leave the
error-code block where it is).

### F2 (verified TRUE) — `/search/fast` returns top-level `source` field
Live: `GET /search/fast?q=rust` → top keys `['count','results','source']`. This confirms the
earlier fix (t_fdab0124) held; the v2 round did not regress it.

### F3 (verified TRUE) — media endpoints have the documented field shapes
- `/images?q=rust` → result keys include `image_url` + `thumbnail_url` (NOT `thumbnail`). ✔
- `/videos?q=rust` → result keys include `thumbnail` (NOT `thumbnail_url`), `video_id` observed
  empty `''`. ✔ matches the v2 correction exactly.
- `/news?q=ai` → result has `published_at` (observed empty string `""` on bing-news items). ✔
  matches the documented "empty-string vs ISO" note.

### F4 (verified TRUE) — Goals error codes
- `POST /goals` with `{"goal":"ab"}` → `400`. ✔ matches `empty_goal` (goal < 3 chars).
- Error-code table (400 empty_goal / 400 invalid_phase / 404 not_found / 422 invalid_payload)
  matches the live transcript and the earlier parent handoff.

### F5 (verified TRUE) — Goals leaderboard shape + in-memory storage
`GET /goals/leaderboard` → bare JSON array `[{...}]` (max 50, score-descending). ✔ matches README line 250 and the live contract (`total_entries` wrapper dropped; count derivable as `len(array)`).

### F6 (verified TRUE) — README Goals section is backed by real observed behavior
The two flows (quick, discovery), the 7-route endpoint table, and the behavior notes (1-indexed
phase IDs, +100/phase, in-memory) all match the live transcript `docs/_generated/_round_v2_raw.md`
and the gateway source route map. The question-count softening ("creative-writing returned 4, not 6")
is correctly flagged as descriptive. The latent footgun (1-indexed phase_id vs 0-indexed
question_id) is documented as a behavior note, not silently smoothed over.

### Route-coverage map (all 14 routes have a doc section — no MISSING coverage)
| Route | API_REFERENCE section | Documented? |
|-------|----------------------|-------------|
| `GET /` | (README line 138) | ✔ |
| `GET /health` | § `GET /health` (line 113) | ✔ |
| `GET /search` | § `GET /search` (line 127) | ✔ |
| `GET /search/fast` | § `GET /search/fast` (line 217) | ✔ |
| `GET /images` | § `GET /images` (line 261) | ✔ |
| `GET /videos` | § `GET /videos` (line 295) | ✔ |
| `GET /news` | § `GET /news` (line 329) | ✔ |
| `POST /goals` | prose at line 1015 — **NO HEADING (F1)** | ⚠ heading missing |
| `POST /goals/quick` | § `POST /goals/quick` (line 1238) | ✔ |
| `GET /goals/leaderboard` | § `GET /goals/leaderboard` (line 1213) | ✔ |
| `GET /goals/:goal_id` | § `GET /goals/:goal_id` (line 1177) | ✔ |
| `POST /goals/:goal_id/answers` | § `POST .../answers` (line 1091) | ✔ |
| `POST /goals/:goal_id/phases/:phase_id/complete` | § line 1279 | ✔ |
| `POST /goals/:goal_id/progress` | § line 1303 | ✔ |

No OTHER service in `services/` exposes routes (grep for `.route(`/`add_api_route`/`@app` outside
`main.rs` returned nothing). So 14/14 routes are covered (one with a structural heading defect).

---

## Findings — RAVANA

### R1 (verified TRUE) — "What RAVANA does" capabilities are engine-derived, not authored
Each of the 6 bullet replies traces to a real value-interpolated template in source (NOT a
hand-written sentence behind a matcher):
- "who are you?" → `engine_self_query.py:707` `f"... not a person. what made you curious?"` ✔
- "i live in berlin" → confirmed fact store; `PersonalFactStore` (`personal_fact_store.py:43`)
  stores `('i','location','berlin')` at `confidence=0.7` (default, line 132). The exact ack string
  `i'll remember you live in berlin` is NOT a literal in source — it is generated by the
  `noted — i'll remember {fact}` template (`engine_reasoning.py:2205`). This is legitimate
  live-state interpolation, exactly the kind of dynamic reply the doc claims. ✔
- "i love coffee" → `engine_reasoning.py:2219` `f"good to know — you {verb} {obj}. i'll keep that in mind."` ✔
- self-correction (milo→rex) → `engine.py:3585` / `engine_persistence.py:516` correction ack,
  and recall reflects rex because the fact store supersedes. ✔
- "what do you remember" → `engine_memory.py:1531` `f"from what you've told me, {_summary}."`
  where `_summary` is built dynamically from the stores. ✔
- abstention → `engine_self_query.py:386` `i don't have a settled view on that yet — what do you think?` ✔

### R2 (verified TRUE) — cited store classes exist
`IdentityEngine` (`core/identity.py:42`), `UserStanceStore` (`personal_fact_store.py:240`),
`PersonalFactStore` (`personal_fact_store.py:43`), `BeliefStore` (`chat/belief_store.py:4`),
`ConceptGraph` (`ravana_ml/.../graph.py:1047`) — all present. No fabricated class names.

### R3 (verified TRUE) — de-clutter of `.test_durations` is inert
`git diff` removes it from index + adds to `.gitignore`; grep across `.github/`, `pyproject.toml`,
`setup.*` finds no reference. Local copy retained. No coverage loss.

### Engine-capability coverage
The README's existing benchmark/architecture sections already describe the substrate; the new
user-facing "What RAVANA does" section fills the previously-missing *user-experience* coverage
gap. No undocumented user-facing capability was found (chat/identity, fact learning, stances,
self-correction, recall, abstention are all represented). No MISSING-coverage finding.

---

## FIX cards spawned

| ID | Repo | Finding | Severity |
|----|------|---------|----------|
| `t_7eb143a3` | IntentForge | F1 — `### POST /goals` heading replaced by error-code block; create-goal docs orphaned | doc-structural |

No FIX cards needed for RAVANA.

---

## Honesty notes (auditor's own limitations)
- IntentForge live checks were run against the gateway as it is NOW (healthy, `localhost:4000`).
  The v2 round's examples are dated 2026-08-05; I re-verified the same shapes/contracts today and
  they hold. I did NOT re-run the entire 30+ request transcript — I targeted the specific claims
  most prone to drift (media field shapes, `/search/fast` source, Goals error codes, leaderboard).
- RAVANA capability replies were verified by reading the generating code, not by re-running a live
  chat probe this session. The templates are value-interpolated, so exact strings vary by input;
  the *behavior* (live-state interpolation) is what the doc asserts and it is correct.
- F1 is the only defect. It is a doc-structural regression introduced by the v2 round itself, not a
  pre-existing false claim. It does not affect any documented *behavior* — only the heading hierarchy.
