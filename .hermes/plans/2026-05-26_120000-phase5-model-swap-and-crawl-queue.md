# IntentForge Phase 5: Model Swap + Crawl Queue + Intent-Driven Search

**Date:** 2026-05-26
**Goal:** Replace heavy Qwen2.5-0.5B with lightweight classifier, build real crawl queue, implement intent-driven ranking — all under 2s latency.

---

## Current State (from code audit)

| Component | Status | Problem |
|-----------|--------|---------|
| Intent Engine | Qwen2.5-0.5B GGUF (400MB) | 500-1500ms per query, overkill for classification |
| Embeddings | all-MiniLM-L6-v2 (22M, 80MB) | Already good, keep it |
| Crawler | Single URL fetch, no queue | No URL discovery, no crawl scheduling, no dedup |
| Indexer | Tantivy BM25 + cosine | Working, needs content-type-aware scheduling |
| Meta-Search | SearXNG (Bing+Brave), Whoogle, Invidious | Results fire-and-forget, not indexed |
| Gateway | Axum orchestrator | 5s timeout, no engine rotation, no caching |

---

## Phase 5A: Model Swap (Intent Engine Rewrite)

### What Changes
**File:** `services/intent-engine/src/main.rs`
**File:** `services/intent-engine/Cargo.toml`

### Architecture: Two-Layer Intent System

```
Layer 1: Rule-Based Pre-Classifier (< 1ms)
  - Regex patterns catch 60-70% of queries instantly
  - High confidence → skip Layer 2 entirely
  - Categories: navigational, technical, how-to, comparison, fresh, transactional

Layer 2: Embedding Similarity Classifier (~2ms)
  - Uses existing all-MiniLM-L6-v2 (already loaded!)
  - Pre-compute centroid embeddings for each intent category
  - Cosine similarity to each centroid → highest wins
  - NO additional model needed — reuses embedding model
  - Accuracy ~85-90% (sufficient, DistilBERT can be added later if needed)
```

### Why NOT DistilBERT (for now)
- Adding DistilBERT requires ONNX Runtime or rust-bert dependency (heavy)
- The existing MiniLM-L6-v2 embeddings can classify intent via centroid similarity
- Rule-based Layer 1 handles the obvious cases (<1ms)
- This gets us to <15ms intent classification with ZERO new dependencies
- DistilBERT can be added as Layer 3 later if accuracy is insufficient

### Implementation Steps

1. **Remove Qwen2.5-0.5B** from intent-engine
   - Remove: `candle-transformers` dependency (only needed for Qwen)
   - Remove: Qwen model loading, tokenizer, generation loop
   - Remove: `models/qwen2.5-0.5b-instruct-q4_k_m.gguf`, `models/tokenizer.json`
   - Remove: `setup.sh` Qwen download

2. **Implement Rule-Based Pre-Classifier** (Layer 1)
   - Regex patterns for each intent category
   - Returns (intent, confidence) tuple
   - If confidence > 0.85 → return immediately

3. **Implement Embedding Centroid Classifier** (Layer 2)
   - Define centroid embeddings for 6 categories (computed from example queries)
   - Store centroids as static arrays in code (or load from JSON)
   - At query time: compute embedding, cosine similarity to each centroid
   - Return highest-scoring intent if similarity > 0.6

4. **Update `/analyze` endpoint**
   - Remove generative JSON output (constraints, expanded_queries)
   - Return: `{ intent, confidence, constraints: [], expanded_queries: [original_query] }`
   - Constraints and expanded_queries can be added back later with heuristics

5. **Update `setup.sh`**
   - Remove Qwen download
   - Keep MiniLM-L6-v2 download (still needed for embeddings)

### Intent Categories (6)
- `navigational` — user wants a specific site/page
- `informational` — user wants to learn something
- `technical` — user needs code/APIs/dev docs
- `how-to` — user wants step-by-step instructions
- `comparison` — user wants to compare options
- `transactional` — user wants to buy/download/act
- `fresh` — user wants recent/timely info

---

## Phase 5B: Crawl Queue Service

### What Changes
**File:** `services/crawler/src/main.rs` (major rewrite)
**File:** `services/crawler/Cargo.toml` (add dependencies)

### Architecture

```
Meta-Search Results → Crawl Queue (priority, dedup) → Crawl Workers (Tor/VPN) → Indexer
                           ↑                                    |
                           |                                    ↓
                      Seed URLs                          Link Extraction → URL Discovery
```

### Implementation Steps

1. **Build Crawl Queue** (in-memory + file-backed)
   - Priority queue using `BinaryHeap` (priority = search rank)
   - URL deduplication using `HashSet` + URL normalization
   - Per-domain rate limiter (sliding window, 2s between same-domain)
   - Crawl status tracking: pending → crawling → completed/failed
   - File-backed persistence (JSON/TOML) for queue survival across restarts

2. **Add Crawl Scheduling** (content-type-aware)
   - News/articles: 1h refresh
   - Documentation: 24h refresh
   - Homepages: 12h refresh
   - Forums: 2h refresh
   - Default: 4h refresh
   - Detect from URL patterns + content headers

3. **Add URL Discovery**
   - Extract links from crawled pages
   - Normalize URLs (remove fragments, trailing slashes)
   - Filter: same-depth relevance check
   - Cap: max 20 discovered URLs per source page

4. **Add Seed URL List**
   - ~200 high-value domains in config file
   - Categories: reference, tech docs, Q&A, code, news, packages
   - Crawled on startup, refreshed per schedule

5. **Wire Meta-Search → Crawl Queue**
   - Gateway sends top 10 meta-search results to crawl queue after each search
   - HTTP POST to crawler `/enqueue` endpoint
   - Crawler deduplicates against existing index

6. **Update Auto-Updater**
   - Replace blanket 300s with content-type-aware scheduling
   - Check crawl_queue table, not just "older than 300s"

### New Dependencies (Cargo.toml)
```toml
url = "2"           # URL normalization
bloom-filter = "0.1" # Fast URL dedup (optional, HashSet may suffice)
```

---

## Phase 5C: Gateway Intent-Driven Ranking

### What Changes
**File:** `services/gateway/src/main.rs`

### Implementation Steps

1. **Update `calculate_intent_boost`**
   - Make it algorithmic (signal-based, not keyword-list)
   - Add domain authority scoring
   - Add freshness decay scoring
   - Add local index bonus

2. **Multi-Signal Ranking**
   ```
   final_score = (w1 * rrf_score)           // 0.30 - Rank fusion
               + (w2 * intent_boost)         // 0.20 - Intent match
               + (w3 * freshness_score)      // 0.15 - Recency
               + (w4 * authority_score)      // 0.15 - Domain authority
               + (w5 * local_index_bonus)    // 0.10 - Local trust
               + (w6 * embedding_similarity) // 0.10 - Semantic match
   ```

3. **Domain Authority Table**
   - Pre-computed scores for known domains
   - High: wikipedia.org, github.com, stackoverflow.com, MDN, official docs
   - Medium: dev.to, medium.com, reddit.com, news sites
   - Low: unknown, content farms, SEO spam
   - File-backed, updatable

4. **Reduce Timeout**
   - Change from 5s to 1.5s for meta-search
   - Add 100-300ms jitter between engine requests

5. **Cache Meta-Search Results**
   - 5-min TTL cache for identical queries
   - Reduces rate limiting pressure

---

## Phase 5D: Engine Rotation & Anti-Rate-Limiting

### What Changes
**File:** `services/meta-search-engines/searxng/searxng/settings.yml`
**File:** `services/gateway/src/main.rs`

### Implementation Steps

1. **Enable More SearXNG Engines**
   - Enable: Qwant, Startpage, Mojeek, DuckDuckGo
   - Keep: Bing, Brave
   - Disable: Google (CAPTCHA-prone via SearXNG)

2. **Engine Rotation in Gateway**
   - Round-robin or random engine selection per query
   - Backoff: if engine returns error/429, disable for 15min
   - Health check: periodic liveness probes

3. **Request Spacing**
   - 100-300ms jitter between engine requests
   - Per-engine sliding window rate limiter

---

## Execution Order

1. **Phase 5A** (Model Swap) — biggest latency win, no new deps
2. **Phase 5B** (Crawl Queue) — enables self-improving index
3. **Phase 5C** (Ranking) — improves result quality
4. **Phase 5D** (Engine Rotation) — reduces rate limiting

---

## Latency Budget (Target: <2s)

| Component | Current | After Phase 5A | After All |
|-----------|---------|----------------|-----------|
| Intent classify | 500-1500ms | <15ms | <15ms |
| Embedding | 50-100ms | 50-100ms | 50-100ms |
| Meta-search | 1000-4000ms | 1000-4000ms | <1500ms (timeout) |
| Local index | 10-50ms | 10-50ms | 10-50ms |
| Ranking | 5ms | 5ms | <10ms |
| **Total** | **2000-6000ms** | **1100-4200ms** | **<1500ms** |

---

## Files Changed

| File | Change |
|------|--------|
| `services/intent-engine/src/main.rs` | Remove Qwen, add rule-based + centroid classifier |
| `services/intent-engine/Cargo.toml` | Remove candle-transformers |
| `services/intent-engine/setup.sh` | Remove Qwen download |
| `services/crawler/src/main.rs` | Add crawl queue, URL discovery, seed URLs, scheduling |
| `services/crawler/Cargo.toml` | Add url crate |
| `services/crawler/seed_urls.toml` | New: seed URL config |
| `services/gateway/src/main.rs` | Multi-signal ranking, engine rotation, caching |
| `services/gateway/Cargo.toml` | Add moka cache |
| `services/meta-search-engines/searxng/searxng/settings.yml` | Enable more engines |
| `MILESTONES.md` | Update progress |
| `HERMES_GOAL.md` | Update status |

---

## Risks & Mitigations

1. **Centroid classifier accuracy** — if <85%, add DistilBERT as Layer 3
2. **Crawl queue memory** — cap at 10K URLs, file-back for persistence
3. **Tor latency for crawling** — use VPN for user-facing, Tor for bulk crawl
4. **SearXNG engine failures** — circuit breaker + fallback to local index
5. **Docker build time** — ~40min, test changes incrementally
