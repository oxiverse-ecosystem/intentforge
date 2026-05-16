use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use scraper::{Html, Selector};

#[derive(Deserialize)]
struct CrawlParams {
    url: String,
}

#[derive(Serialize)]
struct CrawlResponse {
    url: String,
    title: String,
    content: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/crawl", get(handle_crawl));

    let addr = SocketAddr::from(([0, 0, 0, 0], 5001));
    tracing::info!("Crawler listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_crawl(Query(params): Query<CrawlParams>) -> Json<CrawlResponse> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let html_content = match client.get(&params.url).send().await {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };

    let (title, cleaned_content) = {
        let document = Html::parse_document(&html_content);
        
        // Extract Title
        let title_selector = Selector::parse("title").unwrap();
        let title = document.select(&title_selector).next()
            .map(|e| e.text().collect::<Vec<_>>().join(""))
            .unwrap_or_else(|| "No Title".to_string());

        // Basic content extraction
        let mut main_content = String::new();
        let selectors = vec!["main", "article", ".content", "#content", "body"];
        
        for sel_str in selectors {
            let sel = Selector::parse(sel_str).unwrap();
            if let Some(element) = document.select(&sel).next() {
                main_content = element.text().collect::<Vec<_>>().join(" ");
                if main_content.len() > 200 { break; }
            }
        }

        let cleaned = main_content.trim().chars().take(5000).collect::<String>();
        tracing::info!("Extracted Title: {}", title);
        (title, cleaned)
    };

    // Get embedding from Intent Engine
    let intent_engine_url = "http://localhost:3000/embed";
    let embedding_resp = client.get(intent_engine_url)
        .query(&[("text", &cleaned_content)])
        .send()
        .await;

    let embedding: Option<Vec<f32>> = if let Ok(resp) = embedding_resp {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            json["embedding"].as_array().map(|arr| {
                arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
            })
        } else { None }
    } else { None };

    // Push to Indexer
    let indexer_url = "http://localhost:6000/index";
    let index_payload = serde_json::json!({
        "url": params.url,
        "title": title,
        "content": cleaned_content,
        "embedding": embedding,
    });

    let _ = client.post(indexer_url).json(&index_payload).send().await;

    Json(CrawlResponse {
        url: params.url,
        title,
        content: cleaned_content,
    })
}
