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

```json
{
  "query": "python web framework",
  "intent": "technical",
  "category": "informational",
  "confidence": 0.75,
  "constraints": ["+python", "+web", "-django"],
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
  "expanded_queries": ["python web framework", "python web framework documentation", "python web framework examples"],
  "distribution": {
    "navigational": 0.39,
    "informational": 0.20,
    "technical": 0.15,
    "how-to": 0.12,
    "comparison": 0.08,
    "fresh": 0.04,
    "transactional": 0.02
  },
  "results": [
    {
      "url": "https://bottlepy.org/docs/dev/",
      "title": "Bottle: Python Web Framework",
      "content": "Bottle is a fast, simple and lightweight WSGI micro web-framework...",
      "score": 0.970,
      "authority": 0.90,
      "sources": ["bing", "brave", "duckduckgo"],
      "is_local": false,
      "published_date": null
    }
  ],
  "results_before_filter": 24,
  "results_after_filter": 18,
  "total": 18,
  "limit": 10,
  "offset": 0,
  "has_more": true,
  "applied_constraints": ["not:django", "site:docs.python.org"],
  "ignored_constraints": ["price:>0"],
  "warnings": null,
  "spell_corrected_query": null,
  "geo_location": null,
  "query_quality": "high"
}
```

**Status Codes**

| Code | Meaning                                     |
|------|---------------------------------------------|
| 200  | Success (may return empty `results: []`)    |
| 400  | Missing, empty, or non-alphabetic query `q` |

---

### `GET /search/fast`

Fast search endpoint. Returns results from the local crawl index only — no SearXNG, no intent analysis, no constraint extraction. Designed for instant feedback (~100ms) while `/search` runs in parallel.

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |
| `limit`   | int    | no       | `24`    | Max results to return      |

**Response** `200 OK`

```json
{
  "results": [
    {
      "url": "https://example.com/page",
      "title": "Example Page Title",
      "content": "Snippet of page content...",
      "score": 0.85,
      "authority": 0.70,
      "is_local": true,
      "sources": ["local"]
    }
  ],
  "count": 12,
  "source": "local"
}
```

**Note:** The top-level `source` field is always `"local"` for this endpoint. Each result also carries a `sources` array inside its object.

**Notes**
- No intent classification, constraint extraction, or spell correction.
- No pagination metadata (`total`, `has_more`, etc.) — only returns a snapshot.
- Results come from the local crawl index only.
- Useful for frontend: call `/search/fast` + `/search` in parallel for instant + full results.

---

### `GET /images`

Image search via SearXNG (`categories=images`).

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "title": "Image Title",
      "url": "https://example.com/page",
      "image_url": "https://example.com/image.jpg",
      "thumbnail_url": "https://example.com/thumb.jpg",
      "description": "Image description or alt text",
      "source": "bing images",
      "score": 0.92
    }
  ],
  "count": 20,
  "query": "rust programming"
}
```

---

### `GET /videos`

Video search via SearXNG (`categories=videos`).

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "title": "Video Title",
      "url": "https://youtube.com/watch?v=...",
      "description": "Video description...",
      "video_id": "",
      "thumbnail": "https://i.ytimg.com/vi/.../hqdefault.jpg",
      "source": "bing videos",
      "score": 0.95
    }
  ],
  "count": 20,
  "query": "rust tutorial"
}
```

---

### `GET /news`

News search via SearXNG (`categories=news`).

**Query Parameters**

| Parameter | Type   | Required | Default | Description                |
|-----------|--------|----------|---------|----------------------------|
| `q`       | string | yes      | —       | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "title": "News Article Title",
      "url": "https://news.example.com/article",
      "description": "Article summary or snippet...",
      "published_at": "2026-05-30T12:00:00",
      "source": "hackernews",
      "score": 0.90
    }
  ],
  "count": 10,
  "query": "AI news"
}
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

**Multiple operators** can be combined: `site:arxiv.org site:wikipedia.org quantum computing` → both sites are applied.

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
| `after_date` | string | Date lower bound (YYYY-MM-DD) |
| `before_date` | string | Date upper bound (YYYY-MM-DD) |
| `price_min` | float | Minimum price |
| `price_max` | float | Maximum price |
| `price_lt` | float | Upper price bound from `<` operator |
| `price_gt` | float | Lower price bound from `>` operator |

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
- Short words (< 3 chars) and known 3-letter terms are left unchanged
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

All endpoints cache responses by query (case-insensitive, trimmed). Cache keys are normalized to lowercase and trimmed. Cached responses are served instantly (~6–15ms).

| Endpoint | Cache TTL |
|----------|-----------|
| `/search` | 1800s (30 min) |
| `/search/fast` | 1800s (30 min) |
| `/images` | 300s (5 min) |
| `/videos` | 300s (5 min) |
| `/news` | 300s (5 min) |

The cache is reset on the first request after expiry, not on a fixed interval. This means traffic spikes only see at most one cache-miss request per TTL window.

---

## Error Handling

| Status | Body | When |
|--------|------|------|
| `200`  | `{"intent":...,"error":"upstream_unavailable","message":"All upstream search engines timed out...","results":[]}` | All upstream search engines failed (timeout/rate-limit) |
| `200`  | `{"intent":...,"results":[]}` | Valid query, no results found from any backend |
| `400`  | `{"error":"empty_query","message":"Query parameter 'q' is empty","results":[]}` | `q` is missing, empty, or whitespace |
| `400`  | `{"error":"empty_query","message":"Query must contain at least one alphabetic character","results":[]}` | `q` has no letters (e.g. `"123"`, `"][]["`) |

The API never returns 5xx to clients. Internal errors (SearXNG timeout, engine failures) are handled gracefully — the gateway returns partial results from whichever backends succeeded, or an empty results array with a diagnostic `message` if all failed.

**Upstream failure diagnostics:**
When all search engines fail, the response includes:
- `error: "upstream_unavailable"`
- `message: "All upstream search engines timed out or failed to respond. This is a temporary upstream/connectivity issue, not a genuine zero-hit. Please retry."`
- `warnings: ["No web results were returned by the upstream search engines for this query."]`

---

## Performance & Stress Test Results

Results from live testing of the development instance (`localhost:4000`):

| Metric | Value |
|--------|-------|
| Cold start (first request) | ~10.1s (cache miss; includes backend warmup) |
| Warm request latency | **2.8–5.7ms** (cached) |
| Concurrent request handling | 5 simultaneous requests: all 200 OK, ~4.6–5.7ms each |
| Sequential request consistency | Requests 2–10: all < 4ms, no degradation |
| Cache-hit ratio | Nearly 100% for repeated queries within TTL window |

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
| `POST` | `/goals` | Create a goal → get 6–9 domain-specific questions |
| `POST` | `/goals/:goal_id/answers` | Submit answers → get full phased roadmap |
| `GET`  | `/goals/:goal_id` | Get goal status and roadmap by ID |
| `GET`  | `/goals/leaderboard` | Get leaderboard of all goals (sorted by score) |
| `POST` | `/goals/quick` | One-shot: goal → full roadmap immediately (no questions) |

---

### `POST /goals`

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

Goals are classified using the intent engine (same endpoint used by `/search`). The engine extracts one of:

| Intent | Typical Goal Keywords |
|--------|----------------------|
| `technical` | build, develop, create app, software |
| `informational` | research, paper, study |
| `how-to` | learn, understand, master |
| `comparison` | compare, vs, alternative |
| `informational` (default) | falls through with no strong signal |

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
