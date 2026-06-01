# Stress Test Results — 2026-06-01 (Post Exploration Detector)

**Date**: 2026-06-01  
**Target**: localhost:4000 (dev container)  
**Queries**: 55 (25 general + 30 complex)  
**Pacing**: 1.8s between requests  

## Scorecard

| Metric | Value |
|--------|-------|
| Intent accuracy | 38/55 (69.1%) |
| Result availability | 55/55 (100%) |
| Top-5 relevance (keyword) | 274/275 (100%) |
| Top-5 relevance (human inspection) | 54/55 (98%) |
| Source diversity | 9 unique sources |
| Neg constraint violations | 0 |
| Cache speedup | 399x (6ms vs 2.4s) |
| Confidence calibration | correct=0.579, wrong=0.471 |
| Distribution field | 55/55 present |

## Latency

| Percentile | Value |
|------------|-------|
| p50 | 2404ms |
| p90 | 3094ms |
| p95 | 4196ms |
| p99 | 4433ms |
| Cached | 6ms |

## Per-Intent Accuracy

| Intent | Correct | Total | % | Avg Latency | Avg Results | Avg Relevance |
|--------|---------|-------|---|-------------|-------------|---------------|
| comparison | 11 | 11 | 100% | 2700ms | 35 | 0.911 |
| fresh | 1 | 1 | 100% | 4253ms | 96 | 0.800 |
| how-to | 2 | 15 | 13% | 2313ms | 35 | 0.875 |
| informational | 9 | 11 | 82% | 2843ms | 33 | 0.877 |
| navigational | 2 | 2 | 100% | 2107ms | 36 | 1.000 |
| technical | 11 | 13 | 85% | 2358ms | 33 | 0.833 |
| transactional | 2 | 2 | 100% | 2473ms | 24 | 1.000 |

## Concurrent (10 queries, 5 workers)

| Metric | Value |
|--------|-------|
| Succeeded | 10/10 |
| Wall time | 10031ms |
| Throughput | 1.00 req/s |
| p50 | 3972ms |
| p95 | 7894ms |

## Source Contribution

| Source | Results |
|--------|---------|
| duckduckgo | 575 |
| startpage | 436 |
| bing | 305 |
| local | 159 |
| whoogle | 150 |
| mojeek | 137 |
| brave | 107 |
| google news | 50 |
| bing news | 7 |

## Issues Found (Deep Inspection)

### 1. Score Uniformity (CRITICAL)
ALL top-1 results score exactly 0.970. P99 normalization compresses
everything to the same value. Score field is meaningless for downstream
consumers. Ranking ORDER is correct but score MAGNITUDE is lost.

### 2. Domain Repetition in Top-5
- "tailwindcss v4 migration": dev.to 2x in top-5
- "stripe webhook retry": dev.to 2x in top-5
- "k8s HPA prometheus": oneUptime.com 4x in top-8
- MAX_PER_DOMAIN=3 applies to full result set, not per-slot

### 3. No Programming Language Filtering
"implement rate limiting token bucket in GO gin middleware" → Python #1
Languages (Go, Python, Rust, Java, C++) are hard technical entities
that should act as constraints, not normal tokens.

### 4. Name Collision / Entity Resolution
"figma" → figma.jp (Japanese figurine company) at #8
Same class of bug as "Steps" pop group. Needs entity-level
disambiguation, not just token matching.

### 5. Intent: how-to → technical (13/15 failures)
tech_detector absorbs how-to queries that contain technical terms.
Queries like "docker compose healthcheck restart policy" have strong
technical keywords that overwhelm how-to signals. The retrieval layer
compensates — results are correct despite wrong intent label.

## Assessment

```
Intent Classification : B   (69% — how-to weakness)
Retrieval             : A   (98% top-5 relevance)
Ranking               : B+  (correct order, meaningless scores)
Constraint Handling   : A   (0 violations)
Score Calibration     : D   (all top-1 = 0.970)
Result Diversification: B   (minor domain repetition)
```

**Key insight**: Users experience result quality, not classifier accuracy.
98% top-5 relevance on a diverse technical benchmark is the standout metric.

## Priority Fixes

1. Fix score compression (separate ranking score from confidence score)
2. Add position-aware domain diversity re-ranking
3. Add ontology-based technical entity constraints (language filtering)
4. Improve intent classification (how-to vs technical boundary)
