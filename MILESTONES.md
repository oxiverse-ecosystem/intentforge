# IntentForge-v2 Milestones

## Phase 1: Meta-Search Infrastructure (Complete)
- [x] Set up **SearXNG** service (Docker)
- [x] Set up **Whoogle** service (Docker)
- [x] Set up **Invidious** service (Docker)
- [x] Verify connectivity and health checks for all three

## Phase 2: Privacy Layer (Complete)
- [x] Configure **Tor** and **Tor Bridges** (Built-in fetcher implemented)
- [x] Configure **OpenVPN** (via **Gluetun**)
- [x] Implement strict isolation logic (container network mode isolation)

## Phase 3: Intent Engine (AI) (Complete → Rewritten)
- [x] ~~Download and load **Qwen2.5-0.5B** GGUF~~ REMOVED (too heavy, 500-1500ms)
- [x] Implement Rust-based inference using **Candle** (with mmap)
- [x] ~~Develop Query Expansion logic (LLM-driven)~~ Replaced with rule-based + centroid
- [ ] Develop Result Re-ranking logic (Cross-scoring)

### Phase 3.5: Model Swap (Complete — 2026-05-26)
- [x] Remove Qwen2.5-0.5B from intent engine (saves ~400MB, ~1400ms latency)
- [x] Implement Layer 1: Rule-based pre-classifier (< 1ms, regex patterns)
- [x] Implement Layer 2: Embedding centroid classifier (~2ms, uses existing MiniLM-L6-v2)
- [x] 7 intent categories: navigational, informational, technical, how-to, comparison, transactional, fresh
- [x] Update gateway to handle new IntentResponse format (confidence field)
- [x] Update setup.sh to remove Qwen downloads
- [x] Latency: intent classification now < 15ms (was 500-1500ms)

## Phase 4: Core Services (Rust) (Complete → Enhanced)
- [x] Implement **Gateway** (Aggregator API)
- [x] Implement **Crawler** (Static HTML & Text Extraction)
- [x] Implement **Indexer** (Tantivy + LanceDB ready)
- [x] Integrate Crawler results into the Indexer (Knowledge Warming)

### Phase 4.5: Crawl Queue (Complete — 2026-05-26)
- [x] Build priority queue (BinaryHeap, priority = search rank)
- [x] URL deduplication (HashSet + URL normalization)
- [x] Per-domain rate limiter (2s sliding window)
- [x] Content-type-aware crawl scheduling (News=1h, Docs=24h, Forum=2h, etc.)
- [x] URL discovery from crawled pages (link extraction, max 20 per page)
- [x] Seed URL list (~13 high-value domains on startup)
- [x] Background crawl worker (5s poll interval)
- [x] Background refresh checker (content-type-aware intervals)
- [x] /enqueue endpoint for gateway to feed meta-search results
- [x] /queue/status endpoint for monitoring
- [x] Gateway feeds top 10 meta-search results into crawl queue per query

## Phase 5: Search Quality (In Progress)
- [x] Gateway timeout reduced from 5s to 1.5s
- [x] Gateway feeds meta-search results to crawl queue (was fire-and-forget)
- [ ] Enable more SearXNG engines (Qwant, Startpage, Mojeek)
- [ ] Engine rotation + backoff
- [ ] Multi-signal ranking (RRF + intent + freshness + authority + embedding)
- [ ] Domain authority table
- [ ] Meta-search result caching (5-min TTL)

## Phase 6: Automation & UI
- [ ] Integrate **Watchtower** for auto-updates
- [ ] Build Search Frontend
- [ ] Final end-to-end testing
