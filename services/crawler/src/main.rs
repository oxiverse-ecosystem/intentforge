use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BinaryHeap, VecDeque};
use std::cmp::Ordering;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

/// Bounded two-generation dedup set (amortized O(1) insert/contains).
///
/// `cur` fills to MAX_SEEN_URLS/2, then rotates: `old` is dropped, `cur`
/// becomes `old`, and a fresh `cur` starts. Total size is bounded by
/// MAX_SEEN_URLS; eviction granularity is the oldest half-generation.
/// Rationale vs alternatives: an insertion-ordered set with per-entry FIFO
/// eviction (IndexSet::shift_remove_index(0)) is O(n) PER INSERT once at
/// capacity — a 2M-element shift on every enqueue. Generational rotation
/// gives the same bound with O(1) ops and zero new dependencies.
///
/// Eviction makes a URL re-crawlable again — which is what revives the
/// refresh checker for aged content (an unbounded set silently blocked ALL
/// re-enqueues, leaving the refresh checker permanently dead) and bounds
/// memory, snapshot size, and startup time on a long-running instance.
struct SeenSet {
    old: HashSet<String>,
    cur: HashSet<String>,
    /// Total budget across both generations; rotation triggers at cap/2.
    cap: usize,
}

impl SeenSet {
    fn new() -> Self {
        Self::with_capacity(MAX_SEEN_URLS)
    }

    fn with_capacity(cap: usize) -> Self {
        Self { old: HashSet::new(), cur: HashSet::new(), cap: cap.max(2) }
    }

    fn contains(&self, url: &str) -> bool {
        self.cur.contains(url) || self.old.contains(url)
    }

    /// Insert a URL; returns false if already present. Rotates generations
    /// when the current one reaches half the total budget.
    fn insert(&mut self, url: String) -> bool {
        if self.contains(&url) {
            return false;
        }
        if self.cur.len() >= self.cap / 2 {
            self.old = std::mem::take(&mut self.cur);
            tracing::info!("Seen-URL set rotated: dropped oldest generation, {} URLs retained", self.old.len());
        }
        self.cur.insert(url)
    }

    /// Generations are disjoint by construction (insert checks both).
    fn len(&self) -> usize {
        self.old.len() + self.cur.len()
    }

    /// Snapshot as a single Vec, oldest generation first, so the persisted
    /// format stays a plain array (identical to the pre-bounded format) and
    /// reload rebuilds the same age ordering.
    fn to_vec(&self) -> Vec<String> {
        self.old.iter().chain(self.cur.iter()).cloned().collect()
    }
}

struct CrawlQueueManager {
    queue: BinaryHeap<CrawlEntry>,
    /// Bounded dedup set — see SeenSet.
    seen_urls: SeenSet,
    domain_limiter: DomainRateLimiter,
    max_queue_size: usize,
    max_discovered_per_page: usize,
}

impl CrawlQueueManager {
    fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            seen_urls: SeenSet::new(),
            domain_limiter: DomainRateLimiter::new(750),
            max_queue_size: 50000,
            max_discovered_per_page: 5,
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
        // SeenSet::insert is bounded: it rotates generations at capacity,
        // evicting the oldest half so memory/snapshot size can't grow forever.
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

    fn is_seen(&self, normalized: &str) -> bool {
        self.seen_urls.contains(normalized)
    }

    /// Clone the persistable state into a QueueSnapshot. Called under the
    /// queue lock — kept to a cheap O(n) clone; serialization and disk I/O
    /// happen OUTSIDE the lock (see the snapshot saver task) so dequeue and
    /// the /enqueue handler are not stalled for the duration of a full JSON
    /// encode + write of up to 50k entries.
    fn snapshot(&self, content_cap: usize) -> QueueSnapshot {
        QueueSnapshot {
            version: 1,
            queue: self.queue.iter().cloned().collect(),
            seen_urls: self.seen_urls.to_vec(),
            max_queue_size: self.max_queue_size,
            max_discovered_per_page: self.max_discovered_per_page,
            content_cap,
        }
    }

    /// Load a previously saved snapshot, restoring pending entries + seen set
    /// + the persisted adaptive content cap (defaults to the 8000 seed for
    /// pre-v1 snapshots). Entries are re-pushed so the BinaryHeap priority
    /// ordering is rebuilt. Falls back to the .bak copy if the primary file
    /// is corrupt (e.g. torn write on hard kill); only returns None when both
    /// are unreadable, and logs loudly in that case.
    fn load(path: &str) -> Option<(Self, usize)> {
        let snap = match Self::read_snapshot(path) {
            Some(s) => s,
            None => {
                let bak = format!("{}.bak", path);
                match Self::read_snapshot(&bak) {
                    Some(s) => {
                        tracing::warn!("Primary queue snapshot {} unreadable/corrupt; restored from backup {}", path, bak);
                        s
                    }
                    None => {
                        if std::path::Path::new(path).exists() {
                            tracing::error!("Queue snapshot {} AND backup are corrupt — starting with an EMPTY queue", path);
                        }
                        return None;
                    }
                }
            }
        };
        let mut mgr = CrawlQueueManager::new();
        mgr.max_queue_size = snap.max_queue_size;
        mgr.max_discovered_per_page = snap.max_discovered_per_page;
        for url in snap.seen_urls {
            mgr.seen_urls.insert(url);
        }
        for entry in snap.queue {
            mgr.queue.push(entry);
        }
        Some((mgr, snap.content_cap))
    }

    fn read_snapshot(path: &str) -> Option<QueueSnapshot> {
        let data = std::fs::read(path).ok()?;
        serde_json::from_slice(&data).ok()
    }
}

/// Atomically persist a snapshot: keep a best-effort backup of the previous
/// good file, write to a temp file, fsync, then rename over the target.
/// rename() is atomic on the same filesystem, so a hard kill mid-save leaves
/// either the old complete file or the new complete file — never a torn one.
fn save_snapshot_atomic(path: &str, snap: &QueueSnapshot) -> std::io::Result<()> {
    use std::io::Write;
    let data = serde_json::to_vec(snap)?;
    // Best-effort backup of the current good snapshot (load() falls back to it).
    if std::path::Path::new(path).exists() {
        let _ = std::fs::copy(path, format!("{}.bak", path));
    }
    let tmp = format!("{}.tmp", path);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&data)?;
        f.sync_data()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Serializable snapshot of the crawl queue manager.
#[derive(Serialize, Deserialize)]
struct QueueSnapshot {
    /// Schema version. 0 = implicit pre-versioning format (no content_cap).
    #[serde(default)]
    version: u32,
    queue: Vec<CrawlEntry>,
    seen_urls: Vec<String>,
    max_queue_size: usize,
    max_discovered_per_page: usize,
    /// Persisted adaptive content cap so a restart doesn't reset the cap to
    /// its seed and re-converge from scratch (a gate saved but never reloaded
    /// is dead). Pre-v1 snapshots default to the historical 8000 seed.
    #[serde(default = "default_content_cap")]
    content_cap: usize,
}

fn default_content_cap() -> usize {
    8000
}

/// Distribution-driven content cap: p90 of recent RAW (pre-truncation)
/// content lengths, clamped to a sane range. Keeps enough text for quality
/// scoring + embedding while trimming the long tail, instead of a fixed
/// 8000-char truncation. The window MUST be fed raw lengths — feeding
/// truncated lengths pins the cap (see crawl_and_index).
fn adaptive_content_cap(window: &VecDeque<usize>, cached: usize) -> usize {
    if window.len() < 16 {
        return cached;
    }
    let mut v: Vec<usize> = window.iter().copied().collect();
    v.sort_unstable();
    let idx = ((v.len() as f64) * 0.90) as usize;
    let p90 = v[idx.min(v.len() - 1)];
    p90.clamp(CONTENT_CAP_MIN, CONTENT_CAP_MAX)
}

// ─── Adaptive controller bounds (not magic numbers in the hot loop) ──
const MIN_CONCURRENCY: usize = 4;
const MAX_CONCURRENCY: usize = 30;
/// Per-doc latency deadband (seconds/doc): above SLOW we shed concurrency
/// (pipeline saturated), below FAST we add it (headroom). Between, hold.
/// Latency is volume-independent — unlike batch throughput, it does not read
/// "low" just because the queue drained and the batch was small. Values are
/// the unit conversion of the previous 6/15 docs-sec deadband; initial
/// setpoints, to be replaced by an adaptive baseline if steady-state
/// measurement shows oscillation.
const CONCURRENCY_SLOW_SPD: f64 = 1.0 / 6.0; // ~167ms/doc
const CONCURRENCY_FAST_SPD: f64 = 1.0 / 15.0; // ~67ms/doc
/// Only adjust concurrency when the batch was filled to at least this
/// fraction of the target — a partial batch means the queue drained or
/// domain rate limits starved it, and the latency sample says nothing about
/// pipeline saturation.
const CONCURRENCY_ADJUST_FRACTION: f64 = 0.8;
const CONTENT_CAP_MIN: usize = 2000;
/// Storage clamp on the adaptive cap, anchored to the MEASURED raw-length
/// distribution (538-page sample, 2026-07-27, scripts/analyze_content_lengths.py):
/// p50=6353, p75=24k, p90=101k, p99=938k chars. The corpus is so heavy-tailed
/// that raw p90 sits ~8x above any sane storage bound — the p90 statistic is
/// permanently clamp-bound here, so the clamp itself is the deliberate,
/// data-justified cap. 6500 ≈ measured p50: half of pages are stored in full;
/// beyond that, chars belong overwhelmingly to 100k+ mega-pages (87.6% of all
/// raw chars lie past 12000). Embedding quality is unaffected (MiniLM truncates
/// at 512 tokens ≈ 2.5k chars). Measured footprint: 6500 stores ~67% of the
/// 12000-clamp baseline (-33%) and ~87% of the old fixed 8000 (-13%).
/// The adaptive p90 still governs BELOW the clamp: if the corpus shifts
/// shorter, the cap follows it down toward CONTENT_CAP_MIN.
const CONTENT_CAP_MAX: usize = 6500;
const CAP_RECOMPUTE_EVERY: usize = 50;
const QUEUE_SAVE_PATH: &str = "/app/queue_data/queue_snapshot.json";
const QUEUE_SAVE_INTERVAL: u64 = 20; // seconds
/// Upper bound on the seen-URL dedup set (both generations combined).
/// At ~120 bytes/entry of Rust String overhead this is ~240 MB in memory and
/// ~300 MB serialized — within the 1 GB container limit with headroom.
/// Oldest half-generation is dropped on rotation, making those URLs
/// re-crawlable (deliberate: it revives the refresh path for aged content).
const MAX_SEEN_URLS: usize = 2_000_000;

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

/// Rolling performance state for the adaptive-concurrency controller.
/// `ewma_spd` = exponentially-weighted per-doc latency (seconds/doc) of the
/// crawl+embed+ingest pipeline. When saturated (high latency) we shed
/// concurrency; when there is headroom (low latency) we add it. Latency is
/// volume-independent, and updates are gated on the batch being at capacity,
/// so a drained queue cannot masquerade as saturation.
struct PerfState {
    ewma_spd: f64,
    last_batch_docs: usize,
}

struct AppState {
    queue: Mutex<CrawlQueueManager>,
    crawl_client: reqwest::Client,
    indexer_url: String,
    embed_url: String,
    ingest_buffer: Mutex<Vec<serde_json::Value>>,
    /// Adaptive crawl concurrency (batch size dequeued per tick). Bounded
    /// [MIN_CONCURRENCY, MAX_CONCURRENCY]; adjusted from observed throughput.
    concurrency: Mutex<usize>,
    /// Window of recent cleaned-content lengths, used to derive a
    /// distribution-driven content cap (p90, clamped) instead of a fixed 8000.
    content_window: Mutex<VecDeque<usize>>,
    /// Cached content cap, recomputed every CAP_RECOMPUTE_EVERY pages.
    content_cap: Mutex<usize>,
    cap_counter: Mutex<usize>,
    /// Rolling pipeline throughput for the concurrency controller.
    perf: Mutex<PerfState>,
}

#[derive(Clone, Serialize)]
struct IngestRequest {
    url: String,
    title: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority: Option<f64>,
}

// Flush buffered docs to the indexer via the batch endpoint (one POST + one
// commit for N docs). Returns number flushed.
async fn flush_ingest_buffer(state: &Arc<AppState>) -> usize {
    let batch: Vec<serde_json::Value> = {
        let mut buf = state.ingest_buffer.lock().await;
        if buf.is_empty() {
            return 0;
        }
        std::mem::take(&mut *buf)
    };
    let n = batch.len();
    let payload = serde_json::json!({ "documents": batch, "replace_existing": true });
    if let Err(e) = state.crawl_client
        .post(format!("{}/ingest_batch", state.indexer_url))
        .json(&payload)
        .send()
        .await
    {
        tracing::warn!("Batch ingest POST failed: {:?}", e);
        return 0;
    }
    tracing::info!("Flushed batch of {} doc(s) to indexer", n);
    n
}

// Buffer size that triggers an immediate flush.
const INGEST_FLUSH_THRESHOLD: usize = 20;


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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let crawl_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    // Restore the crawl queue from a prior snapshot if present, so a restart
    // resumes in-flight crawling instead of re-seeding from scratch. Also
    // restores the adaptive content cap so it doesn't re-converge from seed.
    if let Some(dir) = std::path::Path::new(QUEUE_SAVE_PATH).parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("Could not create queue snapshot dir {:?}: {:?}", dir, e);
        }
    }
    let (queue, restored_cap) = CrawlQueueManager::load(QUEUE_SAVE_PATH)
        .unwrap_or_else(|| (CrawlQueueManager::new(), default_content_cap()));
    // Clamp the restored cap: a snapshot written under an older, wider
    // CONTENT_CAP_MAX must not resurrect a cap outside the current bounds.
    let restored_cap = restored_cap.clamp(CONTENT_CAP_MIN, CONTENT_CAP_MAX);
    let restored = queue.seen_count();
    if restored > 0 {
        tracing::info!("Restored crawl queue snapshot: {} seen URLs, content_cap={}", restored, restored_cap);
    }

    let state = Arc::new(AppState {
        queue: Mutex::new(queue),
        crawl_client,
        indexer_url: "http://127.0.0.1:6000".to_string(),
        embed_url: "http://127.0.0.1:3005/embed".to_string(),
        ingest_buffer: Mutex::new(Vec::with_capacity(64)),
        concurrency: Mutex::new(20),
        content_window: Mutex::new(VecDeque::with_capacity(300)),
        content_cap: Mutex::new(restored_cap),
        cap_counter: Mutex::new(0),
        perf: Mutex::new(PerfState { ewma_spd: 0.0, last_batch_docs: 0 }),
    });

    // Queue snapshot saver: persist pending URLs + seen set periodically so a
    // crash/restart loses at most QUEUE_SAVE_INTERVAL of progress.
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(QUEUE_SAVE_INTERVAL));
            loop {
                interval.tick().await;
                // Clone under a short lock; serialize + write OUTSIDE the lock
                // so dequeue/enqueue are not blocked by JSON encoding + disk I/O.
                let cap = *state.content_cap.lock().await;
                let snap = {
                    let q = state.queue.lock().await;
                    q.snapshot(cap)
                };
                if let Err(e) = save_snapshot_atomic(QUEUE_SAVE_PATH, &snap) {
                    tracing::warn!("Queue snapshot save failed: {:?}", e);
                }
            }
        });
    }

    // Batch-ingest flush ticker: flushes the shared ingest buffer every 500ms so
    // docs committed to the indexer at most 500ms after crawl, regardless of volume.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                flush_ingest_buffer(&state).await;
            }
        });
    }

    // Seed URL injection on startup
    {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            tracing::info!("Injecting seed URLs into crawl queue...");
            let mut queue = state.queue.lock().await;
            let now = now_secs();
            let mut seeded = 0;
            let mut skipped_seen = 0;
            for (url, source) in default_seed_urls() {
                // If a snapshot was restored, its seen-set already covers seeds;
                // only enqueue seeds we haven't seen to avoid redundant re-crawls.
                let normalized = normalize_url(url);
                if queue.seen_count() > 0 && queue.is_seen(&normalized) {
                    skipped_seen += 1;
                    continue;
                }
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
            tracing::info!("Seeded {} new URLs (skipped {} already-seen from snapshot) into crawl queue", seeded, skipped_seen);
        });
    }

    // Background crawl worker — batch concurrent crawling
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Dequeue up to `concurrency` entries (adaptive; respects domain
                // rate limits). Concurrency is tuned from observed pipeline
                // throughput so we shed load when embed/upstream is saturated.
                let now_ms = now_millis();
                let concurrency = *state.concurrency.lock().await;
                let mut batch: Vec<CrawlEntry> = Vec::new();
                {
                    let mut queue = state.queue.lock().await;
                    for _ in 0..concurrency {
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

                tracing::info!("Crawling batch of {} URLs (concurrency={})", batch.len(), concurrency);

                let batch_start = Instant::now();

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

                // Enqueue discovered links from all successful crawls.
                // Backpressure: once the queue is >=80% full, stop discovering new
                // links so the crawler can actually drain to a steady state instead
                // of growing without bound (discovery is free, crawl is ~1s each).
                {
                    let mut queue = state.queue.lock().await;
                    if queue.len() < (queue.max_queue_size * 4 / 5) {
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
                    } else {
                        tracing::info!("Queue at capacity ({}), pausing discovery to drain", queue.len());
                    }
                }

                // ── Adaptive concurrency: tune from observed PER-DOC latency,
                // gated on the batch being at capacity. A partial batch means
                // the queue drained or domain limits starved it — that sample
                // says nothing about pipeline saturation, so skip adjustment
                // (previously a drained queue read as "saturated" and collapsed
                // concurrency toward MIN while idle). Sustained high latency ⇒
                // saturated (embed/upstream slow) ⇒ shed; low latency ⇒
                // headroom ⇒ add. Deadband between. Bounded [MIN, MAX].
                let elapsed = batch_start.elapsed().as_secs_f64().max(0.001);
                let batch_at_capacity =
                    batch.len() as f64 >= (concurrency as f64 * CONCURRENCY_ADJUST_FRACTION);
                if batch_at_capacity {
                    let spd = elapsed / batch.len() as f64; // seconds per doc
                    let ewma = {
                        let mut perf = state.perf.lock().await;
                        perf.ewma_spd = if perf.ewma_spd == 0.0 {
                            spd
                        } else {
                            0.7 * perf.ewma_spd + 0.3 * spd
                        };
                        perf.last_batch_docs = batch.len();
                        perf.ewma_spd
                    };
                    let mut conc = state.concurrency.lock().await;
                    if *conc > 0 {
                        if ewma >= CONCURRENCY_SLOW_SPD && *conc > MIN_CONCURRENCY {
                            *conc = (*conc).saturating_sub(2).max(MIN_CONCURRENCY);
                            tracing::info!("Pipeline saturated ({:.0}ms/doc >= {:.0}ms): concurrency -> {}", ewma * 1000.0, CONCURRENCY_SLOW_SPD * 1000.0, *conc);
                        } else if ewma <= CONCURRENCY_FAST_SPD && *conc < MAX_CONCURRENCY {
                            *conc = (*conc + 2).min(MAX_CONCURRENCY);
                            tracing::info!("Pipeline headroom ({:.0}ms/doc <= {:.0}ms): concurrency -> {}", ewma * 1000.0, CONCURRENCY_FAST_SPD * 1000.0, *conc);
                        }
                    }
                } else {
                    let mut perf = state.perf.lock().await;
                    perf.last_batch_docs = batch.len();
                    tracing::debug!("Partial batch ({}/{}) — holding concurrency (starved, not saturated)", batch.len(), concurrency);
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

// ─── Domain Authority Score ───────────────────────────────────────
// Algorithmic domain authority based on URL structure signals.
// Same approach as the gateway's domain_authority_score() — TLD trust,
// subdomain patterns, path signals, URL complexity.
// NOT hardcoded — based on structural URL analysis.

fn compute_domain_authority(url: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let host = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();

    let mut score: f32 = 0.5; // baseline for unknown domains

    // ── TLD-based trust scoring (algorithmic) ──
    if host.ends_with(".edu") || host.ends_with(".gov") || host.ends_with(".ac.uk") {
        score += 0.3;
    } else if host.ends_with(".org") || host.ends_with(".net") {
        score += 0.1;
    } else if host.rfind('.').map_or(false, |i| {
        let tld = &host[i+1..];
        tld.len() == 2 && tld.chars().all(|c| c.is_alphabetic())
    }) {
        score += 0.05;
    }

    // ── Subdomain pattern scoring ──
    let doc_prefixes = ["docs.", "doc.", "developer.", "dev.", "learn.",
                        "api.", "reference.", "manual.", "wiki.", "help.", "support."];
    if doc_prefixes.iter().any(|p| host.starts_with(p)) {
        score += 0.25;
    }

    // ── Path pattern scoring ──
    let doc_paths = ["/docs/", "/doc/", "/api/", "/reference/", "/documentation/",
                     "/manual/", "/guide/", "/tutorial/", "/handbook/", "/wiki/"];
    if doc_paths.iter().any(|p| url_lower.contains(p)) {
        score += 0.2;
    }

    // Code hosting signal: /owner/repo pattern
    let path = Url::parse(url).ok().map(|u| u.path().to_lowercase()).unwrap_or_default();
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path_segments.len() >= 2 {
        let has_repo_pattern = path_segments[0].len() >= 2
            && path_segments[1].len() >= 2
            && !path_segments[0].contains('.')
            && !path_segments[1].contains('.');
        if has_repo_pattern {
            score += 0.1;
        }
    }

    // Package registry signal: version-like patterns
    let has_version_pattern = path_segments.iter().any(|s| {
        s.starts_with('v') && s[1..].chars().all(|c| c.is_numeric() || c == '.') && s.len() >= 2
    });
    if has_version_pattern {
        score += 0.1;
    }

    // URL complexity signals
    let host_parts: Vec<&str> = host.split('.').collect();
    if host_parts.len() == 2 {
        score += 0.1; // bare domain = primary site
    } else if host_parts.len() >= 5 {
        score -= 0.1; // too many subdomains = CDN/UGC
    }

    if url_lower.contains('?') {
        let query_part = url_lower.split('?').nth(1).unwrap_or("");
        let param_count = query_part.matches('&').count();
        if param_count > 5 {
            score -= 0.1;
        }
    }

    // Content farm / clickbait signals
    let spam_patterns = ["content-farm", "clickbait", "top10best", "bestof",
                         "listicle", "buzzfeed"];
    if spam_patterns.iter().any(|p| url_lower.contains(p)) {
        score -= 0.2;
    }

    // UGC signals
    let ugc_signals = url_lower.contains("/thread/") || url_lower.contains("/question/")
        || url_lower.contains("/post/") || url_lower.contains("/comment/")
        || url_lower.contains("/discussion/") || url_lower.contains("/q/");
    if ugc_signals {
        score += 0.05;
    }

    score.clamp(0.0, 1.0)
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

    // Distribution-driven content cap (p90 of recent lengths, clamped). Read
    // once here; the crawl block below is sync so the value stays valid.
    let cap = *state.content_cap.lock().await;

    // Parse + extract everything from Html in a block so it's dropped before any .await
    // (scraper::Html contains Cell<usize> which is not Send)
    let (title, cleaned_content, raw_len, links, pub_date, quality_score) = {
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
        // Clean: collapse whitespace, then truncate to the distribution-driven
        // content cap (p90 of recent RAW lengths, clamped) instead of a fixed 8000.
        // raw_len is measured BEFORE truncation: feeding post-truncation lengths
        // into the cap window makes the cap self-fulfilling (every sample <= cap
        // => p90 <= cap => monotonically non-increasing from seed, never widens).
        let (cleaned_content, raw_len): (String, usize) = {
            let collapsed: String = main_content.split_whitespace().collect::<Vec<_>>().join(" ");
            let raw_len = collapsed.chars().count();
            (collapsed.chars().take(cap).collect(), raw_len)
        };

        let links = extract_links(&document, &entry.url);

        // Extract publication date from meta tags, <time> elements, or URL
        let pub_date = extract_publication_date(&document, &entry.url);

        // Compute content quality score
        let quality_score = compute_content_quality(&cleaned_content);

        (title, cleaned_content, raw_len, links, pub_date, quality_score)
    };

    // Feed the content-length distribution for the adaptive cap: record this
    // page's RAW (pre-truncation) length, and every CAP_RECOMPUTE_EVERY pages
    // recompute the p90 cap from the raw distribution.
    {
        let mut w = state.content_window.lock().await;
        w.push_back(raw_len);
        tracing::debug!("Content length: raw={} truncated={} cap={}", raw_len, cleaned_content.chars().count(), cap);
        if w.len() > 300 {
            w.pop_front();
        }
        let mut ctr = state.cap_counter.lock().await;
        *ctr += 1;
        if *ctr >= CAP_RECOMPUTE_EVERY {
            *ctr = 0;
            let new_cap = adaptive_content_cap(&w, cap);
            *state.content_cap.lock().await = new_cap;
            tracing::info!("Content cap adapted (p90 of {} samples) -> {}", w.len(), new_cap);
        }
    }

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

    let authority = compute_domain_authority(&entry.url);

    let mut index_payload = serde_json::json!({
        "url": entry.url,
        "title": title,
        "content": cleaned_content,
        "embedding": embedding,
        "quality": quality_score as f64,
        "authority": authority as f64,
    });

    // Include publication timestamp if found (enables real freshness scoring)
    if let Some(ts) = pub_date {
        index_payload["timestamp"] = serde_json::json!(ts);
    }

    // Buffer the doc for batched ingest instead of one POST per crawl.
    {
        let mut buf = state.ingest_buffer.lock().await;
        buf.push(index_payload);
        if buf.len() >= INGEST_FLUSH_THRESHOLD {
            drop(buf);
            flush_ingest_buffer(&state).await;
        }
    }

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

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_set_dedup_and_bound() {
        let mut s = SeenSet::with_capacity(10); // rotation at 5
        assert!(s.insert("a".to_string()));
        assert!(!s.insert("a".to_string()), "duplicate must be rejected");
        assert!(s.contains("a"));
        // Fill well past capacity; the set must stay bounded.
        for i in 0..100 {
            s.insert(format!("url{}", i));
        }
        assert!(s.len() <= 10, "seen set exceeded its bound: {}", s.len());
        // Recently inserted URLs must still be present (dedup still works).
        assert!(s.contains("url99"));
        // The oldest URL was evicted by rotation and is re-crawlable.
        assert!(!s.contains("a"), "oldest generation should have been evicted");
    }

    #[test]
    fn seen_set_to_vec_disjoint_and_complete() {
        let mut s = SeenSet::with_capacity(4); // rotation at 2
        for i in 0..3 {
            s.insert(format!("u{}", i));
        }
        let v = s.to_vec();
        assert_eq!(v.len(), s.len());
        let uniq: HashSet<&String> = v.iter().collect();
        assert_eq!(uniq.len(), v.len(), "generations must be disjoint");
    }

    #[test]
    fn snapshot_v1_round_trip() {
        let mut mgr = CrawlQueueManager::new();
        mgr.enqueue(CrawlEntry {
            url: "https://example.com/page".to_string(),
            priority: 1.0,
            source: "test".to_string(),
            content_type: ContentType::Article,
            discovered_at: 123,
            attempts: 0,
        });
        let snap = mgr.snapshot(9500);
        let data = serde_json::to_vec(&snap).unwrap();
        let back: QueueSnapshot = serde_json::from_slice(&data).unwrap();
        assert_eq!(back.version, 1);
        assert_eq!(back.content_cap, 9500);
        assert_eq!(back.queue.len(), 1);
        assert_eq!(back.seen_urls.len(), 1);
    }

    #[test]
    fn snapshot_v0_backward_compat() {
        // Pre-versioning snapshot: no `version`, no `content_cap` fields.
        // Must load with defaults (version=0, cap=8000) — zero data loss.
        let v0 = r#"{
            "queue": [{"url":"https://old.example/a","priority":0.5,"source":"seed","content_type":"Article","discovered_at":1,"attempts":0}],
            "seen_urls": ["https://old.example/a"],
            "max_queue_size": 50000,
            "max_discovered_per_page": 5
        }"#;
        let snap: QueueSnapshot = serde_json::from_str(v0).expect("v0 snapshot must deserialize");
        assert_eq!(snap.version, 0);
        assert_eq!(snap.content_cap, 8000);
        assert_eq!(snap.queue.len(), 1);
        assert_eq!(snap.seen_urls.len(), 1);
    }

    #[test]
    fn atomic_save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("crawler_snap_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("snap.json");
        let path_str = path.to_str().unwrap();

        let mut mgr = CrawlQueueManager::new();
        mgr.enqueue(CrawlEntry {
            url: "https://example.com/x".to_string(),
            priority: 2.0,
            source: "test".to_string(),
            content_type: ContentType::Article,
            discovered_at: 42,
            attempts: 0,
        });
        let snap = mgr.snapshot(10000);
        save_snapshot_atomic(path_str, &snap).unwrap();
        // Second save creates the .bak of the first.
        save_snapshot_atomic(path_str, &snap).unwrap();
        assert!(path.exists());
        assert!(dir.join("snap.json.bak").exists());
        assert!(!dir.join("snap.json.tmp").exists(), "tmp file must not linger");

        let (loaded, cap) = CrawlQueueManager::load(path_str).expect("load must succeed");
        assert_eq!(cap, 10000);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.seen_count(), 1);

        // Corrupt the primary — load must fall back to .bak.
        std::fs::write(&path, b"{corrupt").unwrap();
        let (loaded2, cap2) = CrawlQueueManager::load(path_str).expect(".bak fallback must succeed");
        assert_eq!(cap2, 10000);
        assert_eq!(loaded2.len(), 1);

        // Corrupt both — load must return None (fresh start), not panic.
        std::fs::write(dir.join("snap.json.bak"), b"{also corrupt").unwrap();
        assert!(CrawlQueueManager::load(path_str).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adaptive_cap_uses_raw_distribution() {
        // Window of raw lengths ABOVE the current cap must widen the cap
        // (up to the clamp) — the old post-truncation feed made this
        // impossible. Use a cached cap below the samples to prove widening.
        let w: VecDeque<usize> = (0..32).map(|_| 5000).collect();
        let cap = adaptive_content_cap(&w, 3000);
        assert_eq!(cap, 5000, "cap must widen toward raw p90");
        // And the clamp still applies on both ends.
        let w_long: VecDeque<usize> = (0..32).map(|_| 50000).collect();
        assert_eq!(adaptive_content_cap(&w_long, 3000), CONTENT_CAP_MAX);
        let w_short: VecDeque<usize> = (0..32).map(|_| 100).collect();
        assert_eq!(adaptive_content_cap(&w_short, 3000), CONTENT_CAP_MIN);
    }
}
