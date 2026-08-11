# IntentForge v2 — API Reference

> **Base URL (Production):** `https://api.oxiverse.com`
> **Base URL (Development):** `http://localhost:4000`
> **Protocol:** HTTPS (production) / HTTP (development)
> **Format:** JSON
> **Authentication:** None (public API)

---

## Table of Contents

- [Endpoints](#endpoints)
  - [GET /](#get-)
  - [GET /health](#get-health)
  - [GET /search](#get-search)
  - [GET /search/fast](#get-searchfast)
  - [GET /images](#get-images)
  - [GET /videos](#get-videos)
  - [GET /news](#get-news)
  - [GET /spellcheck](#get-spellcheck)
  - [GET /analyze](#get-analyze)
  - [GET /inspect](#get-inspect)
  - [POST /goals](#post-goals)
  - [POST /goals/quick](#post-goalsquick)
  - [GET /goals/:goal_id](#get-goalsgoal_id)
  - [POST /goals/:goal_id/answers](#post-goalsgoal_idanswers)
  - [GET /goals/leaderboard](#get-goalsleaderboard)
- [Query Parameters](#query-parameters)
- [Response Structures](#response-structures)
- [Advanced Query Operators](#advanced-query-operators)
- [Pagination](#pagination)
- [Intent Classification](#intent-classification)
- [Constraint Extraction](#constraint-extraction)
- [Spell Correction](#spell-correction)
- [Geolocation & Local Queries](#geolocation--local-queries)
- [Scoring & Ranking](#scoring--ranking)
- [Caching](#caching)
- [Error Handling](#error-handling)
- [Performance & Stress Test Results](#performance--stress-test-results)
- [Goals API](#goals-api)
- [Examples](#examples)
- [Architecture](#architecture)
- [Getting Started (verified)](#getting-started-verified)

---

## Getting Started (verified)

The steps below were **executed on 2026-08-05** against the already-running dev stack (`localhost:4000`). The stack is brought up with `make dev-up` (see `Makefile` → builds `services/docker-compose.dev.yml`). All example responses are real and traceable to `docs/_generated/api-transcript.md`.

**1. Health check**
```bash
curl -s http://localhost:4000/health
# → OK   (HTTP 200)
```

**2. Root identifier**
```bash
curl -s http://localhost:4000/
# → IntentForge-v2 Gateway   (HTTP 200, text/plain)
```

**3. First real search**
```bash
curl -s "http://localhost:4000/search?q=rust+async+web+framework" | head -c 400
# → {"category":"informational","confidence":0.6,"constraints":["+async","+rust","+web"], ...}
#    (HTTP 200, ~4.9 s cold; ~15 ms on repeat within the 5-min cache)
```

**4. Reading the response**
- `intent` — the detailed subtype (`technical`, `informational`, `comparison`, `fresh`, `navigational`, `how-to`, `local`, `transactional`).
- `category` — the coarse bucket (`navigational` / `informational` / `transactional`).
- `confidence` — a real float (~0.3–0.9); **not a fixed `0.75`**.
- `distribution` — full intent-probability breakdown across all 8 subtypes.
- `results[]` — ranked `MergedResult` objects (see below).
- `results_before_filter` / `results_after_filter` / `total` — counts for constraint diagnostics.

**5. Intent classes (verified live)**
| Query | `intent` | `confidence` (observed) |
|-------|----------|------------------------|
| `what is quantum computing` | informational | 0.31 |
| `rust async web framework` | technical | 0.60 |
| `react vs vue vs angular` | comparison | 0.90 |
| `latest AI news today` | fresh | 0.70 |
| `python docs` | technical | 0.60 |
| `buy domain name` | transactional | (see block 11) |

**6. Error handling (verified live)**
```bash
curl -s -o /dev/null -w "%{http_code}" "http://localhost:4000/search?q="
# → 400  (body: {"error":"empty_query","message":"Query parameter 'q' is empty",...})
curl -s -o /dev/null -w "%{http_code}" "http://localhost:4000/search?q=r"
# → 400  (body: {"error":"invalid_query","message":"Query has no retrievable content (stopword-only or single character)",...})
# Note: a protected single char like `go` or `rust` is EXEMPT and returns 200.
```

---

## Endpoints

### `GET /`

Returns a plain-text identifier string.

**Response**
```
200 OK
Content-Type: text/plain

IntentForge-v2 Gateway
```

---

### `GET /health`

Health check endpoint. Returns `"OK"` when the gateway is running.

**Response**
```
200 OK
Content-Type: text/plain

OK
```

---

### `GET /search`

Full search endpoint. Queries multiple backends (SearXNG via VPN, local index) in parallel, classifies intent, extracts constraints, auto-corrects spelling, deduplicates, scores, and ranks results.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                          |
|-----------|--------|----------|---------|--------------------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded)           |
| `limit`   | int    | no       | `24`    | Max results to return (pagination)   |
| `offset`  | int    | no       | `0`     | Result offset (pagination)           |
| `count`   | int    | no       | `24`    | Alias for `limit`                    |
| `n`       | int    | no       | `24`    | Alias for `limit`                    |

**Response** `200 OK`

> **Verified shape (this session):** a successful `/search` returns these top-level keys (observed on every live response):
> `query`, `intent`, `category`, `confidence`, `constraints`, `structured_constraints`, `expanded_queries`, `distribution`, `results`, `results_before_filter`, `results_after_filter`, `total`, `limit`, `offset`, `has_more`.
> Optionally present: `applied_constraints` (when operators/negations are applied), `spell_corrected_query` (when a correction fired), `query_quality` (only on `low`/`junk` queries), `deep_result`, `price_verified` (transactional).
> `geo_location`, `warnings`, `ignored_constraints` were **absent** from all observed successful responses (declared-but-omitted `None` fields).
> **`confidence` is a real float in ~0.30–0.90**, not always `0.75` — the value depends on the query and the intent engine.

Example (real response truncated; full body in `docs/_generated/api-transcript.md` block 12 — `python web framework not django`):

```json
{
  "query": "python web framework not django",
  "intent": "technical",
  "category": "informational",
  "confidence": 0.60,
  "constraints": ["+python", "+web", "-django"],
  "applied_constraints": ["not:django"],
  "structured_constraints": {
    "positive": ["python", "web"],
    "negative": ["django"],
    "entities": [],
    "language": null,
    "file_types": [],
    "sites": [],
    "phrases": [],
    "intitle": [],
    "inurl": [],
    "intext": [],
    "related": []
  },
  "distribution": {
    "navigational": 0.39,
    "informational": 0.16,
    "technical": 0.08,
    "how-to": 0.11,
    "comparison": 0.03,
    "fresh": 0.04,
    "transactional": 0.16,
    "local": 0.03
  },
  "results": [
    {
      "url": "https://www.infoworld.com/article/2338670/3-python-web-frameworks-for-beautiful-front-ends.html",
      "title": "3 Python web frameworks for beautiful front ends - InfoWorld",
      "content": "We'll look at three Python web frameworks that follow this paradigm...",
      "score": 1.0,
      "authority": 1.0,
      "quality": 1.0,
      "is_local": false,
      "sources": ["duckduckgo"]
    }
  ],
  "results_before_filter": 25,
  "results_after_filter": 25,
  "total": 25,
  "limit": 24,
  "offset": 0,
  "has_more": true
}
```

> Note: `expanded_queries` and `distribution` are present on every response; they were omitted from the abridged example above for brevity. See block 12 in the transcript for the full body.

**Status Codes**

| Code | `error` | Meaning |
|------|---------|---------|
| 200  | — | Success (may return non-empty `results`) |
| 400  | `empty_query` | `q` missing, empty, whitespace-only, or containing no alphabetic character |
| 400  | `invalid_query` | Stopword-only / single non-protected character, or gibberish (`query_quality: "junk"`) |

All error bodies are JSON (see Error Handling). Verified live: blocks 18–25 in `docs/_generated/api-transcript.md`.

---

### `GET /search/fast`

Fast search endpoint. Returns results from the local crawl index only — no SearXNG, no intent analysis, no constraint extraction. Designed for instant feedback (~100ms) while `/search` runs in parallel.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |
| `limit`   | int    | no       | `24`    | Max results to return      |

**Response** `200 OK`

> **Verified (this session):** the real body is `{ "count": <int>, "results": [ ... ], "source": <str> }`. The top-level `source` field IS present (observed value `"local"` on this instance); each result also carries `sources: ["local"]` internally. `count` reflects the number returned (default 10 in this run; `limit` is accepted but the local fast path returned 10).

```json
{
  "count": 10,
  "source": "local",
  "results": [
    {
      "url": "https://rustwebframework.org/",
      "title": "Rust Web Framework",
      "content": "Rust Web Framework GitHub Getting started ...",
      "score": 1.0,
      "authority": 0.70,
      "is_local": true,
      "quality": 1.0,
      "sources": ["local"]
    }
  ]
}
```

Raw observed body: `docs/_generated/api-transcript.md` block 13.

**Notes**
- No intent classification, constraint extraction, or spell correction.
- No pagination metadata (`total`, `has_more`, etc.) — only returns a snapshot.
- Results come from the local crawl index only.
- Useful for frontend: call `/search/fast` + `/search` in parallel for instant + full results.

---

### `GET /images`

Image search via SearXNG (`categories=images`). Returns a flat list with image + page metadata.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK` — top-level `{ count, query, results[] }`. Each result (verified live, 2026-08-05):

```json
{
  "count": 32,
  "query": "rust programming",
  "results": [
    {
      "title": "Getting started - Rust Programming Language",
      "url": "https://rust-lang.org/learn/get-started/",
      "image_url": "https://www.rust-lang.org/static/images/rust-social-wide.jpg",
      "thumbnail_url": "https://ts2.mm.bing.net/th?id=OIP.W8KBrJgmsIlYtn24AhHfSQHaDt&pid=15.1",
      "description": "Getting started - Rust Programming Language",
      "source": "bing images",
      "score": 0.9000000357627869
    }
  ]
}
```

> **Verified fields:** `title`, `url`, `image_url`, `thumbnail_url`, `description`, `source`, `score`. Note the image endpoint returns **`image_url`** (full image) and **`thumbnail_url`** — not the `thumbnail` field used by `/videos`. Full raw body: `docs/_generated/_round_v2_raw.md` block `### IMAGES rust programming`.

---

### `GET /videos`

Video search via SearXNG (`categories=videos`). Returns a flat list with `thumbnail` (not `thumbnail_url`) and `video_id`.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK` — top-level `{ count, query, results[] }`. Each result (verified live, 2026-08-05):

```json
{
  "count": 31,
  "query": "rust tutorial",
  "results": [
    {
      "title": "Learn Rust Programming - Complete Course 🦀",
      "url": "https://www.youtube.com/watch?v=BpPEoZW5IiY",
      "description": "1.2M views - Jun 8, 2023 - YouTube - freeCodeCamp.org",
      "thumbnail": "https://th.bing.com/th/id/OVP.X9INETUn2tEG8KJL2Wrl3QHgFo?w=243&h=136&c=7&rs=1&qlt=70&o=7&pid=2.1&rm=3",
      "video_id": "",
      "source": "bing videos",
      "score": 0.6499999761581421
    }
  ]
}
```

> **Verified fields:** `title`, `url`, `description`, `thumbnail`, `video_id` (observed empty in this run), `source`, `score`. Note `thumbnail` here vs `thumbnail_url` in `/images`. `video_id` was empty on all observed results. Full raw body: `docs/_generated/_round_v2_raw.md` block `### VIDEOS rust tutorial`.

---

### `GET /news`

News search via SearXNG (`categories=news`). Returns a flat list with `published_at`.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK` — top-level `{ count, query, results[] }`. Each result (verified live, 2026-08-05):

```json
{
  "count": 39,
  "query": "artificial intelligence",
  "results": [
    {
      "title": "As computer science enrollments drop, artificial intelligence classes fill up",
      "url": "https://www.msn.com/en-us/money/careersandeducation/as-computer-science-enrollments-drop-artificial-intelligence-classes-fill-up/ar-AA29jdXb",
      "description": "Hiring for entry-level software developers has slowed, and college enrollment in computer science is declining ...",
      "published_at": "",
      "source": "bing news",
      "score": 0.800000011920929
    }
  ]
}
```

> **Verified fields:** `title`, `url`, `description`, `published_at` (observed empty string `""` on most bing-news items; hackernews items carried ISO timestamps like `"2019-11-13T23:17:23"`), `source` (`"bing news"` / `"hackernews"`), `score`. Full raw body: `docs/_generated/_round_v2_raw.md` block `### NEWS artificial intelligence`.

---

### `GET /spellcheck`

Spelling-correction preview. Exposes the engine's in-process SymSpell + LinSpell index (the same one `/search` uses to auto-correct) as a standalone "did you mean?" service, so a client can warn the user *before* issuing a search. No LLM, no network, no extra indexing — it reads the dictionary that is already built at gateway startup.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | The query/phrase to check  |

**Response** `200 OK` — top-level `{ query, corrected, changed, corrections[] }`:

```json
{
  "query": "pythn programing langauge",
  "corrected": "python programming language",
  "changed": true,
  "corrections": [
    { "original": "pythn", "suggestion": "python", "in_dictionary": false },
    { "original": "programing", "suggestion": "programming", "in_dictionary": true },
    { "original": "langauge", "suggestion": "language", "in_dictionary": false }
  ]
}
```

> **Verified (this round, 2026-08-09):** a typo string returns `changed: true` with a `correction` per changed token. Protected brands/tech terms (`openai`, `rust`, `kubernetes`, …) are never "corrected" — `"openai rust tutorial"` returns `changed: false` and an empty `corrections` array. URL tokens, code tokens (`.` `/` `@` `#` `$` or containing a digit), and very short words (< 4 chars) are skipped by the corrector and **omitted entirely from the `corrections` array** — the endpoint only lists tokens it actually proposed fixing, so the client never flags skipped tokens as typos. They are still preserved verbatim in the whole-query `corrected` string. The `corrected` string matches what `/search` runs for the same query (verified 2026-08-09: `/spellcheck?q=pythn+programing+langauge` → `corrected: "python programming language"`, and `/search?q=pythn+programing+langauge` returns `"query":"python programming language"`). Example: `/spellcheck?q=pythn+kubernetes.io` returns `changed:true` with `corrections` containing **only** `pythn→python` (the `kubernetes.io` URL token is skipped and absent from `corrections`, but retained in `corrected`).

**Empty query** returns `400` with the standard error envelope (same shape as `/search`):

```json
{ "error": "empty_query", "message": "Query parameter 'q' is empty", "results": [], "query": "", "corrected": "", "changed": false, "corrections": [] }
```

**Notes**
- Pure function of the query + the built dictionary; no per-query tuned constants, no domain allow/deny lists.
- The endpoint is additive — it does not change `/search` ranking, negation gating, or calibration. It is a read-only preview of the existing correction path.

```bash
# See what the engine would correct in a query
curl "http://localhost:4000/spellcheck?q=pythn+programing+langauge"
# → {"query":"pythn programing langauge","corrected":"python programming language","changed":true,"corrections":[...]}
```

---

### `GET /analyze`

Engine-introspection endpoint for the query negation / `is_real_exclusion` gate (DEFECT A transparency). Exposes the same extraction + gating functions `/search` runs (`extract_query_negative_terms_with_dropped` + `is_real_exclusion`) as inspectable JSON, so a client can see **why** a negative term was kept as a search exclusion, declined as an unrecognized entity, or dropped as a HOW-not-WHAT manner qualifier. No LLM, no network — it is a pure function of the query over the already-loaded signal state.

This is the read-only companion to `/spellcheck`: it does **not** change `/search` ranking, negation gating, or calibration. It only makes the engine's negation reasoning *legible*. (The actual DEFECT A ranking behavior — a `without oven` query still ranking "Cook Salmon IN the Oven" at the top — is **not** fixed by this endpoint; `/analyze` surfaces the cause so a future fix is observable and testable.)

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | The query/phrase to analyze|

**Response** `200 OK` — top-level `{ query, contrastive_framing, exclusions[], declined[], manner_qualifiers[], decisions[] }`:

- `exclusions` — terms kept as real search exclusions (recognized entity or contrastive framing).
- `declined` — negation candidates the gate declined (neither a recognized entity nor in contrastive framing — kept to avoid penalizing unrelated topical words).
- `manner_qualifiers` — HOW-not-WHAT terms (e.g. `without soap`) the engine deliberately does NOT exclude.
- `contrastive_framing` — `true` when the query reads as a compare/versus/alternative/instead-of/double-negation expression.
- `decisions[]` — one per candidate term, each `{ term, decision, reason }`, covering every negation candidate exactly once (never silently dropped).

```json
{
  "query": "javascript not java not typescript",
  "contrastive_framing": true,
  "exclusions": ["java", "typescript"],
  "declined": [],
  "manner_qualifiers": [],
  "decisions": [
    { "term": "java", "decision": "exclusion", "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)" },
    { "term": "typescript", "decision": "exclusion", "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)" }
  ]
}
```

> **Verified (this round, 2026-08-10):** all examples below were executed against the live dev stack at `localhost:4000` (gateway rebuilt at commit `c31cea0`). Contrastive `not X` → `exclusions` with `contrastive_framing: true` (e.g. `javascript not java not typescript` → `["java","typescript"]`). A manner phrase `without soap` → `manner_qualifiers` with empty `exclusions`/`declined` (`how to clean a cast iron skillet without soap after cooking eggs` → `["soap"]`). A generic `not spicy` with no contrastive framing → `declined` (`best spicy ramen not spicy` → `["spicy"]`). The DEFECT A trigger `best way to cook salmon without an oven` → `manner_qualifiers: ["oven"]` — the root cause is now *visible* instead of silent. The transparency invariant holds: every negation candidate appears in exactly one bucket.

**Empty query** returns `400` with the standard error envelope (same shape as `/search` and `/spellcheck`):

```json
{ "error": "empty_query", "message": "Query parameter 'q' is empty", "query": "", "exclusions": [], "declined": [], "manner_qualifiers": [] }
```

**Notes**
- Pure function of the query + the loaded signal state; no per-query tuned constants, no domain allow/deny lists, no magic constants.
- The endpoint is additive — it does not change `/search` ranking, negation gating, or calibration. It is a read-only preview of the existing negation path.
- A test (`analyze_endpoint_exposes_negation_decisions`) locks the bucket-routing behavior; the gateway suite is 80/80 passing.

```bash
# Inspect how the engine gated a query's negation terms
curl "http://localhost:4000/analyze?q=javascript+not+java+not+typescript"
# → {"query":"javascript not java not typescript","contrastive_framing":true,"exclusions":["java","typescript"],"declined":[],"manner_qualifiers":[],"decisions":[...]}

# See why a "without X" manner phrase was NOT turned into an exclusion
curl "http://localhost:4000/analyze?q=how+to+clean+a+cast+iron+skillet+without+soap"
# → {"manner_qualifiers":["soap"],"exclusions":[],"declined":[],"contrastive_framing":false,...}
# ```

---

### `GET /inspect`

Unified pre-search introspection. Generalizes the `/analyze` (negation) and
`/spellcheck` (spelling) transparency endpoints into **one** additive,
zero-side-effect payload that mirrors the *entire* `/search` reasoning pipeline
a client can inspect **before** issuing a search:

1. **spelling** — same `spellcheck_query` fn `/search` pre-corrects with.
2. **negation** — the `exclusions` / `declined` / `manner_qualifiers` split + per-term `decisions[]` (identical to `/analyze`).
3. **intent** — the pure no-network fallback classifier (`fallback_intent`) + coarse `category`.
4. **constraints** — the gateway's own operator parser (`extract_gateway_constraints`) + the `applied_constraints` shape `/search` reports.
5. **recency** — `derive_recency_window`, so the client can see whether a "latest"/"this week" phrase would inject a date window.
6. **quality** — `query_quality_flag` (junk/low/normal), the same gate that decides graceful degradation.

It is the read-only companion to `/analyze` and `/spellcheck`: it does **not**
change `/search` ranking, negation gating, calibration, or fetch anything. It
reuses the *exact* functions `/search` calls, so the preview always matches real
engine behavior. No per-query strings, no domain allow/deny lists, no magic
constants.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | The query/phrase to inspect|

**Response** `200 OK` — top-level `{ query, spelling, negation, intent, constraints, recency, quality }`:

```json
{
  "query": "python web framework not django",
  "spelling": { "corrected": "python web framework not django", "changed": false, "corrections": [] },
  "negation": {
    "contrastive_framing": false,
    "exclusions": ["django"],
    "declined": [],
    "manner_qualifiers": [],
    "decisions": [
      { "term": "django", "decision": "exclusion", "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)" }
    ]
  },
  "intent": { "intent": "informational", "category": "informational", "confidence": 0.30000001192092896 },
  "constraints": {
    "applied_constraints": ["lang:en"],
    "structured": { "entities": [], "file_types": [], "intext": [], "intitle": [], "inurl": [], "language": "en", "negative": [], "phrases": [], "positive": [], "related": [], "sites": [] }
  },
  "recency": { "window": null, "phrase_detected": false },
  "quality": { "flag": "", "valid_ratio": 1.0 }
}
```

> **Verified (this round, 2026-08-10T1401Z):** every claim below was executed
> against the live dev stack at `localhost:4000` (gateway rebuilt at the round's
> feature commit `ca4362c`/`ea11acd`). All 6 cases returned `200` unless noted. The
> endpoint reuses the same pure functions `/search` runs (no network, deterministic).
>
> | Query | What was observed |
> |-------|-------------------|
> | `python web framework not django` | `negation.exclusions:["django"]`, but `contrastive_framing:false` (plain `not X` without compare/versus framing); `constraints.applied_constraints:["lang:en"]` (plain words are not operator-extracted, and `django` is a negation *exclusion*, not a `+django` positive); `intent: informational`, `confidence: 0.3`. |
> | `javascript not java not typescript` | `contrastive_framing:true`, `negation.exclusions:["java","typescript"]`, `declined:[]`, `manner_qualifiers:[]`. |
> | `best way to cook salmon without an oven` | `negation.manner_qualifiers:["oven"]`, `exclusions:[]`, `declined:[]` — the manner HOW-not-WHAT term is correctly NOT excluded. |
> | `latest AI news this week` | `recency.phrase_detected:true`, `recency.window:{"after":"2026-08-03","before":"2026-08-10"}`; `applied_constraints` also gained `after:2026-08-03`, `before:2026-08-10`. |
> | `rust async web framework site:github.com filetype:rs` | `applied_constraints:["lang:en","site:github.com","filetype:rs"]`; `structured.sites:["github.com"]`, `structured.file_types:["rs"]`. |
> | `pythn programing langauge` | `spelling.changed:true`, `spelling.corrected:"python programming language"`, 3 `corrections` (`pythn→python`, `programing→programming`, `langauge→language`). |
> | `openai rust tutorial` | `spelling.changed:false`, `corrections:[]` — protected brand terms are never "corrected" (shared protected-term set, no hardcoded allow list). |
>
> The transparency invariant holds: every negation candidate appears in exactly one
> negation bucket (`exclusions` / `declined` / `manner_qualifiers`).

**Empty query** returns `400` with the standard error envelope (same shape as `/search`, `/spellcheck`, `/analyze`):

```json
{ "error": "empty_query", "message": "Query parameter 'q' is empty", "query": "", "spelling": {"corrected":"","changed":false,"corrections":[]}, "negation": {"exclusions":[],"declined":[],"manner_qualifiers":[],"contrastive_framing":false,"decisions":[]}, "intent": {"intent":"","category":"","confidence":0.0}, "constraints": {"structured":{}, "applied_constraints":[]}, "recency": {"window":null,"phrase_detected":false}, "quality": {"flag":"low","valid_ratio":0.0} }
```

**Notes**
- Pure function of the query + the loaded signal state; no per-query tuned constants, no domain allow/deny lists, no magic constants.
- The endpoint is additive — it does not change `/search` ranking, negation gating, or calibration. It is a read-only preview of the existing engine path, generalized to the full pipeline.
- The feature commit `ca4362c` added 6 inspect tests (`inspect_endpoint_shape_matches_docs` + 5 behavior tests) locking the shape and the negation/constraints/recency/spelling/quality contracts; this docs pass adds a 7th (`inspect_pure_fn_handles_empty_input_safely`) covering the pure-fn path behind the documented `400` empty_query envelope. All 7 are pure-fn (`build_inspect`) and run via `cargo test -p gateway` on the GitHub Actions runner (no live server needed). CI verified this round: 81 tests passed, 0 failed.

```bash
# See the full /search reasoning surface for a query in one call
curl "http://localhost:4000/inspect?q=python+web+framework+not+django"
# → {"query":"python web framework not django","spelling":{...},"negation":{...},"intent":{...},"constraints":{...},"recency":{...},"quality":{...}}

# A fresh-phrase query: see the recency window /search would apply
curl "http://localhost:4000/inspect?q=latest+ai+news+this+week"
# → {"recency":{"window":{"after":"...","before":"..."},"phrase_detected":true},...}
```

---

## Query Parameters

All search endpoints accept the following standard parameters:

| Parameter | Type   | Required | Default | Endpoints       | Description |
|-----------|--------|----------|---------|-----------------|-------------|
| `q`       | string | yes      | —       | all             | The search query. URL-encode special characters (e.g. `<` → `%3C`, `>` → `%3E`). |
| `limit`   | int    | no       | `24`    | `/search`       | Maximum number of results to return. |
| `offset`  | int    | no       | `0`     | `/search`       | Number of results to skip (for pagination). |
| `count`   | int    | no       | `24`    | `/search`       | Alias for `limit`. |
| `n`       | int    | no       | `24`    | `/search`       | Alias for `limit`. |

---

## Advanced Query Operators

The `/search` endpoint parses a rich set of operators directly from the query string. These are extracted into `applied_constraints` and `structured_constraints` in the response.

| Operator | Example | Description | Extracted As |
|----------|---------|-------------|-------------|
| `site:` | `site:arxiv.org transformers` | Restrict results to a specific domain | `sites: ["arxiv.org"]` |
| `filetype:` | `react filetype:pdf` | Filter by file extension | `file_types: ["pdf"]` |
| `intitle:` | `intitle:rust web framework` | Require term in page title | `intitle: ["rust"]` |
| `inurl:` | `inurl:api python` | Require term in URL | `inurl: ["api"]` |
| `intext:` | `intext:benchmark` | Require term in page body | `intext: ["benchmark"]` |
| `after:` | `after:2026-01-01` | Only results published after date | `after_date: "2026-01-01"` |
| `before:` | `before:2025-01-01` | Only results published before date | `before_date: "2025-01-01"` |
| `price:<` or `price:>` | `price:%3C50 headphones` | Filter by price range (URL-encode `<` to `%3C`, `>` to `%3E`) | `price_lt: 50.0` or `price_gt: 50.0` |
| `price_min:` | `price_min:10` | Minimum price | `price_min: 10.0` |
| `price_max:` | `price_max:100` | Maximum price | `price_max: 100.0` |
| `lang:` | `lang:en` | Language filter (ISO code). Auto-applied as `lang:en` for English queries even without explicit use. | `language: "en"` |
| `related:` | `related:python.org` | Find related pages | `related: ["python.org"]` |
| `NOT:` | `python web framework NOT:flask` | **Hard-exclude** any result mentioning the term (single token, or quote a phrase: `NOT:"visual studio code"`). Unlike the natural-language `not X` (a soft penalty gated on entity/contrastive recognition that *declines* unrecognized terms like `flask`), `NOT:` is an **unconditional structural exclude** — any result whose title/content/url contains the term is dropped. General escape hatch for the DEFECT-A class; surfaced in `applied_constraints` as `not:<term>`. | `hard_exclusions: ["flask"]` |

> **Verification status (this session, 2026-08-05):** the following operators were exercised live and confirmed to populate `structured_constraints` + `applied_constraints`: `site:` (block 32), `filetype:` (block 33), `intitle:` (block 34), `inurl:` (block 35), `after:` (block 36), and the natural-language negation `not X` (block 12 → `negative:["django"]`, `applied_constraints:["not:django"]`). The `fresh`/`today` intent auto-produced `after_date`+`before_date` = today (block 9). The **`price:` / `price_min:` / `price_max:`** operators and `lang:` / `intext:` / `related:` were **NOT exercised this session** — treat their extraction as unverified. 

**Multiple operators** can be combined: `site:arxiv.org site:wikipedia.org quantum computing` → both sites are applied (observed).

### The `NOT:` hard-exclusion operator (verified live 2026-08-11)

`NOT:` is an **explicit, unconditional structural exclude** — the general, non-hardcoded escape hatch for the DEFECT-A class of limitations. It is parsed by the gateway's own operator extractor (`extract_gateway_constraints`), independent of the intent engine's entity/contrastive recognition.

- **Syntax:** `NOT:<term>` for a single token, or `NOT:"<phrase>"` for a multi-word term (up to 4 words). Terms are lowercased for case-insensitive matching.
- **Behaviour:** any result whose **title, content, or URL contains the term** (substring match) is hard-dropped by `should_filter_by_constraints`. This fires *before* the soft-penalty ranking path, so it removes the page entirely rather than demoting it.
- **Surfaced as:** `structured_constraints.hard_exclusions: ["<term>"]` and `applied_constraints: ["not:<term>"]` on `/search` and `/inspect`.
- **Never forwarded upstream:** `preprocess_searxng_query` strips `NOT:` so SearXNG does not treat `<term>` as a literal search word and re-surface it.
- **Alt-listing exemption (by design):** a comparison / "alternatives" page that merely *mentions* the excluded term in a referential context (alt-score > 0.3) is **kept**, exactly like the `site:` / `filetype:` negative gates and the committed test `not_operator_keeps_alt_listing_page`. So `"Best Flask Alternatives"` survives `NOT:flask`.
- **Contrast with bare `not X`:** the natural-language `not X` is a *soft* topical penalty gated on entity/contrastive recognition (`is_real_exclusion`) — an unrecognized tech term like `flask` is *declined*, leaving flask pages in the results (the DEFECT-A limitation). `NOT:flask` always excludes it. Use `NOT:` when you know a term is off-topic and don't want to rely on entity recognition.

**Verified live example** (gateway rebuilt at commit `c3ee023` + the `/search` integration fix from this round; `localhost:4000`, cold cache):

```bash
curl "http://localhost:4000/search?q=python%20web%20framework%20NOT:flask"
# → 200; top-level:
#   "query": "python web framework NOT:flask"
#   "applied_constraints": ["not:flask"]
#   "structured_constraints": { ... "hard_exclusions": ["flask"], "negative": [], ... }
#   "results": [ ... ]   # non-exempt pages whose title/url mention "flask" are dropped;
#                        # comparison pages that only reference flask (e.g.
#                        # "Which Is the Best Python Web Framework: Django, Flask, or FastAPI?")
#                        # are retained by the alt-listing exemption above
```

> **Honest note on the original feature commit:** commit `c3ee023` added the `NOT:` parser, the hard-drop gate, the upstream-strip, and the `/inspect` + `applied_constraints` reporting code, but the `/search` path never copied `gateway_extracted.hard_exclusions` into the merged `structured_constraints`. As a result `/inspect` reported `hard_exclusions: ["flask"]` while `/search` silently dropped the operator (`applied_constraints: null`, flask pages retained) — the feature's "verified cold" claim was only true at the unit/`/inspect` level, not in integrated `/search`. A follow-up fix (this round) copies `hard_exclusions` into the merged constraints so `/search` now hard-drops and reports `NOT:` as documented above. The regression test `not_operator_reported_in_inspect_applied_constraints` locks the reporting contract.

**Natural language date ranges** are automatically converted:
- `"past 7 days"`, `"last week"`, `"this month"` → `after:YYYY-MM-DD before:YYYY-MM-DD`
- `"yesterday"`, `"today"`, `"recent"`, `"latest"`, `"fresh"`

---

## Pagination

The `/search` endpoint supports cursor-free pagination via `limit` and `offset` parameters.

**Response fields:**

| Field | Type | Description |
|-------|------|-------------|
| `results` | array | The paginated slice of results (length ≤ `limit`) |
| `total` | int | Total number of results available after filtering (NOT the length of `results`) |
| `limit` | int | The effective limit applied (may differ from request if capped) |
| `offset` | int | The effective offset applied |
| `has_more` | bool | Whether more results exist beyond the current page (`offset + limit < total`) |
| `results_before_filter` | int | Result count before any constraint filtering |
| `results_after_filter` | int | Result count after all constraint filtering (identical to `total`) |

**Example pagination flow:**

```bash
# Page 1
curl "http://localhost:4000/search?q=python&limit=10&offset=0"
# → has_more: true, total: 47

# Page 2
curl "http://localhost:4000/search?q=python&limit=10&offset=10"
# → has_more: true, total: 47

# Page 5 (last page)
curl "http://localhost:4000/search?q=python&limit=10&offset=40"
# → has_more: false, total: 47
```

---

## Response Structures

### `UnifiedResponse` (from `/search`)

| Field | Type | Always Present | Description |
|-------|------|:---:|-------------|
| `query` | string | ✓ | The final query used (after spell correction, if any) |
| `intent` | string | — | Detailed intent subtype (see [Intent Classification](#intent-classification)) |
| `category` | string | — | Standard search category (`navigational`, `informational`, `transactional`) |
| `confidence` | float | — | Intent classification confidence (0.0–1.0) |
| `constraints` | string[] | ✓ | Extracted constraints in `["+term", "-term"]` format |
| `structured_constraints` | object | ✓ | Parsed constraints with typed fields |
| `expanded_queries` | string[] | ✓ | Query expansions / reformulations used for broader search |
| `distribution` | object | — | Intent distribution breakdown across all categories |
| `results` | array | ✓ | Ranked search results (see `MergedResult` below) |
| `results_before_filter` | int | — | Count before constraint filtering |
| `results_after_filter` | int | — | Count after constraint filtering |
| `total` | int | — | Total results after filtering (alias for `results_after_filter`) |
| `limit` | int | — | Effective pagination limit |
| `offset` | int | — | Effective pagination offset |
| `has_more` | bool | — | Whether more pages exist |
| `applied_constraints` | string[] | — | Which operators were understood and enforced |
| `ignored_constraints` | string[] | — | Which operators could not take effect (e.g. date range with no parseable dates in results) |
| `warnings` | string[] | — | Human-readable diagnostics (empty result set, upstream flakiness) |
| `spell_corrected_query` | string | — | Original query before spell correction (only present if correction was applied) |
| `geo_location` | object | — | Client IP geolocation or query-derived location (see [Geolocation](#geolocation--local-queries)) |
| `query_quality` | string | — | Quality rating of the query: `"high"`, `"medium"`, `"low"`, or `"noise"` |
| `error` | string | — | Error code (`"empty_query"`, `"upstream_unavailable"`, etc.) |
| `message` | string | — | Human-readable error or status message |

### `MergedResult` (from `/search`)

The unified result type for the main search endpoint. Results can come from local index, web search engines, or both. When a URL appears in multiple sources, sources are merged and a consensus boost is applied.

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | Result URL |
| `title` | string | Page title |
| `content` | string | Snippet or description |
| `score` | float | Relevance score (0.0–1.0) |
| `authority` | float | Domain authority score (0.0–1.0) |
| `sources` | string[] | Which backends returned this result (e.g. `["bing", "brave", "local"]`) |
| `is_local` | bool | Whether this result came from the local crawl index |
| `published_date` | string | ISO 8601 date string (if available from upstream) |

### `StructuredConstraints` (from `/search`)

| Field | Type | Description |
|-------|------|-------------|
| `positive` | string[] | Positive constraint terms (`+python`, `+web`) |
| `negative` | string[] | Negative constraint terms (`-django`) |
| `entities` | object[] | Semantic query entities with roles (`Target`, `Reference`, `Comparison`, `Exclusion`) |
| `language` | string | Detected programming language or natural language (`null` if none detected) |
| `file_types` | string[] | File type restrictions (`["pdf", "doc"]`) |
| `sites` | string[] | Site restrictions (`["arxiv.org", "wikipedia.org"]`) |
| `phrases` | string[] | Exact phrase requirements |
| `intitle` | string[] | Terms required in page title |
| `inurl` | string[] | Terms required in URL |
| `intext` | string[] | Terms required in page body text |
| `related` | string[] | Related-page queries |
| `after_date` | string | Date lower bound (YYYY-MM-DD). **Observed** (e.g. `after:2024-01-01` → `"after_date":"2024-01-01"`; `latest AI news today` → both `after_date` and `before_date` = today). |
| `before_date` | string | Date upper bound (YYYY-MM-DD). **Observed** (see above). |
| `price_min` | float | Minimum price. **Unverified this session** — the `price:<` / `price:>` / `price_min:` / `price_max:` operators were NOT exercised live; these field names are taken from source, not observed output. |
| `price_max` | float | Maximum price. **Unverified this session.** |
| `price_lt` | float | Upper price bound from `<` operator. **Unverified this session.** |
| `price_gt` | float | Lower price bound from `>` operator. **Unverified this session.** |

---

## Intent Classification

The intent engine classifies queries into detailed subtypes. The `category` field maps these to standard search categories.

| Intent | Category | Description | Example |
|--------|----------|-------------|---------|
| `navigational` | navigational | Looking for a specific site or page | "python docs", "github login" |
| `informational` | informational | General knowledge seeking | "what is quantum computing" |
| `technical` | informational | Developer/technical docs | "rust async web framework" |
| `how-to` | informational | Step-by-step instructions | "how to deploy docker" |
| `comparison` | informational | Comparing options | "react vs vue vs angular" |
| `fresh` | informational | Time-sensitive / news | "latest AI news today" |
| `local` | informational | Location-specific queries | "restaurants near me", "coffee shops in tokyo" |
| `transactional` | transactional | Purchase/action intent | "buy domain name" |

The `distribution` field provides a full breakdown across all intent types as float probabilities (summing to ~1.0), useful for threshold-based decision making.

---

## Constraint Extraction

The intent engine extracts positive and negative constraints from natural language queries.

**Negative constraint patterns:**
- `not X` → `"python web framework not django"` → negative: `["django"]`
- `without X` → `"text editor without vim"` → negative: `["vim"]`
- `except X` → `"javascript framework except react"` → negative: `["react"]`
- `besides X` → `"css framework besides bootstrap"` → negative: `["bootstrap"]`
- `excluding X` → `"database excluding mongodb"` → negative: `["mongodb"]`
- `no X` → `"linux distro no ubuntu"` → negative: `["ubuntu"]`
- `minus X` → `"frontend framework minus angular"` → negative: `["angular"]`
- `other than X` → `"programming language other than java"` → negative: `["java"]`
- `alternative to X` → `"search engine alternative to google"` → negative: `["google"]`
- `instead of X` → `"static site generator instead of jekyll"` → negative: `["jekyll"]`

**Positive constraints** are extracted from the remaining meaningful terms (excluding stop words and negation terms).

**Smart substring matching for negation:**
The engine prevents false-positive hits for negative constraints:
- `"not java"` does NOT filter out `"javascript"` results
- `"not go"` does NOT filter out `"google"` or `"django"` results
- Short terms (< 3 chars) use exact-match only
- Terms must dominate the token length (≥ 75%) for compound matches

**Synonym expansion for negative constraints:**
- `aws` → also matches `"amazon"`, `"amazon web services"`
- `gcp` → also matches `"google cloud"`, `"google"`
- `azure` → also matches `"microsoft"`, `"microsoft azure"`
- `vscode` → also matches `"vs code"`, `"visual studio code"`
- `google workspace` → also matches `"gsuite"`, `"google docs"`, etc.

**Inspecting the gate:** the same extraction + `is_real_exclusion` decision is exposed read-only by `GET /analyze` (see [endpoint reference](#get-analyze)). Use it to confirm whether a negative phrase was turned into a real exclusion, declined as an unrecognized entity, or dropped as a HOW-not-WHAT manner qualifier (e.g. `without soap`).

---

## Spell Correction

The API automatically detects and corrects spelling errors in queries using a two-stage SymSpell + LinSpell approach with an embedded 15,000+ word frequency dictionary.

**When a correction is applied:**
- The `query` field reflects the corrected query
- The `spell_corrected_query` field contains the original (uncorrected) query
- The `expanded_queries` array includes expansions of the corrected form

**Protection against false positives:**
- Protected brand/tech terms (`openai`, `kubernetes`, `podman`, etc.) are NEVER corrected
- Tech terms with unusual character bigrams (e.g. `"podman"`) are not English-ified
- Short words (< 4 chars; `MIN_CORRECT_LENGTH`) and known 3-letter terms are left unchanged
- URLs, code terms, and words with numbers/special characters are never touched
- Single-character substitutions between two natural-looking words are blocked (prevents `"ramen"` → `"raven"`)

**Examples:**
| Input | Corrected | Notes |
|-------|-----------|-------|
| `"python programing"` | `"python programming"` | Missing 'm' |
| `"rust progamming langauge"` | `"rust programming language"` | Double letter + vowel |
| `"openai"` | (unchanged) | Protected brand |
| `"kubernetes"` | (unchanged) | Protected brand |
| `"embaras"` | `"embarrass"` | Misspelling → correct form |

---

## Geolocation & Local Queries

The API supports two geolocation mechanisms:

### 1. IP-Derived Geolocation
When the GeoLite2 database is available, the client's IP address is used to approximate location and return regionally relevant results.

### 2. Query-Derived Location
If the query explicitly mentions a location from the [gazetteer](#location-gazetteer), it overrides the IP-derived location. This means a user in India searching `"restaurants in tokyo"` gets Japan-localized results.

**Response field:** `geo_location`
```json
{
  "geo_location": {
    "country_code": "JP",
    "country_name": "Japan",
    "region": null,
    "city": "tokyo",
    "postal_code": null,
    "latitude": null,
    "longitude": null,
    "time_zone": null
  }
}
```

### Location Gazetteer
The built-in gazetteer maps 70+ countries and 40+ major cities to ISO-3166 country codes. Whole-word matching prevents false positives (e.g. `"java"` the language never matches `"Japan"`).

**Supported countries:** US, GB, CA, AU, NZ, DE, FR, ES, IT, PT, NL, IE, SE, NO, DK, FI, PL, AT, CH, BE, RU, UA, TR, GR, JP, CN, KR, IN, SG, HK, BR, MX, AR, AE, SA, EG, IL, TH, VN, ID, MY, PH, ZA, NG, KE, CZ, HU, RO, HR, SI, and more.

**Supported cities:** Tokyo, London, Paris, Berlin, Madrid, Rome, Amsterdam, Dublin, Stockholm, Oslo, Copenhagen, Helsinki, Moscow, Kyiv, Istanbul, Athens, Beijing, Shanghai, Seoul, Delhi, Mumbai, Bangalore, Singapore, Sydney, Melbourne, Auckland, New York, San Francisco, Los Angeles, Chicago, Seattle, Boston, Austin, Toronto, Vancouver, Sao Paulo, Mexico City, Dubai, Cairo, Bangkok, Jakarta, Cape Town, Lagos, and more.

---

## Scoring & Ranking

Results are scored using a multi-signal ranking algorithm:

| Signal | Description | Range |
|--------|-------------|-------|
| **Base relevance** | Query-term overlap with title/content/URL | 0.0–1.0 |
| **Domain authority** | TLD trust, subdomain patterns, path signals | 0.0–1.0 |
| **Freshness decay** | Exponential decay based on URL date signals | 0.0–1.0 |
| **Intent boost** | URL/title structural signals matching intent | 1.0–2.5x multiplier |
| **Consensus boost** | Bonus when multiple engines return the same URL | +0.1 per source |
| **Content quality** | Shannon entropy + gibberish detection | 0.0–1.0 |
| **Constraint scoring** | Boost for positive constraint matches, penalty for negatives | 0.0–1.0 |

**Domain authority signals** (fully algorithmic, no hardcoded lists):
- `.edu` / `.gov` TLDs: +0.3
- `.org` / `.net` TLDs: +0.1
- `.ac.uk` academic TLDs: +0.3
- Documentation subdomains (`docs.`, `api.`, `dev.`): +0.25
- Documentation paths (`/docs/`, `/api/`, `/reference/`): +0.2
- Clean bare domains (2-part host): +0.1
- Deep descriptive paths with long segments: +0.2 – 0.35
- Code repo patterns (`/owner/repo/`): +0.1
- Spam/clickbait path patterns: -0.2
- Too many subdomains (≥ 5 parts): -0.1

**Freshness half-lives** (per intent):
| Intent | Half-Life |
|--------|-----------|
| `fresh` (news) | 6 hours |
| `how-to` / `informational` | 7 days |
| `comparison` | 14 days |
| `technical` / `transactional` | 30 days |
| `navigational` | 90 days |

**Alternative listing page detection:**
When a query uses negative constraints like `"not react not vue"`, pages titled "Top 10 React Alternatives" are detected using:
- Title comparison signals (`alternative`, `vs`, `best N`, `comparison`) — 70% weight
- URL path signals (`/alternatives/`, `/vs/`, `/compare/`) — 20% weight
- Content marker signals — 10% weight

These pages receive a negative-constraint exemption so they still rank well despite mentioning excluded terms.

---

## Caching

All endpoints cache responses by query. The cache key is the **trimmed, lowercased query** combined with a pagination key (`limit`/`count`/`n` and `offset`), so two requests for the same query with different `limit`/`offset` do **not** share a cached body (see `handle_search` cache-key logic).

> **Verified behavior (2026-08-05, dev instance `localhost:4000`):** The `/search` cache TTL is **5 minutes (300 s)**, not 30 minutes. Repeating an identical `/search?q=rust%20async%20web%20framework` request returned in **~15 ms** (vs ~4–5 s cold), confirming an in-memory cache. Source: `services/gateway/src/main.rs` cache-key builder + 5-min TTL comment at the cache-check block.

| Endpoint | Cache TTL (observed / documented) |
|----------|-----------------------------------|
| `/search` | **300 s (5 min)** — measured; docs previously stated 30 min |
| `/search/fast` | 300 s (5 min) — measured latency ~13 ms on repeat |
| `/images` | 300 s (5 min) |
| `/videos` | 300 s (5 min) |
| `/news` | 300 s (5 min) |

The cache is reset on the first request after expiry, not on a fixed interval. This means traffic spikes only see at most one cache-miss request per TTL window.

**Latency figures (measured, this session):**
- Cold (uncached) `/search`: ~3.8–4.9 s wall-clock (includes SearXNG-over-VPN + Tor fan-out, intent engine, indexer, ranking).
- Cached `/search` repeat: ~3–15 ms.
- `/search/fast`: ~13 ms (local index only).
- `/images`: ~2–4 s cold.
- `/news`: ~2.3 s cold.
- Error responses (400): ~3–24 ms.

---

## Error Handling

> **Verified (this session, 2026-08-05):** All error responses return **HTTP 400** with a JSON body. There are two distinct error codes, both observed live:

| Status | `error` code | `message` | When (verified) |
|--------|-------------|-----------|-----------------|
| `400` | `empty_query` | `Query parameter 'q' is empty` | `q` missing, empty, or whitespace-only (verified: `/search` and `/search?q=`) |
| `400` | `empty_query` | `Query must contain at least one alphabetic character` | `q` has no letters (e.g. `"123"`, `"]]["`) |
| `400` | `invalid_query` | `Query has no retrievable content (stopword-only or single character)` | Stopword-only (`the and or`) **or** a single non-protected character (`r`). A single **protected** term (`go`, `rust`, `c++`) is exempt and searches normally (verified). |
| `400` | `invalid_query` | `Query appears to be gibberish; no results returned` | Query flagged `junk` by the quality classifier (verified: `zxqw lkjasd qwe`, and non-Latin script `为什么天空是蓝色的`). Body includes `"query_quality":"junk"`. |

Error bodies always include the full `UnifiedResponse` envelope (with `null` for `intent`/`category`/`confidence`/`distribution`, empty `results: []`, and an empty `structured_constraints` object), plus `error`, `message`, and (for gibberish) `query_quality`. See `docs/_generated/api-transcript.md` blocks 18–25 for the exact raw bodies.

**Notes on the documented-but-not-observed cases:**
- The `200` + `{"error":"upstream_unavailable",...}` failure mode described below was **NOT triggered** in this session — all upstreams (SearXNG via VPN, SearXNG2 via Tor, local indexer) were healthy. Treat it as *aspirational/unverified* until reproduced. The API is designed never to return 5xx; internal errors are handled gracefully (partial results from whichever backends succeeded, or an empty `results` array with a diagnostic `message`).
- `warnings` and `geo_location` fields are declared in the `UnifiedResponse` struct but were **absent** from every successful `/search` response observed this session (they serialize as `None` and are omitted). Do not assume they are always present.

**Upstream failure diagnostics (documented, unverified this session):**
When all search engines fail, the response is documented to include:
- `error: "upstream_unavailable"`
- `message: "All upstream search engines timed out or failed to respond. This is a temporary upstream/connectivity issue, not a genuine zero-hit. Please retry."`
- `warnings: ["No web results were returned by the upstream search engines for this query."]`

---

## Performance & Stress Test Results

Results from live testing of the development instance (`localhost:4000`), measured **this session (2026-08-05)**:

| Metric | Value (measured) |
|--------|-------|
| Cold `/search` (cache miss) | ~3.8–4.9 s wall-clock (includes SearXNG-over-VPN + Tor fan-out, intent engine, indexer, ranking) |
| Cached `/search` repeat | ~3–15 ms (5-min TTL; verified by repeating an identical query) |
| `/search/fast` (local index) | ~13 ms |
| `/images` cold | ~2–4 s |
| `/news` cold | ~2.3 s |
| `/videos` cold | ~3–5 s |
| Error responses (400) | ~3–24 ms |
| Concurrent requests | Not re-measured this session (see notes) |

**Notes / caveats:**
- The earlier doc claimed a ~10.1 s cold start and 2.8–5.7 ms "warm" latency. This session measured **~3.8–4.9 s cold** and **~3–15 ms cached** — the upstream fan-out dominates; absolute numbers vary with VPN/Tor egress and upstream engine load.
- Concurrent / stress testing was **not re-run this session**; the 5-simultaneous-200-OK figure from prior docs is unverified here.
- Cache-hit ratio approaches 100% for repeated queries within the 5-min TTL window.

**Sources of latency breakdown (uncached):**
- SearXNG backend queries: 2–5s (parallel, includes VPN/Tor routing)
- Intent classification: < 10ms
- Deduplication + scoring: < 5ms
- Spell correction: < 1ms

**Recommended frontend pattern:**
```javascript
// Fire both requests simultaneously
const [fast, full] = await Promise.all([
  fetch('/search/fast?q=' + encodeURIComponent(query)).then(r => r.json()),
  fetch('/search?q=' + encodeURIComponent(query)).then(r => r.json())
]);

// Show fast results immediately (~100ms)
renderResults(fast.results);

// Replace with full results when ready (~3-5s uncached, ~5ms cached)
renderResults(full.results);
```

---

## Examples

### Basic search

```bash
curl "https://api.oxiverse.com/search?q=python+programming"
```

### Search with negative constraint

```bash
curl "https://api.oxiverse.com/search?q=python+web+framework+not+django"
```

### Search with multiple negative constraints

```bash
curl "https://api.oxiverse.com/search?q=javascript+not+java+not+typescript"
```

### Site-restricted search

```bash
curl "https://api.oxiverse.com/search?q=site:arxiv.org+transformer+attention"
```

### Multi-site restricted search

```bash
curl "https://api.oxiverse.com/search?q=site:arxiv.org+site:wikipedia.org+quantum+computing"
```

### File type filter

```bash
curl "https://api.oxiverse.com/search?q=react+filetype:pdf"
```

### In-title search

```bash
curl "https://api.oxiverse.com/search?q=intitle:rust+async+framework"
```

### Comparison query

```bash
curl "https://api.oxiverse.com/search?q=react+vs+vue+vs+angular+2026"
```

### Location-aware search

```bash
curl "https://api.oxiverse.com/search?q=best+restaurants+in+tokyo+not+sushi"
```

### Recency / date-constrained search

```bash
curl "https://api.oxiverse.com/search?q=latest+AI+news+past+7+days"
```

### Price-constrained search

```bash
curl "https://api.oxiverse.com/search?q=price%3A%3C50+wireless+headphones"
```

### Paginated search

```bash
# Page 1: first 5 results
curl "https://api.oxiverse.com/search?q=python+tutorial&limit=5&offset=0"

# Page 2: next 5 results
curl "https://api.oxiverse.com/search?q=python+tutorial&limit=5&offset=5"
```

### How-to query with expanded queries

```bash
curl "https://api.oxiverse.com/search?q=how+to+deploy+docker+compose+production"
```

### Spell correction demonstration

```bash
# Query with typos — the engine autocorrects
curl "https://api.oxiverse.com/search?q=rust+progamming+langauge"
```

### Image search

```bash
curl "https://api.oxiverse.com/images?q=rust+logo"
```

### Video search

```bash
curl "https://api.oxiverse.com/videos?q=kubernetes+tutorial"
```

### News search

```bash
curl "https://api.oxiverse.com/news?q=AI+news"
```

### Fast local-only search

```bash
curl "https://api.oxiverse.com/search/fast?q=python"
```

### Parallel fast + full search (frontend pattern)

```javascript
// Fire both requests simultaneously
const [fast, full] = await Promise.all([
  fetch('/search/fast?q=' + encodeURIComponent(query)).then(r => r.json()),
  fetch('/search?q=' + encodeURIComponent(query)).then(r => r.json())
]);

// Show fast results immediately (~100ms)
renderResults(fast.results);

// Replace with full results when ready (~3-5s)
renderResults(full.results);
```

### Extract results with jq

```bash
# Get top-5 titles
curl -s "https://api.oxiverse.com/search?q=python" | jq '.results[:5][] | {title, url, score}'

# Get negative constraints
curl -s "https://api.oxiverse.com/search?q=python+not+django" | jq '.structured_constraints.negative'

# Get intent and confidence
curl -s "https://api.oxiverse.com/search?q=how+to+deploy+docker" | jq '{intent, confidence, category}'

# Get spell correction info
curl -s "https://api.oxiverse.com/search?q=rust+progamming" | jq '{query, spell_corrected_query}'

# Inspect the negation gate (DEFECT A transparency)
curl -s "http://localhost:4000/analyze?q=javascript+not+java+not+typescript" | jq '{contrastive_framing, exclusions, declined, manner_qualifiers}'

# Inspect the FULL /search reasoning surface in one additive call (no fetch)
curl -s "http://localhost:4000/inspect?q=best+way+to+cook+salmon+without+an+oven" | jq '{negation, recency, intent}'

# Get pagination metadata
curl -s "https://api.oxiverse.com/search?q=python&limit=5" | jq '{total, limit, offset, has_more}'
```

---

## Architecture

```
Client → Traefik (SSL) → Gateway (port 4000)
                              │
                              ├→ Intent Engine (classify + extract constraints)
                              │   ├── Intent classification (8 subtypes)
                              │   ├── Constraint extraction (positive/negative/operators)
                              │   ├── Spell correction (SymSpell + LinSpell)
                              │   ├── Location detection (gazetteer)
                              │   └── Query expansion
                              │
                              ├→ SearXNG (web search via VPN — bing, brave, duckduckgo, startpage, mojeek)
                              ├→ SearXNG2 (parallel VPN fan-out — additional engine instances)
                              ├→ Local Index (crawled pages via local indexer)
                              │
                              ├→ Deduplicate + Score + Rank → Response
                              │   ├── URL deduplication + source merging
                              │   ├── Domain authority (algorithmic)
                              │   ├── Freshness decay (per-intent half-life)
                              │   ├── Intent boost (structural signals)
                              │   ├── Consensus boost (multi-source bonus)
                              │   ├── Content quality (Shannon entropy)
                              │   ├── Alternative listing page detection
                              │   └── Constraint filtering
                              │
                              └── Media Endpoints
                                  ├── /images (via SearXNG categories=images)
                                  ├── /videos (via SearXNG categories=videos)
                                  └── /news (via SearXNG categories=news)
```

All search backends are queried in parallel with configurable timeouts. Results are deduplicated by URL, scored using multi-signal ranking, filtered by constraints, paginated, and returned as a unified list.

**Privacy:** All outbound search requests go through VPN (Gluetun) or Tor. No user data, tracking, or analytics is included in responses or used for ranking.

---

## Goals API

The Goals feature transforms a user's long-term goal (e.g. "build an AI assistant", "write a fantasy novel", "start a SaaS business") into a personalized, phased roadmap with curated resources, deadlines, and deliverables.

The system works in two flows:
1. **Discovery Flow** (`POST /goals` → questions → `POST /goals/:id/answers` → roadmap)
2. **Quick Flow** (`POST /goals/quick` → immediate full roadmap)

**Flow Overview**
```
Discovery Flow:
  Client                                    Gateway
    │                                         │
    ├─ POST /goals  { goal: "..." } ──────────┤
    │                                         ├── classify_goal() → intent engine
    │                                         ├── search_resources() → 20 results from /search
    │                                         ├── generate_questions() → domain-specific questions
    │                                         ├── GoalStore.insert() → goal_0001
    │◄──────── { goal_id, questions } ────────┤
    │                                         │
    ├─ POST /goals/{id}/answers { answers } ──┤
    │                                         ├── generate_roadmap() → phased plan
    │                                         ├── GoalStore.update_roadmap()
    │◄─────────── { roadmap } ────────────────┤

Quick Flow (one-shot):
  Client                                    Gateway
    │                                         │
    ├─ POST /goals/quick  { goal: "..." } ────┤
    │                                         ├── classify_goal()
    │                                         ├── search_resources()
    │                                         ├── default_answers (Q1=timeline, Q2=hours)
    │                                         ├── generate_roadmap() → full plan
    │◄─────── { goal_id, roadmap } ───────────┤
```

---

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/goals` | Create a goal → get domain-specific questions (observed 4 for `creative-writing`; varies by domain) |
| `POST` | `/goals/:goal_id/answers` | Submit answers → get full phased roadmap |
| `GET`  | `/goals/:goal_id` | Get goal status and roadmap by ID |
| `GET`  | `/goals/leaderboard` | Get leaderboard of all goals (sorted by score) |
| `POST` | `/goals/quick` | One-shot: goal → full roadmap immediately (no questions) |

---

### Goals Error Codes (verified live, 2026-08-05)

All Goals errors return a JSON body with `error` + `message`.

| Status | `error` | When (verified) |
|--------|---------|-----------------|
| `400` | `empty_goal` | `goal` missing or `< 3` characters (e.g. `{"goal":"ab"}`) |
| `400` | `invalid_phase` | `phase_id` not in `1..total_phases`. Phase IDs are **1-indexed** — `phase_id:0` returns this (verified: `POST /goals/goal_0002/progress` with `{"phase_id":0}` → `invalid_phase`). Use the `id` field from each roadmap phase. |
| `404` | `not_found` | Goal ID does not exist (e.g. `GET /goals/goal_does_not_exist`) |
| `422` | `invalid_payload` | Malformed JSON body or missing required field (from the custom `AppJson` extractor) |

See `docs/_generated/_round_v2_raw.md` for the exact raw bodies (`GOALS update progress ...` and the corrected `phase_id:1` blocks).

### POST /goals

Creates a new goal and returns domain-specific questions tailored to the goal type.

**Request Body**

```json
{
  "goal": "build a full-stack web app for project management with team collaboration"
}
```

**Response** `200 OK`

```json
{
  "goal_id": "goal_0001",
  "goal": "build a full-stack web app for project management with team collaboration",
  "intent": "technical",
  "questions": [
    {
      "id": 1,
      "question": "What is your target timeline for this goal?",
      "description": "How much calendar time do you want to allocate? This sets the pacing of each phase.",
      "options": [
        "1 month — Quick sprint",
        "3 months — Quarter project",
        "6 months — Half-year journey",
        "12 months — Year-long mastery",
        "Flexible — No strict deadline"
      ],
      "type": "single_choice"
    },
    {
      "id": 2,
      "question": "How many hours per week can you dedicate?",
      "description": "Consistency matters more than intensity...",
      "options": ["1-5 hours — Casual", "5-10 hours — Evenings", "10-20 hours — Half-time", "20+ hours — Full-time"],
      "type": "single_choice"
    },
    {
      "id": 3,
      "question": "What architecture pattern do you want to follow?",
      "description": "The architecture shapes how your components communicate and scale.",
      "options": [
        "Monolithic — simple, single deployable",
        "Microservices — independent, deployable services",
        "Serverless — functions as a service",
        "Event-driven — message queues and async processing",
        "Jamstack — static frontend + APIs"
      ],
      "type": "single_choice"
    }
  ],
  "total_questions": 7,
  "created_at": "2026-07-29T12:00:00Z",
  "next_step": {
    "method": "POST",
    "path": "/goals/goal_0001/answers",
    "body": {
      "answers": [
        {"question_id": 1, "answer": "..."}
      ]
    }
  }
}
```

**Status Codes**

| Code | Meaning |
|------|---------|
| 200  | Goal created successfully |

**Input validation:** Goal must be at least 3 characters.

---

### `POST /goals/:goal_id/answers`

Submits answers to the questions from `POST /goals` and generates a personalized phased roadmap.

**Request Body**

```json
{
  "answers": [
    {"question_id": 1, "answer": "3 months — Quarter project"},
    {"question_id": 2, "answer": "5-10 hours — Evenings & weekends"},
    {"question_id": 3, "answer": "Microservices — independent, deployable services"},
    {"question_id": 4, "answer": "Hybrid — SQL + cache layer (Redis)"},
    {"question_id": 5, "answer": "Container / Kubernetes (Docker, EKS, GKE)"},
    {"question_id": 6, "answer": "WebSockets for live bidirectional communication"},
    {"question_id": 7, "answer": "A completed product ready for users"}
  ]
}
```

**Response** `200 OK`

```json
{
  "goal_id": "goal_0001",
  "goal": "build a full-stack web app for project management with team collaboration",
  "intent": "technical",
  "roadmap": {
    "title": "Your Personalized Roadmap: build a full-stack web app...",
    "overview": "A 12-week journey (5-10 hours/week) across 4 phases.",
    "phases": [
      {
        "id": 1,
        "title": "Architecture & Planning",
        "description": "Design the system architecture, choose your tech stack...",
        "duration_weeks": 3,
        "deadline": "2026-08-19 (buffer: 2026-08-26)",
        "buffer_days": 7,
        "objectives": [
          "Design system architecture and component diagram",
          "Choose tech stack and dependencies",
          "Set up development environment and CI/CD",
          "Define API contracts and data models"
        ],
        "resources": [
          {
            "title": "How to Build a Project Management App: Step-by-Step",
            "url": "https://example.com/tutorial",
            "resource_type": "article",
            "description": "A comprehensive guide to building..."
          }
        ],
        "deliverables": [
          "Architecture document with diagrams",
          "Tech stack decision record",
          "Development environment with CI/CD"
        ],
        "completion_type": "foundation",
        "is_completed": false
      }
    ],
    "total_duration_weeks": 12,
    "total_buffer_days": 28
  },
  "created_at": "2026-07-29T12:00:00Z",
  "status": "active",
  "completed_phases": 0,
  "total_phases": 4,
  "score": 0
}
```

**Status Codes**

| Code | Meaning |
|------|---------|
| 200  | Roadmap generated successfully |

**Error Codes**

| Code | Meaning |
|------|---------|
| `not_found` | Goal ID does not exist. Create one first with `POST /goals`. |

---

### `GET /goals/:goal_id`

Retrieves the goal status and full roadmap (if answers have been submitted).

**Response** `200 OK`

```json
{
  "goal_id": "goal_0001",
  "goal": "build a full-stack web app for project management with team collaboration",
  "intent": "technical",
  "roadmap": { ... },
  "created_at": "2026-07-29T12:00:00Z",
  "status": "active",
  "completed_phases": 0,
  "total_phases": 4,
  "score": 0
}
```

**Status field values:** `"active"` (in progress), `"completed"` (all phases completed).

**Status Codes**

| Code | Meaning |
|------|---------|
| 200  | Goal found (roadmap may be `null` if not yet generated) |

**Error Codes**

| Code | Meaning |
|------|---------|
| `not_found` | Goal ID does not exist |

---

### `GET /goals/leaderboard`

Returns all goals sorted by score (descending). Max 50 entries.

**Response** `200 OK`

```json
{
  "entries": [
    {
      "goal_id": "goal_0001",
      "goal": "build a full-stack web app...",
      "user_name": "Anonymous",
      "score": 0,
      "completed_phases": 0,
      "total_phases": 4,
      "created_at": "2026-07-29T12:00:00Z"
    }
  ],
  "total_entries": 1
}
```

---

### `POST /goals/quick`

One-shot endpoint that creates a goal and generates a full roadmap immediately without asking questions. Uses sensible defaults (3-month timeline, 5-10 hours/week).

**Request Body**

```json
{
  "goal": "build a full-stack web app for managing personal finances"
}
```

**Response** `200 OK`

```json
{
  "goal_id": "goal_0002",
  "goal": "build a full-stack web app for managing personal finances",
  "intent": "technical",
  "resource_count": 12,
  "roadmap": {
    "title": "Your Personalized Roadmap: build a full-stack web app...",
    "overview": "A 12-week journey (5-10 hours/week) across 4 phases.",
    "phases": [ ... ],
    "total_duration_weeks": 12,
    "total_buffer_days": 28
  },
  "created_at": "2026-07-29T12:00:05Z",
  "status": "active",
  "completed_phases": 0,
  "total_phases": 4,
  "score": 0
}
```

`resource_count` represents the total distributed resources curated across all roadmap phases.

**Input validation:** Goal must be at least 3 characters.

---

### `POST /goals/:goal_id/phases/:phase_id/complete`

Marks a specific 1-indexed phase as completed, recalculating progress score (+100 pts per phase, +500 bonus pts upon 100% completion).

**Request Body**
None (empty body).

**Response** `200 OK`

```json
{
  "goal_id": "goal_0001",
  "goal": "build a web app",
  "completed_phase_id": 1,
  "completed_phases": 1,
  "total_phases": 4,
  "score": 100,
  "status": "active",
  "roadmap": { ... }
}
```

---

### `POST /goals/:goal_id/progress`

Sets specific phase completion status via JSON payload.

**Request Body**

```json
{
  "phase_id": 1,
  "is_completed": true
}
```

**Response** `200 OK`

```json
{
  "goal_id": "goal_0001",
  "goal": "build a web app",
  "phase_id": 1,
  "is_completed": true,
  "completed_phases": 1,
  "total_phases": 4,
  "score": 100,
  "status": "active",
  "roadmap": { ... }
}
```

---

### Domain-Specific Question Banks

The `/goals` endpoint detects the goal's domain using keyword analysis and returns tailored questions.

| Domain | Goal Keywords | Questions Returned |
|--------|---------------|-------------------|
| `ai-ml` | ai, machine learning, llm, chatbot, recommendation, deep learning, nlp | 7 questions: timeline, hours, AI system type, data strategy, compute infrastructure, evaluation, success vision |
| `web-app` | website, web app, frontend, full-stack | 7 questions: timeline, hours, architecture pattern, data persistence, deployment, real-time features, success vision |
| `api-backend` | api, backend, microservice, serverless, graphql | 7 questions: timeline, hours, architecture, persistence, deployment, real-time, success vision |
| `mobile` | mobile, ios, android, react native, flutter | 6 questions: timeline, hours, platform target, backend/API, offline sync, success vision |
| `systems` | system, embedded, kernel, low-level, driver, firmware | 5 questions: timeline, hours, target hardware, performance profile, success vision |
| `devops` | devops, ci/cd, deployment, kubernetes, infrastructure, terraform | 5 questions: timeline, hours, infrastructure scale, cloud provider, success vision |
| `research` | research, paper, study, thesis, experiment, publication | 7 questions: timeline, hours, methodology, publication outlet, tools/resources, collaboration, success vision |
| `creative-writing` | write, novel, book, story, poem, script | 6 questions: timeline, hours, genre/format, process style, editing approach, success vision |
| `creative-design` | design, art, illustration, animation, graphic, ui/ux | 5 questions: timeline, hours, design medium, toolchain, success vision |
| `business` | startup, business, company, venture, saas, e-commerce | 6 questions: timeline, hours, business model, target customer, business stage, success vision |
| `lifestyle` | cook, recipe, fitness, workout, guitar, piano, yoga, gardening | 5 questions: timeline, hours, activity focus, practice style, success vision |
| `learning` | learn, course, tutorial, certification | 5 questions: timeline, hours, learning style, assessment goal, success vision |
| `general-tech` | build, develop, create, platform, tool, framework | 7 questions: same as web-app bank |
| `general` | (no specific keywords matched) | 3 questions: timeline, hours, success vision |

All questions are `"type": "single_choice"` with curated options.

---

### Roadmap Structure

The generated roadmap contains:

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | "Your Personalized Roadmap: {goal}" |
| `overview` | string | Summary of total duration, weekly hours, and phase count |
| `phases` | Phase[] | Ordered list of phases (3–6 phases) |
| `total_duration_weeks` | int | Total project duration in weeks |
| `total_buffer_days` | int | Total buffer days (= phases × 7) |

**Phase Fields**

| Field | Type | Description |
|-------|------|-------------|
| `id` | int | 1-indexed phase number |
| `title` | string | Phase title (e.g. "Architecture & Planning") |
| `description` | string | Detailed description with goal name |
| `duration_weeks` | int | Number of weeks allocated to this phase |
| `deadline` | string | Hard deadline + buffer date, e.g. `"2026-08-19 (buffer: 2026-08-26)"` |
| `buffer_days` | int | Always 7 (1 week buffer per phase) |
| `objectives` | string[] | 4 actionable objectives for the phase |
| `deliverables` | string[] | 3–4 concrete deliverables to complete |
| `resources` | Resource[] | 2–5 curated resources (articles, docs, videos, papers) |
| `completion_type` | string | Type: `"foundation"`, `"prototype"`, `"feature_complete"`, `"project"`, `"final_delivery"` |
| `is_completed` | bool | Whether the phase is marked complete (default: `false`) |

**Resource Fields**

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Resource title from search result |
| `url` | string | Full URL to the resource |
| `resource_type` | string | `"article"`, `"documentation"`, `"video"`, or `"paper"` (inferred from URL pattern) |
| `description` | string | Snippet or description of the resource (first 200 chars) |

**Phase Sequencing by Domain**

| Domain | Phase 1 | Phase 2 | Phase 3 | Phase 4 / Final |
|--------|---------|---------|---------|----------------|
| Technical (web-app, mobile, api, systems) | Architecture & Planning | Core Implementation | Integration & Testing | Launch & Polish |
| AI/ML | Architecture & Planning | Model Development & Training | Integration & Optimization | Launch & Polish |
| Research | Literature Review & Research Design | Data Collection & Analysis | Analysis & Drafting | Publication & Dissemination |
| Creative (writing, design) | Concept Development & Planning | Drafting & Creation | Revision & Refinement | Production & Publication |
| Business | Market Research & Strategy | MVP Development | Testing & Iteration | Launch & Growth |
| Learning | Foundation & Curriculum Planning | Core Learning | Practice & Projects | Mastery & Assessment |

---

### Deadline Calculation

Deadlines are computed from the **current system time** at request time, not hardcoded. The timeline answer (Q1) determines total duration:

| Timeline Answer | Total Weeks | Phases |
|----------------|-------------|--------|
| `"1 month — Quick sprint"` | 4 | 3 |
| `"3 months — Quarter project"` | 12 | 4 |
| `"6 months — Half-year journey"` | 24 | 5 |
| `"12 months — Year-long mastery"` | 48 | 6 |
| `"Flexible — No strict deadline"` | 12 | 4 (default) |

Each phase gets equal weeks (`total_weeks / phases`). Each phase has a hard deadline + 7-day buffer.

---

### Resource Curation

Resources are sourced from the search API (`GET /search?q={goal}&limit=20`) — the same engine used for web search. Results are categorized by URL pattern:

- `youtube.com`, `youtu.be`, `vimeo.com` → `"video"`
- `/docs/`, `/api/`, `/reference/`, `/wiki/` → `"documentation"`
- `arxiv.org`, `researchgate.net`, `acm.org`, `ieee.org` → `"paper"`
- Everything else → `"article"`

Resources are distributed round-robin across phases. If the search returns 20 results for a 4-phase roadmap, each phase gets 5 resources.

If search fails (timeout or zero results), fallback resources with Google search links are generated per phase.

---

### Intent Classification

Goals are classified using the intent engine (same endpoint used by `/search`) or keyword detection. Observed `intent` values (verified live, 2026-08-05) include `learning` (e.g. `"learn to build a privacy-first search engine using Rust"`), `creative-writing` (e.g. `"write a novel in 6 months"`), and `technical`. Other documented goal domains include `ai-ml`, `web-app`, `api-backend`, `mobile`, `systems`, `devops`, `research`, `creative-design`, `business`, `lifestyle`, `general-tech`, and `general`.

> **Note on question count (verified):** a `creative-writing` goal returned `total_questions: 4` (timeline, hours, 2× free_text). The domain-specific question-bank table below lists `creative-writing` as 6 questions — this may not match the live generator, which can emit a smaller tailored set. Treat the per-domain counts as descriptive, not a hard contract.

The classification uses keyword detection first (fast path), then falls back to the intent engine HTTP call.

---

### Examples

#### Create a goal and get questions (AI/ML domain)

```bash
curl -s -X POST "http://localhost:4000/goals" \
  -H "Content-Type: application/json" \
  -d '{"goal":"build a recommendation engine using deep learning"}' | jq
```

#### Create a goal and get questions (Research domain)

```bash
curl -s -X POST "http://localhost:4000/goals" \
  -H "Content-Type: application/json" \
  -d '{"goal":"research and publish a paper on transformer optimization"}' | jq '.questions'
```

#### Submit answers and get a roadmap

```bash
GOAL_ID=$(curl -s -X POST "http://localhost:4000/goals" \
  -H "Content-Type: application/json" \
  -d '{"goal":"build a mobile fitness tracking app"}' | jq -r '.goal_id')

curl -s -X POST "http://localhost:4000/goals/$GOAL_ID/answers" \
  -H "Content-Type: application/json" \
  -d '{
    "answers": [
      {"question_id": 1, "answer": "3 months — Quarter project"},
      {"question_id": 2, "answer": "10-20 hours — Half-time commitment"},
      {"question_id": 3, "answer": "Cross-platform (React Native, Flutter)"},
      {"question_id": 4, "answer": "Custom REST/GraphQL API"},
      {"question_id": 5, "answer": "Full offline-first with background sync"},
      {"question_id": 6, "answer": "A working prototype I can demo"}
    ]
  }' | jq '.roadmap.phases[] | {id, title, deadline, completion_type}'
```

#### Get goal status

```bash
curl -s "http://localhost:4000/goals/goal_0001" | jq '{status, completed_phases, total_phases}'
```

#### Quick one-shot roadmap

```bash
curl -s -X POST "http://localhost:4000/goals/quick" \
  -H "Content-Type: application/json" \
  -d '{"goal":"build a rust web framework"}' | jq '.roadmap.phases[].title'
```

#### Leaderboard

```bash
curl -s "http://localhost:4000/goals/leaderboard" | jq '.entries[] | {goal, total_phases}'
```

#### Extract phase resources (with jq)

```bash
# Get all resources across all phases
curl -s -X POST "http://localhost:4000/goals/quick" \
  -H "Content-Type: application/json" \
  -d '{"goal":"learn kubernetes"}' | jq '.roadmap.phases[].resources[] | {title, resource_type, url}'
```

---

### Goals Architecture

```
Client → Gateway (port 4000)
              │
              ├→ POST /goals
              │     ├── classify_goal() → Intent Engine (port 3005)
              │     ├── search_resources() → Search API (localhost:4000/search)
              │     ├── generate_questions() → domain detection → question bank
              │     └── GoalStore (in-memory HashMap)
              │
              ├→ POST /goals/:id/answers
              │     ├── generate_roadmap() → phase_content (domain-aware)
              │     ├── curate_resources() → round-robin distribution
              │     └── GoalStore.update_roadmap()
              │
              ├→ POST /goals/quick
              │     ├── classify_goal() + search_resources()
              │     ├── default_answers (timeline=3mo, hours=5-10)
              │     └── generate_roadmap() → immediate result
              │
              ├→ GET /goals/:id
              │     └── GoalStore.get()
              │
              └→ GET /goals/leaderboard
                    └── GoalStore.leaderboard() → sorted by score
```

**Data Flow:** Goals are stored in-memory (non-persistent across restarts). Resources are fetched in real-time from the search API. The intent engine classifies each goal for phase content customization. Deadlines are computed from the current system time + user's timeline answer.
