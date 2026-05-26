use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BinaryHeap};
use std::cmp::Ordering;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use scraper::{Html, Selector};
use reqwest::Url;

// ─── Crawl Queue ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrawlEntry {
    url: String,
    priority: f32,
    source: String,
    content_type: ContentType,
    discovered_at: u64,
    attempts: u32,
}

impl PartialEq for CrawlEntry {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}
impl Eq for CrawlEntry {}

impl PartialOrd for CrawlEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.priority.partial_cmp(&other.priority)
    }
}

impl Ord for CrawlEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ContentType {
    News,
    Documentation,
    Homepage,
    Forum,
    Product,
    Article,
    Unknown,
}

impl ContentType {
    fn refresh_interval_secs(&self) -> u64 {
        match self {
            ContentType::News => 3600,
            ContentType::Documentation => 86400,
            ContentType::Homepage => 43200,
            ContentType::Forum => 7200,
            ContentType::Product => 21600,
            ContentType::Article => 14400,
            ContentType::Unknown => 14400,
        }
    }
}

fn detect_content_type(url: &str) -> ContentType {
    let url_lower = url.to_lowercase();

    if url_lower.contains("/news/") || url_lower.contains("/article/")
        || url_lower.contains("/press/") || url_lower.contains("/blog/")
        || url_lower.contains("news.ycombinator.com")
        || url_lower.contains("arstechnica.com")
        || url_lower.contains("theverge.com")
        || url_lower.contains("techcrunch.com")
    {
        return ContentType::News;
    }

    if url_lower.contains("/docs/") || url_lower.contains("/doc/")
        || url_lower.contains("/api/") || url_lower.contains("/reference/")
        || url_lower.contains("/documentation/") || url_lower.contains("/manual/")
        || url_lower.contains("docs.rs") || url_lower.contains("doc.rust-lang")
        || url_lower.contains("developer.mozilla.org")
        || url_lower.contains("learn.microsoft.com")
    {
        return ContentType::Documentation;
    }

    if url_lower.contains("/forum/") || url_lower.contains("/thread/")
        || url_lower.contains("/discussion/") || url_lower.contains("/q/")
        || url_lower.contains("stackoverflow.com")
        || url_lower.contains("reddit.com")
        || url_lower.contains("news.ycombinator.com/item")
    {
        return ContentType::Forum;
    }

    if url_lower.contains("/product/") || url_lower.contains("/p/")
        || url_lower.contains("/item/") || url_lower.contains("/store/")
        || url_lower.contains("/shop/") || url_lower.contains("/pricing")
    {
        return ContentType::Product;
    }

    if let Ok(parsed) = Url::parse(url) {
        let path = parsed.path();
        if path == "/" || path.is_empty() {
            return ContentType::Homepage;
        }
    }

    ContentType::Unknown
}

// ─── Domain Rate Limiter ─────────────────────────────────────────────

struct DomainRateLimiter {
    last_request: HashMap<String, u64>,
    min_delay_ms: u64,
}

impl DomainRateLimiter {
    fn new(min_delay_ms: u64) -> Self {
        Self {
            last_request: HashMap::new(),
            min_delay_ms,
        }
    }

    fn can_request(&self, domain: &str, now_ms: u64) -> bool {
        match self.last_request.get(domain) {
            Some(&last) => now_ms.saturating_sub(last) >= self.min_delay_ms,
            None => true,
        }
    }

    fn record_request(&mut self, domain: &str, now_ms: u64) {
        self.last_request.insert(domain.to_string(), now_ms);
    }
}

// ─── Crawl Queue Manager ─────────────────────────────────────────────

struct CrawlQueueManager {
    queue: BinaryHeap<CrawlEntry>,
    seen_urls: HashSet<String>,
    domain_limiter: DomainRateLimiter,
    max_queue_size: usize,
    max_discovered_per_page: usize,
}

impl CrawlQueueManager {
    fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            seen_urls: HashSet::new(),
            domain_limiter: DomainRateLimiter::new(2000),
            max_queue_size: 10000,
            max_discovered_per_page: 20,
        }
    }

    fn enqueue(&mut self, entry: CrawlEntry) -> bool {
        let normalized = normalize_url(&entry.url);
        if self.seen_urls.contains(&normalized) {
            return false;
        }
        if self.queue.len() >= self.max_queue_size {
            return false;
        }
        self.seen_urls.insert(normalized);
        self.queue.push(entry);
        true
    }

    fn dequeue(&mut self, now_ms: u64) -> Option<CrawlEntry> {
        let mut skipped = Vec::new();
        let mut result = None;

        while let Some(entry) = self.queue.pop() {
            if let Ok(parsed) = Url::parse(&entry.url) {
                if let Some(domain) = parsed.host_str() {
                    if self.domain_limiter.can_request(domain, now_ms) {
                        self.domain_limiter.record_request(domain, now_ms);
                        result = Some(entry);
                        break;
                    } else {
                        skipped.push(entry);
                    }
                }
            }
        }

        for entry in skipped {
            self.queue.push(entry);
        }

        result
    }

    fn len(&self) -> usize {
        self.queue.len()
    }

    fn seen_count(&self) -> usize {
        self.seen_urls.len()
    }
}

fn normalize_url(url: &str) -> String {
    if let Ok(mut parsed) = Url::parse(url) {
        parsed.set_fragment(None);
        let mut s = parsed.as_str().to_string();
        if s.ends_with('/') && s.len() > 1 {
            s.pop();
        }
        s.to_lowercase()
    } else {
        url.to_lowercase()
    }
}

// ─── Seed URLs ───────────────────────────────────────────────────────

fn default_seed_urls() -> Vec<(&'static str, &'static str)> {
    vec![
        ("https://en.wikipedia.org/wiki/Main_Page", "seed"),
        ("https://doc.rust-lang.org/book/", "seed"),
        ("https://docs.python.org/3/", "seed"),
        ("https://developer.mozilla.org/en-US/", "seed"),
        ("https://docs.rs/", "seed"),
        ("https://pkg.go.dev/", "seed"),
        ("https://learn.microsoft.com/en-us/", "seed"),
        ("https://stackoverflow.com/questions", "seed"),
        ("https://github.com/trending", "seed"),
        ("https://news.ycombinator.com/", "seed"),
        ("https://crates.io/", "seed"),
        ("https://pypi.org/", "seed"),
        ("https://www.npmjs.com/", "seed"),
    ]
}

// ─── App State ───────────────────────────────────────────────────────

struct AppState {
    queue: Mutex<CrawlQueueManager>,
    crawl_client: reqwest::Client,
    indexer_url: String,
    embed_url: String,
}

// ─── API Types ───────────────────────────────────────────────────────

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

#[derive(Deserialize)]
struct EnqueueRequest {
    urls: Vec<EnqueueUrl>,
}

#[derive(Deserialize)]
struct EnqueueUrl {
    url: String,
    #[serde(default = "default_priority")]
    priority: f32,
    #[serde(default)]
    source: Option<String>,
}

fn default_priority() -> f32 {
    1.0
}

#[derive(Serialize)]
struct EnqueueResponse {
    queued: usize,
    skipped: usize,
    queue_size: usize,
}

#[derive(Serialize)]
struct QueueStatusResponse {
    queue_size: usize,
    seen_urls: usize,
}

// ─── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let crawl_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let state = Arc::new(AppState {
        queue: Mutex::new(CrawlQueueManager::new()),
        crawl_client,
        indexer_url: "http://127.0.0.1:6000".to_string(),
        embed_url: "http://127.0.0.1:3005/embed".to_string(),
    });

    // Seed URL injection on startup
    {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            tracing::info!("Injecting seed URLs into crawl queue...");
            let mut queue = state.queue.lock().await;
            let now = now_secs();
            let mut seeded = 0;
            for (url, source) in default_seed_urls() {
                let content_type = detect_content_type(url);
                let entry = CrawlEntry {
                    url: url.to_string(),
                    priority: 0.5,
                    source: source.to_string(),
                    content_type,
                    discovered_at: now,
                    attempts: 0,
                };
                if queue.enqueue(entry) {
                    seeded += 1;
                }
            }
            tracing::info!("Seeded {} URLs into crawl queue", seeded);
        });
    }

    // Background crawl worker
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                let now_ms = now_millis();
                let entry = {
                    let mut queue = state.queue.lock().await;
                    queue.dequeue(now_ms)
                };

                let Some(entry) = entry else {
                    continue;
                };

                tracing::info!("Crawling [{}] {} (priority: {:.2})", entry.source, entry.url, entry.priority);

                match crawl_and_index(&state, &entry).await {
                    Ok((_title, _content, links)) => {
                        tracing::info!("Indexed: {} ({} links found)", entry.url, links.len());

                        let mut queue = state.queue.lock().await;
                        let now = now_secs();
                        let mut discovered = 0;
                        for link in links {
                            if discovered >= queue.max_discovered_per_page {
                                break;
                            }
                            let link_type = detect_content_type(&link);
                            let new_entry = CrawlEntry {
                                url: link,
                                priority: entry.priority * 0.7,
                                source: "discovery".to_string(),
                                content_type: link_type,
                                discovered_at: now,
                                attempts: 0,
                            };
                            if queue.enqueue(new_entry) {
                                discovered += 1;
                            }
                        }
                        if discovered > 0 {
                            tracing::info!("Discovered {} new URLs from {}", discovered, entry.url);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to crawl {}: {:?}", entry.url, e);
                    }
                }
            }
        });
    }

    // Background refresh checker
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                let client = reqwest::Client::new();
                match client.get(format!("{}/urls", state.indexer_url)).send().await {
                    Ok(resp) => {
                        if let Ok(urls) = resp.json::<Vec<serde_json::Value>>().await {
                            let now = now_secs();
                            let mut refreshed = 0;
                            let mut queue = state.queue.lock().await;

                            for item in urls {
                                if let Some(url) = item.get("url").and_then(|u| u.as_str()) {
                                    let timestamp = item.get("timestamp").and_then(|t| t.as_u64()).unwrap_or(0);
                                    let content_type = detect_content_type(url);
                                    let interval = content_type.refresh_interval_secs();

                                    if now.saturating_sub(timestamp) > interval {
                                        let entry = CrawlEntry {
                                            url: url.to_string(),
                                            priority: 0.3,
                                            source: "refresh".to_string(),
                                            content_type,
                                            discovered_at: now,
                                            attempts: 0,
                                        };
                                        if queue.enqueue(entry) {
                                            refreshed += 1;
                                        }
                                    }
                                }
                            }
                            if refreshed > 0 {
                                tracing::info!("Queued {} stale URLs for refresh", refreshed);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch URLs from indexer: {:?}", e);
                    }
                }
            }
        });
    }

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/crawl", get(handle_crawl))
        .route("/enqueue", post(handle_enqueue))
        .route("/queue/status", get(handle_queue_status))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 5001));
    tracing::info!("Crawler listening on {} (with crawl queue)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ─── Crawl + Index ───────────────────────────────────────────────────

async fn crawl_and_index(
    state: &Arc<AppState>,
    entry: &CrawlEntry,
) -> Result<(String, String, Vec<String>), Box<dyn std::error::Error + Send + Sync>> {
    let resp = state.crawl_client.get(&entry.url).send().await?;
    let html_content = resp.text().await.unwrap_or_default();

    let document = Html::parse_document(&html_content);

    let title_selector = Selector::parse("title").unwrap();
    let title = document.select(&title_selector).next()
        .map(|e| e.text().collect::<Vec<_>>().join(""))
        .unwrap_or_else(|| "No Title".to_string());

    let mut main_content = String::new();
    let selectors = vec!["main", "article", ".content", "#content", "body"];
    for sel_str in selectors {
        let sel = Selector::parse(sel_str).unwrap();
        if let Some(element) = document.select(&sel).next() {
            main_content = element.text().collect::<Vec<_>>().join(" ");
            if main_content.len() > 200 { break; }
        }
    }
    let cleaned_content: String = main_content.trim().chars().take(5000).collect();

    let links = extract_links(&document, &entry.url);

    let embedding_text = format!("{}. {}", title, cleaned_content);
    let embedding: Option<Vec<f32>> = match state.crawl_client
        .get(&state.embed_url)
        .query(&[("text", &embedding_text)])
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json["embedding"].as_array().map(|arr| {
                    arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
                })
            } else { None }
        }
        Err(_) => None,
    };

    let index_payload = serde_json::json!({
        "url": entry.url,
        "title": title,
        "content": cleaned_content,
        "embedding": embedding,
    });

    let _ = state.crawl_client
        .post(format!("{}/index", state.indexer_url))
        .json(&index_payload)
        .send()
        .await;

    Ok((title, cleaned_content, links))
}

// ─── Link Extraction ─────────────────────────────────────────────────

fn extract_links(document: &Html, base_url: &str) -> Vec<String> {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let link_selector = Selector::parse("a[href]").unwrap();
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for element in document.select(&link_selector) {
        if let Some(href) = element.value().attr("href") {
            match base.join(href) {
                Ok(resolved) => {
                    let url_str = resolved.as_str();

                    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
                        continue;
                    }

                    let lower = url_str.to_lowercase();
                    if lower.contains(".pdf") || lower.contains(".zip")
                        || lower.contains(".tar") || lower.contains(".gz")
                        || lower.contains(".exe") || lower.contains(".dmg")
                        || lower.contains(".jpg") || lower.contains(".png")
                        || lower.contains(".gif") || lower.contains(".svg")
                        || lower.contains(".css") || lower.contains(".js")
                        || lower.contains('#')
                        || lower.contains("javascript:")
                        || lower.contains("mailto:")
                    {
                        continue;
                    }

                    let normalized = normalize_url(url_str);
                    if seen.insert(normalized) {
                        links.push(url_str.to_string());
                    }
                }
                Err(_) => continue,
            }
        }
    }

    links
}

// ─── API Handlers ────────────────────────────────────────────────────

async fn handle_crawl(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CrawlParams>,
) -> Json<CrawlResponse> {
    let html_content = match state.crawl_client.get(&params.url).send().await {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(_) => String::new(),
    };

    let (title, cleaned_content) = {
        let document = Html::parse_document(&html_content);
        let title_selector = Selector::parse("title").unwrap();
        let title = document.select(&title_selector).next()
            .map(|e| e.text().collect::<Vec<_>>().join(""))
            .unwrap_or_else(|| "No Title".to_string());

        let mut main_content = String::new();
        let selectors = vec!["main", "article", ".content", "#content", "body"];
        for sel_str in selectors {
            let sel = Selector::parse(sel_str).unwrap();
            if let Some(element) = document.select(&sel).next() {
                main_content = element.text().collect::<Vec<_>>().join(" ");
                if main_content.len() > 200 { break; }
            }
        }
        let cleaned: String = main_content.trim().chars().take(5000).collect();
        (title, cleaned)
    };

    let embedding_text = format!("{}. {}", title, cleaned_content);
    let embedding: Option<Vec<f32>> = match state.crawl_client
        .get(&state.embed_url)
        .query(&[("text", &embedding_text)])
        .send().await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json["embedding"].as_array().map(|arr| {
                    arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
                })
            } else { None }
        }
        Err(_) => None,
    };

    let index_payload = serde_json::json!({
        "url": params.url,
        "title": title,
        "content": cleaned_content,
        "embedding": embedding,
    });
    let _ = state.crawl_client
        .post(format!("{}/index", state.indexer_url))
        .json(&index_payload)
        .send()
        .await;

    Json(CrawlResponse {
        url: params.url,
        title,
        content: cleaned_content,
    })
}

async fn handle_enqueue(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EnqueueRequest>,
) -> Json<EnqueueResponse> {
    let mut queue = state.queue.lock().await;
    let now = now_secs();
    let mut queued = 0;
    let mut skipped = 0;

    for item in payload.urls {
        let content_type = detect_content_type(&item.url);
        let entry = CrawlEntry {
            url: item.url,
            priority: item.priority,
            source: item.source.unwrap_or_else(|| "meta-search".to_string()),
            content_type,
            discovered_at: now,
            attempts: 0,
        };
        if queue.enqueue(entry) {
            queued += 1;
        } else {
            skipped += 1;
        }
    }

    let queue_size = queue.len();
    Json(EnqueueResponse { queued, skipped, queue_size })
}

async fn handle_queue_status(
    State(state): State<Arc<AppState>>,
) -> Json<QueueStatusResponse> {
    let queue = state.queue.lock().await;
    Json(QueueStatusResponse {
        queue_size: queue.len(),
        seen_urls: queue.seen_count(),
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
