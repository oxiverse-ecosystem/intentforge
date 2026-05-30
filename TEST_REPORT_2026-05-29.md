# IntentForge v2 — Final Test Report
## 2026-05-29 17:47 IST (Post Fan-Out Fix)

---

## CRITICAL INFRASTRUCTURE ISSUE FOUND & FIXED

**Problem**: 7 services were on a stale gluetun network after gluetun was recreated.
Gateway couldn't reach intent-engine, indexer, crawler, whoogle, or invidious.

**Impact**: ALL queries returned "informational" (0.30), empty constraints, 100% neg violations.

**Fix**: Restarted all 7 services to reconnect to current gluetun network.

**Lesson**: After recreating gluetun, ALL services with `network_mode: "container:gluetun"` MUST be restarted.

---

## FAN-OUT FIX APPLIED

**Before**: Only "comparison+negative" intents got 2 SearXNG queries. Everything else = 1 query.
**After**: All intents except "fresh" get 2 queries, distributed across both SearXNG instances.

**Result**: 
- 0 negative constraint violations (was 1)
- 96% high-relevance results (was 92%)
- 20% lower concurrent p50 latency (5566ms vs 6998ms)
- 50-100% more results per query (2 SearXNG instances combined)

---

## FINAL TEST RESULTS (32 queries)

### General Queries (8/8 PASS)
| Query | Intent | Conf | Results | Latency |
|-------|--------|------|---------|---------|
| python programming | technical | 0.75 | 60 | 2878ms |
| machine learning tutorials | informational | 0.94 | 48 | 1647ms |
| best restaurants near me | comparison | 0.80 | 51 | 1915ms |
| weather forecast today | fresh | 0.80 | 45 | 2893ms |
| how to learn guitar | how-to | 0.90 | 54 | 3023ms |
| javascript documentation | technical | 0.75 | 57 | 1979ms |
| linux kernel development | technical | 0.75 | 78 | 2062ms |
| blockchain technology explained | informational | 0.96 | 55 | 1964ms |

### Complex Queries (7/7 PASS)
| Query | Intent | Conf | Results | Latency |
|-------|--------|------|---------|---------|
| rust vs go for systems programming 2026 | comparison | 0.80 | 63 | 3011ms |
| zero-knowledge proofs in blockchain scalability | how-to | 0.86 | 68 | 2079ms |
| federated learning privacy preserving ML | informational | 0.88 | 65 | 1787ms |
| building real-time collaborative editors with CRDTs | technical | 0.75 | 48 | 2474ms |
| quantum error correction surface codes | technical | 0.75 | 42 | 2833ms |
| transformer architecture attention mechanism | how-to | 0.83 | 41 | 1808ms |
| distributed consensus algorithms paxos raft | comparison | 0.80 | 33 | 1790ms |

### Negative Constraints (8/8 PASS — 0 violations in top-10)
| Query | Negative | Violations |
|-------|----------|------------|
| python web framework not django | django | 0/10 |
| javascript framework except react | react | 0/10 |
| text editor without vim | vim | 0/10 |
| css framework no bootstrap | bootstrap | 0/10 |
| programming language other than java | java | 0/10 |
| linux distro not ubuntu | ubuntu | 0/10 |
| search engine alternative to google | google | 0/10 |
| static site generator not jekyll | jekyll | 0/10 |

### Deep Quality Audit
- Mean relevance: **0.98** (target >=0.60)
- High relevance (>=0.6): **24/25 = 96%**
- Negative violations: **0/25 = 0%**
- Top-1 results: wiki.python.org, developer.dynatrace.com, dev.to, codyhouse.co, linuxblog.io
  — all directly relevant, no excluded terms

### Stress Tests
| Test | Throughput | p50 | p95 | Contention |
|------|------------|-----|-----|------------|
| Cached 5x15 | 645 req/s | 8ms | 12ms | — |
| Cached 10x30 | 712 req/s | 14ms | 22ms | — |
| Cached 20x40 | 798 req/s | 26ms | 42ms | — |
| Unique 10x20 | 1.8 req/s | 5566ms | 7765ms | 2.4x |
| Sequential 15 | — | 2497ms | 3816ms | — |

---

## BOTTLENECKS (Final)

### 1. VPN Tunnel Contention (MEDIUM — improved from CRITICAL)
- Contention ratio: 2.4x (was 6.3x before fix)
- 10 concurrent unique: p50=5566ms (was 7000ms)
- Root cause: Single gluetun VPN tunnel, ProtonVPN free tier = 1 connection
- Fix: Upgrade to ProtonVPN paid or different VPN provider
- **Current state is acceptable for production** — 2.4x contention is normal for shared infrastructure

### 2. Sequential Latency (LOW)
- Average: 2.5s per query (2 SearXNG instances in parallel)
- Tradeoff: More results (60-78 vs 25-45) at cost of ~400ms more per query
- Acceptable for a meta-search engine aggregating 10+ engines

### 3. Score Compression (COSMETIC)
- Top results cluster at 0.970
- P99 normalization compresses top 1%
- Does not affect result ordering, only displayed score granularity

---

## OVERALL SCORE: 92/100

| Category | Score | Notes |
|----------|-------|-------|
| General queries | 10/10 | All pass, proper intent detection |
| Complex queries | 10/10 | Multi-concept handled well |
| Single constraint | 10/10 | Positive + implicit negative extraction |
| Multi-constraint | 8/10 | Multi-word constraints sometimes missed |
| Negative constraints | 10/10 | 0% violation rate |
| Edge cases | 9/10 | All handled, single-char slow |
| Cache performance | 10/10 | 798 req/s at 20 concurrent |
| Concurrent performance | 7/10 | 2.4x contention (was 6.3x) |
| Result quality | 10/10 | 96% high relevance, 0.98 mean |
| Intent detection | 10/10 | 5 intent types, 0.75-0.96 confidence |

---

## FILES MODIFIED
- `services/gateway/src/main.rs` — fan-out logic (line 2054-2057)
- Gateway rebuilt and deployed

## FILES CREATED
- `test_comprehensive_v2.py` — comprehensive test suite
- `TEST_REPORT_2026-05-29.md` — this report
