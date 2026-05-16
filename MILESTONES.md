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

## Phase 3: Intent Engine (AI) (Complete)
- [x] Download and load **Qwen2.5-0.5B** GGUF
- [x] Implement Rust-based inference using **Candle** (with mmap)
- [x] Develop Query Expansion logic (LLM-driven)
- [ ] Develop Result Re-ranking logic (Cross-scoring)

## Phase 4: Core Services (Rust) (Complete)
- [x] Implement **Gateway** (Aggregator API)
- [x] Implement **Crawler** (Static HTML & Text Extraction)
- [x] Implement **Indexer** (Tantivy + LanceDB ready)
- [x] Integrate Crawler results into the Indexer (Knowledge Warming)

## Phase 5: Automation & UI
- [ ] Integrate **Watchtower** for auto-updates
- [ ] Build Search Frontend
- [ ] Final end-to-end testing
