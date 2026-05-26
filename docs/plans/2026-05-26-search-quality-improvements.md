# Search Quality Improvements Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make IntentForge-v2 search results excellent for both general and complex queries, better than raw SearXNG, with proper crawler/indexer integration.

**Architecture:** Algorithmic improvements to ranking, quality filtering, and crawl pipeline — no hardcoded lists or band-aids.

**Tech Stack:** Rust (axum, tokio, reqwest, tantivy), Docker

---

## Current State (API Test Results)

- Web results: GOOD (22-35 per query, mostly relevant)
- Local results: GARBAGE (stale Facebook/YouTube error pages)
- Intent classification: MOSTLY ACCURATE (0.75-0.87 confidence)
- Score normalization: MISSING (scores vary 1.20-2.18 across queries)
- Crawler/indexer: POOR integration (stale content, no quality filtering)

## Issues Found

1. **Local results have no quality filter** — line 1128 hardcodes `quality: 0.8` for all local results
2. **No cross-query score normalization** — scores meaningless across different queries
3. **Static ranking weights** — same weights for all intents (fresh vs technical vs navigational)
4. **No engine reliability tracking** — consensus treats all engines equally
5. **Crawler doesn't leverage meta-search quality signals** — feeds URLs but ignores content quality
6. **Semantic threshold too lenient** — 0.15 filter lets through weak matches

---

## Task 1: Add Content Quality Scoring to Indexer Results

**Objective:** Local results should use actual content quality, not hardcoded 0.8

**Files:**
- Modify: `services/indexer/src/main.rs` (add content field to search response)
- Modify: `services/gateway/src/main.rs:75-83` (add content field to IndexerResult)
- Modify: `services/gateway/src/main.rs:1118-1142` (use real quality score)

**Step 1: Update IndexerResult struct to include content**

In `services/gateway/src/main.rs`, update the IndexerResult struct (line 75-83):

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct IndexerResult {
    url: String,
    title: String,
    #[serde(default)]
    content: String,       // ADD: actual content for quality scoring
    #[serde(default)]
    score: f32,
    #[serde(default)]
    authority: f32,
}
```

**Step 2: Update indexer to return content in search results**

In `services/indexer/src/main.rs`, in the search handler (around line 201-344), add content to the response JSON. Find where results are built and add:

```rust
// In the search results building code, include content:
serde_json::json!({
    "url": doc.get_first(url_field).unwrap().as_text().unwrap_or(""),
    "title": doc.get_first(title_field).unwrap().as_text().unwrap_or(""),
    "content": doc.get_first(content_field).unwrap().as_text().unwrap_or(""),  // ADD
    "score": final_score,
    "authority": authority,
})
```

**Step 3: Use real quality score for local results**

In `services/gateway/src/main.rs`, replace the hardcoded quality (line 1128):

```rust
// OLD:
let quality = 0.8; // trusted: we crawled and indexed this ourselves

// NEW:
let quality = content_quality_score(&res.content);
```

**Step 4: Build and test**

```bash
cd services && docker compose build indexer gateway && docker compose up -d indexer gateway
# Wait 30s for startup
curl -s 'http://localhost:4000/search?q=quantum+error+correction' | python -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('local_results', [])[:3]:
    print(f'{r[\"score\"]:.3f} | {r[\"title\"][:60]}')"
```

**Step 5: Commit**

```bash
git add services/gateway/src/main.rs services/indexer/src/main.rs
git commit -m "feat: use real content quality scoring for local results"
```

---

## Task 2: Cross-Query Score Normalization

**Objective:** Make scores comparable across different queries using percentile-based normalization

**Files:**
- Modify: `services/gateway/src/main.rs:1071-1178` (add normalization step)

**Step 1: Add normalization function**

After the ranking loop (after line 1116), add a normalization function:

```rust
/// Normalize scores to [0, 1] using percentile-based scaling.
/// This makes scores comparable across different queries.
/// Uses robust scaling: (score - median) / (p90 - p10) clipped to [0, 1]
fn normalize_scores(scores: &mut [f32]) {
    if scores.len() < 3 {
        return; // not enough data to normalize
    }
    let mut sorted: Vec<f32> = scores.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    
    let p10 = sorted[(sorted.len() as f32 * 0.10) as usize];
    let p90 = sorted[(sorted.len() as f32 * 0.90) as usize];
    let range = (p90 - p10).max(0.001); // avoid division by zero
    
    for score in scores.iter_mut() {
        *score = ((*score - p10) / range).clamp(0.0, 1.0);
    }
}
```

**Step 2: Apply normalization to web results**

After line 1116 (after sorting web_results), add:

```rust
// Normalize scores to [0, 1] for cross-query comparability
let mut web_scores: Vec<f32> = web_results.iter().map(|r| r.score).collect();
normalize_scores(&mut web_scores);
for (i, r) in web_results.iter_mut().enumerate() {
    r.score = web_scores[i];
}
```

**Step 3: Apply normalization to local results**

After line 1143 (after sorting local_results), add:

```rust
let mut local_scores: Vec<f32> = local_results.iter().map(|r| r.score).collect();
normalize_scores(&mut local_scores);
for (i, r) in local_results.iter_mut().enumerate() {
    r.score = local_scores[i];
}
```

**Step 4: Build and test**

```bash
cd services && docker compose build gateway && docker compose up -d gateway
# Test multiple queries — scores should now be in [0, 1] range
for q in "best programming languages 2026" "quantum error correction" "rust async"; do
  echo "=== $q ==="
  curl -s "http://localhost:4000/search?q=$(echo $q | sed 's/ /+/g')" | python -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('web_results', [])[:3]:
    print(f'{r[\"score\"]:.3f} | {r[\"title\"][:60]}')"
done
```

**Step 5: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: normalize scores to [0,1] for cross-query comparability"
```

---

## Task 3: Intent-Specific Ranking Weight Adjustment

**Objective:** Adjust ranking signal weights based on query intent (freshness matters more for news, authority for navigational, etc.)

**Files:**
- Modify: `services/gateway/src/main.rs:575-588` (add intent-aware weights)
- Modify: `services/gateway/src/main.rs:1072` (use intent-specific weights)

**Step 1: Add intent-specific weight constructor**

Replace the `Default` impl for `RankingWeights` (lines 575-588):

```rust
impl RankingWeights {
    fn for_intent(intent: &str) -> Self {
        match intent {
            "fresh" => Self {
                rrf: 0.08,
                intent: 0.05,
                freshness: 0.20,   // news needs recency
                authority: 0.15,   // news needs trustworthy sources
                local_bonus: 0.02,
                quality: 0.10,
                semantic: 0.25,
                consensus: 0.15,
            },
            "technical" => Self {
                rrf: 0.10,
                intent: 0.12,      // technical intent boost matters
                freshness: 0.05,   // docs are stable
                authority: 0.15,   // official docs preferred
                local_bonus: 0.05,
                quality: 0.08,
                semantic: 0.35,    // technical queries need precision
                consensus: 0.10,
            },
            "navigational" => Self {
                rrf: 0.05,
                intent: 0.25,      // navigational intent is dominant
                freshness: 0.03,
                authority: 0.20,   // official sites preferred
                local_bonus: 0.02,
                quality: 0.05,
                semantic: 0.30,
                consensus: 0.10,
            },
            "comparison" => Self {
                rrf: 0.12,
                intent: 0.10,
                freshness: 0.10,   // reviews should be recent
                authority: 0.08,
                local_bonus: 0.02,
                quality: 0.12,     // comparison content quality matters
                semantic: 0.30,
                consensus: 0.16,   // cross-source agreement for comparisons
            },
            _ => Self {  // informational, how-to, default
                rrf: 0.10,
                intent: 0.08,
                freshness: 0.07,
                authority: 0.10,
                local_bonus: 0.05,
                quality: 0.10,
                semantic: 0.30,
                consensus: 0.20,
            },
        }
    }
}
```

**Step 2: Use intent-specific weights**

In `handle_search`, replace line 1072:

```rust
// OLD:
let weights = RankingWeights::default();

// NEW:
let weights = RankingWeights::for_intent(&intent.intent);
```

**Step 3: Build and test**

```bash
cd services && docker compose build gateway && docker compose up -d gateway
# Test fresh query — should prioritize recent results
curl -s 'http://localhost:4000/search?q=latest+AI+regulation+2026' | python -c "
import json, sys
data = json.load(sys.stdin)
print(f'Intent: {data[\"intent\"][\"intent\"]}')
for r in data.get('web_results', [])[:3]:
    print(f'{r[\"score\"]:.3f} | {r[\"title\"][:60]}')"
```

**Step 4: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: intent-specific ranking weight adjustment"
```

---

## Task 4: Local Result Quality Gate

**Objective:** Filter out garbage local results (error pages, stale content) using algorithmic quality checks

**Files:**
- Modify: `services/gateway/src/main.rs:1118-1143` (add quality gate)

**Step 1: Add quality gate for local results**

After computing scores for local results (after line 1142), add filtering:

```rust
// Quality gate: filter out garbage local results
// Use the same semantic threshold as web results + content quality check
local_results.retain(|r| {
    // Must have minimum semantic relevance
    let semantic_ok = r.score > 0.1;
    // Title must not be empty or too short
    let title_ok = r.title.len() > 5;
    // URL must not be an error page pattern
    let url_lower = r.url.to_lowercase();
    let not_error = !url_lower.contains("/error")
        && !url_lower.contains("/404")
        && !url_lower.contains("/login")
        && !url_lower.contains("/signin")
        && !url_lower.contains("/signup");
    semantic_ok && title_ok && not_error
});
```

**Step 2: Build and test**

```bash
cd services && docker compose build gateway && docker compose up -d gateway
# Test — local results should be fewer but higher quality
curl -s 'http://localhost:4000/search?q=quantum+error+correction' | python -c "
import json, sys
data = json.load(sys.stdin)
local = data.get('local_results', [])
print(f'Local results: {len(local)}')
for r in local[:3]:
    print(f'{r[\"score\"]:.3f} | {r[\"title\"][:60]} | {r[\"url\"][:50]}')"
```

**Step 3: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: algorithmic quality gate for local results"
```

---

## Task 5: Engine Reliability Tracking

**Objective:** Weight consensus score by per-engine historical reliability (not all engines equal)

**Files:**
- Modify: `services/gateway/src/main.rs:545-560` (update consensus to use reliability)
- Modify: `services/gateway/src/main.rs:617-676` (add reliability tracking to circuit breaker)

**Step 1: Add reliability tracking to EngineHealth**

Update the `EngineHealth` struct (line 621-625):

```rust
struct EngineHealth {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
    open_until: Option<Instant>,
    // NEW: reliability tracking
    total_queries: u64,
    successful_queries: u64,
    avg_result_quality: f32, // rolling average of result quality from this engine
}
```

**Step 2: Add reliability methods**

Add to the `CircuitBreaker` impl:

```rust
fn get_reliability(&self, engine: &str) -> f32 {
    let engines = self.engines.lock().unwrap();
    if let Some(health) = engines.get(engine) {
        if health.total_queries == 0 {
            return 0.7; // default for unknown engines
        }
        let success_rate = health.successful_queries as f32 / health.total_queries as f32;
        // Blend success rate with result quality
        (success_rate * 0.6 + health.avg_result_quality * 0.4).clamp(0.3, 1.0)
    } else {
        0.7 // default
    }
}

fn record_query_quality(&self, engine: &str, quality: f32) {
    let mut engines = self.engines.lock().unwrap();
    let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
        consecutive_failures: 0,
        last_failure: None,
        open_until: None,
        total_queries: 0,
        successful_queries: 0,
        avg_result_quality: 0.5,
    });
    health.total_queries += 1;
    health.successful_queries += 1;
    // Exponential moving average
    health.avg_result_quality = health.avg_result_quality * 0.9 + quality * 0.1;
}
```

**Step 3: Update consensus to use reliability**

Update `consensus_score` function (line 551-560):

```rust
fn consensus_score(sources: &[String], circuit: &CircuitBreaker) -> f32 {
    if sources.is_empty() {
        return 0.3;
    }
    let unique_sources: std::collections::HashSet<&String> = sources.iter().collect();
    let count = unique_sources.len() as f32;
    
    // Weight by engine reliability
    let reliability_sum: f32 = unique_sources.iter()
        .map(|s| circuit.get_reliability(s))
        .sum();
    let avg_reliability = reliability_sum / count.max(1.0);
    
    // Logarithmic scaling weighted by reliability
    let base = (0.3 + 0.2 * (count - 1.0).max(0.0).ln()).clamp(0.3, 0.95);
    base * avg_reliability
}
```

**Step 4: Update all consensus_score calls**

Update line 1095 and 1139 to pass the circuit breaker:

```rust
// Line 1095:
let consensus = consensus_score(&res.sources, &state.circuit);

// Line 1139:
consensus_score(&["local".to_string()], &state.circuit),
```

**Step 5: Build and test**

```bash
cd services && docker compose build gateway && docker compose up -d gateway
curl -s 'http://localhost:4000/search?q=rust+async+runtime' | python -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('web_results', [])[:5]:
    print(f'{r[\"score\"]:.3f} | sources={r[\"sources\"]} | {r[\"title\"][:50]}')"
```

**Step 6: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: engine reliability tracking for consensus scoring"
```

---

## Task 6: Improve Crawl Pipeline Integration

**Objective:** Better feed meta-search results into crawler with quality signals

**Files:**
- Modify: `services/gateway/src/main.rs:1145-1165` (improve crawl feed)
- Modify: `services/crawler/src/main.rs` (use quality signals from gateway)

**Step 1: Improve crawl URL selection**

Replace the crawl feed logic (lines 1145-1165) with quality-aware selection:

```rust
// 7. Feed high-quality Meta-Search Results into Crawl Queue
// Only feed results that pass quality thresholds — don't waste crawl budget on garbage
let crawl_urls: Vec<serde_json::Value> = web_results.iter()
    .filter(|r| {
        // Quality thresholds for crawling
        r.score > 0.3                    // must be reasonably ranked
        && !r.content.is_empty()         // must have content snippet
        && r.title.len() > 10            // must have real title
    })
    .take(20) // feed more URLs for better coverage
    .enumerate()
    .map(|(i, r)| {
        serde_json::json!({
            "url": r.url,
            "priority": r.score,
            "source": format!("meta-search:{}", r.engine),
            "quality_hint": r.score,  // pass quality signal to crawler
        })
    })
    .collect();
```

**Step 2: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: quality-aware crawl URL selection from meta-search"
```

---

## Task 7: Raise Semantic Threshold and Improve Filtering

**Objective:** Be more aggressive about filtering irrelevant results

**Files:**
- Modify: `services/gateway/src/main.rs:1109-1115` (raise threshold)

**Step 1: Raise semantic threshold**

Replace lines 1109-1115:

```rust
// OLD: threshold was 0.15
web_results.retain(|_| {
    let keep = semantic_scores[_idx] >= 0.15;
    _idx += 1;
    keep
});

// NEW: adaptive threshold based on result count
let semantic_threshold = if web_results.len() > 30 { 0.25 }
    else if web_results.len() > 20 { 0.20 }
    else { 0.15 };
let mut _idx = 0;
web_results.retain(|_| {
    let keep = semantic_scores[_idx] >= semantic_threshold;
    _idx += 1;
    keep
});
```

**Step 2: Build and test**

```bash
cd services && docker compose build gateway && docker compose up -d gateway
# Test with a complex query
curl -s 'http://localhost:4000/search?q=how+does+quantum+error+correction+work' | python -c "
import json, sys
data = json.load(sys.stdin)
web = data.get('web_results', [])
print(f'Web results: {len(web)}')
for r in web[:5]:
    print(f'{r[\"score\"]:.3f} | {r[\"title\"][:60]}')"
```

**Step 3: Commit**

```bash
git add services/gateway/src/main.rs
git commit -m "feat: adaptive semantic threshold based on result volume"
```

---

## Task 8: Rebuild All Services and Full Integration Test

**Objective:** Rebuild everything and verify end-to-end quality

**Step 1: Rebuild all services**

```bash
cd services && docker compose build && docker compose up -d
sleep 30  # wait for startup
```

**Step 2: Run comprehensive test suite**

```bash
for q in \
  "best programming languages 2026" \
  "how does quantum error correction work" \
  "rust async runtime comparison tokio vs async-std" \
  "Hebbian plasticity in artificial neural networks" \
  "latest AI regulation updates 2026"; do
  echo ""
  echo "========================================="
  echo "QUERY: $q"
  echo "========================================="
  curl -s "http://localhost:4000/search?q=$(echo $q | sed 's/ /+/g')" | python -c "
import json, sys
data = json.load(sys.stdin)
intent = data['intent']
print(f'Intent: {intent[\"intent\"]} (conf: {intent[\"confidence\"]:.2f})')
web = data.get('web_results', [])
local = data.get('local_results', [])
print(f'Web: {len(web)} | Local: {len(local)}')
print('Top 5 web results:')
for r in web[:5]:
    print(f'  {r[\"score\"]:.3f} | {r[\"sources\"]} | {r[\"title\"][:60]}')
if local:
    print('Top 3 local results:')
    for r in local[:3]:
        print(f'  {r[\"score\"]:.3f} | {r[\"title\"][:60]}')
"
done
```

**Step 3: Commit all changes**

```bash
git add -A
git commit -m "feat: comprehensive search quality improvements"
```

---

## Verification Criteria

- [ ] All 5 test queries return relevant top-5 results
- [ ] Scores are normalized to [0, 1] range
- [ ] Local results are filtered (no Facebook/YouTube error pages)
- [ ] Fresh queries prioritize recent results
- [ ] Technical queries prioritize official docs/repos
- [ ] No hardcoded domain lists (all algorithmic)
- [ ] Crawler receives quality-filtered URLs from meta-search
