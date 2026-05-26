use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
    result_type: Option<String>, // "video", "playlist", "channel"
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search));

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_search(Query(params): Query<SearchParams>) -> Json<UnifiedResponse> {
    // Timeout HTTP client — 1.5s for meta-search (VPN is fast)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap();

    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // 1. Run Intent Analysis and Embedding in parallel
    // Note: Intent-engine is now on port 3005
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
    // Detect freshness intent
    let freshness_keywords = ["latest", "recent", "week", "month", "today", "newest", "cve", "vulnerability"];
    let is_freshness_query = intent.constraints.iter().any(|c| {
        let c_low = c.to_lowercase();
        freshness_keywords.iter().any(|&k| c_low.contains(k))
    }) || q.to_lowercase().contains("latest") || q.to_lowercase().contains("recent");

    // Use vector for Local Indexer (Semantic Search)
    let mut indexer_query = if let Some(ref v) = vector {
        let v_json = serde_json::to_string(v).unwrap();
        format!("http://127.0.0.1:6000/search?q={}&vector={}&min_score=0.5", q_encoded, urlencoding::encode(&v_json))
    } else {
        format!("http://127.0.0.1:6000/search?q={}", q_encoded)
    };

    if is_freshness_query {
        indexer_query.push_str("&freshness_boost=true");
    }

    // Use original user query for external web searches to avoid bot-detection and CAPTCHAs
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

    // 5. Aggregate Web Results from SearXNG, Whoogle, and Invidious
    let mut web_results: Vec<SearxResult> = Vec::new();

    // Parse SearXNG
    match searx_res {
        Ok(searx_data) => {
            tracing::info!("SearXNG returned {} results", searx_data.results.len());
            web_results.extend(searx_data.results);
        }
        Err(e) => {
            tracing::error!("SearXNG request failed/timed out: {:?}", e);
        }
    }

    // Parse Whoogle
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

    // Parse Invidious
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

    // Deduplicate Web Results by URL
    let mut unique_web_results = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    for res in web_results {
        if seen_urls.insert(res.url.clone()) {
            unique_web_results.push(res);
        }
    }
    let mut web_results = unique_web_results;

    // Apply Intent-Driven Scoring & Boosting
    // Boost Web Results
    for (i, res) in web_results.iter_mut().enumerate() {
        let base_score = 10.0 / (i + 1) as f32; // base score based on search rank
        let boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        res.score = base_score * boost;
    }
    web_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Boost Local Results
    for res in local_results.iter_mut() {
        let boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        res.score *= boost;
    }
    local_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 6. Feed Meta-Search Results into Crawl Queue (asynchronous)
    // This is how the index grows organically from user searches
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

fn calculate_intent_boost(url: &str, title: &str, query: &str, intent: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let title_lower = title.to_lowercase();
    let query_lower = query.to_lowercase();
    let intent_lower = intent.to_lowercase();

    // Extract search query terms (excluding short words)
    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    let mut boost = 1.0;

    match intent_lower.as_str() {
        "navigation" | "navigational" => {
            // Boost official documentation and homepages
            if let Ok(parsed_url) = reqwest::Url::parse(url) {
                if let Some(host) = parsed_url.host_str() {
                    for term in &query_terms {
                        if host.contains(term) {
                            boost += 0.5;
                            let path = parsed_url.path();
                            if path == "/" || path.is_empty() {
                                boost += 0.5; // Homepage boost
                            }
                        }
                    }
                }
            }

            if url_lower.contains("docs.") 
                || url_lower.contains("doc.") 
                || url_lower.contains("/docs/") 
                || url_lower.contains("/doc/") 
                || url_lower.contains("documentation") 
                || url_lower.contains("wiki")
                || title_lower.contains("documentation")
                || title_lower.contains("official")
                || title_lower.contains("homepage")
            {
                boost += 0.6;
            }
        }
        "technical" => {
            // Boost code repositories, API documentation, and libraries
            if url_lower.contains("github.com") 
                || url_lower.contains("gitlab.com") 
                || url_lower.contains("docs.rs") 
                || url_lower.contains("crates.io") 
                || url_lower.contains("npmjs.com") 
                || url_lower.contains("pypi.org") 
                || url_lower.contains("/api/") 
                || url_lower.contains("reference")
            {
                boost += 0.5;
            }
        }
        "how-to" | "conceptual" | "informational" | "comparison" | "fresh" => {
            // Boost tutorials, guides, wikis, discussion forums, and news
            if url_lower.contains("stackoverflow.com") 
                || url_lower.contains("reddit.com") 
                || url_lower.contains("/blog/") 
                || url_lower.contains("/tutorial/") 
                || url_lower.contains("/guide/") 
                || url_lower.contains("wikipedia.org") 
                || url_lower.contains("dev.to") 
                || url_lower.contains("medium.com")
                || url_lower.contains("news.ycombinator.com")
                || url_lower.contains("/news/")
                || url_lower.contains("/article/")
            {
                boost += 0.4;
            }
        }
        "transactional" => {
            // Boost product pages, downloads, pricing pages
            if url_lower.contains("/download") 
                || url_lower.contains("/pricing") 
                || url_lower.contains("/signup") 
                || url_lower.contains("/store") 
                || url_lower.contains("/shop")
            {
                boost += 0.5;
            }
        }
        _ => {}
    }

    boost
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
