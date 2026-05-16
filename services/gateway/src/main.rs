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

#[derive(Serialize, Deserialize, Debug)]
struct IntentResponse {
    query: String,
    intent: String,
    constraints: Vec<String>,
    expanded_queries: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SearxResponse {
    results: Vec<SearxResult>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SearxResult {
    title: String,
    url: String,
    content: String,
    engine: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct IndexerResult {
    url: String,
    title: String,
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
    let client = reqwest::Client::new();
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // 1. Run Intent Analysis and Embedding in parallel
    let intent_url = format!("http://localhost:3000/analyze?q={}", q_encoded);
    let embed_url = format!("http://localhost:3000/embed?text={}", q_encoded);

    let (intent_res, embed_res) = tokio::join!(
        client.get(&intent_url).send(),
        client.get(&embed_url).send()
    );

    // 2. Process Intent & Embedding
    let intent: IntentResponse = match intent_res {
        Ok(resp) => resp.json().await.unwrap_or_else(|_| fallback_intent(&q)),
        Err(_) => fallback_intent(&q),
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

    // 3. Search Local Indexer and Web in parallel
    // Detect freshness intent
    let freshness_keywords = ["latest", "recent", "week", "month", "today", "newest", "cve", "vulnerability"];
    let is_freshness_query = intent.constraints.iter().any(|c| {
        let c_low = c.to_lowercase();
        freshness_keywords.iter().any(|&k| c_low.contains(k))
    }) || q.to_lowercase().contains("latest") || q.to_lowercase().contains("recent");

    // Use vector for Local Indexer (Semantic Search)
    let mut indexer_query = if let Some(ref v) = vector {
        let v_json = serde_json::to_string(v).unwrap();
        format!("http://localhost:6000/search?q={}&vector={}&min_score=0.8", q_encoded, urlencoding::encode(&v_json))
    } else {
        format!("http://localhost:6000/search?q={}", q_encoded)
    };

    if is_freshness_query {
        indexer_query.push_str("&freshness_boost=true");
    }

    // Use expanded query for Web
    let search_query = intent.expanded_queries.first().cloned().unwrap_or(q.clone());
    let mut searx_url = format!("http://localhost:8080/search?q={}&format=json", urlencoding::encode(&search_query));
    
    // Pass time range to SearXNG if detected
    if is_freshness_query {
        searx_url.push_str("&time_range=week");
    }

    let (indexer_res, searx_res) = tokio::join!(
        client.get(&indexer_query).send(),
        client.get(&searx_url).send()
    );

    // 4. Process Results
    let local_results: Vec<IndexerResult> = match indexer_res {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };

    let searx_resp: SearxResponse = match searx_res {
        Ok(resp) => resp.json().await.unwrap_or(SearxResponse { results: vec![] }),
        Err(_) => SearxResponse { results: vec![] },
    };

    // 5. Knowledge Warming (Asynchronous)
    let top_urls: Vec<String> = searx_resp.results.iter().take(3).map(|r| r.url.clone()).collect();
    tracing::info!("Triggering knowledge warming for {} URLs", top_urls.len());
    
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        for url in top_urls {
            let crawl_url = format!("http://localhost:5001/crawl?url={}", urlencoding::encode(&url));
            tracing::info!("Warming: {}", url);
            let _ = client.get(&crawl_url).send().await;
        }
    });

    Json(UnifiedResponse {
        intent,
        local_results,
        web_results: searx_resp.results,
    })
}

fn fallback_intent(q: &str) -> IntentResponse {
    IntentResponse {
        query: q.to_string(),
        intent: "Unknown".to_string(),
        constraints: vec![],
        expanded_queries: vec![q.to_string()],
    }
}
