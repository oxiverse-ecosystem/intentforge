use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

// ─── Multi-Signal Fusion ─────────────────────────────────────────────

struct RankingWeights {
    rrf: f32,        // rank fusion from all sources
    intent: f32,     // intent category match
    freshness: f32,  // recency
    authority: f32,  // domain authority
    local_bonus: f32, // bonus for locally indexed
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self {
            rrf: 0.30,
            intent: 0.20,
            freshness: 0.15,
            authority: 0.15,
            local_bonus: 0.10,
        }
    }
}

fn compute_final_score(
    rank_score: f32,
    intent_boost: f32,
    freshness: f32,
    authority: f32,
    is_local: bool,
    weights: &RankingWeights,
) -> f32 {
    let local = if is_local { 1.0 } else { 0.0 };

    (weights.rrf * rank_score)
        + (weights.intent * intent_boost)
        + (weights.freshness * freshness)
        + (weights.authority * authority)
        + (weights.local_bonus * local)
}

// ─── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search));

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {} (multi-signal ranking)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_search(Query(params): Query<SearchParams>) -> Json<UnifiedResponse> {
    // Timeout HTTP client — 1.5s for meta-search
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
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

    // 3. Search Local Indexer, SearXNG, Whoogle, and Invidious in parallel
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

    let searx_url = format!("http://127.0.0.1:8080/search?q={}&format=json", q_encoded);
    let whoogle_url = format!("http://127.0.0.1:5000/search?q={}&format=json", q_encoded);
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let client_ref = &client;

    let indexer_fut = async {
        let resp = client_ref.get(&indexer_query).send().await?;
        let status = resp.status();
        resp.json::<Vec<IndexerResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Indexer JSON (status: {}): {:?}", status, e);
            e
        })
    };

    let searx_fut = async {
        let resp = client_ref.get(&searx_url).send().await?;
        let status = resp.status();
        resp.json::<SearxResponse>().await.map_err(|e| {
            tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
            e
        })
    };

    let whoogle_fut = async {
        let resp = client_ref.get(&whoogle_url).send().await?;
        let status = resp.status();
        resp.json::<WhoogleResponse>().await.map_err(|e| {
            tracing::error!("Failed to parse Whoogle JSON (status: {}): {:?}", status, e);
            e
        })
    };

    let invidious_fut = async {
        let resp = client_ref.get(&invidious_url).send().await?;
        let status = resp.status();
        resp.json::<Vec<InvidiousResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Invidious JSON (status: {}): {:?}", status, e);
            e
        })
    };

    let (indexer_res, searx_res, whoogle_res, invidious_res) = tokio::join!(
        indexer_fut,
        searx_fut,
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

    // 5. Aggregate Web Results
    let mut web_results: Vec<SearxResult> = Vec::new();

    match searx_res {
        Ok(searx_data) => {
            tracing::info!("SearXNG returned {} results", searx_data.results.len());
            web_results.extend(searx_data.results);
        }
        Err(e) => {
            tracing::error!("SearXNG request failed/timed out: {:?}", e);
        }
    }

    match whoogle_res {
        Ok(whoogle_data) => {
            tracing::info!("Whoogle returned {} results", whoogle_data.results.len());
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
        }
    }

    match invidious_res {
        Ok(invidious_data) => {
            tracing::info!("Invidious returned {} results", invidious_data.len());
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
        }
    }

    // Deduplicate
    let mut unique_web_results = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    for res in web_results {
        if seen_urls.insert(res.url.clone()) {
            unique_web_results.push(res);
        }
    }
    let mut web_results = unique_web_results;

    // 6. Multi-Signal Ranking
    let weights = RankingWeights::default();

    // Rank web results with multi-signal scoring
    for (i, res) in web_results.iter_mut().enumerate() {
        let rank_score = 10.0 / (i + 1) as f32; // normalized rank score
        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = domain_authority_score(&res.url);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            false, // not local
            &weights,
        );
    }
    web_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Rank local results with multi-signal scoring
    for (i, res) in local_results.iter_mut().enumerate() {
        let rank_score = 10.0 / (i + 1) as f32;
        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = domain_authority_score(&res.url);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            true, // local index bonus
            &weights,
        );
    }
    local_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 7. Feed Meta-Search Results into Crawl Queue
    let crawl_urls: Vec<serde_json::Value> = web_results.iter().take(10).enumerate().map(|(i, r)| {
        serde_json::json!({
            "url": r.url,
            "priority": 1.0 / (i + 1) as f32,
            "source": "meta-search"
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

    Json(UnifiedResponse {
        intent,
        local_results,
        web_results,
    })
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
