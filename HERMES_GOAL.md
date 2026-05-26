# IntentForge-v2 — Hermes Agent Goal Specification v2

## Mission Statement
Build a self-hosted, privacy-first search engine that delivers excellent results
in under 2 seconds for both general and complex queries — without relying on
external AI APIs and without being rate-limited by meta search engines.
The system must run entirely on CPU, be lightweight, and continuously improve
its own index through automated crawling fed by meta-search results.

---

## 1. Model Selection — Kill the 0.5B LLM, Use a Classifier Instead

### Current State
- Intent engine uses **Qwen2.5-0.5B (Q4_K_M GGUF)** via Candle for intent analysis
- Embedding uses **all-MiniLM-L6-v2** (22M params, 80MB) — this is already good, keep it
- Problem: 0.5B model is heavy for CPU inference (~500-1500ms per query), adds massive latency

### Key Insight: DON'T use an LLM for intent classification

Google does NOT use an LLM to classify search intent. They use lightweight
classifiers + rule-based systems. Intent classification is a **narrow
classification task** — not a generation task. Using a 0.5B generative model
for it is like using a Ferrari to deliver pizza.

### The Right Architecture: Two-Layer Intent System

```
Layer 1: Rule-Based Pre-Classifier (< 1ms)
  - Regex patterns catch 60-70% of queries instantly
  - "how to X" → informational/how-to
  - "buy X" / "download X" → transactional
  - "X.com" / "official X" → navigational
  - "latest X" / "X 2026" → fresh/news
  - "X vs Y" / "best X" → comparison/commercial
  - "X API" / "X library" / "X documentation" → technical
  - If confidence is high → SKIP Layer 2 entirely

Layer 2: DistilBERT Classifier (5-15ms on CPU)
  - Only runs when Layer 1 is ambiguous
  - Fine-tuned distilbert-base-uncased (66M params, 260MB)
  - Outputs: one of 6 intent labels + confidence score
  - If confidence < 0.7 → fallback to informational
```

### Why DistilBERT (Not SmolLM2, Not Qwen, Not Gemma)

| Model | Params | Size | CPU Inference | Task | Notes |
|-------|--------|------|---------------|------|-------|
| **distilbert-base-uncased** | **66M** | **260MB** | **5-15ms** | **Classification** | **BEST FIT. Purpose-built for classification. HuggingFace pipelines.** |
| all-MiniLM-L6-v2 | 22M | 80MB | 2-5ms | Embeddings | Already in use. Can also classify with similarity. |
| SmolLM2-135M-Instruct | 135M | 80MB GGUF | 100-300ms | Generation | Still an LLM. Overkill for classification. |
| Qwen2.5-0.5B | 500M | 400MB GGUF | 500-1500ms | Generation | CURRENT. Way too heavy. |
| bge-small-en-v1.5 | 33M | 130MB | 3-8ms | Embeddings | Good but DistilBERT is better for classification. |
| gte-small | 33M | 130MB | 3-8ms | Embeddings | Same — embedding model, not classifier. |

### Google's Approach (Reference)
Google's search intent system uses:
1. **Query structure analysis** — navigational queries contain brand names, URLs, "official"
2. **Click-through signals** — what results people click trains the classifier
3. **Knowledge graph matching** — entity recognition boosts navigational intent
4. **Lightweight classifiers** — NOT LLMs. BERT-tiny/DistilBERT-class models.
5. **Rule-based fallbacks** — regex patterns for obvious cases

Their Gemma 3 models (1B, 4B, 12B) are for on-device GENERATION tasks,
not for search intent classification. We should follow their classification
approach, not their generation approach.

### Implementation Plan
1. Keep all-MiniLM-L6-v2 for embeddings (already optimal at 22M params)
2. Add DistilBERT for intent classification via `rust-bert` or ONNX Runtime
3. Build rule-based pre-classifier as Layer 1 (regex, <1ms)
4. Remove Qwen2.5-0.5B entirely — it's dead weight
5. If `rust-bert` is too heavy, use ONNX Runtime with quantized DistilBERT (INT8, ~66MB)

### Fallback: If DistilBERT integration is too complex
Use the existing all-MiniLM-L6-v2 embeddings for intent classification too:
- Pre-compute embeddings for representative queries per intent category
- At query time, compute query embedding, find nearest intent centroid
- This is ~2ms and requires no additional model
- Accuracy will be lower (~85% vs ~93% for DistilBERT) but still good

---

## 2. Query Intent Categories — 6 Broad Categories (Algorithmic, Not Hardcoded)

### Taxonomy

```
┌─────────────────────────────────────────────────────────────────┐
│                    QUERY INTENT TAXONOMY                         │
├──────────────┬──────────────────────────────────────────────────┤
│ NAVIGATIONAL │ User wants to reach a specific site/page         │
│              │ Boost: official sites, docs, homepages, wikis    │
│              │ Signals: brand names, ".com", "official", URLs   │
│              │ Sources: local index (if crawled) + direct match │
│              │ Examples: "python docs", "github login", "mdn"   │
├──────────────┼──────────────────────────────────────────────────┤
│ INFORMATIONAL│ User wants to learn/understand something          │
│              │ Boost: wikis, tutorials, guides, blogs, forums   │
│              │ Signals: "what is", "how to", "explain", "why"  │
│              │ Sources: all (meta-search + local index)         │
│              │ Examples: "what is rust borrow checker"          │
├──────────────┼──────────────────────────────────────────────────┤
│ TRANSACTIONAL│ User wants to DO something (buy, download, sign) │
│              │ Boost: product pages, pricing, download links    │
│              │ Signals: "buy", "download", "install", "sign up"│
│              │ Sources: meta-search (commercial engines)        │
│              │ Examples: "buy mechanical keyboard", "download X"│
├──────────────┼──────────────────────────────────────────────────┤
│ TECHNICAL    │ User needs code, APIs, or dev documentation      │
│              │ Boost: GitHub, docs.rs, StackOverflow, MDN       │
│              │ Signals: language names, "API", "library", "SDK" │
│              │ Sources: local index (crawled docs) + meta-search│
│              │ Examples: "rust async runtime", "python requests"│
├──────────────┼──────────────────────────────────────────────────┤
│ FRESH/NEWS   │ User wants recent/timely information              │
│              │ Boost: news sites, recent timestamps, RSS feeds  │
│              │ Signals: "latest", year mentions, "today", CVEs  │
│              │ Sources: meta-search + RSS feeds + news APIs     │
│              │ Examples: "latest CVE", "rust 1.80 release"      │
├──────────────┼──────────────────────────────────────────────────┤
│ COMPARISON   │ User wants to compare options or find "best"      │
│              │ Boost: review sites, comparison tables, forums   │
│              │ Signals: "vs", "versus", "best", "top", "compare"│
│              │ Sources: meta-search + forums                    │
│              │ Examples: "rust vs go performance", "best IDE"   │
└──────────────┴──────────────────────────────────────────────────┘
```

### Why 6 (Not 5, Not 10)
- 5 was missing COMPARISON — a very common intent ("X vs Y", "best X")
- 6 categories cover 97%+ of real queries
- Each maps to a DIFFERENT retrieval strategy and boost profile
- The classifier (DistilBERT or embedding similarity) handles 6 labels reliably
- Too many categories = confusion, diluted boost weights

### Algorithmic Intent Detection (NOT Hardcoded Keywords)

The intent classifier should use SIGNALS, not keyword lists:

1. **Query structure signals** (rule-based, <1ms):
   - Contains URL or domain pattern → navigational (boost: 2.0)
   - Starts with "how to" / "what is" / "why does" → informational (boost: 1.5)
   - Contains "vs" / "versus" / "compared to" → comparison (boost: 1.3)
   - Contains year or "latest" / "recent" / "new" → fresh (boost: 1.8)
   - Contains "buy" / "price" / "download" → transactional (boost: 1.4)
   - Contains programming language names / "API" / "docs" → technical (boost: 1.6)

2. **Embedding similarity** (when rule-based is ambiguous):
   - Pre-compute centroid embeddings for each category from 50+ example queries
   - At query time: compute query embedding, cosine similarity to each centroid
   - Highest similarity wins (if > 0.6 threshold)
   - Cost: ~2ms (embedding already computed for search)

3. **DistilBERT classifier** (highest accuracy, only when above are uncertain):
   - Fine-tuned on MS MARCO / query intent datasets
   - Runs in 5-15ms on CPU
   - Only triggered when both rule-based and embedding similarity are inconclusive

### Per-Category Retrieval Strategy

| Intent | Meta-Search Priority | Local Index Priority | Boost Domains | Timeout |
|--------|---------------------|---------------------|---------------|---------|
| Navigational | Low (fallback) | HIGH (if crawled) | Official sites, docs | 1.0s |
| Informational | HIGH | Medium | Wikis, guides, blogs | 1.5s |
| Transactional | HIGH | Low | Product pages, pricing | 1.5s |
| Technical | Medium | HIGH (crawled docs) | GitHub, docs.rs, SO | 1.5s |
| Fresh/News | HIGH | Low (stale data) | News sites, RSS | 1.0s |
| Comparison | HIGH | Medium | Review sites, forums | 1.5s |

---

## 3. Crawler & Indexer — Self-Updating Search Index

### Current State
- Crawler: single URL fetch via scraper crate, extracts title + content, sends to indexer
- Indexer: Tantivy full-text + embedding storage, RRF fusion search
- Auto-updater: checks every 60s for URLs older than 300s, re-crawls them
- Knowledge warming: gateway spawns crawl tasks for top 3 web results after each search
- Privacy: all traffic through Gluetun (ProtonVPN + Tor)

### Critical Problems
1. **No seed URL list** — crawler only activates on user searches
2. **No URL discovery** — crawler never finds new URLs on its own
3. **Meta search results are fire-and-forget** — SearXNG/Whoogle results shown but NOT indexed
4. **No crawl scheduling** — all URLs treated equally, no priority
5. **300s staleness is arbitrary** — news needs 1h, docs need 24h
6. **No crawl queue** — no priority queue, no dedup, no rate limiting per domain

### Goal — Build a Self-Improving Index with Crawl Queue

#### Architecture: Crawl Queue Service

```
┌─────────────────────────────────────────────────────────────────┐
│                    CRAWL PIPELINE                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ Meta-    │    │ URL      │    │ Crawl    │    │ Indexer  │  │
│  │ Search   │───▶│ Frontier │───▶│ Worker   │───▶│ (Tantivy │  │
│  │ Results  │    │ (Queue)  │    │ (Tor/VPN)│    │  +Embed) │  │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘  │
│       │               ▲               │               │         │
│       │               │               ▼               │         │
│       │          ┌────┴────┐    ┌──────────┐          │         │
│       │          │ URL     │◀──│ Link     │          │         │
│       │          │ Discovery│   │ Extractor│          │         │
│       │          └─────────┘    └──────────┘          │         │
│       │                                               │         │
│       │          ┌─────────┐                          │         │
│       └─────────▶│ Seed    │──────────────────────────┘         │
│                  │ URLs    │                                     │
│                  └─────────┘                                     │
└─────────────────────────────────────────────────────────────────┘
```

#### 3A. Meta-Search → Crawler Pipeline (CRITICAL — HIGHEST PRIORITY)

Every time SearXNG, Whoogle, or Invidious returns results, automatically
feed the top URLs into the crawl queue. This is how the index grows organically.

```
User Query → Gateway → Meta-Search (SearXNG/Whoogle/Invidious)
                              │
                              ▼
                     Top N results (URLs)
                              │
                              ▼
                     Crawl Queue (priority queue, deduped)
                              │
                              ▼
                     Crawl Worker (async, Tor/VPN)
                              │
                              ▼
                     Indexer (Tantivy + embeddings)
                              │
                              ▼
                     Future queries benefit from local index
```

Implementation:
- Gateway sends top 10 web results to crawl queue after every search
- Crawl queue deduplicates against existing index (skip if < 1 hour old)
- Priority: higher-ranked results get crawled first
- Rate limiting: max 5 concurrent crawls, 2s delay between same-domain requests
- The crawl queue is a new Rust service (or module in crawler) with:
  - Priority queue (binary heap, priority = search rank)
  - URL deduplication (Bloom filter for fast check, then exact DB check)
  - Per-domain rate limiting (sliding window)
  - Crawl status tracking (pending, crawling, completed, failed)

#### 3B. Smart Crawl Scheduling (Content-Type Aware)

Replace the blanket 300s refresh with content-type-aware scheduling:

| Content Type | Refresh Interval | Detection Method |
|---|---|---|
| News/articles | 1 hour | URL pattern (/news/, /article/) + recent date in content |
| Documentation | 24 hours | URL pattern (/docs/, /api/, /reference/) |
| Homepages | 12 hours | URL path is "/" or empty |
| Product pages | 6 hours | URL pattern (/product/, /p/, /item/) |
| Forum/discussion | 2 hours | URL pattern (/forum/, /thread/, /discussion/) |
| Everything else | 4 hours | Default |

Implementation:
- Each indexed URL stores: last_crawled_at, content_type, crawl_interval
- Auto-updater checks crawl_queue table, not just "older than 300s"
- Content type detected from URL patterns + content analysis

#### 3C. URL Discovery from Crawled Pages (Web Crawl Frontier)

When crawling a page, extract ALL links. Filter and queue relevant ones:

```
For each link on crawled page:
  1. Normalize URL (remove fragments, normalize trailing slashes)
  2. Check if already in index (Bloom filter → exact check)
  3. Compute embedding similarity to original query
  4. If similarity > 0.5 AND depth < 2 → add to crawl queue
  5. Cap: max 20 discovered URLs per source page
  6. Cap: max 100 total discovered URLs per original search query
```

This creates a web crawl frontier that grows from search queries.
Over time, the index becomes self-sustaining.

#### 3D. Seed URL List (Baseline Index)

Pre-crawl high-value domains on startup. Provides baseline results
even before any user queries. Refresh every 24 hours.

Seed categories:
- **Reference:** wikipedia.org, wiktionary.org, archive.org
- **Tech docs:** docs.python.org, doc.rust-lang.org, developer.mozilla.org,
  docs.rs, pkg.go.dev, learn.microsoft.com
- **Q&A:** stackoverflow.com, superuser.com, askubuntu.com
- **Code:** github.com/trending, gitlab.com/explore
- **News:** news.ycombinator.com, reddit.com/r/programming,
  arstechnica.com, theverge.com
- **Package registries:** crates.io, pypi.org, npmjs.com

Implementation:
- Seed URLs stored in a config file (TOML/JSON)
- Crawled on first startup, then refreshed per schedule
- Each seed domain has its own crawl_interval setting
- Total seed URLs: ~200-500 (manageable, grows over time)

#### 3E. Crawl Worker Architecture

The crawl worker should be a separate async task pool:

```rust
// Conceptual structure
struct CrawlWorker {
    client: reqwest::Client,        // with Tor/VPN proxy
    queue: CrawlQueue,              // shared priority queue
    rate_limiter: DomainRateLimiter, // per-domain sliding window
    max_concurrent: usize,          // 5 concurrent crawls
    politeness_delay: Duration,     // 2s between same-domain requests
}
```

Key behaviors:
- Pulls highest-priority URL from queue
- Checks domain rate limiter (skip if too recent)
- Fetches via Tor (for anonymity) or VPN (for speed)
- Extracts: title, content (via readability/trafilatura), links, metadata
- Computes embedding for content
- Sends to indexer (Tantivy + embedding storage)
- Extracts links → feeds back into URL discovery

---

## 4. Latency Target — Under 2 Seconds End-to-End

### Current Bottlenecks
| Component | Current | Target | How |
|---|---|---|---|
| Intent analysis (LLM) | ~500-1500ms | **<15ms** | Replace with DistilBERT classifier + rules |
| Embedding generation | ~50-100ms | **<10ms** | all-MiniLM-L6-v2 is already fast |
| Meta-search (SearXNG+Whoogle+Invidious) | ~1000-4000ms | **<1500ms** | Parallel + 1.5s timeout |
| Local index search | ~10-50ms | **<20ms** | Tantivy is already fast |
| Result fusion + ranking | ~5ms | **<10ms** | Already fast |
| **Total** | **~2000-6000ms** | **<1500ms** | **5-10x improvement** |

### How to Hit <2 Seconds

1. **Kill the 0.5B LLM** — This is the single biggest win. DistilBERT = 15ms vs 1500ms.
2. **Parallel everything** — Intent classification, embedding, and meta-search fire simultaneously.
3. **Rule-based pre-classifier** — 60-70% of queries skip the model entirely (<1ms).
4. **Cache aggressively** — Meta-search results cached 5 min (same query, same results).
5. **Timeout meta-search at 1.5s** — If SearXNG/Whoogle don't respond, proceed without them.
6. **Local index first** — Always search local index in parallel. It's fast (<20ms) and provides fallback results.
7. **Pre-warm everything** — Models loaded at startup, tensors pre-allocated.
8. **VPN for search, Tor for crawling** — VPN is faster for user-facing queries.

### Latency Budget

```
Time 0ms:    Query arrives at gateway
Time 0ms:    Fire parallel: [intent classify, embed, meta-search, local search]
Time 5ms:    Rule-based pre-classifier returns (if confident → done)
Time 15ms:   DistilBERT classifier returns (if needed)
Time 10ms:   Embedding computed
Time 20ms:   Local index results returned
Time 1500ms: Meta-search timeout (whichever responds first)
Time 1510ms: Result fusion + ranking
Time 1520ms: Response sent to user
```

Total: ~1.5s worst case (waiting for meta-search), ~50ms best case (local only + cached).

---

## 5. Privacy & Anti-Rate-Limiting Strategy

### Current State
- All services run through Gluetun (ProtonVPN + Tor)
- SearXNG has Google and DuckDuckGo disabled (rate limiting)
- Only Bing and Brave are active in SearXNG

### Goal — Never Be Rate-Limited, Ever

#### 5A. Engine Rotation
SearXNG already supports multiple engines. Configure rotation:
- **Primary pool:** Bing, Brave, Qwant, Startpage, Mojeek, DuckDuckGo
- **Rotate per query:** Round-robin or random selection across engines
- **Backoff:** If engine returns 429/captcha, disable for 15 minutes, auto-re-enable
- **Health check:** Periodic liveness probes to re-enable recovered engines

#### 5B. Request Spacing & Jitter
- Spread meta-search requests across engines with 100-300ms jitter
- Never send simultaneous requests to the same engine
- Per-engine rate limiter (sliding window, max N requests per minute)

#### 5C. Privacy Layer Separation
- **Meta-search (SearXNG/Whoogle):** Route through VPN (faster, sufficient privacy)
- **Crawling:** Route through Tor (slower but anonymous, for bulk fetching)
- **Intent engine:** Local only, no network needed
- **Tor auto-bridge updates:** Configure obfs4 bridges for Tor, auto-rotate

#### 5D. Self-Sufficiency Over Meta-Search
The long-term strategy: **the more we crawl and index ourselves, the less
we depend on meta-search engines.** Meta-search is a bootstrap mechanism.
Over time, the local index should handle 80%+ of queries without hitting
external engines.

Target progression:
- Week 1: 20% local, 80% meta-search
- Month 1: 50% local, 50% meta-search
- Month 3: 80% local, 20% meta-search (meta-search for fresh/long-tail only)

---

## 6. Ranking & Result Fusion

### Current State
- RRF (Reciprocal Rank Fusion) to merge local + meta-search results
- Intent boost applied per category
- Deduplication by URL

### Goal — Smarter Fusion

#### 6A. Multi-Signal Ranking
Each result gets a composite score from multiple signals:

```
final_score = (w1 * rrf_score)           // Rank fusion from all sources
            + (w2 * intent_boost)         // Intent category match
            + (w3 * freshness_score)      // Recency (exponential decay)
            + (w4 * authority_score)      // Domain authority (pre-computed)
            + (w5 * local_index_bonus)    // Bonus for locally indexed (we trust it more)
            + (w6 * embedding_similarity) // Semantic match to query
```

Default weights (tunable):
- w1=0.30, w2=0.20, w3=0.15, w4=0.15, w5=0.10, w6=0.10

#### 6B. Domain Authority (Pre-computed)
Maintain a lightweight domain authority table:
- High authority: wikipedia.org, github.com, stackoverflow.com, MDN, official docs
- Medium: tech blogs (medium.com, dev.to), news sites, Reddit
- Low: unknown domains, content farms, SEO spam
- Updated periodically based on crawl success + user feedback

#### 6C. Freshness Decay
```
freshness_score = exp(-age_hours / half_life_hours)
```
- half_life depends on intent: news=6h, technical=720h (30 days), default=168h (7 days)

---

## 7. Implementation Phases

### Phase 1: Model Swap & Intent (Week 1)
- [ ] Remove Qwen2.5-0.5B from intent engine
- [ ] Implement rule-based pre-classifier (Layer 1)
- [ ] Add DistilBERT classifier via ONNX Runtime (Layer 2)
- [ ] Update gateway to use new intent system
- [ ] Benchmark: intent classification < 15ms, end-to-end < 1.5s
- [ ] Test on 50 diverse queries

### Phase 2: Crawl Queue & Meta-Search Pipeline (Week 2)
- [ ] Build crawl queue service (priority queue, dedup, rate limiting)
- [ ] Wire gateway to feed meta-search results into crawl queue
- [ ] Implement crawl worker with Tor/VPN proxy
- [ ] Add content-type-aware crawl scheduling
- [ ] Test: index grows by 100+ URLs per day of active use

### Phase 3: Seed URLs & URL Discovery (Week 3)
- [ ] Create seed URL config file (~200 URLs)
- [ ] Implement startup seed crawling
- [ ] Add link extraction + relevance filtering from crawled pages
- [ ] Implement depth-limited URL discovery (max 2 hops)
- [ ] Test: baseline index available before any user queries

### Phase 4: Ranking & Polish (Week 4)
- [ ] Implement multi-signal ranking (RRF + intent + freshness + authority)
- [ ] Add domain authority table
- [ ] Add freshness decay scoring
- [ ] Engine rotation + backoff for SearXNG
- [ ] Cache layer for meta-search results
- [ ] Final benchmark: < 2s end-to-end, 90%+ intent accuracy

---

## 8. What NOT to Change
- Keep Tantivy for full-text search — it's fast and working
- Keep all-MiniLM-L6-v2 for embeddings — it's already optimal (22M params)
- Keep the Docker Compose architecture — it's clean
- Keep the Axum/Rust stack — it's performant
- Keep SearXNG/Whoogle/Invidious as meta-search sources
- Keep Gluetun for privacy layer

---

## Summary of Deliverables

1. **Model swap:** Replace Qwen2.5-0.5B with DistilBERT classifier (66M params, 15ms)
2. **Rule-based pre-classifier:** Regex patterns catch 60-70% of queries in <1ms
3. **Intent taxonomy:** 6 categories with per-category retrieval strategies
4. **Crawl queue service:** Priority queue, dedup, rate limiting, status tracking
5. **Meta-search → Index pipeline:** Auto-crawl and index top results from every search
6. **Smart crawl scheduling:** Content-type-aware refresh intervals
7. **URL discovery:** Extract and follow links from crawled pages (depth-limited)
8. **Seed URLs:** Pre-crawl ~200 high-value domains on startup
9. **Multi-signal ranking:** RRF + intent + freshness + authority + embedding similarity
10. **Engine rotation:** Prevent rate limiting across SearXNG engines
11. **Privacy separation:** VPN for search, Tor for crawling
12. **Caching:** 5-min meta-search cache, model pre-warming

### Success Criteria
- End-to-end search latency < 2 seconds (p95)
- Intent classification < 15ms (p95)
- Local index grows by 100+ URLs per day of active use
- No rate limiting from any meta-search engine after 1 week
- Intent classification accuracy > 90% on 6-category taxonomy
- System runs entirely on CPU with no external API dependencies
- After 1 month: 50%+ of queries served from local index alone
