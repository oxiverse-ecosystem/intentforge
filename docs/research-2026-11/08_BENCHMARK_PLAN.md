# 08 — Benchmark and Evaluation Plan

**Status:** Proposed for founder review.
**Companion documents:** `02`, `04`, `05`, `06`, `09`.
**Goal:** make every architectural claim measurable, reproducible, and falsifiable.

---

## 1. Principles

1. **Separate synthetic from real-world.** Synthetic tests are deterministic and run in CI. Real-world evaluation is periodic and labelled with its date and conditions.
2. **Every metric has a threshold.** A number without a pass/fail bar is not a benchmark.
3. **Every benchmark is reproducible.** Fixed seeds, fixed datasets, recorded environment.
4. **Measure the failure modes, not just the happy path.** Coverage without precision is worthless.
5. **Benchmarks must be runnable by an AI agent** with no human setup.

---

## 2. WebGraph Benchmarks

### 2.1 URL discovery coverage

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Seed expansion rate | URLs discovered per seed | ≥ 5 for a curated domain | Run against a fixed seed set |
| Sitemap parse recall | sitemap URLs found / sitemap URLs present | ≥ 0.99 | Fixture sitemaps |
| robots.txt rule accuracy | correct allow/deny decisions | ≥ 0.99 | RFC 9309 test corpus |
| Link extraction recall | links extracted / links present | ≥ 0.98 | Fixture HTML |
| Off-scope rejection rate | out-of-scope URLs rejected | ≥ 0.99 | Fixture with mixed scopes |

### 2.2 Crawl performance

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Crawl success rate | 2xx / attempts | ≥ 0.90 | Instrumented run |
| Crawl latency p50/p95 | time to first byte | site-dependent | Instrumented run |
| Politeness compliance | requests within budget / total | 1.00 | Audit log analysis |
| Budget adherence | pages per domain ≤ budget | 1.00 | Audit log analysis |
| Blocked/banned rate | 403/429 per domain | ≤ 0.05 | Audit log analysis |

### 2.3 Content extraction

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Title extraction accuracy | correct title / pages | ≥ 0.98 | Annotated fixture |
| Main content accuracy | token F1 vs. reference | ≥ 0.90 | Annotated fixture |
| Structured data recall | entities found / entities present | ≥ 0.95 | schema.org fixture |
| Structured data precision | correct entities / entities found | ≥ 0.98 | schema.org fixture |
| Language detection accuracy | correct language / pages | ≥ 0.95 | Fixture with known languages |
| Canonical URL accuracy | correct canonical / pages | ≥ 0.98 | Fixture |

### 2.4 Change detection

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Change detection accuracy | correct state / revisions | ≥ 0.98 | Fixture with known changes |
| False positive rate | unchanged flagged changed / total | ≤ 0.02 | Fixture |
| False negative rate | changed flagged unchanged / total | ≤ 0.02 | Fixture |
| Redirect detection | correct target / redirects | ≥ 0.99 | Fixture |
| Removal detection | correct 404 handling | ≥ 0.99 | Fixture |

### 2.5 Duplicate detection

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Exact dedup precision | correct merges / merges | 1.00 | Fixture |
| Near-dup cluster purity | same-cluster items truly similar | ≥ 0.95 | Fixture |
| Near-dup recall | similar pairs clustered / similar pairs | ≥ 0.90 | Fixture |

### 2.6 Resource and compliance

| Metric | Definition | Threshold |
|---|---|---|
| Peak memory per crawl worker | RSS | ≤ 512 MiB |
| Peak CPU per crawl worker | % of one core | ≤ 100% |
| Document size limit enforcement | oversize rejected | 1.00 |
| SSRF defence | internal URL rejected | 1.00 |
| robots.txt fail-closed | crawl blocked when robots unfetchable | 1.00 |

---

## 3. Commerce Benchmarks

### 3.1 Coverage

| Metric | Definition | Threshold |
|---|---|---|
| Product coverage | products with ≥ 1 offer / products | ≥ 0.95 |
| Regional coverage | products with an offer in the target country | report only |
| Source diversity | products with ≥ 2 independent sources | report only |

### 3.2 Identity resolution

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Match precision | correct merges / merges | ≥ 0.98 | Annotated pair set |
| Match recall | correct merges / true pairs | ≥ 0.90 | Annotated pair set |
| GTIN match accuracy | correct / GTIN-based merges | ≥ 0.999 | Annotated |
| Weak-match auto-merge rate | auto-merges without strong ID | **0 by design** | Audit |
| Review queue precision | items needing review / queued | report only | Sampling |

### 3.3 Offer accuracy

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Price accuracy | price within tolerance of source | ≥ 0.99 | Re-fetch sample |
| Availability accuracy | state matches source | ≥ 0.98 | Re-fetch sample |
| Currency correctness | correct ISO code / offers | 1.00 | Schema validation |
| Stale price rate | offers older than target / offers | ≤ 0.10 | Freshness audit |
| Divergence detection | outliers flagged / outliers present | ≥ 0.90 | Injected outliers |

### 3.4 Source reliability

| Metric | Definition | Threshold |
|---|---|---|
| Ingestion success rate | completed runs / total runs | ≥ 0.95 |
| Schema drift detection | shape changes detected | 1.00 |
| Credential expiry detection | auth failures detected | 1.00 |

---

## 4. Unified Search Benchmarks

### 4.1 Relevance

| Metric | Definition | Threshold | Method |
|---|---|---|---|
| Precision@5 | relevant in top 5 / 5 | ≥ 0.70 | Judged query set |
| Precision@10 | relevant in top 10 / 10 | ≥ 0.65 | Judged query set |
| Recall@20 | relevant retrieved / relevant existing | ≥ 0.70 | Judged query set |
| NDCG@10 | graded relevance | ≥ 0.70 | Judged query set |
| MRR | mean reciprocal rank of first relevant | ≥ 0.75 | Judged query set |

### 4.2 Product search

| Metric | Definition | Threshold |
|---|---|---|
| Product Precision@5 | correct products / 5 | ≥ 0.75 |
| Offer correctness | displayed offer matches source | ≥ 0.99 |
| Comparison correctness | compared attributes correct | ≥ 0.98 |
| Filter accuracy | filtered-out results truly irrelevant | ≥ 0.98 |
| Geo relevance | results available in the requested country | ≥ 0.95 |

### 4.3 Performance

| Metric | Definition | Threshold |
|---|---|---|
| Web search p50 | end-to-end | ≤ 300 ms |
| Web search p95 | end-to-end | ≤ 800 ms |
| Product search p50 | end-to-end | ≤ 400 ms |
| Product search p95 | end-to-end | ≤ 1000 ms |
| Price history p95 | end-to-end | ≤ 500 ms |
| Concurrent load | 50 concurrent, error rate | ≤ 0.01 |

---

## 5. Datasets

### 5.1 Synthetic (CI, deterministic)

| Dataset | Purpose | Size |
|---|---|---|
| `fixture_pages` | extraction, language, canonical | ~200 HTML files |
| `fixture_sitemaps` | sitemap parsing | ~20 XML files |
| `fixture_robots` | RFC 9309 rule evaluation | ~50 rule sets |
| `fixture_products` | identity resolution | ~10k listings, known truth |
| `fixture_queries` | relevance judging | ~200 queries with graded labels |
| `fixture_attack_urls` | SSRF / malformed URL defence | ~100 URLs |

**Generation:** a checked-in generator script with a fixed seed. Datasets are committed so CI is reproducible.

### 5.2 Real-world (periodic, labelled)

| Dataset | Purpose | Cadence |
|---|---|---|
| Live crawl sample | extraction and change detection | monthly |
| Live product sample | price/availability accuracy | weekly |
| Query log sample | relevance | monthly, anonymised |

**Real-world datasets must record:** collection date, source, conditions, and any known bias.

---

## 6. Ground-Truth Methodology

| Task | Method |
|---|---|
| Extraction | Human-annotated reference extraction for a sample |
| Identity | Pairwise human judgements on a stratified sample |
| Relevance | Pooled judging with at least 2 annotators; adjudicate disagreements |
| Price/availability | Independent re-fetch from a different egress |
| Change detection | Controlled fixtures with injected changes |

**Inter-annotator agreement** must be reported. Low agreement means the task definition is wrong, not that the annotators are.

---

## 7. Evaluation Scripts

```
services/benchmark/
├── src/
│   ├── webgraph.rs
│   ├── commerce.rs
│   ├── search.rs
│   └── report.rs
├── datasets/
│   ├── synthetic/
│   └── real_world/
├── expected/
└── README.md
```

- One binary, subcommands per suite.
- Outputs JSON for machine comparison and Markdown for humans.
- Exit code non-zero on threshold failure.
- Baseline results are committed so regressions are visible in review.

---

## 8. Regression and Load Testing

| Test | Trigger | Threshold |
|---|---|---|
| Unit + integration | every PR | 100% pass |
| Synthetic benchmark | every PR | no metric regresses > 2% |
| Load test | nightly | p95 within threshold |
| Failure injection | weekly | graceful degradation only |
| Real-world benchmark | monthly | reported to founder |

### Failure injection scenarios

1. PostgreSQL unavailable → API returns a clear error; no panic.
2. Commerce Index unavailable → web results still served; product strip omitted.
3. Identity service unavailable → no merging; results shown separately.
4. Upstream feed returns malformed data → rejected; run marked partial.
5. Upstream feed returns an unexpected schema → detected; alert raised.
6. Crawler egress blocked → budget exhausted; domains deprioritised.
7. Queue backlog exceeds cap → backpressure; new URLs deferred.
8. Parser OOM on a hostile document → document rejected; service survives.

---

## 9. Acceptance Gates

| Gate | When | Criteria |
|---|---|---|
| G1 | Phase 2 complete | crawl success ≥ 0.90, politeness 1.00 |
| G2 | Phase 3 complete | Precision@10 ≥ 0.65, p95 ≤ 800 ms |
| G3 | Phase 5 complete | identity precision ≥ 0.98, no weak auto-merges |
| G4 | Phase 6 complete | product Precision@5 ≥ 0.75, offer correctness ≥ 0.99 |
| G5 | Phase 7 complete | all gates green under load + failure injection |

---

## 10. Open Questions

1. **Judged query set size.** How many queries are needed for a stable NDCG? Recommendation: start with 200, expand.
2. **Annotator access.** Who judges? Internal only, or a paid panel?
3. **Real-world sample bias.** How to avoid overfitting to the domains easiest to crawl?
4. **Baseline comparison.** Against which public engine should relevance be compared, given different index scope?
5. **Benchmark cadence cost.** Real-world evaluation consumes crawl budget; how much is acceptable?

---

*Proposal only. Thresholds are initial proposals and must be calibrated against real data in Phase 0.*
