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
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();

    // ── News detection: path signals + subdomain signals ──
    let news_paths = ["/news/", "/article/", "/press/", "/blog/", "/story/",
                     "/breaking/", "/report/", "/headline/"];
    let news_subdomains = ["news.", "blog.", "press.", "media."];
    if news_paths.iter().any(|p| url_lower.contains(p))
        || news_subdomains.iter().any(|s| host.starts_with(s))
    {
        return ContentType::News;
    }

    // ── Documentation detection: path + subdomain signals ──
    let doc_paths = ["/docs/", "/doc/", "/api/", "/reference/", "/documentation/",
                     "/manual/", "/guide/", "/tutorial/", "/handbook/", "/wiki/",
                     "/learn/", "/examples/", "/getting-started/"];
    let doc_subdomains = ["docs.", "doc.", "developer.", "dev.", "learn.",
                          "api.", "reference.", "manual.", "wiki."];
    if doc_paths.iter().any(|p| url_lower.contains(p))
        || doc_subdomains.iter().any(|s| host.starts_with(s))
    {
        return ContentType::Documentation;
    }

    // ── Forum detection: path signals ──
    let forum_paths = ["/forum/", "/thread/", "/discussion/", "/q/", "/question/",
                       "/topic/", "/post/", "/comment/", "/reply/", "/ask/",
                       "/community/", "/board/"];
    if forum_paths.iter().any(|p| url_lower.contains(p)) {
        return ContentType::Forum;
    }

    // ── Product detection: path signals ──
    let product_paths = ["/product/", "/p/", "/item/", "/store/", "/shop/",
                         "/pricing", "/buy/", "/purchase/", "/cart/"];
    if product_paths.iter().any(|p| url_lower.contains(p)) {
        return ContentType::Product;
    }

    // ── Article detection: long-form content paths ──
    let article_paths = ["/article/", "/essay/", "/write-up/", "/publication/",
                         "/paper/", "/research/"];
    if article_paths.iter().any(|p| url_lower.contains(p)) {
        return ContentType::Article;
    }

    // ── Homepage detection: bare root path ──
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
            max_queue_size: 50000,
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
        // ── Documentation & Learning ──
        ("https://en.wikipedia.org/wiki/Main_Page", "seed"),
        ("https://doc.rust-lang.org/book/", "seed"),
        ("https://docs.python.org/3/", "seed"),
        ("https://developer.mozilla.org/en-US/", "seed"),
        ("https://docs.rs/", "seed"),
        ("https://pkg.go.dev/", "seed"),
        ("https://learn.microsoft.com/en-us/", "seed"),
        ("https://devdocs.io/", "seed"),
        ("https://kotlinlang.org/docs/", "seed"),
        ("https://www.typescriptlang.org/docs/", "seed"),
        ("https://react.dev/learn", "seed"),
        ("https://vuejs.org/guide/", "seed"),
        ("https://angular.dev/overview", "seed"),
        ("https://nextjs.org/docs", "seed"),
        ("https://docs.docker.com/", "seed"),
        ("https://kubernetes.io/docs/", "seed"),
        ("https://terraform.io/docs", "seed"),
        ("https://aws.amazon.com/documentation/", "seed"),
        // ── Q&A & Community ──
        ("https://stackoverflow.com/questions", "seed"),
        ("https://github.com/trending", "seed"),
        ("https://news.ycombinator.com/", "seed"),
        ("https://www.reddit.com/r/programming/", "seed"),
        ("https://www.reddit.com/r/rust/", "seed"),
        ("https://www.reddit.com/r/python/", "seed"),
        ("https://lobste.rs/", "seed"),
        ("https://dev.to/", "seed"),
        ("https://medium.com/tag/programming", "seed"),
        // ── Package Registries ──
        ("https://crates.io/", "seed"),
        ("https://pypi.org/", "seed"),
        ("https://www.npmjs.com/", "seed"),
        ("https://rubygems.org/", "seed"),
        ("https://maven.org/", "seed"),
        ("https://nuget.org/", "seed"),
        // ── News & Tech ──
        ("https://arxiv.org/list/cs.AI/recent", "seed"),
        ("https://arxiv.org/list/cs.CL/recent", "seed"),
        ("https://techcrunch.com/", "seed"),
        ("https://www.theverge.com/tech", "seed"),
        ("https://arstechnica.com/", "seed"),
        ("https://www.wired.com/tag/programming/", "seed"),
        // ── Open Source & Tools ──
        ("https://github.com/topics/machine-learning", "seed"),
        ("https://github.com/topics/web-framework", "seed"),
        ("https://github.com/topics/search-engine", "seed"),
        ("https://alternativeto.net/", "seed"),
        ("https://www.producthunt.com/", "seed"),
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

    // Background crawl worker — batch concurrent crawling
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                // Dequeue up to 5 entries (respecting domain rate limits)
                let now_ms = now_millis();
                let mut batch: Vec<CrawlEntry> = Vec::new();
                {
                    let mut queue = state.queue.lock().await;
                    for _ in 0..5 {
                        if let Some(entry) = queue.dequeue(now_ms) {
                            batch.push(entry);
                        } else {
                            break;
                        }
                    }
                }

                if batch.is_empty() {
                    continue;
                }

                tracing::info!("Crawling batch of {} URLs", batch.len());

                // Process batch concurrently
                let futs: Vec<_> = batch.iter().map(|entry| {
                    let state = state.clone();
                    let entry = entry.clone();
                    async move {
                        tracing::info!("Crawling [{}] {} (priority: {:.2})", entry.source, entry.url, entry.priority);
                        match crawl_and_index(&state, &entry).await {
                            Ok((title, content, links)) => {
                                tracing::info!("Indexed: {} ({} links found, {} chars)", entry.url, links.len(), content.len());
                                Some((entry, links))
                            }
                            Err(e) => {
                                tracing::warn!("Failed to crawl {}: {:?}", entry.url, e);
                                None
                            }
                        }
                    }
                }).collect();

                let results = futures::future::join_all(futs).await;

                // Enqueue discovered links from all successful crawls
                {
                    let mut queue = state.queue.lock().await;
                    let now = now_secs();
                    let mut total_discovered = 0;
                    for result in results.into_iter().flatten() {
                        let (entry, links) = result;
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
                        total_discovered += discovered;
                    }
                    if total_discovered > 0 {
                        tracing::info!("Discovered {} new URLs from batch", total_discovered);
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

// ─── Content Quality Gate ────────────────────────────────────────────
// Reject content that's too short, too repetitive, or gibberish.
// This prevents the local index from being polluted with low-quality pages.

fn is_indexworthy(title: &str, content: &str) -> bool {
    // Must have a real title
    if title.len() < 5 || title == "No Title" {
        return false;
    }

    // Must have meaningful content (at least 200 chars of actual text)
    if content.len() < 200 {
        return false;
    }

    // Check for gibberish: Shannon entropy
    let entropy = {
        let mut freq = [0u32; 128];
        let mut total = 0u32;
        for ch in content.chars().take(2000) {
            if (ch as usize) < 128 {
                freq[ch as usize] += 1;
                total += 1;
            }
        }
        if total == 0 { return false; }
        let mut h = 0.0f32;
        for &f in &freq {
            if f > 0 {
                let p = f as f32 / total as f32;
                h -= p * p.log2();
            }
        }
        h
    };

    // Natural language entropy is 3.5-5.5. Below 2.5 = repetitive, above 6.5 = random
    if entropy < 2.5 || entropy > 6.5 {
        return false;
    }

    // Check alpha ratio — must be mostly text, not code/numbers
    let alpha_count = content.chars().filter(|c| c.is_alphabetic()).count();
    let alpha_ratio = alpha_count as f32 / content.len().max(1) as f32;
    if alpha_ratio < 0.3 {
        return false; // probably code or data, not readable text
    }

    true
}

// ─── Content Quality Score (for indexing) ────────────────────────────
// Returns a 0.0-1.0 quality score to store in the index.
// Uses Shannon entropy + alpha ratio + word length analysis.
// NOT hardcoded — based on information-theoretic measures.

fn compute_content_quality(content: &str) -> f32 {
    if content.len() < 100 {
        return 0.2;
    }

    let mut score: f32 = 1.0;

    // Shannon entropy
    let entropy = {
        let mut freq = [0u32; 128];
        let mut total = 0u32;
        for ch in content.chars().take(2000) {
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
        score *= 0.2;
    } else if entropy < 3.0 {
        score *= 0.5;
    } else if entropy > 6.5 {
        score *= 0.3;
    }

    // Alpha ratio
    let alpha_count = content.chars().filter(|c| c.is_alphabetic()).count();
    let alpha_ratio = alpha_count as f32 / content.len().max(1) as f32;
    if alpha_ratio < 0.4 {
        score *= 0.4;
    }

    // Word length analysis
    let words: Vec<&str> = content.split_whitespace().collect();
    if !words.is_empty() {
        let avg_word_len: f32 = words.iter().map(|w| w.len() as f32).sum::<f32>() / words.len() as f32;
        if avg_word_len > 20.0 {
            score *= 0.2;
        } else if avg_word_len > 15.0 {
            score *= 0.5;
        }
    }

    // Content length bonus — longer content is generally more useful (up to a point)
    let len_bonus = (content.len() as f32 / 5000.0).min(1.0) * 0.1;
    score += len_bonus;

    score.clamp(0.0, 1.0)
}

// ─── Publication Date Extraction ─────────────────────────────────────
// Extracts publication dates from HTML meta tags, <time> elements, and URL patterns.
// Returns a Unix timestamp if found, None otherwise.
// This enables real freshness scoring instead of guessing from URLs.

fn extract_publication_date(document: &Html, url: &str) -> Option<u64> {
    // 1. Try <meta property="article:published_time"> (most reliable)
    let meta_selectors = [
        "meta[property='article:published_time']",
        "meta[property='article:modified_time']",
        "meta[name='datePublished']",
        "meta[name='date']",
        "meta[name='DC.date']",
        "meta[name='dcterms.modified']",
        "meta[itemprop='datePublished']",
    ];

    for sel_str in &meta_selectors {
        let sel = Selector::parse(sel_str).unwrap();
        if let Some(element) = document.select(&sel).next() {
            if let Some(content) = element.value().attr("content") {
                if let Some(ts) = parse_date_string(content) {
                    return Some(ts);
                }
            }
        }
    }

    // 2. Try <time datetime="...">
    let time_sel = Selector::parse("time[datetime]").unwrap();
    if let Some(element) = document.select(&time_sel).next() {
        if let Some(datetime) = element.value().attr("datetime") {
            if let Some(ts) = parse_date_string(datetime) {
                return Some(ts);
            }
        }
    }

    // 3. Try URL date patterns: /2026/05/26/, /2026-05-, etc.
    let url_lower = url.to_lowercase();
    let url_date_patterns = [
        // /2026/05/26/
        regex::Regex::new(r"/(\d{4})/(\d{2})/(\d{2})/").unwrap(),
        // /2026/05/
        regex::Regex::new(r"/(\d{4})/(\d{2})/").unwrap(),
        // 2026-05-26
        regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap(),
    ];

    for re in &url_date_patterns {
        if let Some(caps) = re.captures(&url_lower) {
            let year: u32 = caps.get(1).map(|m| m.as_str().parse().unwrap_or(0)).unwrap_or(0);
            let month: u32 = caps.get(2).map(|m| m.as_str().parse().unwrap_or(1)).unwrap_or(1);
            let day: u32 = caps.get(3).map(|m| m.as_str().parse().unwrap_or(1)).unwrap_or(1);

            if year >= 2000 && year <= 2030 && month >= 1 && month <= 12 && day >= 1 && day <= 31 {
                // Approximate Unix timestamp
                let days_since_epoch = (year as u64 - 1970) * 365 + (month as u64 - 1) * 30 + day as u64;
                return Some(days_since_epoch * 86400);
            }
        }
    }

    None
}

fn parse_date_string(date_str: &str) -> Option<u64> {
    // Try ISO 8601: "2026-05-26T10:30:00Z" or "2026-05-26"
    let cleaned = date_str.trim().trim_end_matches('Z').trim_end_matches('z');

    // Extract date part (first 10 chars: YYYY-MM-DD)
    if cleaned.len() >= 10 {
        let date_part = &cleaned[..10];
        let parts: Vec<&str> = date_part.split('-').collect();
        if parts.len() == 3 {
            if let (Ok(year), Ok(month), Ok(day)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                if year >= 2000 && year <= 2030 && month >= 1 && month <= 12 && day >= 1 && day <= 31 {
                    let days_since_epoch = (year as u64 - 1970) * 365 + (month as u64 - 1) * 30 + day as u64;
                    return Some(days_since_epoch * 86400);
                }
            }
        }
    }

    None
}

// ─── Crawl + Index ───────────────────────────────────────────────────

async fn crawl_and_index(
    state: &Arc<AppState>,
    entry: &CrawlEntry,
) -> Result<(String, String, Vec<String>), Box<dyn std::error::Error + Send + Sync>> {
    let resp = state.crawl_client.get(&entry.url).send().await?;
    let html_content = resp.text().await.unwrap_or_default();

    // Parse + extract everything from Html in a block so it's dropped before any .await
    // (scraper::Html contains Cell<usize> which is not Send)
    let (title, cleaned_content, links, pub_date, quality_score) = {
        let document = Html::parse_document(&html_content);

        let title_selector = Selector::parse("title").unwrap();
        let title = document.select(&title_selector).next()
            .map(|e| e.text().collect::<Vec<_>>().join(""))
            .unwrap_or_else(|| "No Title".to_string());

        // Try progressively broader selectors for main content
        let mut main_content = String::new();
        let selectors = vec!["main", "article", ".content", "#content",
                            ".post", ".entry", ".article-body", "body"];
        for sel_str in selectors {
            let sel = Selector::parse(sel_str).unwrap();
            if let Some(element) = document.select(&sel).next() {
                main_content = element.text().collect::<Vec<_>>().join(" ");
                if main_content.len() > 500 { break; }
            }
        }
        // Clean: collapse whitespace, take first 8000 chars (increased from 5000)
        let cleaned_content: String = {
            let collapsed: String = main_content.split_whitespace().collect::<Vec<_>>().join(" ");
            collapsed.chars().take(8000).collect()
        };

        let links = extract_links(&document, &entry.url);

        // Extract publication date from meta tags, <time> elements, or URL
        let pub_date = extract_publication_date(&document, &entry.url);

        // Compute content quality score
        let quality_score = compute_content_quality(&cleaned_content);

        (title, cleaned_content, links, pub_date, quality_score)
    };

    // Content quality gate — don't pollute the index with gibberish
    if !is_indexworthy(&title, &cleaned_content) {
        tracing::info!("Skipping low-quality page: {} (title_len={}, content_len={})",
                       entry.url, title.len(), cleaned_content.len());
        return Ok((title, cleaned_content, links));
    }

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

    let mut index_payload = serde_json::json!({
        "url": entry.url,
        "title": title,
        "content": cleaned_content,
        "embedding": embedding,
        "quality": quality_score as f64,
    });

    // Include publication timestamp if found (enables real freshness scoring)
    if let Some(ts) = pub_date {
        index_payload["timestamp"] = serde_json::json!(ts);
    }

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
