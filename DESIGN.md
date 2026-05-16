# IntentForge-v2 Design Specification

## 1. Overview
IntentForge-v2 is an advanced, privacy-first search engine driven by an "Intent-First" algorithm. It prioritizes user intent over keyword matching and operates via a decentralized-style architecture using Tor and VPN proxies.

## 2. Core Architecture
The system is built as a set of microservices orchestrated via **Docker Compose**, ensuring high portability and isolation.

### 2.1 Services
- **Gateway (Rust/Axum):** The primary entry point. Aggregates results from the local index, meta-search engines, and specialized sources (YouTube, News).
- **Intent Engine (Rust/Candle):** Uses a local, quantized **Qwen2.5-0.5B** LLM to perform:
    - **Query Expansion:** Generating semantic variations of the user's query.
    - **Intent Analysis:** Categorizing the user's goal (e.g., informational, transactional, navigational).
    - **Re-ranking:** Sorting aggregated results based on their relevance to the identified intent.
- **Crawler (Rust):** A high-performance, static HTML crawler.
    - **Privacy:** Operates strictly through Tor or VPN tunnels.
    - **Intelligence:** Implements a dynamic routing logic (Tor primary -> VPN fallback).
- **Indexer (Rust/Tantivy/LanceDB):**
    - **Tantivy:** Provides fast, full-text search capabilities for crawled content.
    - **LanceDB:** Used for vector storage to support semantic search and intent-matching.
- **Meta-Search Engines:**
    - **SearXNG:** Aggregates results from multiple public search engines.
    - **Whoogle:** A privacy-respecting Google Search wrapper.
    - **Invidious:** Self-hosted YouTube frontend for video results.
- **News Integration:** Aggregates news via Google News RSS or API.

## 3. Infrastructure & Networking
- **Proxies:** 
    - **Tor:** Primary anonymity layer with Tor Bridges support.
    - **OpenVPN:** Secondary layer for high-bandwidth or Tor-blocked domains.
    - **Cloudflare:** Used for additional proxying and edge-level protection.
- **Automation:**
    - **Watchtower:** Automatically monitors and updates all running containers to the latest versions.
- **Containerization:** Unified Docker Compose setup for ease of deployment.

## 4. Search Workflow
1. **Input:** User submits a query to the Gateway.
2. **Expansion:** Intent Engine expands the query using the local LLM.
3. **Retrieval:** Gateway triggers parallel searches:
    - Local Index (Tantivy/LanceDB).
    - SearXNG / Whoogle.
    - Invidious (YouTube).
    - Google News.
4. **Ranking:** Intent Engine collects all results and re-ranks them based on the semantic match with the user's intent.
5. **Output:** Cleaned, ranked, and privacy-scrubbed results are served to the user.

## 5. Security & Privacy
- **No Tracking:** Zero user profiling or search history logging.
- **Strict Isolation:** The crawler only activates when secure tunnels (Tor/VPN) are established.
- **Local AI:** All intent processing happens on-device; no query data is sent to external LLM providers.
