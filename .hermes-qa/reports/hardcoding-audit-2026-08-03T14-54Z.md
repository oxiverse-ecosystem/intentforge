# IntentForge Hardcoding Audit — 2026-08-03

Auditor: independent sweep agent (kanban t_a368ea98). Scope: **whole repo**, not tonight's diff.

## Method

Read every module that produces user-visible output or decides WHAT to say. Applied the
deciding test to each candidate:

> "Can the system change this BY ITSELF, through experience?"
>   YES -> legitimate.  NO -> hardcoded, must go.

Files mapped:
- `services/gateway/src/main.rs` (10,383) — search ranking, intent merge, /search. Returns
  result JSON built from real crawler/indexer state. No authored prose. **CLEARED.**
- `services/gateway/src/goals.rs` (2,058) — **HOT ZONE.** Goal wizard + roadmap generator.
- `services/gateway/src/dictionary.rs` (1,899) — lexicon used for MATCHING. **ALLOWED** (vocabulary, not reply).
- `services/gateway/src/spell.rs` (1,176) — SymSpell; data-driven edit-distance. **ALLOWED.**
- `services/gateway/src/clean.rs` (1,055) — query cleaning. "tell me a joke" hits are
  structural detection, not replies. **CLEARED.**
- `services/intent-engine/src/main.rs` (2,807) — returns an intent LABEL + confidence. Detects
  chitchat lexically but emits only a classification the gateway's ranking consumes. **CLEARED**
  (no canned reply branch exists in the gateway — verified).
- `services/indexer/src/main.rs` (905) — BM25/RRF fusion. Data-driven. **CLEARED.**
- `services/crawler/src/main.rs` (1,793) — structural URL analysis. **CLEARED.**
- `services/privacy-layer`, `services/meta-search-engines`, `services/geolite2-updater`,
  `services/shared-signals`, `services/traefik` — no user-facing prose. **CLEARED.**

## Verdict

**3 violations, all in `services/gateway/src/goals.rs`.** The entire `/goals` feature is an
authored-prose generator keyed by a keyword→domain router. No violation found anywhere else in
the repo. The search engine core is clean (its many "NO hardcoded" comments are accurate).

---

## VIOLATION V1 — CRITICAL — `phase_content()` authored roadmap narrative, objectives, deliverables

`goals.rs:991-1461`

`generate_roadmap()` calls `phase_content(idx, total, goal, domain, is_technical)` which returns
**fully hand-written** `title`, `raw_desc`, `objectives: Vec<String>`, `deliverables: Vec<String>`
for every phase. The only variable is the user's goal spliced into the prose:

```rust
format!("Design system architecture, choose tech stack, and set up foundation for '{}'.", goal_short)
vec!["Design system architecture and component diagram".to_string(),
     "Choose tech stack and dependencies".to_string(),
     "Set up development environment and CI/CD".to_string(),
     "Define API contracts and data models".to_string()]
```

There are ~13 domain branches × (first phase + last phase + up to 5 middle-phase patterns), each
returning invented advice. The phase *structure* (timeline→week count→phase count, deadline math,
`buffer_days`, `score`) IS derived from real state (the user's timeline answer + `SystemTime::now()`)
and that part is correct and must be preserved. Only the **narrative content** is faked.

**Why it fails the test:** the system cannot change a single sentence by living. Adding a new
domain's advice, or correcting weak advice, requires a human editing this function. It is a
question→advice dictionary wearing a `match` statement. This is the single largest block of faked
intelligence in the repo and it is directly user-visible via `/goals`, `/goals/quick`,
`/goals/:id`.

**Suggested fix (state-derived, no retraining, no runtime LLM):** derive the phase content from
REAL state only:
- phase title/description/objectives/deliverables are built from the user's goal text + their own
  answers (especially the Q99 success vision, which after V2 becomes free text — i.e. the user's
  own words) + the computed timeline. No invented advisory sentences.
- Where the system has nothing derived to say, emit an honest empty-state ("No generated guidance
  for this phase — define your own objectives") instead of an authored paragraph.
- Thread the user's answers into `phase_content` so the roadmap *echoes what the user said*, not
  what we wrote.

---

## VIOLATION V2 — HIGH — `generate_questions()` authored domain question + option banks

`goals.rs:379-903`

`detect_sub_domain()` (198-377) keyword-routes the goal to a domain label, and `generate_questions()`
pushes 3-6 hand-written questions per domain, each with an authored `description` (advisory prose)
and an authored `options` list. Example (ai-ml):

```rust
Question {
    question: "What type of AI/ML system are you building?".to_string(),
    description: "Different AI systems need different architectures — from simple API wrappers to custom model training pipelines.".to_string(),
    options: vec!["LLM-powered app using existing APIs (OpenAI, Claude, etc.)".to_string(),
                  "Custom model training and fine-tuning".to_string(), ...],
}
```

The Q99 "success vision" block (837-887) is another authored option bank keyed by domain.

**Why it fails the test:** the domain-specific option lists and the descriptions are invented
knowledge about what the user *might* build — the system cannot revise them by living. A
keyword→advice dictionary in all but name.

**Suggested fix:**
- Keep the two generic scaffolding questions (Q1 timeline, Q2 hours/week). They are universal form
  fields with a fixed time-bucket taxonomy — legitimate UI, not faked intelligence.
- Delete the per-domain `match` blocks that push Q3-Q6 (authored descriptions + option banks).
  Replace with ONE open-text question that echoes the user's goal: "What are the key things you
  need to plan for '<goal>'?" — the *user* supplies the content.
- Make Q99 free-text: keep the prompt ("What would make this goal feel truly accomplished to you?")
  which is a legitimate question, but remove the authored 5-option bank so the user types their own
  answer. That answer then feeds V1 as real state.

---

## VIOLATION V3 — MEDIUM — `curate_resources()` fabricated resource descriptions

`goals.rs:1478-1491`

When the search call yields no resources, the code fabricates `Resource` entries:

```rust
Resource { title: format!("Getting Started: {}", goal),
           url: format!("https://www.google.com/search?q=get+started+with+{}", encoded),
           resource_type: "article".to_string(),
           description: "A comprehensive guide to getting started.".to_string() }
```

**Why it fails the test (partially):** the URL is derived from the goal (legitimate real state — a
real search link). But the `description` strings ("A comprehensive guide to getting started.",
"Learn foundational knowledge.", "Deep dive into core concepts.") are authored fluff describing a
resource that does not exist. They create the illusion of curated resources where there are none.

**Suggested fix:** keep the constructed search URL (real state) but replace the invented
description prose with an honest label or return an empty resource list with an honest marker
("No curated resources found — open web search"). An honest empty state beats a fabricated blurb.

---

## BORDERLINE (reported, not changed)

- **intent-engine chitchat/how-to lexical overrides** (`intent-engine/src/main.rs:2599-2638,
  2662+`): these are *classifiers* that return an intent label, not a pre-written reply. The rule
  bans "a matcher that returns a pre-written reply" — a classification label consumed by ranking
  is a legitimate signal, not a reply. **CLEARED**, with one caution: if a gateway branch is ever
  added that returns canned chitchat prose keyed on this label, that would be the RAVANA-class
  violation. None exists today (verified: `grep chitchat` in gateway finds no reply branch).
- **`detect_sub_domain` keyword router** (`goals.rs:198-377`): a router to advisory content. The
  routing itself is classifier-like (legitimate), but since its destination is V1's authored prose,
  the whole chain is captured by V1. Not double-counted.

## OK / CLEARED (so it is not re-flagged later)

- dictionary.rs / spell.rs lexicons — matching vocabulary, explicitly allowed.
- gateway main.rs ranking, penalties, recency, P0-P8 fixes — all data-driven from result state;
  the many "NO hardcoded" comments are accurate; no authored reply prose.
- crawler URL-structure analysis — structural, not reply.
- privacy-layer / meta-search-engines / geolite2 / shared-signals / traefik — no prose output.
- All error/validation messages ("Goal must be at least 3 characters", "Failed to update goal",
  "Goal '…' not found") are honest, not fake-depth. Allowed.
- Roadmap *structure* (phase count from timeline, deadline math, buffer_days, score) — derived from
  real state; preserve it when fixing V1.

---

## Plan (driving the fix)

1. **V3** — `curate_resources()` honest empty-state / honest label. (commit + verify + ping)
2. **V2** — `generate_questions()` drop authored domain banks + Q99 option bank → free text. (commit + verify + ping)
3. **V1** — `phase_content()` derive from goal + timeline + user's Q99 answer; remove all authored narrative. (commit + verify + ping)

Each fix is state-derived, needs no retraining and no runtime LLM. Verification is done live
against the running dev gateway (`localhost:4000/goals/quick`) after a Docker rebuild.

No findings were invented. The search engine core is genuinely clean.
