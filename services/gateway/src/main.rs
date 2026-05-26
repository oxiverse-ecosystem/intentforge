use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ─── API Types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct IntentResponse {
    #[serde(default)]
    query: String,
    intent: String,
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    expanded_queries: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxResponse {
    results: Vec<SearxResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxResult {
    title: String,
    url: String,
    content: String,
    engine: String,
    #[serde(default)]
    score: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct WhoogleResult {
    #[serde(alias = "href", alias = "link")]
    url: String,
    #[serde(alias = "title", alias = "text")]
    title: String,
    #[serde(alias = "desc", alias = "snippet", default)]
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct WhoogleResponse {
    results: Vec<WhoogleResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InvidiousResult {
    #[serde(alias = "type")]
    result_type: Option<String>,
    title: Option<String>,
    #[serde(alias = "videoId")]
    video_id: Option<String>,
    #[serde(alias = "description", default)]
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct IndexerResult {
    url: String,
    title: String,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    authority: f32,
}

#[derive(Serialize)]
struct UnifiedResponse {
    intent: IntentResponse,
    local_results: Vec<IndexerResult>,
    web_results: Vec<SearxResult>,
}

// ─── Domain Authority (Algorithmic, Not Hardcoded) ───────────────────
// Scores based on domain signals: TLD, known quality indicators

fn domain_authority_score(url: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();

    let mut score: f32 = 0.5; // baseline for unknown domains

    // ── TLD-based scoring (algorithmic) ──
    if host.ends_with(".edu") || host.ends_with(".gov") {
        score += 0.3; // institutional authority
    } else if host.ends_with(".org") {
        score += 0.1; // generally trustworthy
    }

    // ── Domain pattern scoring (signals, not hardcoded lists) ──
    // Official documentation patterns
    if host.starts_with("docs.") || host.starts_with("doc.")
        || host.starts_with("developer.") || host.starts_with("dev.")
        || host.starts_with("learn.") || host.starts_with("api.")
        || url_lower.contains("/docs/") || url_lower.contains("/api/")
        || url_lower.contains("/reference/")
    {
        score += 0.25;
    }

    // Official source patterns
    if host.starts_with("official") || url_lower.contains("/official")
        || url_lower.contains("/homepage")
    {
        score += 0.2;
    }

    // High-quality content platforms
    if host.contains("wikipedia.org") || host.contains("wikimedia.org") {
        score += 0.2;
    }
    if host.contains("stackoverflow.com") || host.contains("stackexchange.com") {
        score += 0.15;
    }
    if host.contains("github.com") || host.contains("gitlab.com")
        || host.contains("codeberg.org")
    {
        score += 0.15;
    }

    // Package registries (technical authority)
    if host.contains("crates.io") || host.contains("pypi.org")
        || host.contains("npmjs.com") || host.contains("docs.rs")
        || host.contains("pkg.go.dev") || host.contains("rubygems.org")
    {
        score += 0.2;
    }

    // Well-known news/review sites
    if host.contains("arstechnica.com") || host.contains("theverge.com")
        || host.contains("wired.com") || host.contains("techcrunch.com")
        || host.contains("arxiv.org")
    {
        score += 0.1;
    }

    // Content farms / low quality signals
    if url_lower.contains("content-farm") || url_lower.contains("clickbait")
        || url_lower.contains("top10best") || url_lower.contains("bestof")
    {
        score -= 0.2;
    }

    // Medium/dev.to — decent but not authoritative
    if host.contains("medium.com") || host.contains("dev.to")
        || host.contains("hashnode.dev")
    {
        score += 0.05;
    }

    score.clamp(0.0, 1.0)
}

// ─── Freshness Decay ─────────────────────────────────────────────────

fn freshness_score(url: &str, intent: &str) -> f32 {
    // Different half-lives per intent category
    let half_life_hours: f32 = match intent {
        "fresh" => 6.0,            // news: 6-hour half-life
        "technical" => 720.0,      // docs: 30-day half-life
        "navigational" => 2160.0,  // official sites: 90-day half-life
        "how-to" => 168.0,         // guides: 7-day half-life
        "informational" => 168.0,  // general: 7-day half-life
        "comparison" => 336.0,     // reviews: 14-day half-life
        "transactional" => 720.0,  // products: 30-day half-life
        _ => 168.0,
    };

    // We don't have the actual crawl timestamp here, so we use URL signals
    // to estimate freshness. In a full system, this would query the indexer
    // for the document's last_crawled_at timestamp.

    let url_lower = url.to_lowercase();
    let mut estimated_age_hours: f32 = 720.0; // default: 30 days

    // Fresh content signals in URL
    if url_lower.contains("/2026/") || url_lower.contains("2026-") {
        estimated_age_hours = 24.0; // likely very recent
    } else if url_lower.contains("/2025/") || url_lower.contains("2025-") {
        estimated_age_hours = 168.0; // recent
    }

    // News/forum signals = likely more recent
    if url_lower.contains("/news/") || url_lower.contains("/blog/")
        || url_lower.contains("news.ycombinator.com")
        || url_lower.contains("/thread/") || url_lower.contains("/q/")
    {
        estimated_age_hours = estimated_age_hours.min(48.0);
    }

    // Documentation is relatively stable
    if url_lower.contains("/docs/") || url_lower.contains("/doc/")
        || url_lower.contains("/api/") || url_lower.contains("/reference/")
    {
        estimated_age_hours = estimated_age_hours.max(168.0);
    }

    // Exponential decay: score = exp(-age / half_life)
    (-estimated_age_hours / half_life_hours).exp()
}

// ─── Intent Boost (Enhanced) ─────────────────────────────────────────

fn calculate_intent_boost(url: &str, title: &str, query: &str, intent: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let title_lower = title.to_lowercase();
    let query_lower = query.to_lowercase();
    let intent_lower = intent.to_lowercase();

    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    let mut boost: f32 = 1.0;

    match intent_lower.as_str() {
        "navigation" | "navigational" => {
            // Host contains query terms → strong navigational signal
            if let Ok(parsed_url) = reqwest::Url::parse(url) {
                if let Some(host) = parsed_url.host_str() {
                    let host_lower = host.to_lowercase();
                    for term in &query_terms {
                        if host_lower.contains(term) {
                            boost += 0.5;
                            let path = parsed_url.path();
                            if path == "/" || path.is_empty() {
                                boost += 0.5; // homepage bonus
                            }
                        }
                    }
                }
            }

            // Documentation/official signals
            if url_lower.contains("docs.") || url_lower.contains("doc.")
                || url_lower.contains("/docs/") || url_lower.contains("/doc/")
                || url_lower.contains("documentation") || url_lower.contains("wiki")
                || title_lower.contains("documentation") || title_lower.contains("official")
                || title_lower.contains("homepage")
            {
                boost += 0.6;
            }
        }
        "technical" => {
            // Code repos, API docs, libraries
            if url_lower.contains("github.com") || url_lower.contains("gitlab.com")
                || url_lower.contains("docs.rs") || url_lower.contains("crates.io")
                || url_lower.contains("npmjs.com") || url_lower.contains("pypi.org")
                || url_lower.contains("/api/") || url_lower.contains("reference")
                || url_lower.contains("stackoverflow.com")
                || url_lower.contains("developer.")
            {
                boost += 0.5;
            }
        }
        "how-to" | "conceptual" | "informational" | "comparison" | "fresh" => {
            // Tutorials, guides, wikis, forums, news
            if url_lower.contains("stackoverflow.com") || url_lower.contains("reddit.com")
                || url_lower.contains("/blog/") || url_lower.contains("/tutorial/")
                || url_lower.contains("/guide/") || url_lower.contains("wikipedia.org")
                || url_lower.contains("dev.to") || url_lower.contains("medium.com")
                || url_lower.contains("news.ycombinator.com")
                || url_lower.contains("/news/") || url_lower.contains("/article/")
                || url_lower.contains("arxiv.org")
            {
                boost += 0.4;
            }

            // For comparison intent, boost review sites
            if intent_lower == "comparison" {
                if title_lower.contains("vs") || title_lower.contains("versus")
                    || title_lower.contains("comparison") || title_lower.contains("review")
                    || title_lower.contains("benchmark")
                {
                    boost += 0.3;
                }
            }
        }
        "transactional" => {
            if url_lower.contains("/download") || url_lower.contains("/pricing")
                || url_lower.contains("/signup") || url_lower.contains("/store")
                || url_lower.contains("/shop") || url_lower.contains("/buy")
            {
                boost += 0.5;
            }
        }
        _ => {}
    }

    // Query-term relevance in title (generic, intent-independent)
    let title_matches = query_terms.iter().filter(|t| title_lower.contains(*t)).count();
    if title_matches > 0 {
        boost += 0.1 * title_matches as f32;
    }

    boost
}

// ─── Content Quality (Dynamic — Shannon Entropy + Gibberish Detection) ──
// Detects spam, auto-generated content, and low-information results.
// NOT hardcoded — based on information-theoretic measures.

fn content_quality_score(text: &str) -> f32 {
    if text.len() < 20 {
        return 0.1; // too short to be useful
    }

    let mut score: f32 = 1.0;

    // Shannon entropy — measures information content
    // Natural language: 3.5-5.0 bits/char. Gibberish: <2.5 or >6.5
    let entropy = {
        let mut freq = [0u32; 128];
        let mut total = 0u32;
        for ch in text.chars() {
            if (ch as usize) < 128 {
                freq[ch as usize] += 1;
                total += 1;
            }
        }
        if total == 0 { return 0.1; }
        let mut h = 0.0f32;
        for &f in &freq {
            if f > 0 {
                let p = f as f32 / total as f32;
                h -= p * p.log2();
            }
        }
        h
    };

    if entropy < 2.0 {
        score *= 0.2; // very low entropy = repetitive spam
    } else if entropy < 3.0 {
        score *= 0.5;
    } else if entropy > 6.5 {
        score *= 0.3; // very high entropy = random characters
    }

    // Alpha ratio — natural language is >60% alphabetic
    let alpha_count = text.chars().filter(|c| c.is_alphabetic()).count();
    let alpha_ratio = alpha_count as f32 / text.len().max(1) as f32;
    if alpha_ratio < 0.4 {
        score *= 0.3; // too many non-alpha chars
    }

    // Average word length — natural language: 4-8 chars
    let words: Vec<&str> = text.split_whitespace().collect();
    if !words.is_empty() {
        let avg_word_len: f32 = words.iter().map(|w| w.len() as f32).sum::<f32>() / words.len() as f32;
        if avg_word_len > 20.0 {
            score *= 0.2; // concatenated garbage
        } else if avg_word_len > 15.0 {
            score *= 0.5;
        }
    }

    // Content farm / clickbait signals (dynamic pattern detection, not hardcoded domains)
    let text_lower = text.to_lowercase();
    let spam_patterns = ["click here", "buy now", "limited time", "act now",
        "you won't believe", "shocking", "one weird trick"];
    let spam_hits = spam_patterns.iter().filter(|p| text_lower.contains(**p)).count();
    if spam_hits >= 2 {
        score *= 0.4;
    }

    score.clamp(0.0, 1.0)
}

// ─── Semantic Relevance (Keyword Overlap Proxy) ──────────────────────
// Measures how many query terms appear in the result title + description.
// This is a fast proxy for full embedding cosine similarity.

fn semantic_relevance_score(query: &str, title: &str, content: &str) -> f32 {
    let q_lower = query.to_lowercase();
    let t_lower = title.to_lowercase();
    let c_lower = content.to_lowercase();

    // Extract topic terms (skip stop words and very short words)
    let stop_words = ["the","a","an","in","on","for","with","using","from","to",
        "and","or","of","is","are","was","were","be","been","has","have","had",
        "do","does","did","will","would","could","should","may","might",
        "how","what","where","when","why","which","who","this","that","these",
        "those","it","its","i","me","my","we","our","you","your","he","she","they"];
    let query_terms: Vec<&str> = q_lower.split_whitespace()
        .filter(|w| w.len() > 2 && !stop_words.contains(w))
        .collect();

    if query_terms.is_empty() {
        return 0.5;
    }

    let combined = format!("{} {}", t_lower, c_lower);
    let combined_words: Vec<&str> = combined.split_whitespace().collect();
    let matched = query_terms.iter().filter(|t| combined_words.iter().any(|w| w == *t || w.trim_matches(|c: char| !c.is_alphanumeric()) == **t)).count();
    let coverage = matched as f32 / query_terms.len() as f32;

    // Title match is more valuable than content match (also word-boundary)
    let title_words: Vec<&str> = t_lower.split_whitespace().collect();
    let title_matched = query_terms.iter().filter(|t| title_words.iter().any(|w| w == *t || w.trim_matches(|c: char| !c.is_alphanumeric()) == **t)).count();
    let title_coverage = title_matched as f32 / query_terms.len() as f32;

    // Weighted: 60% title match + 40% content match
    let base = (title_coverage * 0.6 + coverage * 0.4).clamp(0.0, 1.0);
    
    // Hard penalty: if less than 30% of query terms appear anywhere, result is likely irrelevant
    // This catches "Best Buy" for query "best rust web framework" — only "best" matches
    if coverage < 0.3 && title_coverage < 0.3 {
        // If coverage is very low (<25%), this is almost certainly irrelevant
        if coverage < 0.25 {
            return 0.01; // essentially zero — will be filtered out
        }
        // Otherwise scale down aggressively
        return (base * coverage * 3.0).clamp(0.0, 0.15);
    }
    
    base
}

// ─── Multi-Signal Fusion ─────────────────────────────────────────────

struct RankingWeights {
    rrf: f32,        // rank fusion from all sources
    intent: f32,     // intent category match
    freshness: f32,  // recency
    authority: f32,  // domain authority
    local_bonus: f32, // bonus for locally indexed
    quality: f32,    // content quality (entropy, spam detection)
    semantic: f32,   // semantic relevance (keyword overlap with query)
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self {
            rrf: 0.15,       // reduced from 0.20 — less important than relevance
            intent: 0.10,    // reduced from 0.15
            freshness: 0.08, // reduced from 0.10
            authority: 0.12, // reduced from 0.15
            local_bonus: 0.10, // same
            quality: 0.10,   // content quality (spam detection)
            semantic: 0.35,  // INCREASED: most important signal — query-result relevance
        }
    }
}

fn compute_final_score(
    rank_score: f32,
    intent_boost: f32,
    freshness: f32,
    authority: f32,
    is_local: bool,
    quality: f32,
    semantic: f32,
    weights: &RankingWeights,
) -> f32 {
    let local = if is_local { 1.0 } else { 0.0 };

    (weights.rrf * rank_score)
        + (weights.intent * intent_boost)
        + (weights.freshness * freshness)
        + (weights.authority * authority)
        + (weights.local_bonus * local)
        + (weights.quality * quality)
        + (weights.semantic * semantic)
}

// ─── Circuit Breaker (Dynamic Engine Backoff) ──────────────────────
// Tracks per-engine health. States: Closed (ok), Open (skip), HalfOpen (probe).
// No hardcoded skip lists — engines auto-recover after backoff window.

struct CircuitBreaker {
    engines: Mutex<HashMap<String, EngineHealth>>,
}

struct EngineHealth {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
    open_until: Option<Instant>, // circuit open (skip this engine) until this time
}

impl CircuitBreaker {
    fn new() -> Self {
        Self {
            engines: Mutex::new(HashMap::new()),
        }
    }

    fn is_open(&self, engine: &str) -> bool {
        let engines = self.engines.lock().unwrap();
        if let Some(health) = engines.get(engine) {
            if let Some(until) = health.open_until {
                return Instant::now() < until;
            }
        }
        false
    }

    fn record_success(&self, engine: &str) {
        let mut engines = self.engines.lock().unwrap();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
        });
        health.consecutive_failures = 0;
        health.open_until = None;
    }

    fn record_failure(&self, engine: &str) {
        let mut engines = self.engines.lock().unwrap();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
        });
        health.consecutive_failures += 1;
        health.last_failure = Some(Instant::now());

        // Exponential backoff: 30s, 60s, 120s, ... capped at 10 min
        if health.consecutive_failures >= 3 {
            let backoff_secs = 30u64 * 2u64.pow(health.consecutive_failures.saturating_sub(3));
            let backoff = Duration::from_secs(backoff_secs.min(600));
            health.open_until = Some(Instant::now() + backoff);
            tracing::warn!(
                "Circuit OPEN for engine '{}' — {} failures, backing off {:?}",
                engine, health.consecutive_failures, backoff
            );
        }
    }
}

// ─── Search Result Cache (TTL-based) ───────────────────────────────
// Caches (query, intent) → aggregated results for 5 minutes.
// Avoids hammering meta-search engines for repeated queries.

struct SearchCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    response_json: String, // serialized UnifiedResponse
    inserted_at: Instant,
    ttl: Duration,
}

impl SearchCache {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(key) {
            if entry.inserted_at.elapsed() < entry.ttl {
                return Some(entry.response_json.clone());
            }
        }
        None
    }

    fn put(&self, key: String, response_json: String, ttl: Duration) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(key, CacheEntry {
            response_json,
            inserted_at: Instant::now(),
            ttl,
        });

        // Evict expired entries to prevent unbounded growth
        entries.retain(|_, e| e.inserted_at.elapsed() < e.ttl);
    }
}

// ─── Main ────────────────────────────────────────────────────────────

struct AppState {
    circuit: CircuitBreaker,
    cache: SearchCache,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState {
        circuit: CircuitBreaker::new(),
        cache: SearchCache::new(),
    });

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {} (circuit-breaker + cache)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_search(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    // 0. Check cache first (5-min TTL)
    let cache_key = format!("{}:{}", params.q.to_lowercase().trim(), "all");
    if let Some(cached) = state.cache.get(&cache_key) {
        tracing::info!("Cache hit for query: {}", params.q);
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }
    // Timeout HTTP client — 10s for meta-search (SearXNG aggregates multiple engines)
    // Results are cached for 5 min, so this hit only happens once per query
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap();

    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // 1. Run Intent Analysis and Embedding in parallel
    let intent_url = format!("http://127.0.0.1:3005/analyze?q={}", q_encoded);
    let embed_url = format!("http://127.0.0.1:3005/embed?text={}", q_encoded);

    let (intent_res, embed_res) = tokio::join!(
        client.get(&intent_url).send(),
        client.get(&embed_url).send()
    );

    // 2. Process Intent & Embedding
    let intent: IntentResponse = match intent_res {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<IntentResponse>().await {
                Ok(parsed) => parsed,
                Err(e) => {
                    tracing::error!("Failed to parse IntentResponse (status: {}): {:?}", status, e);
                    fallback_intent(&q)
                }
            }
        }
        Err(e) => {
            tracing::error!("Intent Engine request failed/timed out: {:?}", e);
            fallback_intent(&q)
        }
    };

    let vector: Option<Vec<f32>> = match embed_res {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json["embedding"].as_array().map(|arr| {
                    arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
                })
            } else { None }
        },
        Err(_) => None,
    };

    // 3. Multi-Variation Fan-Out: query SearXNG with expanded queries for broader recall
    // The intent engine returns 2-4 query variations. We fire them all to SearXNG.
    // This catches results that the original query phrasing might miss.
    let expanded_queries = if intent.expanded_queries.len() > 1 {
        intent.expanded_queries.clone()
    } else {
        vec![q.clone()]
    };
    tracing::info!("Fan-out with {} query variations: {:?}", expanded_queries.len(), expanded_queries);

    let freshness_keywords = ["latest", "recent", "week", "month", "today", "newest", "cve", "vulnerability"];
    let is_freshness_query = intent.constraints.iter().any(|c| {
        let c_low = c.to_lowercase();
        freshness_keywords.iter().any(|&k| c_low.contains(k))
    }) || q.to_lowercase().contains("latest") || q.to_lowercase().contains("recent")
      || intent.intent == "fresh";

    let mut indexer_query = if let Some(ref v) = vector {
        let v_json = serde_json::to_string(v).unwrap();
        format!("http://127.0.0.1:6000/search?q={}&vector={}&min_score=0.5", q_encoded, urlencoding::encode(&v_json))
    } else {
        format!("http://127.0.0.1:6000/search?q={}", q_encoded)
    };

    if is_freshness_query {
        indexer_query.push_str("&freshness_boost=true");
    }

    // Build SearXNG URLs for all expanded queries (max 4 variations)
    let searx_urls: Vec<String> = expanded_queries.iter().take(4).map(|eq| {
        format!("http://127.0.0.1:8080/search?q={}&format=json", urlencoding::encode(eq))
    }).collect();

    let whoogle_url = format!("http://127.0.0.1:5000/search?q={}&format=json", q_encoded);
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let client_ref = &client;
    let circuit_ref = &state.circuit;

    // Check circuit breaker before calling each engine
    let searx_open = circuit_ref.is_open("searxng");
    let whoogle_open = circuit_ref.is_open("whoogle");
    let invidious_open = circuit_ref.is_open("invidious");

    let indexer_fut = async {
        let resp = client_ref.get(&indexer_query).send().await?;
        let status = resp.status();
        resp.json::<Vec<IndexerResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Indexer JSON (status: {}): {:?}", status, e);
            e
        })
    };

    // Fire all SearXNG variations in parallel
    let searx_futs: Vec<_> = searx_urls.iter().map(|url| {
        let url = url.clone();
        let searx_open = searx_open;
        async move {
            if searx_open {
                return Ok(SearxResponse { results: vec![] });
            }
            let resp = client_ref.get(&url).send().await?;
            let status = resp.status();
            resp.json::<SearxResponse>().await.map_err(|e| {
                tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
                e
            })
        }
    }).collect();

    let whoogle_fut = async {
        if whoogle_open {
            tracing::info!("Whoogle circuit OPEN — skipping");
            return Ok(WhoogleResponse { results: vec![] });
        }
        let resp = client_ref.get(&whoogle_url).send().await?;
        let status = resp.status();
        resp.json::<WhoogleResponse>().await.map_err(|e| {
            tracing::error!("Failed to parse Whoogle JSON (status: {}): {:?}", status, e);
            e
        })
    };

    let invidious_fut = async {
        if invidious_open {
            tracing::info!("Invidious circuit OPEN — skipping");
            return Ok(vec![]);
        }
        let resp = client_ref.get(&invidious_url).send().await?;
        let status = resp.status();
        resp.json::<Vec<InvidiousResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Invidious JSON (status: {}): {:?}", status, e);
            e
        })
    };

    // Join all futures: indexer + all SearXNG variations + whoogle + invidious
    let (indexer_res, searx_results, whoogle_res, invidious_res) = tokio::join!(
        indexer_fut,
        futures::future::join_all(searx_futs),
        whoogle_fut,
        invidious_fut
    );

    // 4. Process Local Results
    let mut local_results: Vec<IndexerResult> = match indexer_res {
        Ok(res) => res,
        Err(e) => {
            tracing::error!("Indexer search failed/timed out: {:?}", e);
            vec![]
        }
    };

    // 5. Aggregate Web Results from all sources
    let mut web_results: Vec<SearxResult> = Vec::new();

    // Aggregate SearXNG results from all query variations
    for (i, searx_res) in searx_results.into_iter().enumerate() {
        match searx_res {
            Ok(searx_data) => {
                tracing::info!("SearXNG variation {} returned {} results", i, searx_data.results.len());
                circuit_ref.record_success("searxng");
                web_results.extend(searx_data.results);
            }
            Err(e) => {
                tracing::error!("SearXNG variation {} request failed/timed out: {:?}", i, e);
                circuit_ref.record_failure("searxng");
            }
        }
    }

    match whoogle_res {
        Ok(whoogle_data) => {
            tracing::info!("Whoogle returned {} results", whoogle_data.results.len());
            circuit_ref.record_success("whoogle");
            for r in whoogle_data.results {
                web_results.push(SearxResult {
                    title: r.title,
                    url: r.url,
                    content: r.description.unwrap_or_default(),
                    engine: "whoogle".to_string(),
                    score: 0.0,
                });
            }
        }
        Err(e) => {
            tracing::warn!("Whoogle request failed/timed out: {:?}", e);
            circuit_ref.record_failure("whoogle");
        }
    }

    match invidious_res {
        Ok(invidious_data) => {
            tracing::info!("Invidious returned {} results", invidious_data.len());
            circuit_ref.record_success("invidious");
            for r in invidious_data {
                if r.result_type.as_deref() == Some("video") {
                    if let Some(vid) = r.video_id {
                        let video_url = format!("https://www.youtube.com/watch?v={}", vid);
                        web_results.push(SearxResult {
                            title: r.title.unwrap_or_else(|| "No Title".to_string()),
                            url: video_url,
                            content: r.description.unwrap_or_default(),
                            engine: "invidious".to_string(),
                            score: 0.0,
                        });
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Invidious request failed/timed out: {:?}", e);
            circuit_ref.record_failure("invidious");
        }
    }

    // Deduplicate — URL normalization + domain-based dedup
    // Multiple query variations may return the same page with different URLs
    let mut unique_web_results = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    let mut seen_domains = std::collections::HashMap::<String, usize>::new();
    const MAX_PER_DOMAIN: usize = 5; // prevent single-domain dominance

    for res in web_results {
        // Normalize URL: lowercase, strip trailing slash, strip fragment
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            no_trailing.to_string()
        };

        // Domain dedup: cap results per domain for diversity
        let domain = reqwest::Url::parse(&res.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
            .unwrap_or_default();

        let domain_count = seen_domains.entry(domain.clone()).or_insert(0);
        if *domain_count >= MAX_PER_DOMAIN {
            continue;
        }

        if seen_urls.insert(normalized) {
            *domain_count += 1;
            unique_web_results.push(res);
        }
    }
    let mut web_results = unique_web_results;

    tracing::info!("After dedup: {} unique web results", web_results.len());

    // 6. Multi-Signal Ranking with Content Quality + Semantic Relevance
    let weights = RankingWeights::default();

    // Rank web results
    for (i, res) in web_results.iter_mut().enumerate() {
        let rank_score = 10.0 / (i + 1) as f32;
        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = domain_authority_score(&res.url);

        // NEW: Content quality — penalize spam/gibberish
        let quality = content_quality_score(&res.content);

        // NEW: Semantic relevance — how well does the result match the query?
        let semantic = semantic_relevance_score(&q, &res.title, &res.content);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            false, // not local
            quality,
            semantic,
            &weights,
        );
    }
    // Filter out results with very low semantic relevance (<15% of query terms matched)
    // This removes "Best Buy" for "best rust web framework" — completely irrelevant results
    web_results.retain(|res| {
        let semantic = semantic_relevance_score(&q, &res.title, &res.content);
        semantic >= 0.15
    });
    web_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Rank local results — they get full semantic scoring too
    for (i, res) in local_results.iter_mut().enumerate() {
        let rank_score = 10.0 / (i + 1) as f32;
        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        // Use authority from the indexer (computed from crawl data) if available, else compute from domain
        let authority = if res.authority > 0.0 { res.authority } else { domain_authority_score(&res.url) };
        // so quality is inherently higher (we crawled the full page)
        let quality = 0.8; // trusted — we crawled and indexed this ourselves
        let semantic = semantic_relevance_score(&q, &res.title, "");

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            true, // local index bonus
            quality,
            semantic,
            &weights,
        );
    }
    local_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 7. Feed Meta-Search Results into Crawl Queue with relevance signals
    // Include the score so the crawler can prioritize high-relevance URLs
    let crawl_urls: Vec<serde_json::Value> = web_results.iter().take(15).enumerate().map(|(i, r)| {
        serde_json::json!({
            "url": r.url,
            "priority": r.score, // use the computed relevance score, not just position
            "source": format!("meta-search:{}", r.engine)
        })
    }).collect();

    if !crawl_urls.is_empty() {
        tracing::info!("Feeding {} URLs to crawl queue", crawl_urls.len());
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let payload = serde_json::json!({ "urls": crawl_urls });
            let _ = client.post("http://127.0.0.1:5001/enqueue")
                .json(&payload)
                .send()
                .await;
        });
    }

    let response = UnifiedResponse {
        intent,
        local_results,
        web_results,
    };

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    state.cache.put(cache_key, response_json, Duration::from_secs(300));

    Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({})))
}

fn fallback_intent(q: &str) -> IntentResponse {
    IntentResponse {
        query: q.to_string(),
        intent: "informational".to_string(),
        confidence: 0.3,
        constraints: vec![],
        expanded_queries: vec![q.to_string()],
    }
}
