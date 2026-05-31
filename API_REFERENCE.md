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
- [Response Structures](#response-structures)
- [Intent Classification](#intent-classification)
- [Constraint Extraction](#constraint-extraction)
- [Scoring & Ranking](#scoring--ranking)
- [Caching](#caching)
- [Error Handling](#error-handling)
- [Examples](#examples)

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

Full search endpoint. Queries multiple backends (SearXNG, Whoogle, Invidious, local index) in parallel, classifies intent, extracts constraints, deduplicates, scores, and ranks results.

**Query Parameters**

| Parameter | Type   | Required | Description                          |
|-----------|--------|----------|--------------------------------------|
| `q`       | string | yes      | Search query (URL-encoded)           |

**Response** `200 OK`

```json
{
  "intent": "technical",
  "category": "informational",
  "confidence": 0.75,
  "constraints": ["+python", "+web", "-django"],
  "structured_constraints": {
    "positive": ["python", "web"],
    "negative": ["django"]
  },
  "expanded_queries": ["python web framework", "python web development"],
  "results": [
    {
      "url": "https://bottlepy.org/docs/dev/",
      "title": "Bottle: Python Web Framework",
      "content": "Bottle is a fast, simple and lightweight WSGI micro web-framework...",
      "score": 0.970,
      "authority": 0.90,
      "sources": ["startpage", "bing", "mojeek"],
      "is_local": false
    }
  ]
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

| Parameter | Type   | Required | Description                |
|-----------|--------|----------|----------------------------|
| `q`       | string | yes      | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "url": "https://example.com/page",
      "title": "Example Page Title",
      "content": "Snippet of page content...",
      "score": 0.85,
      "authority": 0.70
    }
  ],
  "count": 12,
  "query": "python web framework"
}
```

**Notes**
- No intent classification or constraint extraction.
- Results come from the local crawl index only.
- Useful for frontend: call `/search/fast` + `/search` in parallel for instant + full results.

---

### `GET /images`

Image search via SearXNG (`categories=images`).

**Query Parameters**

| Parameter | Type   | Required | Description                |
|-----------|--------|----------|----------------------------|
| `q`       | string | yes      | Search query (URL-encoded) |

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
      "source": "bing_images"
    }
  ],
  "count": 20,
  "query": "rust programming"
}
```

---

### `GET /videos`

Video search via SearXNG (`categories=videos`) and Invidious.

**Query Parameters**

| Parameter | Type   | Required | Description                |
|-----------|--------|----------|----------------------------|
| `q`       | string | yes      | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "title": "Video Title",
      "url": "https://youtube.com/watch?v=...",
      "description": "Video description...",
      "video_id": "dQw4w9WgXcQ",
      "thumbnail": "https://i.ytimg.com/vi/.../hqdefault.jpg",
      "source": "youtube"
    }
  ],
  "count": 15,
  "query": "rust tutorial"
}
```

---

### `GET /news`

News search via SearXNG (`categories=news`).

**Query Parameters**

| Parameter | Type   | Required | Description                |
|-----------|--------|----------|----------------------------|
| `q`       | string | yes      | Search query (URL-encoded) |

**Response** `200 OK`

```json
{
  "results": [
    {
      "title": "News Article Title",
      "url": "https://news.example.com/article",
      "description": "Article summary or snippet...",
      "published_at": "2026-05-30T12:00:00",
      "source": "google_news"
    }
  ],
  "count": 10,
  "query": "AI news"
}
```

---

## Response Structures

### `MergedResult` (from `/search`)

The unified result type used by the main search endpoint. Results can come from local index, web search engines, or both. When a URL appears in multiple sources, sources are merged and a consensus boost is applied.

| Field      | Type     | Description |
|------------|----------|-------------|
| `url`      | string   | Result URL |
| `title`    | string   | Page title |
| `content`  | string   | Snippet or description |
| `score`    | float    | Relevance score (0.0–1.0) |
| `authority`| float    | Domain authority score (0.0–1.0) |
| `sources`  | string[] | Which backends returned this result (e.g. `["startpage", "bing", "local"]`) |
| `is_local` | bool     | Whether this result came from the local crawl index |

### `UnifiedResponse` (from `/search`)

| Field                    | Type            | Description |
|--------------------------|-----------------|-------------|
| `intent`                 | string          | Detailed intent subtype (see below) |
| `category`               | string          | Standard search category (`navigational`, `informational`, `transactional`) |
| `confidence`             | float           | Intent classification confidence (0.0–1.0) |
| `constraints`            | string[]        | Legacy constraint format (`["+python", "-django"]`) |
| `structured_constraints` | object          | Parsed constraints with `positive` and `negative` string arrays |
| `expanded_queries`       | string[]        | Query expansions / reformulations |
| `results`                | MergedResult[]  | Ranked search results |

---

## Intent Classification

The intent engine classifies queries into detailed subtypes. The `category` field maps these to standard search categories.

| Intent          | Category        | Description | Example |
|-----------------|-----------------|-------------|---------|
| `navigational`  | navigational    | Looking for a specific site or page | "python docs", "github login" |
| `informational` | informational   | General knowledge seeking | "what is quantum computing" |
| `technical`     | informational   | Developer/technical docs | "rust async web framework" |
| `how-to`        | informational   | Step-by-step instructions | "how to deploy docker" |
| `comparison`    | informational   | Comparing options | "react vs vue vs angular" |
| `fresh`         | informational   | Time-sensitive / news | "latest AI news today" |
| `transactional` | transactional   | Purchase/action intent | "buy domain name" |

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

**Domain authority signals** (fully algorithmic, no hardcoded lists):
- `.edu` / `.gov` TLDs: +0.3
- `.org` / `.net` TLDs: +0.1
- Documentation subdomains (`docs.`, `api.`, `dev.`): +0.25
- Documentation paths (`/docs/`, `/api/`, `/reference/`): +0.2
- Clean bare domains (2-part host): +0.1
- Spam/clickbait path patterns: -0.2

**Freshness half-lives** (per intent):
- `fresh` (news): 6 hours
- `how-to` / `informational`: 7 days
- `comparison`: 14 days
- `technical` / `transactional`: 30 days
- `navigational`: 90 days

---

## Caching

All endpoints cache responses by query (case-insensitive, trimmed).

| Endpoint       | Cache TTL |
|----------------|-----------|
| `/search`      | 1800s (30 min) |
| `/search/fast` | 1800s (30 min) |
| `/images`      | 300s (5 min) |
| `/videos`      | 300s (5 min) |
| `/news`        | 300s (5 min) |

Cache keys are normalized to lowercase and trimmed. Cached responses are served instantly (~6–15ms).

---

## Error Handling

| Status | Body | When |
|--------|------|------|
| `200`  | `{"intent":...,"results":[]}` | Valid query, no results found |
| `400`  | `{"error":"Missing or empty query parameter 'q'"}` | `q` is missing, empty, or whitespace |
| `400`  | `{"error":"Query must contain at least one alphabetic character"}` | `q` has no letters (e.g. `"123"`, `"][]["`) |

The API never returns 5xx to clients. Internal errors (SearXNG timeout, engine failures) are handled gracefully — the gateway returns partial results from whichever backends succeeded, or an empty results array if all failed.

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
```

---

## Architecture

```
Client → Traefik (SSL) → Gateway (port 4000)
                              │
                              ├→ Intent Engine (classify + extract constraints)
                              ├→ SearXNG (web search via VPN — bing, brave, startpage, mojeek, duckduckgo)
                              ├→ SearXNG2 (parallel VPN fan-out)
                              ├→ Whoogle (Google via Tor)
                              ├→ Invidious (YouTube videos)
                              ├→ Local Index (crawled pages)
                              └→ Deduplicate + Score + Rank → Response
```

All search backends are queried in parallel. Results are deduplicated by URL, scored using multi-signal ranking, and returned as a unified list.

**Privacy:** All outbound search requests go through VPN (Gluetun) or Tor. No user data, tracking, or analytics is included in responses.
