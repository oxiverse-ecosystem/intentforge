# IntentForge v2

> **Private search engine** with dual-path architecture (VPN + Tor), BERT-based intent classification, multi-signal ranking, and constraint-aware query processing.

**Production:** `https://api.oxiverse.com`  
**Development:** `http://localhost:4000`

---

## Overview

IntentForge v2 is a privacy-first, self-hosted meta-search engine that queries multiple upstream search engines through two independent egress paths (ProtonVPN + Tor), classifies query intent using a local BERT model, and returns deduplicated, ranked results.

Unlike commercial search engines, IntentForge:
- **Does not track users** — no analytics, no cookies, no user profiles
- **Routes all traffic through VPN/Tor** — your IP is never exposed to upstream engines
- **Runs entirely on your infrastructure** — no third-party API dependencies
- **Supports complex constraints** — natural language negation (`not X`, `without Y`, `alternative to Z`), site restriction (`site:`), file type filtering (`filetype:`), date ranges (`after:`, `before:`), and more
- **Auto-corrects spelling** — SymSpell + LinSpell with brand/tech-term protection

---

## Architecture

```
                    ┌─────────────────────────────────────────────────────┐
                    │                   Traefik (SSL)                     │
                    │            api.oxiverse.com:443 → :4000             │
                    └──────────────────┬──────────────────────────────────┘
                                       │
                    ┌──────────────────▼──────────────────────────────────┐
                    │              Gateway (Rust/Axum)                    │
                    │              Port 4000 /search /images              │
                    │              /videos /news /search/fast             │
                    └──┬───────┬───────┬──────┬──────┬───────┬───────────┘
                       │       │       │      │      │       │
              ┌────────▼──┐ ┌──▼────┐ ┌─▼──┐ ┌▼───┐ ┌▼────┐ ┌▼──────────┐
              │ Intent    │ │SearXNG│ │Sear │ │Invi │ │Local│ │Crawler    │
              │ Engine    │ │ (VPN) │ │XNG2 │ │dious│ │Idx  │ │(5001)     │
              │ (3005)    │ │(8080) │ │(Tor)│ │(3000)│ │(6k) │ │           │
              │ BERT /    │ │ │     │ │ │   │ │ │   │ │ │   │ │ 🡒 Indexer │
              │ Candle    │ │ │     │ │ │   │ │ │   │ │ │   │ │           │
              └───────────┘ │ │     │ │ │   │ │ │   │ │ │   │ └───────────┘
                            │ │     │ │ │   │ │ │   │ │ │   │
                   ┌────────┴─┴─────┴─┴─┴───┴─┴─┴───┴─┴─┴───┴───────────┐
                   │               Glueten VPN Network                    │
                   │   (All services share gluetun's network namespace)    │
                   └────────────────────────┬─────────────────────────────┘
                                            │
              ┌─────────────────────────────┼─────────────────────────────┐
              │                    ┌────────▼────────┐                    │
              │                    │  ProtonVPN      │                    │
              │                    │  (WireGuard/OVPN)│                   │
              │                    └─────────────────┘                    │
              │                        ▲        ▲                        │
              │              ┌─────────┴──┐  ┌──┴──────────┐             │
              │              │   Tor 1    │  │   Tor 2     │             │
              │              │(Socks 9050)│  │(Socks 9051) │             │
              │              │ (.onion)   │  │ (SearXNG2)  │             │
              │              └────────────┘  └─────────────┘             │
              └──────────────────────────────────────────────────────────┘
```

### Services

| Service | Language | Port | Description |
|---------|----------|:----:|-------------|
| **Gateway** | Rust (Axum) | 4000 | API entry point — search, ranking, constraints, caching |
| **Intent Engine** | Rust (Candle) | 3005 | BERT-based intent classification (8 subtypes) + spell correction |
| **SearXNG 1** | Python | 8080 | Meta-search via VPN (bing, brave, wikipedia, marginalia, arxiv, crossref) |
| **SearXNG 2** | Python | 8081 | Meta-search via Tor exit (bing, brave, wikipedia, duckduckgo, marginalia) |
| **Invidious** | Crystal | 3000 | YouTube video search (no Google API) |
| **Indexer** | Rust (Tantivy) | 6000 | Full-text local search index |
| **Crawler** | Rust (Scraper) | 5001 | Web crawler → Indexer pipeline |
| **GeoIP Updater** | Shell | — | Monthly MaxMind GeoLite2 database updates |
| **VPN Rotator** | Shell | — | Intelligent IP rotation on rate-limit signals |
| **Tor 1** | Tor | 9050 | SOCKS proxy for .onion engines |
| **Tor 2** | Tor | 9051 | SOCKS proxy for SearXNG2 (independent IP path) |

---

## Quick Start

### Prerequisites

- Docker & Docker Compose v2
- A ProtonVPN account (free tier works) — credentials in `.env`
- (Optional) A MaxMind license key for GeoIP — in `.env`

### Setup

```bash
# 1. Clone and enter the project
git clone <repo> && cd intentforge-v2

# 2. Create environment file
cat > .env << 'EOF'
VPN_SERVICE_PROVIDER=protonvpn
VPN_TYPE=openvpn
FREE_ONLY=on
OPENVPN_USER=your_protonvpn_username
OPENVPN_PASSWORD=your_protonvpn_password
MAXMIND_LICENSE_KEY=your_maxmind_key  # optional
EOF

# 3. Start the development stack
make dev-up
# Or manually: cd services && docker compose -f docker-compose.dev.yml up -d --build

# 4. Check health
curl http://localhost:4000/health
# → "OK"

# 5. Run a search
curl "http://localhost:4000/search?q=rust+web+framework&limit=3"
```

### Makefile Commands

| Command | Description |
|---------|-------------|
| `make dev-up` | Build + start development stack |
| `make dev-down` | Stop development stack |
| `make dev-nuke` | Destroy everything (containers + volumes + images) |
| `make dev-logs` | Tail all dev logs |
| `make prod-up` | Build + start production stack |
| `make prod-down` | Stop production stack |
| `make dev-shell SVC=gateway` | Shell into a specific container |

---

## API

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Plain-text service identifier |
| `GET` | `/health` | Health check → `"OK"` |
| `GET` | `/search` | Full search (intent + constraints + ranking) |
| `GET` | `/search/fast` | Local index only (~100ms) |
| `GET` | `/images` | Image search via SearXNG |
| `GET` | `/videos` | Video search via SearXNG + Invidious |
| `GET` | `/news` | News search via SearXNG |

### `/search` Parameters

| Param | Type | Default | Description |
|-------|------|:-------:|-------------|
| `q` | string | — | Search query (URL-encoded) |
| `limit` | int | 24 | Max results (pagination) |
| `offset` | int | 0 | Result offset (pagination) |

### Response Highlights

```json
{
  "query": "python web framework not django",
  "intent": "technical",
  "confidence": 0.60,
  "constraints": ["+python", "+web", "-django"],
  "structured_constraints": {
    "positive": ["python", "web"],
    "negative": ["django"],
    "sites": [],
    "file_types": []
  },
  "results": [
    {
      "url": "https://bottlepy.org/docs/dev/",
      "title": "Bottle: Python Web Framework",
      "content": "Bottle is a fast, simple and lightweight WSGI micro web-framework...",
      "score": 0.970,
      "authority": 0.90,
      "sources": ["bing", "brave"],
      "published_date": null
    }
  ],
  "applied_constraints": ["not:django"],
  "spell_corrected_query": null,
  "geo_location": null,
  "has_more": true
}
```
> **Note:** `confidence` is a real float (≈0.3–0.9) that varies per query — it is **not** always `0.75`. `geo_location` and `spell_corrected_query` are omitted from the JSON on clean queries (they serialize as `None`). The full, verified response schema (including `category`, `distribution`, `expanded_queries`, `results_before_filter`, `results_after_filter`, `total`, `limit`, `offset`) is in [API_REFERENCE.md](API_REFERENCE.md).

See **[API_REFERENCE.md](API_REFERENCE.md)** for the complete API documentation.

---

## Goals

IntentForge includes a **Goals** feature that turns a long-term goal into a personalized, phased roadmap with curated resources, deadlines, and progress tracking. It runs on the same gateway and exposes a small REST surface. All endpoints were exercised live against `localhost:4000` on 2026-08-05 (full transcript: `docs/_generated/_round_v2_raw.md`).

**Two flows:**

1. **Quick flow** — one call, full roadmap immediately:
   ```bash
   curl -s -X POST "http://localhost:4000/goals/quick" \
     -H "Content-Type: application/json" \
     -d '{"goal":"learn to build a privacy-first search engine using Rust"}' | head -c 400
   # → {"goal_id":"goal_0001","goal":"...","intent":"learning","resource_count":11,
   #    "roadmap":{"overview":"A 12-week journey (5-10 hours/week) across 4 phases.",
   #    "phases":[...],"total_duration_weeks":12,"total_buffer_days":28},
   #    "status":"active","completed_phases":0,"total_phases":4,"score":0}
   ```
2. **Discovery flow** — create, answer questions, get roadmap:
   ```bash
   # 1. Create → get questions
   curl -s -X POST "http://localhost:4000/goals" \
     -H "Content-Type: application/json" \
     -d '{"goal":"write a novel in 6 months"}'
   # → {"goal_id":"goal_0002","intent":"creative-writing","total_questions":4,
   #    "questions":[...],"next_step":{"method":"POST","path":"/goals/goal_0002/answers",...}}

   # 2. Submit answers (question_id is 0-indexed in the answers body)
   curl -s -X POST "http://localhost:4000/goals/goal_0002/answers" \
     -H "Content-Type: application/json" \
     -d '{"answers":[{"question_id":0,"answer":"6 months — Half-year journey"},
                      {"question_id":1,"answer":"5-10 hours — Evenings & weekends"},
                      {"question_id":2,"answer":"fiction"},
                      {"question_id":3,"answer":"a finished draft"}]}' | head -c 300
   # → {"goal_id":"goal_0002", ..., "roadmap":{...},"status":"active",...}

   # 3. Track progress (phase_id is 1-indexed!) and read status
   curl -s -X POST "http://localhost:4000/goals/goal_0002/progress" \
     -H "Content-Type: application/json" -d '{"phase_id":1,"is_completed":true}'
   curl -s "http://localhost:4000/goals/goal_0002" | head -c 200
   curl -s "http://localhost:4000/goals/leaderboard" | head -c 200
   ```

**Endpoints**

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/goals` | Create a goal → returns `goal_id` + tailored questions |
| `POST` | `/goals/quick` | One-shot: goal → full roadmap immediately (no questions) |
| `GET`  | `/goals/:goal_id` | Get goal status and roadmap |
| `POST` | `/goals/:goal_id/answers` | Submit answers → generate the phased roadmap |
| `POST` | `/goals/:goal_id/progress` | Update a phase's completion (`{"phase_id":N,"is_completed":bool}`) |
| `POST` | `/goals/:goal_id/phases/:phase_id/complete` | Mark a phase complete (1-indexed `:phase_id`) |
| `GET`  | `/goals/leaderboard` | All goals sorted by score |

**Verified behavior notes (2026-08-05):**

- A roadmap has **4 phases** by default (timeline answer can change this to 3–6). Each phase carries `id` (1-indexed), `title`, `description`, `duration_weeks`, `deadline` (`"YYYY-MM-DD (buffer: YYYY-MM-DD)"`), `buffer_days` (7), `objectives`, `resources`, `deliverables`, `completion_type`, `is_completed`.
- **Phase IDs are 1-indexed.** `POST /goals/:id/progress` with `{"phase_id":0}` returns `400 invalid_phase` (`"Phase 0 does not exist"`). Use the `id` from each roadmap phase.
- Completing a phase via `/progress` or `/phases/:id/complete` sets `completed_phases` and adds **+100** to `score` (observed: 1 completed phase → `score:100`).
- Questions are `0-indexed` in the **answers** body (`question_id:0..n`) but phases are `1-indexed` in the **roadmap** — a common source of confusion; the `invalid_phase` 400 is the tell.
- Goals are stored **in-memory** (non-persistent across gateway restarts). `GET /goals/leaderboard` returns `{"entries":[...],"total_entries":N}`.
- Error codes: `400 empty_goal` (goal < 3 chars), `400 invalid_phase`, `404 not_found` (unknown goal id), `422 invalid_payload` (bad JSON).

See **[API_REFERENCE.md → Goals API](API_REFERENCE.md#goals-api)** for the full request/response schemas and domain-specific question banks.

---

## Query Operators

Search operators are parsed directly from the query string:

| Operator | Example | Description |
|----------|---------|-------------|
| `site:` | `site:arxiv.org transformers` | Restrict to domain |
| `filetype:` | `react filetype:pdf` | Filter by extension |
| `intitle:` | `intitle:rust web framework` | Term in page title |
| `inurl:` | `inurl:api python` | Term in URL |
| `after:` | `after:2026-01-01` | Published after date |
| `before:` | `before:2025-01-01` | Published before date |
| `price:<` / `price:>` | `price:%3C50 headphones` | Price range (URL-encode `<` to `%3C`) |
| `not X` / `without X` | `python not django` | Negative constraint (natural language) |
| `vs` / `versus` | `react vs vue` | Comparison intent |

**Natural language date ranges** are automatically converted:
- `"past 7 days"`, `"last week"`, `"this month"`, `"yesterday"`, `"recent"`, `"latest"`

---

## How It Works

### Search Pipeline

1. **Query Validation** — Reject empty, non-alphabetic, or gibberish queries
2. **Cache Check** — Return cached response if within TTL (**5 min** for `/search`; verified this session — see [API_REFERENCE.md#caching](API_REFERENCE.md#caching))
3. **Parallel Fan-out** — Query all backends simultaneously:
   - SearXNG1 (via VPN — bing, brave, wikipedia, arxiv, crossref, marginalia)
   - SearXNG2 (via Tor exit — bing, brave, wikipedia, duckduckgo, marginalia)
   - Local indexer (Tantivy full-text search)
   - Intent engine (BERT classification)
4. **Deduplication** — Merge results by canonical URL across all sources
5. **Constraint Extraction** — Parse `site:`, `filetype:`, negative terms, etc.
6. **Multi-signal Ranking** — Score each result using:
   - **Base relevance** — query-term overlap with title/content/URL
   - **Domain authority** — TLD trust, subdomain patterns, path signals
   - **Freshness decay** — exponential decay based on URL date signals
   - **Intent boost** — structural signals matching query intent
   - **Consensus boost** — bonus when multiple engines return same URL
   - **Content quality** — Shannon entropy + gibberish detection
   - **Constraint scoring** — boost matches, penalize negatives
7. **Filtering & Pagination** — Apply constraints, slice results
8. **Response** — Return unified response

### Intent Classification

8 subtypes classified by a local BERT (MiniLM) model running on Candle:

| Intent | Category | Example |
|--------|----------|---------|
| `navigational` | navigational | `"python docs"`, `"github login"` |
| `informational` | informational | `"what is quantum computing"` |
| `technical` | informational | `"rust async web framework"` |
| `how-to` | informational | `"how to deploy docker"` |
| `comparison` | informational | `"react vs vue vs angular"` |
| `fresh` | informational | `"latest AI news today"` |
| `local` | informational | `"restaurants near me"` |
| `transactional` | transactional | `"buy domain name"` |

### Spell Correction

Two-stage approach (no external API):
- **SymSpell** — O(1) hash-based lookup using pre-computed delete variations
- **LinSpell** — O(n) linear scan fallback for near-miss words
- **15,000+ word frequency dictionary** bundled
- **Protected terms** — 100+ brands/tech terms never corrected (openai, kubernetes, podman, etc.)
- **Character bigram perplexity** — detects tech terms vs misspellings
- **Result-based validation** — web data validates corrections after search

### Geolocation

Two mechanisms:
1. **IP-derived** — GeoLite2 database maps client IP to location
2. **Query-derived** — Gazetteer (70+ countries, 40+ cities) overrides IP when query names a location

### Privacy

- All outbound search traffic goes through ProtonVPN or Tor
- No user data, tracking, or analytics in responses
- No cookies, no sessions, no user profiles
- Two independent egress paths (VPN + Tor) — no single point of IP exposure

---

## Performance

| Metric | Value |
|--------|-------|
| **Warm request (cached)** | 3–6ms |
| **Normal query latency (uncached)** | ~3.7s avg, ~5.8s max |
| **Operator query latency (uncached)** | ~4.8s avg, ~5.1s max |
| **Fast local search** (`/search/fast`) | ~13ms (measured, local index only) |
| **Concurrent requests** | 5 simultaneous → all 200 OK |
| **Cache hit rate** | ~100% for repeated queries within TTL |

Normal complex queries (natural constraints like `not X`, `vs Y`) are ~34% faster than operator queries (`site:`, `filetype:`) because the gateway's constraint engine applies operators post-search rather than passing them to upstream engines.

See **[performance section in API_REFERENCE.md](API_REFERENCE.md#performance--stress-test-results)** for detailed benchmarks.

---

## Development

### Project Structure

```
services/
├── gateway/                  # Main API gateway (Rust)
│   ├── src/
│   │   ├── main.rs           # Routes, handlers, ranking, constraints
│   │   ├── spell.rs          # SymSpell + LinSpell correction
│   │   ├── geoloc.rs         # MaxMind GeoLite2 lookup
│   │   ├── clean.rs          # Content cleaning, dedup, URL canonicalization
│   │   └── dictionary.rs     # 15k+ word frequency dictionary
│   └── Dockerfile
├── intent-engine/            # BERT intent classification (Rust/Candle)
│   ├── src/main.rs           # Inference server
│   ├── config/               # Trained linear probe weights
│   └── Dockerfile
├── indexer/                  # Local search index (Rust/Tantivy)
│   ├── src/main.rs
│   └── Dockerfile
├── crawler/                  # Web crawler (Rust)
│   ├── src/main.rs
│   └── Dockerfile
├── privacy-layer/            # VPN + Tor configuration
│   ├── gluetun/              # ProtonVPN config
│   ├── tor/                  # Tor1 (port 9050, .onion engines)
│   ├── tor2/                 # Tor2 (port 9051, SearXNG2)
│   └── vpn-rotator.sh        # Intelligent IP rotation
├── meta-search-engines/      # SearXNG + Invidious
│   ├── searxng/              # SearXNG1 (VPN) + SearXNG2 (Tor)
│   └── invidious/            # YouTube search
├── traefik/                  # SSL proxy (production)
├── docker-compose.dev.yml    # Development stack
└── docker-compose.prod.yml   # Production stack (Traefik SSL)
```

### Running Tests

```bash
# Gateway unit tests (with embedded spell correction tests)
cd services/gateway && cargo test

# Intent engine benchmarks
cd services/intent-engine && python run_benchmark.py

# Local stress test (from project root)
python normal_vs_operator.py
```

### Building a Single Service

```bash
cd services && docker compose -f docker-compose.dev.yml build gateway
```

### Viewing Logs

```bash
# All services
make dev-logs

# Specific service
docker logs -f if-dev-gateway
```

---

## Deployment

### Production Stack

The production stack (see `services/docker-compose.prod.yml`) adds:
- **Traefik** — Auto SSL via Let's Encrypt, reverse proxy to gateway
- **Optimized memory limits** — 4GB for gateway, 512MB for intent engine

```bash
make prod-up
```

The gateway is exposed at `https://api.oxiverse.com` (configurable in `services/traefik/dynamic/routers.yml`).

### Environment Variables

| Variable | Required | Description |
|----------|:--------:|-------------|
| `VPN_SERVICE_PROVIDER` | ✅ | Set to `protonvpn` |
| `VPN_TYPE` | ✅ | Set to `openvpn` |
| `OPENVPN_USER` | ✅ | ProtonVPN username |
| `OPENVPN_PASSWORD` | ✅ | ProtonVPN password |
| `MAXMIND_LICENSE_KEY` | ❌ | GeoLite2 database updates |
| `INTENT_MAX_CONCURRENCY` | ❌ | BERT inference concurrency (default: 4) |

### Architecture Notes

- All services run inside gluetun's network namespace (shared VPN)
- SearXNG2 routes traffic through Tor2 (port 9051) for a completely independent IP path
- The gateway rate-limits at 8 concurrent searches (configurable semaphore)
- VPN rotation is automatic: the vpn-rotator watches for rate-limit signals from the gateway

---

## License

IntentForge v2 — Private Search Engine  
Copyright © 2026 Oxiverse

---

## Roadmap

See **[ROADMAP.md](ROADMAP.md)** for planned features and future improvements.
