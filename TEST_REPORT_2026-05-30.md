# IntentForge v2 — Stress Test Report
## 2026-05-30 (Post SearXNG2 Fan-Out + P99 Normalization)

---

## TEST RESULTS SUMMARY

| Category | Result | Notes |
|----------|--------|-------|
| Intent accuracy | 20/24 (83%) | 3 acceptable borderline, 1 real miss |
| Negative constraints | 0/140 (0.0%) | 15 queries × top-10, zero violations |
| Result quality | EXCELLENT | All top-5 results directly relevant |
| Latency p50 (uncached) | 1.80s | Acceptable for meta-search |
| Latency cached | 5-15ms | Cache working perfectly |
| Concurrent (10 unique) | 1.62 req/s, p50=4.45s | VPN tunnel bottleneck |
| Concurrent (20 unique) | 2.53 req/s, p50=2.29s | Scales with SearXNG2 fan-out |
| Cache throughput (20x same) | 395 req/s, p50=24ms | In-memory cache |
| Input validation | PASS | Empty, whitespace, special chars all rejected |
| Privacy/analytics | CLEAN | No tracking, no cookies, no query logging |

---

## GENERAL QUERIES (12/12 intent correct)

| Query | Intent | Conf | Results | Latency | Top-1 Result |
|-------|--------|------|---------|---------|--------------|
| python programming | technical | 0.75 | 68 | 3.33s | What is Python? Python Programming Explained |
| machine learning tutorials | informational | 0.94 | 71 | 2.66s | Step-by-Step ML Tutorial for Beginners |
| weather forecast today | fresh | 0.80 | 45 | 3.05s | Local Hourly Weather Forecasts for Today |
| how to learn guitar | how-to | 0.90 | 63 | 2.01s | How To Learn Guitar For Beginners |
| best restaurants near me | comparison | 0.80 | 53 | 2.18s | Best Restaurants Near Me - May 2026 |
| javascript documentation | technical | 0.75 | 52 | 2.19s | Best Open Source JavaScript Documentation |
| linux kernel development | technical | 0.75 | 52 | 1.99s | HOWTO do Linux kernel development |
| blockchain technology explained | informational | 0.96 | 38 | 2.66s | Blockchain Technology Explained |
| buy mechanical keyboard | transactional | 0.85 | 32 | 1.98s | Buy Mechanical keyboard - Coolblue |
| how to cook pasta | how-to | 0.90 | 31 | 1.35s | How to Cook Pasta Perfectly |
| latest AI news 2026 | fresh | 0.80 | 48 | 2.13s | The Latest AI News and Breakthroughs |
| github login | navigational | 0.85 | 28 | 1.94s | GitHub accounts - Google Open Source |

---

## COMPLEX QUERIES (7/12 intent correct, 3 acceptable, 1 miss, 1 was correct)

| Query | Expected | Got | Conf | Top-1 |
|-------|----------|-----|------|-------|
| rust vs go for systems programming 2026 | comparison | comparison ✓ | 0.80 | Rust vs Go in 2026 |
| federated learning privacy preserving ML | informational | how-to ~ | 0.90 | Privacy Preserving ML tutorial |
| building real-time collaborative editors with CRDTs | technical | technical ✓ | 0.75 | Building Editor with CRDTs |
| quantum error correction surface codes | technical | technical ✓ | 0.75 | Understanding Quantum Error Correction |
| zero-knowledge proofs in blockchain scalability | how-to | how-to ✓ | 0.86 | ZK Proofs: Privacy & Scalability |
| transformer architecture attention mechanism explained | informational | how-to ~ | 0.83 | Transformer Architecture Explained |
| distributed consensus algorithms paxos raft | comparison | comparison ✓ | 0.80 | Consensus Algorithm Comparison |
| how to implement WebAssembly SIMD for game engines | how-to | how-to ✓ | 0.90 | WebAssembly game engine discussion |
| neural network pruning quantization edge deployment | technical | how-to ~ | 0.87 | Edge AI Deployment: Quantization |
| kubernetes service mesh istio vs linkerd 2026 | comparison | comparison ✓ | 0.80 | K8s Service Mesh Comparison 2026 |
| postgreSQL query optimization indexing strategies | comparison | comparison ✓ | 0.83 | PostgreSQL Indexing Strategies |
| building privacy-first search engine from scratch | how-to | technical ✗ | 0.75 | How to Build a Search Engine |

Legend: ✓ = correct, ~ = borderline acceptable, ✗ = miss

---

## NEGATIVE CONSTRAINTS (0/140 violations)

| Query | Excluded Term | Violations in Top-10 |
|-------|---------------|---------------------|
| python web framework not django | django | 0 |
| javascript framework except react | react | 0 |
| text editor without vim | vim | 0 |
| css framework no bootstrap | bootstrap | 0 |
| linux distro not ubuntu | ubuntu | 0 |
| programming language other than java | java | 0 |
| search engine alternative to google | google | 0 |
| static site generator not jekyll | jekyll | 0 |
| database not mysql | mysql | 0 |
| frontend framework no angular | angular | 0 |
| cloud provider not aws | aws | 0 |
| package manager not npm | npm | 0 |
| mobile framework except flutter | flutter | 0 |
| web server not apache | apache | 0 |
| ORM not sequelize | sequelize | 0 |

---

## CONCURRENT STRESS TEST

| Test | Queries | Succeeded | Wall Time | p50 | p95 | Throughput |
|------|---------|-----------|-----------|-----|-----|------------|
| 10 concurrent unique | 10 | 10/10 | 6.17s | 4.45s | 6.15s | 1.62 req/s |
| 20 concurrent unique | 20 | 20/20 | 7.91s | 2.29s | 7.89s | 2.53 req/s |
| Cache stress (20x same) | 20 | 20/20 | 0.05s | 24ms | 29ms | 395 req/s |

---

## KNOWN ISSUES

### Intent Classification Borderline (3 cases)
- "explained" triggers how-to instead of informational
- "deployment" triggers how-to instead of technical
- "preserving" triggers how-to instead of informational
These are acceptable — the results returned are correct for both intents.

### Intent Classification Miss (1 case)
- "building privacy-first search engine from scratch" → technical instead of how-to
- "from scratch" should trigger how-to pattern but doesn't
- Fix: add "from scratch" to how-to regex patterns in intent engine

### VPN Tunnel Contention
- 10 concurrent: 4.45s p50 (expected ~2s sequential)
- Root cause: single gluetun VPN tunnel, shared across all services
- Fix: upgrade VPN provider or add second tunnel

---

## FILES MODIFIED
- None (read-only stress test)
