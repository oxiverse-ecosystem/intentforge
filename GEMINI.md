# IntentForge-v2 Project Instructions

## Architecture Overview
- **Core Strategy:** Intent-First search. All queries pass through a local LLM (Qwen2.5-0.5B) for expansion and constraint extraction.
- **Privacy Layer:** Gluetun (ProtonVPN) + Tor (Bridges). All outgoing traffic from crawlers/meta-engines must route through `network_mode: "container:gluetun"`.
- **Inference:** Local CPU-only GGUF inference using Rust (Candle) within Docker.

## Development Constraints
- **Docker-Only Execution:** NEVER run Rust/Cargo or Python scripts directly on the host for production-like tasks. Everything must be containerized to ensure networking/proxy consistency.
- **Rust Stack:** Axum (API), Candle (Inference), Tantivy (Text Search), LanceDB (Vector Search).
- **Tech Standards:** 
    - Use `Dockerfile` with multi-stage builds for Rust to keep images lean.
    - Strictly adhere to `network_mode: "service:gluetun"` for any service performing external requests.

## Intent Engine Workflow
1. **Analyze:** Extract `Intent`, `Constraints`, and `Keywords`.
2. **Retrieve:** Parallel fetch from Local (LanceDB/Tantivy) and Meta (SearXNG).
3. **Enhance:** Use Meta results to populate the local index for "Knowledge Warming."
4. **Rerank:** Cross-score all results against the `Intent` map.
