# UI Generation Prompt — IntentForge v2 (for Google Stitch)

> Feed this entire document to the AI UI generator as its system prompt / brief.
> It fully specifies the product, the brand direction, every page, and the exact
> backend API the UI must wire to — verbatim field names, JSON shapes, auth, and
> behavioral gotchas. Build a working frontend, not a mockup.

---

## 1. ROLE & DELIVERABLE

You are a senior frontend engineer and UI designer. Build a **complete, production-quality, single-page web application** that is the user-facing frontend for **IntentForge v2**, a self-hosted, privacy-first meta-search engine. The backend is a real Rust API that is already live and will be called over HTTP — every screen below must actually fetch from and render real API responses, not placeholder data.

**Technical output requirements:**
- A modern, dependency-light SPA (plain HTML/CSS/JS or a Vite + framework build) that is **self-contained and static** — no server-side rendering required.
- Ships as static assets that will be served behind a reverse proxy (Traefik) on the same origin as the API, so **use same-origin relative `/search`, `/goals`, ... paths** (no hardcoded `localhost:4000`, no cross-origin CORS work needed). Base path is configurable via one constant.
- Keep the design system fully themeable via CSS custom properties so the operator can rebrand.
- Responsive: desktop-first but fully usable on mobile.
- Accessibility: semantic HTML, keyboard-navigable, sensible focus states, `prefers-reduced-motion` respected.
- Dark theme by default, light theme as a toggle. No flash of wrong theme on load.
- Loading states on every async call (results arrive in ~0–6 s; never show a blank screen — show spinners/skeletons and, where possible, progressive results).
- Error/empty states for every failure mode documented in §5.

---

## 2. PRODUCT OVERVIEW & POSITIONING

**IntentForge v2** is a **privacy-first, self-hosted meta-search engine**. Unlike commercial search engines it:
- **Does not track users** — no analytics, no cookies, no user profiles, no sessions.
- **Routes ALL outbound search traffic through ProtonVPN and Tor** (two independent egress paths) so the user's IP is never exposed to upstream engines.
- Runs entirely on the operator's own infrastructure — no third-party API dependencies.
- Classifies query intent locally with a BERT model, auto-corrects spelling locally, applies rich natural-language constraints, and returns deduplicated, ranked results with transparent provenance.

**Brand & tone to convey:**
- Privacy and **trust** are the #1 brand value. The UI should feel clean, calm, minimal, and trustworthy — users should *feel* nothing is tracking them.
- **Source transparency is a feature.** Show WHERE each result came from, WHEN a price/fact was observed, and label provenance honestly. Never imply freshness you cannot prove.
- No existing brand assets or palette exist — design one. Suggested personality: technical-but-friendly, developer/privacy-advocate aesthetic. A name/wordmark ("IntentForge", "IntentForge v2", or "Oxiverse") and a simple logomark (e.g. a stylized anvil/forge or a shield/globe motif) are welcome.
- Light-on-dark encouraged. Distinguish with **privacy badges** (e.g. "via VPN", "via Tor", "local index") on results where the data tells you the source path.

---

## 3. APPLICATION PAGES / VIEWS

Build a **top-level tabbed/segmented shell** so planned features can land incrementally. The core live views are **Search** and **Goals**; build the rest of the shell now (even with "coming soon" placeholders) so it doesn't need a rewrite later.

### 3.1 Search (primary view, web)
- Large centered search box with a search button, plus a **mega-dropdown of advanced options** (see §4 Query Operators / §6.1).
- Search results list: each result shows title (link), snippet/description, URL/host, and **source badges** (from `result.sources`, e.g. `["bing","brave","local"]`). Show `score`/`authority` subtly or as a filter — don't clutter.
- **Intelligence panel** (above or beside results) surfaced from the `/search` response: detected `intent` + `category`, `confidence` (as a subtle measure), `expanded_queries` (chips the user can click to re-search), applied constraints, and spelling correction ("Did you mean *X*?" when `spell_corrected_query` differs from `query`).
- **Pagination** via `limit`/`offset` + `has_more` + `total` (see §6.3). Prev/Next and a results-count line.
- **Result-type tabs**: share one query across **Web** (`/search`), **Images** (`/images`), **Videos** (`/videos`), **News** (`/news`). Each renders its own result card shape (§6.7).
- **Fast/instant feedback**: fire `/search/fast` and `/search` in parallel (`Promise.all`); render the fast local results immediately, then replace/augment with the full `/search` results when ready (§6.2). Show a subtle "live local results → full results" affordance.
- If the query is a comparison (`vs`/`versus`) or negated (`not X`), surface it (see `/analyze` behavior, §6.8) with a small explainer chip. The API is transparent about its own reasoning — the UI should mirror that transparency where it helps ("excluded: django").

### 3.2 Goals (roadmap feature)
- **Create view**: a text field for a long-term goal (≥3 chars, ≥3 alphabetic), plus a **Quick Roadmap** button (`/goals/quick`) and the **Discovery flow** (`/goals` → answer questions → roadmap).
- **Discovery flow (wizard)**: after POST `/goals`, render the returned `questions[]` as a stepped form. Submit the answers to POST `/goals/:id/answers` (see auth §5, indexing §6.4).
- **Roadmap view**: renders the returned `roadmap` — overview, total duration/buffer, and each `phase` as a card/section with `title`, `description`, `duration_weeks`, `deadline`, `objectives`, `resources` (external links, show `resource_type`), `deliverables`, `completion_type`, and a **mark-complete** control (POST `/goals/:id/progress` with `{ phase_id, is_completed }`). Show running `score`, `completed_phases`/`total_phases`, and `status`.
- **Goals list / leaderboard**: GET `/goals` (your goals) and GET `/goals/leaderboard` (§6.5 — a **bare JSON array**). Signed-in users who opt in appear on the leaderboard; anonymous visitors see the entries but cannot appear themselves. Empty array = no opted-in entries yet — render a calm empty state, never fabricate rows.
- Anonymous roadmaps persist across gateway restarts but **expire on a sliding 6-hour TTL** (§4 rule 8, §6.4). Show the `expires_at` countdown. Signing in and claiming makes them permanent and portable across devices.

### 3.3 Settings / API key
- A small settings/pane where the user can paste and store an **API key** (only needed for `/goals*`, `/shopping`, `/commerce/extract`; search is open). Store client-side only (localStorage). Never displayed back in plaintext after saving (mask it). Send on every keyed call (§5).

### 3.4 Commerce / Shopping (affiliate-aware, if enabled)
- For transactional queries against `/shopping`, render a **dedicated product block** separate from web results — never merged silently. Show product `name`, `price`/`currency`, `availability`, `condition`, `merchant`, `rating`/`rating_count`, and image when present. **Label provenance**: "price observed 2h ago" from `commerce.observed_at`; show `commerce.source` ("json-ld"/"og"/none) subtly. If `offer_comparisons[]` exists, render a **comparison table** of merchants for that product (price-ordered). See §6.9 for exact fields.

### 3.5 Future tabs (shell placeholders)
Build the nav tabs now, content may be "coming soon": **Unified Search**, **Products/Commerce (catalog/price history)**, **Local**, **Dashboard/Admin** (crawl queue, ingestion status, permissions, takedowns, budgets), **Submit** (URL + merchant catalogue upload). Placeholders only — no fake data.

---

## 4. CORE RULES THE UI MUST RESPECT (behavioral contract)

1. **Privacy is non-negotiable.** No client analytics, no tracking pixels, no external fonts/CDNs that phone home, no cookies beyond the user's own saved API key. Everything (fonts, assets, scripts) must be self-hosted or inline. Do not send the query to any third party; the query goes only to the IntentForge origin.
2. **Optional JSON fields are OMITTED, not null.** The API omits any field that has no value (`published_date`, `geo_location`, `spell_corrected_query`, `shopping`, `price`, `merchant`, ...). Your renderer must treat a **missing key and an explicit `null` identically** and never explode on absent fields. Write a defensive accessor for every optional field.
3. **Never show a stale price/fact as current.** Every commerce fact carries `observed_at` (Unix seconds UTC) and `source`. If either is absent/old, label it ("observation time unknown", "seen 2h ago"). When `offer_comparisons[].mixed_observation_times` is true, show a note that the price spread is **not simultaneous**.
4. **Latency is real.** Cold `/search` is ~3.8–6.8 s, cached ~3–15 ms, `/search/fast` ~13–25 ms. Show progress; do not block the UI; allow the user to keep typing/refine.
5. **Pagination is offset-based** (`limit`/`offset`), no cursors. `has_more = offset + limit < total`. Cache key includes the pagination slice, so paging shares no cached body — pagination is cheap, iterate by re-requesting.
6. **Fail-open filters.** If a constraint would empty a non-empty result set, the API keeps the unfiltered set. The UI should not assert that a filter "removed all results" — trust the returned `results`.
7. **Goal/phase indexing is 1-indexed** and must be echoed from the API verbatim (see §6.4).
8. **Anonymous goals expire.** Roadmaps survive gateway restarts but carry a sliding 6-hour TTL from the last goal activity (`IF_ANON_GOAL_TTL_HOURS=6`); every goal call renews it, and on expiry the goal is deleted, not merely hidden. For `Caller::Anonymous`, responses include `expires_at` (RFC 3339): render a countdown plus a persistent "These roadmaps expire in a few hours" notice, and treat the goal as gone once `expires_at` passes (refetch rather than showing stale local state). Claimed goals omit `expires_at` and never expire; only anonymous goals carry the countdown.
9. **Dates**: `published_date` on results is ISO-8601 when resolvable, else omitted. Use it for "Date" display only when present. Natural-language date phrases in queries auto-convert server-side — you don't need to parse them.

---

## 5. AUTHENTICATION — THREE CREDENTIAL CLASSES (Phase 2)

There are three ways to authorize a request. Search and introspection endpoints remain open.

**Class 1 — service / dev API key.**
- `/shopping` and `/commerce/extract` stay API-key-only. On goal routes the key still works and keeps the original global behavior.
- Send it as `x-api-key: ***>` **or** `Authorization: Bearer ***
- 401 `{"error":"unauthorized"}` = wrong/missing credential; surface a clear state with a link to Settings.

**Class 2 — anonymous owner capability (`X-Anonymous-Owner`).**
- On first load generate a UUID v4 in `localStorage` under `if_owner_ref` (`crypto.randomUUID()`), reuse it on every goal request, never regenerate per call.
- Send `X-Anonymous-Owner: <if_owner_ref>` on every goal request alongside any other credential. Never put it in a query string; never display, log or echo it.
- Two different `if_owner_ref` values cannot see each other's goals. Load **no external scripts, fonts or CDNs** — a third-party script could read `localStorage` and steal the capability.

**Class 3 — Supabase Google account (JWT).**
- Sign in with `@supabase/supabase-js`: `supabase.auth.signInWithOAuth({ provider: 'google' })`. Use same-origin relative API paths; no hardcoded localhost.
- Send the session's access token as `Authorization: Bearer <access_token>` on goal requests. The gateway validates it via Supabase Auth. An expired/invalid token falls back to the anonymous capability rather than hard-failing; prompt to sign in again.
- **Claim on login:** on the first successful sign-in call `POST /goals/claim` with BOTH the JWT and the current `X-Anonymous-Owner`. On success clear `if_owner_ref`, reload the goals list, and show a one-time "Your roadmaps are now saved to your account" state. Claimed goals are portable across devices and never expire.
- Account menu with sign-out. After sign-out generate a fresh `if_owner_ref` and continue anonymously.
- `GET /goals/leaderboard` is public: anonymous visitors see only opted-in claimed entries.

**Consents (Settings / Privacy).**
- `PUT /consents/leaderboard` with `{"enabled": bool}` — opt in to appear on the leaderboard. Default OFF.
- `PUT /consents/hiring_notices` with `{"enabled": bool}` — separate, default OFF, plain-language purpose text, revoke button. Copy must state that no goal text or email is shared with companies.
- Never bundle the two consents into one toggle or pre-check either of them.

## 6. VERBATIM API REFERENCE (wire these exact shapes)

### 6.1 Base + Search `GET /search`
Params: `q` (required, URL-encoded), `limit` (int, default 24), `offset` (int, default 0). `count` and `n` are aliases of `limit`.
Error: `400` with an envelope including `error` (`empty_query` | `invalid_query` | `invalid_parameter`), `message`, and the standard result fields set to empty/defaults.

200 OK shape (verbatim — real example for query `python web framework not django`):
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
    "navigational": 0.39, "informational": 0.16, "technical": 0.08,
    "how-to": 0.11, "comparison": 0.03, "fresh": 0.04,
    "transactional": 0.16, "local": 0.03
  },
  "expanded_queries": ["python web framework not django alternative to django", "..."],
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
Result (`MergedResult`) fields: `url`, `title`, `content`, `score` (0–1), `authority` (0–1), `quality` (0–1), `is_local` (bool), `sources` (string[] of backend names), `published_date` (ISO-8601, **omitted if unresolvable**).

Top-level optional fields (omitted when absent): `applied_constraints` (only when constraints actually applied), `spell_corrected_query` (only when a correction fired — show "Did you mean?" if present and `spell_corrected_query` != `query`), `query_quality` (only when `"low"`/`"junk"`), `recall_gap_terms` (distinct query terms absent from all results — show as an honest "coverage" signal), `shopping` + `offer_comparisons` (transactional — see §6.9), `deep_result`, `price_verified`.
Note: `geo_location` / `warnings` / `ignored_constraints` are declared-but-omitted; do not expect them.

**Query operators** the API parses from the raw `q` string (show as autofill hints/advanced options): `site:example.com`, `filetype:pdf`, `intitle:...`, `inurl:...`, `intext:...`, `after:YYYY-MM-DD` / `before:YYYY-MM-DD`, `price:<N` / `price:>N` (URL-encode `<`/`>`), natural language negation `not X` / `without X`, comparison `X vs Y` / `X versus Y`.

### 6.2 Fast local `GET /search/fast`
Params: `q` (required), `limit` (int, default 24). 200 (verbatim):
```json
{"count": 10, "source": "local", "results": [
  {"url": "https://rustwebframework.org/", "title": "Rust Web Framework",
   "content": "Rust Web Framework GitHub Getting started ...",
   "score": 1.0, "authority": 0.70, "is_local": true, "quality": 1.0, "sources": ["local"]}
]}
```
No pagination metadata. Use for instant results (§3.1).

### 6.3 `has_more`/`total` pagination
`total` and `results_after_filter` are post-filter counts, not `results.length`. Render Prev/Next using `has_more` and `offset + limit`.

### 6.4 Goals — all shapes (KEYED)
`Question` (returned by POST `/goals`) — 4 per goal; **`id` and `phase_id` are 1-indexed** (first is `1`, never `0`):
```json
{
  "goal": {"goal_id":"goal_0002","intent":"creative-writing","total_questions":4,
           "questions":[
             {"id":1,"question":"How long do you plan to pursue this goal?","type":"single_choice","options":["1 month - Quick sprint","3 months - Quarter project","6 months - Half-year journey","12 months - Year-long mastery","Flexible - No strict deadline"]},
             {"id":2,"question":"How many hours per week can you commit?","type":"single_choice","options":["<5 hours - Light focus","5-10 hours — Part-time focus","10-20 hours - Intensive focus",">20 hours - Full-time focus"]},
             {"id":3,"question":"Any specific details or constraints?","type":"free_text"},
             {"id":4,"question":"What does success look like?","type":"free_text"}
           ],
           "next_step":{"method":"POST","path":"/goals/goal_0002/answers"}}
}
```
Submit answers as **`question_id` = the question's `id` from the array** (echo back verbatim, 1-indexed), e.g. body `{"answers":[{"question_id":1,"answer":"6 months — Half-year journey"},{"question_id":2,"answer":"5-10 hours — Part-time focus"},{"question_id":3,"answer":"fiction"},{"question_id":4,"answer":"a finished draft"}]}`.

`roadmap` (in POST `/goals/:id/answers` and `/goals/quick` responses):
```json
{
  "title": "Your Personalized Roadmap: ...",
  "overview": "A 12-week journey (5-10 hours/week) across 4 phases.",
  "phases": [
    {"id": 1, "title":"Architecture & Planning","description":"...",
     "duration_weeks": 3, "deadline":"2026-12-06 (buffer: 2026-12-13)", "buffer_days":7,
     "objectives":["..."],
     "resources":[{"title":"...","url":"https://...","resource_type":"article","description":"<200 chars"}],
     "deliverables":["..."], "completion_type":"foundation", "is_completed":false}
  ],
  "total_duration_weeks": 12, "total_buffer_days": 28
}
```
- `completion_type` ∈ `foundation|prototype|feature_complete|project|final_delivery`.
- Goal status ∈ `active|completed` (also a transient `pending_answers` when no roadmap yet).
- `score = completed_phases * 100` (+500 bonus when all complete).
- **1-indexed**: `POST /goals/:id/progress` body `{"phase_id":1,"is_completed":true}` (a `phase_id` outside `1..total_phases` → `400 invalid_phase`); `unset is_completed` defaults to true; phase can be toggled off.
- `POST /goals/:id/phases/:id/complete` marks a phase complete (1-indexed path segment); it sets `true` only and does not toggle. `POST /goals/:id/progress` toggles a phase on/off.
- `expires_at` (RFC 3339) is an **additive field on anonymous goal responses**, computed as `updated_at + IF_ANON_GOAL_TTL_HOURS` (6 hours by default). Every goal call slides it forward; the goal is deleted once it passes. Render a countdown and the persistent "These roadmaps expire in a few hours" notice (§4 rule 8). If the field is absent (service-key caller), show no expiry UI rather than guessing.
- Errors: `400 empty_goal` (goal < 3 chars) / `400 invalid_goal` (<3 alphabetic) / `400 invalid_answers` / `400 invalid_phase` / `400 invalid_payload` / `404 not_found` / `401 unauthorized`.

Endpoints:
- `POST /goals` → create + questions. `POST /goals/quick` → one-shot roadmap (defaults 3mo, 5–10h). `GET /goals/:id` → `{goal_id, goal, intent, roadmap|null, created_at, status, completed_phases, total_phases, score, expires_at}` (`404` if unknown; `expires_at` is present for anonymous callers). `GET /goals/leaderboard` → **bare JSON array** (not wrapped) of `{goal_id, goal, user_name, score, completed_phases, total_phases, created_at}`, score-desc, ≤50, only claimed, opted-in goals that have a roadmap (§6.5). `GET /goals` → bare array sorted by goal_id, each item carrying `expires_at` for anonymous callers. `GET /goals/:id/progress`, `POST /goals/:id/progress`.

- `POST /goals/claim` → `200` bare array of claimed goals; requires BOTH `Authorization: Bearer <jwt>` and `X-Anonymous-Owner`. Idempotent: second call returns `[]`. On success clear `if_owner_ref`.
- `PUT /consents/:purpose` (`leaderboard` | `hiring_notices`) body `{"enabled": bool}` → `200` consent row; requires JWT. Both default off and revocable.

### 6.5 Leaderboard shape (IMPORTANT)
`GET /goals/leaderboard` returns a JSON **array** `[ {...}, ... ]` (max 50). Count = `Array.length`. Do not read `.entries` or `.results`.

This endpoint is public. It returns only claimed goals whose owner opted in via `PUT /consents/leaderboard`. An empty array means nobody has opted in yet: render a calm empty state, never fabricate rows, and never treat `[]` as an error. Signed-in users get a leaderboard opt-in toggle in Settings / Privacy (default off, revocable).

### 6.6 `GET /spellcheck?q=...`
```json
{"query":"pythn programing langauge","corrected":"python programming language","changed":true,
 "corrections":[{"original":"pythn","suggestion":"python","in_dictionary":false}, ...]}
```
`corrections[]` lists only tokens it would fix. Protected tech/brand terms (`openai`, `rust`, `kubernetes`) → `changed:false`, empty `corrections[]`. Use for a "Did you mean?" highlighter if you like.

### 6.7 Image / Video / News cards — DIFFERENT field names per type (verbatim)
- **`/images`** → `{ count, query, results:[{ title, url, image_url, thumbnail_url, description, source, score }] }` — note **`thumbnail_url`**.
- **`/videos`** → `{ count, query, results:[{ title, url, description, thumbnail, video_id, source, score }] }` — note **`thumbnail`** (no `_url`); `video_id` often empty string.
- **`/news`** → `{ count, query, results:[{ title, url, description, published_at, source, score }] }` — `published_at` is often `""` (empty string) on bing news, ISO timestamp on hackernews. `source` like `"bing news"` / `"hackernews"`.
- `score` values in these are raw floats (e.g. `0.9000000357627869`) — display as a percentage or hide; don't show raw float noise to users.

### 6.8 Introspection endpoints (all `?q=...`, open) — for the explainer panels
- **`/analyze`** (negation transparency): `{ query, contrastive_framing, exclusions[], declined[], manner_qualifiers[], decisions:[{term, decision, reason}] }`. decision ∈ `exclusion|declined|manner_qualifier`.
- **`/geolocate`** `?q=...[&ip=...]`: `{ query, resolved: GeoLocation|null, source: "explicit"|"local_intent_fallback"|"ip"|"none", explicit_location, local_intent }`. GeoLocation: `{ country_code, country_name, region, city, postal_code, latitude, longitude, time_zone }`.
- **`/intent`**: `{ query, intent, category, confidence, contrastive_framing, local_intent, structured_constraints, expanded_queries }`.
- **`/video`**: `{ query, video_intent, video_intent_markers, would_pin_non_video_sources, is_video_source_examples[], intent, note }`.
- **`/inspect`**: full pipeline preview — `{ query, spelling, negation, intent, constraints, recency, quality }`. `constraints.structured` carries the full `structured_constraints` grid; `recency` = `{ window: null|{after,before}, phrase_detected }`; `quality` = `{ flag: "junk"|"low"|"normal"|"", valid_ratio }`.

### 6.9 Shopping / commerce (KEYED) — honest-provenance shape
`GET /shopping?q=...` enriches `/search` results for **exact-model queries**. Each result may carry a `commerce` block (verbatim):
```json
"commerce": {
  "url": "https://merchant.example/p/123",
  "observed_at": 1720000000,
  "source": "json-ld",
  "data": {
    "price": 89.99, "currency": "USD", "availability": "InStock",
    "merchant": "MerchantCo", "condition": "New", "sku": "SKU-1", "gtin": "0874000000000",
    "rating": 4.6, "rating_count": 1200,
    "price_low": null, "price_high": null, "offer_count": 1,
    "name": "Wireless Earbuds Pro", "price_valid_until": "2026-12-31", "image": "https://..."
  }
}
```
Top-level `offer_comparisons[]` (when ≥2 merchants match the same `gtin`/`sku`):
```json
{"product_id":"0874000000000","id_kind":"gtin","mixed_observation_times":false,
 "offers":[{"merchant":"M1","url":"...","price":89.99,"currency":"USD","availability":"InStock","condition":"New","observed_at":1720000000,"source":"json-ld"}, ...]}
```
Render rules: label `observed_at` age ("price observed 2h ago") — **never present as current without it**; if `mixed_observation_times` true, note the spread is not simultaneous; honor `price_valid_until` as a limited-time offer label; show `image` when present. `price_low`/`price_high` used when the page exposes a price range (single `price` null then).

`POST /commerce/extract` body `{html, url}` (both required-or-one): returns the extracted `commerce` block; `400 empty_request` / `400 invalid_url` (url present but not absolute http(s)).

---

## 7. ERROR & EDGE HANDLING CHEAT-SHEET
- `401 unauthorized` → prompt for API key (Settings). Especially on Goals & Commerce.
- `400` → show the `message`/`error` from the envelope; don't render an empty result page as success.
- Missing/omitted optional JSON keys → safe defaults (see §4.2).
- Empty `results:[]` with a 200 → genuine "no results" state; render the intelligence panel + expanded_queries chips so the user can pivot.
- Network failure/timeout → retry affordance; do not silently render nothing.
- Treat `spell_corrected_query`, `applied_constraints`, `expanded_queries`, `offer_comparisons`, `commerce`, `published_date`, `geo_location` as **always-optional**.

---

## 8. DELIVERABLES CHECKLIST
1. Working SPA with Search (Web/Images/Videos/News tabs + advanced operators), Goals (create/quick/discovery wizard/roadmap/progress/leaderboard), Settings (API key), and future-tab placeholders.
2. All navigation and data bound to the real endpoints in §6 with authenticated headers for keyed routes.
3. Themeable design system (CSS variables), dark default, light toggle, responsive, accessible.
4. Progressive loading (fast→full results), skeletons, and complete empty/error states.
5. Provenance labeling for sources, dates, and (if present) commerce prices — never present stale/unknown as current.
6. Fully self-hosted/static (no external CDNs, no tracking). Relative same-origin API paths.
```
