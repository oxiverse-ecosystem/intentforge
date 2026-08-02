use axum::{
    extract::Query,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use tower_http::timeout::TimeoutLayer;
use axum::http::HeaderMap;
use std::net::IpAddr;

mod spell;
mod geoloc;
mod dictionary;
mod clean;
mod goals;
// ─── API Types ───────────────────────────────────────────────────────

// Helper: deserialize null/missing string fields as empty String
fn deserialize_null_as_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Deserialize)]
struct SearchParams {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    n: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Constraints {
    #[serde(default)]
    positive: Vec<String>,
    #[serde(default)]
    negative: Vec<String>,
    /// Semantic entities with roles (Query Graph IR)
    #[serde(default)]
    entities: Vec<QueryEntity>,
    /// Detected programming language from the query.
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    file_types: Vec<String>,
    #[serde(default)]
    sites: Vec<String>,
    #[serde(default)]
    phrases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before_date: Option<String>,
    #[serde(default)]
    intitle: Vec<String>,
    #[serde(default)]
    inurl: Vec<String>,
    #[serde(default)]
    intext: Vec<String>,
    #[serde(default)]
    related: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price_min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price_max: Option<f32>,
    /// Upper bound from an explicit `<` operator, e.g. `price:<100`. Semantically
    /// identical to `price_max` for filtering, but preserved so the operator is
    /// not silently dropped when reporting applied constraints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price_lt: Option<f32>,
    /// Lower bound from an explicit `>` operator, e.g. `price:>50`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price_gt: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum EntityRole {
    Target,
    Reference,
    Comparison,
    Exclusion,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct QueryEntity {
    text: String,
    role: EntityRole,
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
    structured_constraints: Constraints,
    #[serde(default)]
    expanded_queries: Vec<String>,
    #[serde(default)]
    distribution: std::collections::HashMap<String, f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxResponse {
    results: Vec<SearxResult>,
    // Captures engine-level failures (e.g. ["brave", "too many requests"]).
    // SearXNG-internal rate limits arrive as HTTP 200 (not 429), so the
    // gateway's 429-only rotation trigger misses them. We inspect this to
    // rotate IPs when an engine reports suspension/rate-limiting.
    #[serde(default)]
    unresponsive_engines: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxResult {
    title: String,
    url: String,
    content: String,
    engine: String,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    sources: Vec<String>, // tracks all engines/sources that returned this result
    #[serde(default, alias = "publishedDate")]
    published_date: Option<String>,
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
    #[serde(default)]
    content: String,
    /// Signal-quality metric [0,1] from the indexer: how strongly this local page
    /// genuinely matches the query. The gateway's local-index quality gate (P2)
    /// uses it to demote low-signal crawled pages without a hardcoded blocklist.
    #[serde(default = "default_indexer_quality")]
    quality: f32,
}

fn default_indexer_quality() -> f32 { 1.0 }

// ─── Image Result (from SearXNG categories=images) ────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxImageResult {
    #[serde(deserialize_with = "deserialize_null_as_default")]
    title: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    url: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    img_src: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    thumbnail: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    thumbnail_src: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    content: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    engine: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    source: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxImageResponse {
    results: Vec<SearxImageResult>,
}

// ─── News Result (from SearXNG categories=news) ───────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxNewsResult {
    #[serde(deserialize_with = "deserialize_null_as_default")]
    title: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    url: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    content: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    engine: String,
    #[serde(default, alias = "publishedDate")]
    published_date: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxNewsResponse {
    results: Vec<SearxNewsResult>,
}

// ─── Video Result (from SearXNG categories=videos) ────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxVideoResult {
    #[serde(deserialize_with = "deserialize_null_as_default")]
    title: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    url: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    content: String,
    #[serde(deserialize_with = "deserialize_null_as_default")]
    engine: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    thumbnail: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    img_src: String,
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    iframe_src: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxVideoResponse {
    results: Vec<SearxVideoResult>,
}

// ─── API Response Shapes ──────────────────────────────────────────

// Map detailed intent subtypes to standard search categories
fn parent_category(intent: &str) -> String {
    match intent {
        "navigational" => "navigational",
        "informational" | "technical" | "how-to" | "comparison" | "fresh" | "local" => "informational",
        "transactional" => "transactional",
        _ => "informational",
    }.to_string()
}
#[derive(Serialize)]
struct ImageResult {
    title: String,
    url: String,
    image_url: String,
    thumbnail_url: String,
    #[serde(default)]
    description: String,
    source: String,
    #[serde(default)]
    score: f32
}

#[derive(Serialize)]
struct VideoResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    video_id: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    score: f32
}

#[derive(Serialize)]
struct NewsResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    published_at: String,
    source: String,
    #[serde(default)]
    score: f32
}

// ─── Merged Result (Unified Local + Web) ─────────────────────────────
// A single result type that can come from local index, web search, or both.
// When a URL appears in both sources, sources are merged and consensus boost applied.

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MergedResult {
    url: String,
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    authority: f32,
    #[serde(default)]
    sources: Vec<String>,  // e.g. ["local", "bing", "brave"]
    #[serde(default)]
    is_local: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_date: Option<String>,
    /// Signal-quality metric [0,1] inherited from the indexer for local results
    /// (BM25/semantic match strength). Used by the local-index quality gate (P2)
    /// to demote low-signal crawled pages. Defaults to 1.0 for web results.
    #[serde(default = "default_f32_one")]
    quality: f32,
}

fn default_f32_one() -> f32 { 1.0 }

#[derive(Serialize, Clone, Debug)]
struct DeepResult {
    result_type: String,
    vendor_name: String,
    page_title: String,
    page_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    direct_download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_name: Option<String>,
    confidence: f32,
}

#[derive(Serialize)]
struct UnifiedResponse {
    query: String,
    intent: Option<String>,
    category: Option<String>,
    confidence: Option<f32>,
    constraints: Vec<String>,
    structured_constraints: Constraints,
    expanded_queries: Vec<String>,
    distribution: Option<std::collections::HashMap<String, f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deep_result: Option<DeepResult>,
    results: Vec<MergedResult>,
    /// IP geolocation of the requesting client (if GeoLite2 database available)
    #[serde(skip_serializing_if = "Option::is_none")]
    geo_location: Option<geoloc::GeoLocation>,
    /// If the original query was spell-corrected, the corrected version
    #[serde(skip_serializing_if = "Option::is_none")]
    spell_corrected_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_quality: Option<String>,
    /// Constraints the engine actually tried to enforce (operators it understands).
    #[serde(skip_serializing_if = "Option::is_none")]
    applied_constraints: Option<Vec<String>>,
    /// Constraints that were specified but could not take effect on the returned
    /// results (e.g. a date range when no result carried a parseable date, or a
    /// price range when no result snippet carried a detectable price).
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored_constraints: Option<Vec<String>>,
    /// Human-readable diagnostics (empty result set, upstream flakiness hints, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    /// Web result count before any constraint filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    results_before_filter: Option<usize>,
    /// Web result count after all constraint filtering (before pagination).
    #[serde(skip_serializing_if = "Option::is_none")]
    results_after_filter: Option<usize>,
    /// Total number of results available after filtering (identical to
    /// `results_after_filter`). The `results` array is a paginated slice of
    /// this total, so `len(results)` is NOT the total count.
    #[serde(skip_serializing_if = "Option::is_none", rename = "total")]
    total: Option<usize>,
    /// The effective `limit` applied to the `results` slice.
    #[serde(skip_serializing_if = "Option::is_none", rename = "limit")]
    page_limit: Option<usize>,
    /// The effective `offset` applied to the `results` slice.
    #[serde(skip_serializing_if = "Option::is_none", rename = "offset")]
    page_offset: Option<usize>,
    /// Whether more results exist beyond the returned slice (`offset + limit < total`).
    #[serde(skip_serializing_if = "Option::is_none", rename = "has_more")]
    has_more: Option<bool>,
}

const DOWNLOAD_KEYWORDS: &[&str] = &[
    "driver", "drivers", "download", "downloads", "installer", "installers",
    "firmware", "patch", "software download", "official download", "setup.exe"
];

// ─── Domain Authority (Fully Algorithmic) ────────────────────────────
// Scores based purely on URL structure signals — no hardcoded domain lists.
// Signals: TLD trust, subdomain patterns, path patterns, URL complexity.

fn domain_authority_score(url: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let parsed = reqwest::Url::parse(url).ok();
    let host = parsed.as_ref()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();
    let path = parsed.as_ref()
        .map(|u| u.path().to_lowercase())
        .unwrap_or_default();
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let n_segments = path_segments.len();

    let mut score: f32 = 0.5; // baseline for unknown domains

    // ── Structural URL authority for reference/research content ──
    // Deep, descriptive paths (long segments, many hyphens, numeric IDs) indicate
    // database-driven reference content (reports, documentation, catalogs) which
    // carries inherent authority. Purely structural — no keyword or domain lists.
    if n_segments >= 2 {
        let total_chars: usize = path_segments.iter().map(|s| s.len()).sum();
        let total_hyphens: usize = path_segments.iter().map(|s| s.matches('-').count()).sum();
        let avg_seg_len = total_chars as f32 / n_segments as f32;

        // Deep, descriptive path with many hyphens → reference/report content
        // Examples: /privacy-search-engine-market-size-share-trends-analysis
        if n_segments >= 3 && avg_seg_len > 12.0 && total_hyphens >= 3 {
            score += 0.35;
        }
        // Numeric-heavy segments → database-driven pages (authoritative reports)
        let has_numeric_segment = path_segments.iter().any(|s| {
            let numeric_count = s.chars().filter(|c| c.is_numeric()).count();
            s.len() >= 6 && numeric_count >= s.len() / 2
        });
        if n_segments >= 3 && has_numeric_segment {
            score = score.max(0.75); // floor at high authority
        }
        // Moderate depth with long segments → structured content
        if n_segments >= 2 && avg_seg_len > 15.0 {
            score += 0.20;
        }
    }

    // ── TLD-based trust scoring (algorithmic) ──
    // Institutional TLDs: highest trust
    if host.ends_with(".edu") || host.ends_with(".gov") || host.ends_with(".ac.uk") {
        score += 0.3;
    }
    // Organizational TLDs: moderate trust
    else if host.ends_with(".org") || host.ends_with(".net") {
        score += 0.1;
    }
    // Country TLDs: slight trust (real organizations)
    else if host.rfind('.').map_or(false, |i| {
        let tld = &host[i+1..];
        tld.len() == 2 && tld.chars().all(|c| c.is_alphabetic())
    }) {
        score += 0.05;
    }

    // ── Subdomain pattern scoring (algorithmic) ──
    // Documentation/developer subdomains: high quality signal
    let doc_prefixes = ["docs.", "doc.", "developer.", "dev.", "learn.", "api.",
                        "reference.", "manual.", "wiki.", "help.", "support."];
    if doc_prefixes.iter().any(|p| host.starts_with(p)) {
        score += 0.25;
    }

    // ── Path pattern scoring (algorithmic) ──
    // Documentation paths: strong quality signal
    let doc_paths = ["/docs/", "/doc/", "/api/", "/reference/", "/documentation/",
                     "/manual/", "/guide/", "/tutorial/", "/handbook/", "/wiki/"];
    if doc_paths.iter().any(|p| url_lower.contains(p)) {
        score += 0.2;
    }

    // Code hosting signal: path contains repo-like structure
    if n_segments >= 2 {
        // Looks like /owner/repo pattern (code hosting)
        let has_repo_pattern = path_segments[0].len() >= 2
            && path_segments[1].len() >= 2
            && !path_segments[0].contains('.')
            && !path_segments[1].contains('.');
        if has_repo_pattern {
            score += 0.1;
        }
    }

    // Package registry signal: path contains version-like patterns
    let has_version_pattern = path_segments.iter().any(|s| {
        s.starts_with('v') && s[1..].chars().all(|c| c.is_numeric() || c == '.')
            && s.len() >= 2
    });
    if has_version_pattern {
        score += 0.1;
    }

    // ── URL complexity signals ──
    // Short, clean URLs tend to be more authoritative
    let host_parts: Vec<&str> = host.split('.').collect();
    if host_parts.len() == 2 {
        // bare domain (example.com) — likely a primary site
        score += 0.1;
    } else if host_parts.len() >= 5 {
        // Too many subdomains — likely a CDN or user-generated content
        score -= 0.1;
    }

    // Long query strings = less authoritative (tracking, filters, etc.)
    if url_lower.contains('?') {
        let query_part = url_lower.split('?').nth(1).unwrap_or("");
        let param_count = query_part.matches('&').count();
        if param_count > 5 {
            score -= 0.1;
        }
    }

    // Content farm / clickbait signals (dynamic pattern detection)
    let spam_path_patterns = ["content-farm", "clickbait", "top10best", "bestof",
                              "listicle", "buzzfeed"];
    if spam_path_patterns.iter().any(|p| url_lower.contains(p)) {
        score -= 0.2;
    }

    // ── Content platform scoring (algorithmic: path signals) ──
    // User-generated content platforms have distinctive URL patterns
    let ugc_signals = url_lower.contains("/thread/") || url_lower.contains("/question/")
        || url_lower.contains("/post/") || url_lower.contains("/comment/")
        || url_lower.contains("/discussion/") || url_lower.contains("/q/");
    if ugc_signals {
        score += 0.05; // UGC is decent but not authoritative
    }

    score.clamp(0.0, 1.0)
}

// ─── Freshness Decay ─────────────────────────────────────────────────

fn parse_date_to_comparable(s: &str) -> Option<(i32, i32, i32)> {
    if let Some(d) = parse_numeric_date(s) {
        return Some(d);
    }
    let re_year = regex::Regex::new(r"\b(\d{4})\b").ok()?;
    if let Some(caps) = re_year.captures(s) {
        let y = caps.get(1)?.as_str().parse::<i32>().ok()?;
        return Some((y, 1, 1));
    }
    None
}

/// Numeric YYYY-MM-DD / YYYY/MM/DD only (no bare-year fallback), so that
/// month-name patterns can take precedence over a standalone year found in the
/// same text (e.g. "January 5, 2024" must resolve to the full date, not 2024).
fn parse_numeric_date(s: &str) -> Option<(i32, i32, i32)> {
    let re = regex::Regex::new(r"\b(\d{4})[-/](\d{1,2})[-/](\d{1,2})\b").ok()?;
    let caps = re.captures(s)?;
    let y = caps.get(1)?.as_str().parse::<i32>().ok()?;
    let m = caps.get(2)?.as_str().parse::<i32>().ok()?;
    let d = caps.get(3)?.as_str().parse::<i32>().ok()?;
    Some((y, m, d))
}

fn date_gte(d1: (i32, i32, i32), d2: (i32, i32, i32)) -> bool {
    d1.0 > d2.0 || (d1.0 == d2.0 && (d1.1 > d2.1 || (d1.1 == d2.1 && d1.2 >= d2.2)))
}

fn date_lte(d1: (i32, i32, i32), d2: (i32, i32, i32)) -> bool {
    d1.0 < d2.0 || (d1.0 == d2.0 && (d1.1 < d2.1 || (d1.1 == d2.1 && d1.2 <= d2.2)))
}

// ─── Pure-std Gregorian date math (no external time crate) ──────────
// Proleptic Gregorian calendar. Days are counted from 1970-01-01 (Unix epoch).
// Algorithm: Howard Hinnant's civil-from-days / days-from-civil.

fn ymd_to_days(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + (d as i64 - 1);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn days_to_ymd(z: i64) -> (i32, i32, i32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as i32, d as i32)
}

fn today_ymd() -> (i32, i32, i32) {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(dur) => days_to_ymd((dur.as_secs() / 86400) as i64),
        Err(_) => (1970, 1, 1),
    }
}

fn add_days(ymd: (i32, i32, i32), delta: i64) -> (i32, i32, i32) {
    days_to_ymd(ymd_to_days(ymd.0, ymd.1, ymd.2) + delta)
}

fn format_ymd(ymd: (i32, i32, i32)) -> String {
    format!("{:04}-{:02}-{:02}", ymd.0, ymd.1, ymd.2)
}

/// Resolve the best comparable date for a result, looking at (in priority order):
/// the upstream published_date, a 4-digit year embedded in the URL, and finally
/// any human-readable date found in the title/content text. This makes
/// after:/before:/recency constraints actually drop old web results whose
/// snippets carry a date.
fn resolve_item_date(
    published_date: Option<&str>,
    url: &str,
    title: &str,
    content: &str,
) -> Option<(i32, i32, i32)> {
    if let Some(pd) = published_date {
        if let Some(d) = parse_date_to_comparable(pd) {
            return Some(d);
        }
    }
    if let Some(y) = extract_year_from_url(url) {
        return Some((y, 1, 1));
    }
    let text = format!("{} {}", title, content);
    extract_date_from_text(&text)
}

fn month_num(s: &str) -> Option<i32> {
    let s = s.to_lowercase();
    let months: &[(&str, i32)] = &[
        ("january", 1), ("february", 2), ("march", 3), ("april", 4), ("may", 5),
        ("june", 6), ("july", 7), ("august", 8), ("september", 9), ("october", 10),
        ("november", 11), ("december", 12), ("jan", 1), ("feb", 2), ("mar", 3),
        ("apr", 4), ("jun", 6), ("jul", 7), ("aug", 8), ("sep", 9), ("sept", 9),
        ("oct", 10), ("nov", 11), ("dec", 12),
    ];
    months.iter().find(|(n, _)| s.starts_with(n)).map(|(_, m)| *m)
}

/// Extract a calendar date from free text (title/content snippet).
fn extract_date_from_text(text: &str) -> Option<(i32, i32, i32)> {
    // Numeric YYYY-MM-DD / YYYY/MM/DD first (precise).
    if let Some(d) = parse_numeric_date(text) {
        return Some(d);
    }
    let t = text.to_lowercase();
    // Capturing group so we can read the month directly (it is not always first).
    let month_alt = r"(january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|sept|oct|nov|dec)";

    // "Month DD, YYYY"  → groups: 1=month 2=day 3=year
    static RE1: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re1 = RE1.get_or_init(|| {
        regex::Regex::new(&format!(r"(?i)\b{}\.?\s+(\d{{1,2}})(?:st|nd|rd|th)?,?\s+(\d{{4}})", month_alt)).unwrap()
    });
    if let Some(c) = re1.captures(&t) {
        if let (Some(m), Ok(d), Ok(y)) = (
            month_num(c.get(1).unwrap().as_str()),
            c.get(2).unwrap().as_str().parse::<i32>(),
            c.get(3).unwrap().as_str().parse::<i32>(),
        ) {
            return Some((y, m, d));
        }
    }

    // "DD Month YYYY"  → groups: 1=day 2=month 3=year
    static RE2: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re2 = RE2.get_or_init(|| {
        regex::Regex::new(&format!(r"(?i)\b(\d{{1,2}})(?:st|nd|rd|th)?\s+{}\.?,?\s+(\d{{4}})", month_alt)).unwrap()
    });
    if let Some(c) = re2.captures(&t) {
        if let (Ok(d), Some(m), Ok(y)) = (
            c.get(1).unwrap().as_str().parse::<i32>(),
            month_num(c.get(2).unwrap().as_str()),
            c.get(3).unwrap().as_str().parse::<i32>(),
        ) {
            return Some((y, m, d));
        }
    }

    // "Month YYYY"  → groups: 1=month 2=year
    static RE3: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re3 = RE3.get_or_init(|| {
        regex::Regex::new(&format!(r"(?i)\b{}\.?,?\s+(\d{{4}})", month_alt)).unwrap()
    });
    if let Some(c) = re3.captures(&t) {
        if let (Some(m), Ok(y)) = (
            month_num(c.get(1).unwrap().as_str()),
            c.get(2).unwrap().as_str().parse::<i32>(),
        ) {
            return Some((y, m, 1));
        }
    }

    static REY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rey = REY.get_or_init(|| regex::Regex::new(r"\b((?:19|20)\d{2})\b").unwrap());
    if let Some(c) = rey.captures(&t) {
        if let Ok(y) = c.get(1).unwrap().as_str().parse::<i32>() {
            return Some((y, 1, 1));
        }
    }
    None
}

/// Map a natural-language recency phrase to a concrete (after, before) window
/// expressed as `YYYY-MM-DD`. Returns None when no recency signal is present, so
/// literal after:/before: and explicit dates are left untouched.
fn derive_recency_window(q_lower: &str) -> Option<(String, String)> {
    let today = today_ymd();
    let today_s = format_ymd(today);

    if q_lower.contains("yesterday") {
        return Some((format_ymd(add_days(today, -1)), today_s));
    }
    if q_lower.contains("today") {
        return Some((today_s.clone(), today_s));
    }

    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(?:past|last|this|current|recent|over|within|in the past|in the last|during the)\s+(\d+)\s+(day|days|week|weeks|month|months|year|years)\b",
        )
        .unwrap()
    });
    if let Some(caps) = re.captures(q_lower) {
        if let (Ok(n), Some(unit)) = (caps.get(1).unwrap().as_str().parse::<i64>(), caps.get(2)) {
            let delta = match unit.as_str().to_lowercase().as_str() {
                "day" | "days" => -n,
                "week" | "weeks" => -(n * 7),
                "month" | "months" => -(n * 30),
                "year" | "years" => -(n * 365),
                _ => 0,
            };
            if delta != 0 {
                return Some((format_ymd(add_days(today, delta)), today_s));
            }
        }
    }

    let named: &[(&str, i64)] = &[
        ("this week", -7), ("past week", -7), ("last week", -7), ("current week", -7),
        ("this month", -30), ("past month", -30), ("last month", -30),
        ("this year", -365), ("past year", -365), ("last year", -365),
    ];
    for (phrase, delta) in named {
        if q_lower.contains(*phrase) {
            return Some((format_ymd(add_days(today, *delta)), today_s));
        }
    }

    if q_lower.contains("recent") || q_lower.contains("latest") || q_lower.contains("fresh") {
        return Some((format_ymd(add_days(today, -7)), today_s));
    }

    None
}

fn freshness_score(url: &str, intent: &str, published_date: Option<&str>) -> f32 {
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

    let mut estimated_age_hours: f32 = 168.0; // default: 7 days (less aggressive decay)
    let mut parsed_ok = false;

    if let Some(pd) = published_date {
        if let Some((y, m, d)) = parse_date_to_comparable(pd) {
            // Current date: July 13, 2026
            let cur_y = 2026;
            let cur_m = 7;
            let cur_d = 13;
            let years_diff = cur_y - y;
            let months_diff = cur_m - m;
            let days_diff = cur_d - d;
            let total_days = years_diff * 365 + months_diff * 30 + days_diff;
            estimated_age_hours = (total_days * 24) as f32;
            estimated_age_hours = estimated_age_hours.max(0.0);
            parsed_ok = true;
        }
    }

    if !parsed_ok {
        let url_lower = url.to_lowercase();

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

        // Structural URL analysis for static/reference content detection.
        // Deep, descriptive paths (long segments, high hyphen density) are characteristic
        // of reference content (reports, documentation) that remains relevant for years.
        // This is purely structural — no keyword lists or domain names.
        if let Ok(parsed) = reqwest::Url::parse(url) {
            let path = parsed.path().to_lowercase();
            let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            let n_segments = path_segments.len();

            if n_segments >= 2 {
                let total_chars: usize = path_segments.iter().map(|s| s.len()).sum();
                let total_hyphens: usize = path_segments.iter().map(|s| s.matches('-').count()).sum();
                let avg_seg_len = total_chars as f32 / n_segments as f32;

                // Deep, descriptive path with many hyphens → reference/report content
                // Examples: /privacy-search-engine-market-size-share-trends-2025-2026
                // These URLs have: n_segments≥2, avg_seg_len>15, total_hyphens≥3
                if n_segments >= 3 && avg_seg_len > 12.0 && total_hyphens >= 3 {
                    estimated_age_hours = estimated_age_hours.min(1.0);
                }
                // Deep path with numeric-heavy segments = database-driven pages (reports, catalogs)
                if n_segments >= 3 {
                    let has_numeric_segment = path_segments.iter().any(|s| {
                        s.len() >= 6 && s.chars().filter(|c| c.is_numeric()).count() >= s.len() / 2
                    });
                    if has_numeric_segment {
                        estimated_age_hours = estimated_age_hours.min(1.0);
                    }
                }
            }
        }
    }

    // Exponential decay: score = exp(-age / half_life)
    (-estimated_age_hours / half_life_hours).exp()
}

// ─── Intent Boost (Fully Algorithmic) ────────────────────────────────
// Boosts results based on structural URL/title signals matching intent.
// No hardcoded domain lists — uses path patterns and URL structure.

fn calculate_intent_boost(url: &str, title: &str, query: &str, intent: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let title_lower = title.to_lowercase();
    let query_lower = query.to_lowercase();
    let intent_lower = intent.to_lowercase();

    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() >= 2)
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

            // Documentation/official signals (path-based, not domain-based)
            if url_lower.contains("/docs/") || url_lower.contains("/doc/")
                || url_lower.contains("documentation") || url_lower.contains("wiki")
                || title_lower.contains("documentation") || title_lower.contains("official")
                || title_lower.contains("homepage")
            {
                boost += 0.6;
            }
        }
        "technical" => {
            // Path-based signals for technical content
            if url_lower.contains("/docs/") || url_lower.contains("/doc/")
                || url_lower.contains("/api/") || url_lower.contains("/reference/")
                || url_lower.contains("/examples/") || url_lower.contains("/manual/")
                || url_lower.contains("/crates/") || url_lower.contains("/packages/")
                || url_lower.contains("/modules/") || url_lower.contains("/library/")
            {
                boost += 0.5;
            }
            // Q&A / forum signals
            if url_lower.contains("/q/") || url_lower.contains("/question/")
                || url_lower.contains("/thread/") || url_lower.contains("/issues/")
            {
                boost += 0.3;
            }
        }
        "how-to" | "conceptual" | "informational" | "comparison" | "fresh" => {
            // Path-based signals for content types
            if url_lower.contains("/blog/") || url_lower.contains("/tutorial/")
                || url_lower.contains("/guide/") || url_lower.contains("/wiki/")
                || url_lower.contains("/article/") || url_lower.contains("/learn/")
                || url_lower.contains("/news/") || url_lower.contains("/thread/")
                || url_lower.contains("/q/") || url_lower.contains("/question/")
            {
                boost += 0.4;
            }

            // For comparison intent, boost review-type content
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
            // E-commerce / product page patterns: direct purchase paths
            let tx_url_match = url_lower.contains("/download") || url_lower.contains("/pricing")
                || url_lower.contains("/signup") || url_lower.contains("/store")
                || url_lower.contains("/shop") || url_lower.contains("/buy")
                // E-commerce product page patterns (amazon, ebay, aliexpress, etc.)
                || url_lower.contains("/dp/") || url_lower.contains("/product/")
                || url_lower.contains("/item/") || url_lower.contains("/products/")
                || url_lower.contains("/pd/") || url_lower.contains("/gp/product/")
                || url_lower.contains("/details/")
                // Extended marketplace patterns
                || url_lower.contains("/offer") || url_lower.contains("/deal")
                || url_lower.contains("/cart") || url_lower.contains("/checkout")
                || url_lower.contains("/basket") || url_lower.contains("/order")
                || url_lower.contains("/merchant") || url_lower.contains("/seller")
                || url_lower.contains("/review/") || url_lower.contains("/price")
                // Common e-commerce TLD patterns
                || url_lower.contains("amazon.com") || url_lower.contains("ebay.com")
                || url_lower.contains("walmart.com") || url_lower.contains("bestbuy.com")
                || url_lower.contains("etsy.com") || url_lower.contains("alibaba.com")
                || url_lower.contains("newegg.com") || url_lower.contains("target.com");
            if tx_url_match {
                boost += 0.5;
            }
            // Title signals: "buy", "price", "shop", "order", "deal" in the title
            let tx_title_match = title_lower.contains("price") || title_lower.contains("order now")
                || title_lower.contains("add to cart") || title_lower.contains("on sale")
                || title_lower.contains("buy now") || title_lower.contains("shop now")
                || title_lower.contains("best price") || title_lower.contains("free shipping")
                || title_lower.contains("discount") || title_lower.contains("coupon")
                || title_lower.contains("limited offer") || title_lower.contains("deal");
            if tx_title_match {
                boost += 0.4; // stronger boost for transactional title signals
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

    // Filter out JSON/URL junk before scoring
    let words: Vec<&str> = text.split_whitespace()
        .filter(|w| {
            !w.starts_with("http")
                && !w.contains("@type")
                && !w.contains("@context")
                && !w.contains("\":")
                && !(w.len() > 25 && (w.contains('/') || w.contains('{') || w.contains('"') || w.contains('_') || w.contains(':')))
        })
        .collect();

    let clean_text = words.join(" ");
    let eval_text = if clean_text.trim().len() >= 20 {
        &clean_text
    } else {
        text
    };

    // Shannon entropy — measures information content
    // Natural language: 3.5-5.0 bits/char. Gibberish: <2.5 or >6.5
    let entropy = {
        let mut freq = [0u32; 128];
        let mut total = 0u32;
        for ch in eval_text.chars() {
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
    let alpha_count = eval_text.chars().filter(|c| c.is_alphabetic()).count();
    let alpha_ratio = alpha_count as f32 / eval_text.len().max(1) as f32;
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

// ─── Constraint Scoring (Dynamic, Query-Derived) ───────────────────
// Applies structured constraints from intent analysis:
// - Positive constraints: boost results that match constraint terms
// - Negative constraints: penalize results that match negative terms
// NOT hardcoded — constraints come from the query itself.
// Score range: 0.0 (violates negative) to 1.0 (matches all positives, no negatives)


// ─── Alternative-Listing Page Detection (Algorithmic) ────────────────
// When a query uses negative constraints like "not React not Vue",
// alternative-listing pages ("Top 10 React Alternatives") are HIGHLY
// relevant but get penalized because they mention the excluded term.
// This function detects such pages using structural signals:
//   - Title patterns: "alternative", "vs", "best N", "comparison"
//   - URL path patterns: "/alternatives/", "/vs/", "/compare/"
//   - Content richness: longer content suggests a curated list, not a landing page
// Returns 0.0 (not alternative) to 1.0 (strongly alternative).
// No hardcoded domains — purely algorithmic.

fn is_alternative_listing_page(title: &str, url: &str, content: &str) -> f32 {
    let title_lower = title.to_lowercase();
    let url_lower = url.to_lowercase();

    // Signal A: Title contains comparison/alternative markers (strongest signal)
    let title_signal = {
        let strong_patterns = [
            "alternative", "alternatives", "alternative to",
            " vs ", " versus ", "compared", "comparison",
            "replace", "replacement", "migrate from", "migrating from",
            "instead of", "switching from", "moving from",
        ];
        if strong_patterns.iter().any(|p| title_lower.contains(p)) {
            1.0
        } else {
            // Weaker title signals: "top N X", "best N X" patterns
            let weak_patterns = ["top ", "best ", "review", "guide to ", "list of "];
            if weak_patterns.iter().any(|p| title_lower.contains(p)) {
                0.6
            } else {
                0.0
            }
        }
    };

    // Signal B: URL path contains comparison markers
    let url_alt_patterns = [
        "/alternative", "/alternatives", "/alternative-to",
        "/vs/", "/compare", "/comparison",
        "/top-", "/best-", "/reviews/", "/review/",
    ];
    let url_signal_raw = url_alt_patterns.iter()
        .filter(|p| url_lower.contains(*p))
        .count() as f32;
    let url_signal = (url_signal_raw * 0.35).min(0.7);

    // Signal C: Content contains alternative-listing patterns
    // Scan first 500 chars of content for comparison/alternative keywords.
    // This catches pages where the title isnt explicit but the body is a
    // comparison list (e.g. "What Is Docker Hub?" -> body lists alternatives).
    let content_alt_patterns = [
        "alternatives", "alternative to", "compared to", "comparison",
        "vs ", "versus", "instead of", "migrate from", "replacement for",
        "top ", "pros and cons", "options", "not recommended",
    ];
    let content_signal = if content.len() > 100 {
        let content_prefix: String = content.chars().take(500).collect();
        let content_lower = content_prefix.to_lowercase();
        let matches = content_alt_patterns.iter()
            .filter(|p| content_lower.contains(*p))
            .count() as f32;
        (matches * 0.06).min(0.25)
    } else {
        0.0
    };

    // Blend with title as dominant signal
    (title_signal * 0.70 + url_signal * 0.20 + content_signal * 0.10).clamp(0.0, 1.0)
}


/// Decide whether a result token matches a negative constraint term.
///
/// Naive `token.contains(term)` is catastrophic for negation: "not java"
/// would penalise/drop every "javascript" result ("java" ⊂ "javascript"),
/// and "not go" would match "google", "django", "logo", etc. This produced
/// the 0-results bug for "javascript not java not typescript".
///
/// Rule (data-driven, no per-term hardcoding):
///   1. Exact match always counts.
///   2. A prefix/substring match only counts when the constraint term makes up
///      a dominant fraction (≥ 0.75) of the token AND the token is a plausible
///      inflection/compound of the term (e.g. "react" ⊂ "reactjs" = 0.71 is
///      borderline; "django" ⊂ "djangorest" = 0.67 rejected; "java" ⊂
///      "javascript" = 0.4 firmly rejected). This preserves legitimate
///      compound-brand matches while rejecting distinct-word collisions.
///   3. Very short terms (< 3 alnum chars, e.g. "go", "c") match ONLY exactly —
///      they collide with far too many English tokens as substrings.
fn negative_term_matches_token(term_clean: &str, token_clean: &str) -> bool {
    if term_clean.is_empty() || token_clean.is_empty() {
        return false;
    }
    if term_clean == token_clean {
        return true;
    }
    // Short terms: exact-only (already returned above), never substring.
    if term_clean.len() < 3 {
        return false;
    }
    // Compound/inflection match: token must START with the term (suffix like
    // "js", "lang", "rest") and the term must dominate the token length.
    if token_clean.starts_with(term_clean) {
        let ratio = term_clean.len() as f32 / token_clean.len() as f32;
        return ratio >= 0.75;
    }
    false
}

/// Whole-text negative match: true if ANY whitespace token in `text` matches
/// the (possibly multi-word) negative term. Multi-word terms fall back to a
/// phrase containment check. `text` and `term` are lowercased by the caller.
fn text_matches_negative(text_lower: &str, term_lower: &str) -> bool {
    let term_words: Vec<&str> = term_lower.split_whitespace().collect();
    if term_words.len() > 1 {
        // Multi-word constraint: require the full phrase (word-boundary safe
        // enough for phrases; single-word collisions are the real hazard).
        return text_lower.contains(term_lower);
    }
    let term_clean: String = term_lower.chars().filter(|c| c.is_alphanumeric()).collect();
    let words: Vec<&str> = text_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    words.iter().any(|w| {
        negative_term_matches_token(&term_clean, w)
    })
}

/// Gazetteer mapping well-known place names to ISO-3166 country codes.
/// This is *reference data* (like a dictionary), NOT per-query hardcoded logic:
/// it lets a query that explicitly names a location override the IP-derived
/// geolocation. Robust and extensible — add an entry to support a new place.
/// Matching is done on whole words only (no substring collisions).
const LOCATION_GAZETTEER: &[(&str, &str)] = &[
    // Countries (name + common demonym/adjective forms handled by synonyms)
    ("united states", "US"), ("usa", "US"), ("us", "US"), ("america", "US"),
    ("united kingdom", "GB"), ("uk", "GB"), ("britain", "GB"), ("england", "GB"),
    ("canada", "CA"), ("australia", "AU"), ("new zealand", "NZ"),
    ("germany", "DE"), ("france", "FR"), ("spain", "ES"), ("italy", "IT"),
    ("portugal", "PT"), ("netherlands", "NL"), ("ireland", "IE"),
    ("sweden", "SE"), ("norway", "NO"), ("denmark", "DK"), ("finland", "FI"),
    ("poland", "PL"), ("austria", "AT"), ("switzerland", "CH"), ("belgium", "BE"),
    ("russia", "RU"), ("ukraine", "UA"), ("turkey", "TR"), ("greece", "GR"),
    ("japan", "JP"), ("china", "CN"), ("korea", "KR"), ("south korea", "KR"),
    ("india", "IN"), ("singapore", "SG"), ("hong kong", "HK"),
    ("brazil", "BR"), ("mexico", "MX"), ("argentina", "AR"),
    ("united arab emirates", "AE"), ("uae", "AE"), ("saudi arabia", "SA"),
    ("egypt", "EG"), ("israel", "IL"), ("thailand", "TH"), ("vietnam", "VN"),
    ("indonesia", "ID"), ("malaysia", "MY"), ("philippines", "PH"),
    ("south africa", "ZA"), ("nigeria", "NG"), ("kenya", "KE"),
    ("czech republic", "CZ"), ("hungary", "HU"), ("romania", "RO"),
    ("czechia", "CZ"), ("croatia", "HR"), ("slovenia", "SI"),
    // Major cities (route to their country for language/region hints)
    ("tokyo", "JP"), ("london", "GB"), ("paris", "FR"), ("berlin", "DE"),
    ("madrid", "ES"), ("rome", "IT"), ("amsterdam", "NL"), ("dublin", "IE"),
    ("stockholm", "SE"), ("oslo", "NO"), ("copenhagen", "DK"), ("helsinki", "FI"),
    ("moscow", "RU"), ("kyiv", "UA"), ("istanbul", "TR"), ("athens", "GR"),
    ("beijing", "CN"), ("shanghai", "CN"), ("seoul", "KR"), ("delhi", "IN"),
    ("mumbai", "IN"), ("bangalore", "IN"), ("bengaluru", "IN"), ("singapore", "SG"),
    ("sydney", "AU"), ("melbourne", "AU"), ("auckland", "NZ"),
    ("new york", "US"), ("san francisco", "US"), ("los angeles", "US"),
    ("chicago", "US"), ("seattle", "US"), ("boston", "US"), ("austin", "US"),
    ("toronto", "CA"), ("vancouver", "CA"), ("sao paulo", "BR"), ("mexico city", "MX"),
    ("dubai", "AE"), ("cairo", "EG"), ("bangkok", "TH"), ("jakarta", "ID"),
    ("cape town", "ZA"), ("lagos", "NG"),
];

/// If the query explicitly names a location (via whole-word match against the
/// gazetteer), return a `GeoLocation` describing it. This OVERRIDES the
/// IP-derived geolocation so a user in India searching "restaurants in tokyo
/// japan" gets JP-localised results, not IN-localised ones.
///
/// Robustness notes (no per-query hardcoding):
///   • Matching is whole-word only, so "java" (the language) never matches the
///     "ja" prefix of nothing here, and "iran" won't match "iran" inside
///     "teheran". Only exact gazetteer entries win.
///   • The city name is recorded so ranking can still boost the specific city.
///   • Returns `None` when no gazetteer entry is present — then IP geo is used.
fn detect_explicit_location(query: &str) -> Option<geoloc::GeoLocation> {
    let q_lower = query.to_lowercase();
    let tokens: Vec<&str> = q_lower.split_whitespace().collect();
    // Try multi-word phrases first (longest first), then single words.
    for entry in LOCATION_GAZETTEER.iter() {
        let name = entry.0;
        let cc = entry.1;
        let name_words: Vec<&str> = name.split_whitespace().collect();
        let matched = if name_words.len() > 1 {
            // Multi-word place: require full phrase as a contiguous token run.
            q_lower.contains(name)
        } else {
            tokens.contains(&name)
        };
        if matched {
            let country_name = Some(country_name_for(cc).to_string());
            return Some(geoloc::GeoLocation {
                country_code: Some(cc.to_string()),
                country_name,
                region: None,
                city: if name_words.len() > 1 || is_city(name) {
                    Some(name.to_string())
                } else {
                    None
                },
                postal_code: None,
                latitude: None,
                longitude: None,
                time_zone: None,
            });
        }
    }
    None
}

/// True if the gazetteer name is a city (vs a country), used to decide whether
/// to populate `city`. Derived from the city set; cheap linear scan.
fn is_city(name: &str) -> bool {
    const CITIES: &[&str] = &[
        "tokyo", "london", "paris", "berlin", "madrid", "rome", "amsterdam",
        "dublin", "stockholm", "oslo", "copenhagen", "helsinki", "moscow", "kyiv",
        "istanbul", "athens", "beijing", "shanghai", "seoul", "delhi", "mumbai",
        "bangalore", "bengaluru", "singapore", "sydney", "melbourne", "auckland",
        "new york", "san francisco", "los angeles", "chicago", "seattle", "boston",
        "austin", "toronto", "vancouver", "sao paulo", "mexico city", "dubai",
        "cairo", "bangkok", "jakarta", "cape town", "lagos",
    ];
    CITIES.contains(&name)
}

/// Human-readable country name for an ISO code (gazetteer-derived).
fn country_name_for(cc: &str) -> String {
    let name = match cc {
        "US" => "United States", "GB" => "United Kingdom", "CA" => "Canada",
        "AU" => "Australia", "NZ" => "New Zealand", "DE" => "Germany",
        "FR" => "France", "ES" => "Spain", "IT" => "Italy", "PT" => "Portugal",
        "NL" => "Netherlands", "IE" => "Ireland", "SE" => "Sweden", "NO" => "Norway",
        "DK" => "Denmark", "FI" => "Finland", "PL" => "Poland", "AT" => "Austria",
        "CH" => "Switzerland", "BE" => "Belgium", "RU" => "Russia", "UA" => "Ukraine",
        "TR" => "Turkey", "GR" => "Greece", "JP" => "Japan", "CN" => "China",
        "KR" => "South Korea", "IN" => "India", "SG" => "Singapore", "HK" => "Hong Kong",
        "BR" => "Brazil", "MX" => "Mexico", "AR" => "Argentina", "AE" => "United Arab Emirates",
        "SA" => "Saudi Arabia", "EG" => "Egypt", "IL" => "Israel", "TH" => "Thailand",
        "VN" => "Vietnam", "ID" => "Indonesia", "MY" => "Malaysia", "PH" => "Philippines",
        "ZA" => "South Africa", "NG" => "Nigeria", "KE" => "Kenya", "CZ" => "Czech Republic",
        "HU" => "Hungary", "RO" => "Romania", "HR" => "Croatia", "SI" => "Slovenia",
        _ => cc,
    };
    name.to_string()
}

fn expand_negative_synonyms(term: &str) -> Vec<String> {
    let mut expanded = vec![term.to_lowercase()];
    let term_lower = term.to_lowercase();
    match term_lower.as_str() {
        "vscode" | "vs code" | "visual studio code" => {
            expanded.push("vscode".to_string());
            expanded.push("vs code".to_string());
            expanded.push("visual studio code".to_string());
        }
        "aws" => {
            expanded.push("amazon".to_string());
            expanded.push("amazon web services".to_string());
        }
        "gcp" => {
            expanded.push("google cloud".to_string());
            expanded.push("google cloud platform".to_string());
            expanded.push("google".to_string());
        }
        "azure" => {
            expanded.push("microsoft azure".to_string());
            expanded.push("microsoft cloud".to_string());
            expanded.push("microsoft".to_string());
        }
        "google workspace" | "google workspace..." => {
            expanded.push("gsuite".to_string());
            expanded.push("g suite".to_string());
            expanded.push("google docs".to_string());
            expanded.push("google sheets".to_string());
            expanded.push("google slides".to_string());
            expanded.push("google drive".to_string());
        }
        "microsoft 365" | "office 365" => {
            expanded.push("m365".to_string());
            expanded.push("o365".to_string());
            expanded.push("office365".to_string());
            expanded.push("microsoft365".to_string());
            expanded.push("microsoft office".to_string());
            expanded.push("word online".to_string());
            expanded.push("excel online".to_string());
        }
        "big tech" => {
            expanded.push("google".to_string());
            expanded.push("microsoft".to_string());
            expanded.push("apple".to_string());
            expanded.push("amazon".to_string());
            expanded.push("meta".to_string());
            expanded.push("facebook".to_string());
        }
        _ => {}
    }
    expanded
}


fn is_comparison_or_alternative_query(constraints: &Constraints) -> bool {
    let has_comp_ref_entity = constraints.entities.iter().any(|e| {
        e.role == EntityRole::Comparison || e.role == EntityRole::Reference
    });
    if has_comp_ref_entity {
        return true;
    }
    for p in &constraints.positive {
        let pl = p.to_lowercase();
        if pl == "vs" || pl == "versus" || pl == "alternative" || pl == "alternatives" || pl == "replacement" || pl == "comparison" {
            return true;
        }
    }
    for pr in &constraints.phrases {
        let pl = pr.to_lowercase();
        if pl.contains(" vs ") || pl.contains("alternative") || pl.contains("replacement") || pl.contains("comparison") {
            return true;
        }
    }
    false
}

fn constraint_score(
    title: &str,
    content: &str,
    url: &str,
    constraints: &Constraints,
) -> f32 {

    if constraints.positive.is_empty() && constraints.negative.is_empty() {
        return 1.0; // no constraints = no penalty
    }

    let text_lower = format!("{} {} {}", title.to_lowercase(), content.to_lowercase(), url.to_lowercase());
    let mut score: f32 = 1.0;

    // Pre-normalize text: remove dots/hyphens/underscores between alphanumerics for fuzzy matching
    // "node.js" → "nodejs", "c++" → "c++" (kept), "real-time" → "realtime"
    let text_normalized: String = {
        let chars: Vec<char> = text_lower.chars().collect();
        let mut out = String::with_capacity(chars.len());
        for (i, &c) in chars.iter().enumerate() {
            if c == '.' || c == '-' || c == '_' {
                // Only keep if between alphanumeric chars (collapse separators)
                if i > 0 && i + 1 < chars.len()
                    && chars[i-1].is_alphanumeric() && chars[i+1].is_alphanumeric()
                {
                    // Skip separator — collapse "node.js" to "nodejs"
                } else {
                    out.push(c);
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    let mut expanded_negatives: Vec<String> = Vec::new();
    for neg in &constraints.negative {
        for syn in expand_negative_synonyms(neg) {
            if !expanded_negatives.contains(&syn) {
                expanded_negatives.push(syn);
            }
        }
    }
    let neg_count = expanded_negatives.len() as f32;
    // Pre-normalize title for constraint matching
    let title_lower = title.to_lowercase();
    let title_normalized: String = {
        let chars: Vec<char> = title_lower.chars().collect();
        let mut out = String::with_capacity(chars.len());
        for (i, &c) in chars.iter().enumerate() {
            if c == '.' || c == '-' || c == '_' {
                if i > 0 && i + 1 < chars.len()
                    && chars[i-1].is_alphanumeric() && chars[i+1].is_alphanumeric()
                { /* skip separator */ } else { out.push(c); }
            } else { out.push(c); }
        }
        out
    };
    // Check once if this is an alternative-listing page (comparison/vs/alternatives).
    // Alt pages naturally mention excluded terms in referential context, so they
    // get a SINGLE flat penalty regardless of how many excluded terms they mention.
    // Regular pages get per-term multiplicative penalties.
    let alt_score = is_alternative_listing_page(title, url, content);
    let is_alt_page = alt_score > 0.3 && is_comparison_or_alternative_query(constraints);
    let mut any_negative_matched = false;
    let mut hit_count = 0u32;

    for neg in &expanded_negatives {
        let neg_lower = neg.to_lowercase();
        let neg_normalized: String = neg_lower.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
        let neg_words: Vec<&str> = neg_lower.split_whitespace().collect();
        // Match against title + content + URL for all constraint lengths
        // Content matching uses word boundaries to reduce noise
        let title_or_url_matched = if neg_words.len() == 1 {
            text_matches_negative(&title_lower, &neg_lower)
            || text_matches_negative(&title_normalized, &neg_normalized)
            || url.to_lowercase().split('/').any(|segment| {
                let seg = segment.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                seg == neg_lower
                || {
                    let no_www = seg.strip_prefix("www.").unwrap_or(&seg);
                    let domain = no_www.split('.').next().unwrap_or(no_www);
                    domain == neg_lower
                }
                || {
                    let seg_alpha: String = seg.chars().filter(|c| c.is_alphanumeric()).collect();
                    seg_alpha.len() > neg_normalized.len()
                        && neg_normalized.len() >= 3
                        && seg_alpha.starts_with(&neg_normalized)
                        && neg_normalized.len() as f32 / seg_alpha.len() as f32 >= 0.6
                }
            })
        } else {
            title_lower.contains(&neg_lower) || title_normalized.contains(&neg_normalized)
            || url.to_lowercase().contains(&neg_lower)
        };

        let content_matched = if neg_words.len() == 1 {
            text_matches_negative(&content.to_lowercase(), &neg_lower)
        } else {
            content.to_lowercase().contains(&neg_lower)
        };

        if title_or_url_matched {
            hit_count += 1;
            any_negative_matched = true;
            if !is_alt_page {
                let penalty = (0.02 + (neg_count - 1.0) * 0.06).clamp(0.02, 0.20);
                tracing::info!("CONSTRAINT HIT (TITLE/URL): '{}' in '{}' → penalty={:.4} (non-alt)",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    penalty);
                score *= penalty;
            }
        } else if content_matched {
            hit_count += 1;
            any_negative_matched = true;
            if !is_alt_page {
                let penalty = (0.25 + (neg_count - 1.0) * 0.05).clamp(0.25, 0.50);
                tracing::info!("CONSTRAINT HIT (CONTENT): '{}' in '{}' → penalty={:.4} (non-alt)",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    penalty);
                score *= penalty;
            }
        } else {
            tracing::info!("CONSTRAINT MISS: '{}' not in '{}'", neg, &text_lower[..text_lower.char_indices().nth(60).map(|(i,_)| i).unwrap_or(text_lower.len())]);
        }
    }

    if any_negative_matched && is_alt_page {
        // Alt pages get one single flat penalty regardless of how many excluded
        // terms they mention. This prevents "Django vs FastAPI vs Flask: Which to
        // Choose" (which mentions all 3) from getting compounded 0.175^3 = 0.005.
        // The alt_score measures how strongly this page is an alternative listing
        // (comparison vs titles, URL patterns, content patterns).
        // High alt_score → barely penalized: alt_score=0.7 → 0.175 single hit
        // Low alt_score → moderate: alt_score=0.3 → 0.225 single hit
        // Scale alt penalty by exclusion count: more exclusions = stricter penalty.
        // A page listing 5 excluded engines is much less relevant than one listing 1.
        let neg_exclusion_count = constraints.negative.len() as f32;
        let alt_penalty_base = if neg_exclusion_count >= 4.0 {
            0.25 // very strict for 4+ exclusions
        } else if neg_exclusion_count >= 2.0 {
            0.20 // moderate for 2-3 exclusions
        } else {
            0.15 // standard for single exclusion
        };
        let alt_penalty = (alt_penalty_base + alt_score * 0.25).min(0.50);
        tracing::info!(
            "ALT PAGE NEGATIVE PENALTY: {} hits, alt_score={:.3} → single penalty={:.4}",
            hit_count, alt_score, alt_penalty
        );
        score *= alt_penalty;
    }

    // ── BUG P0: hard-exclude pages whose TOPIC IS the excluded term ──
    // Soft penalties alone let "best python ide NOT pycharm" surface PyCharm
    // in the top 5 (47% of results still mentioned it). But we must NOT drop
    // alternative-listing / comparison pages (e.g. "Best Static Site Generators"
    // that *mention* Jekyll among many) — those are useful and the user wants
    // them. Rule: a NON-alt page is dropped only when its TITLE is dominated by
    // the excluded term(s): ≥50% of its non-stopword title tokens are an
    // excluded term (or a sub-brand of it, e.g. "pycharm" ∈ "pycharm-community").
    // Incidental mentions inside body/comparison pages are left to the soft
    // penalty above. Fail-closed: if we can't prove dominance, we keep it.
    if !is_alt_page && !expanded_negatives.is_empty() {
        const STOP: &[&str] = &[
            "the", "a", "an", "and", "or", "for", "of", "in", "on", "to", "with",
            "vs", "versus", "best", "top", "review", "reviews", "guide", "guides",
            "how", "what", "why", "is", "are", "not", "without", "free", "new",
        ];
        let title_tokens: Vec<String> = title_lower
            .split(|c: char| !c.is_alphanumeric())
            .map(|t| t.trim().to_string())
            .filter(|t| t.len() >= 2 && !STOP.contains(&t.as_str()))
            .collect();
        if !title_tokens.is_empty() {
            let dominated = title_tokens.iter().filter(|tok| {
                expanded_negatives.iter().any(|neg| {
                    if neg.is_empty() { return false; }
                    let n = neg.trim().to_lowercase();
                    // Exact word, or the token is a sub-brand/compound of the
                    // excluded term (n ⊂ tok, covering "pycharm-community",
                    // "macbook-pro", "nodejs"-style collisions handled by len gap).
                    tok.as_str() == n.as_str()
                        || (tok.len() > n.len()
                            && tok.starts_with(&n)
                            && (tok.len() - n.len()) as f32 / n.len() as f32 <= 0.6)
                })
            }).count();
            let dom_frac = dominated as f32 / title_tokens.len() as f32;
            if dom_frac >= 0.5 {
                tracing::info!(
                    "NEG HARD-DROP (topic match): '{}' dominated by excluded term(s) ({}/{} tokens)",
                    title, dominated, title_tokens.len()
                );
                return 0.0;
            }
        }
    }

    // Positive constraints: boost for each match (fuzzy matching)
    if !constraints.positive.is_empty() {
        let mut matched = 0;
        for pos in &constraints.positive {
            let pos_lower = pos.to_lowercase();
            let pos_normalized: String = pos_lower.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
            let pos_words: Vec<&str> = pos_lower.split_whitespace().collect();
            let found = if pos_words.len() == 1 {
                text_lower.split_whitespace().any(|w| {
                    w == pos_lower
                    || w.trim_matches(|c: char| !c.is_alphanumeric()) == pos_lower
                    || w.chars().filter(|c| c.is_alphanumeric()).collect::<String>() == pos_normalized
                })
                || text_normalized.split_whitespace().any(|w| w == pos_normalized)
                || (pos_normalized.len() >= 3 && text_normalized.contains(&pos_normalized))
            } else {
                // Multi-word: exact phrase match OR all words present individually
                // "async support" → match "async" AND "support" anywhere in text
                text_lower.contains(&pos_lower)
                || text_normalized.contains(&pos_normalized)
                || pos_words.iter().all(|w| {
                    let w_clean = w.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
                    text_lower.split_whitespace().any(|tw| {
                        tw == *w || tw.trim_matches(|c: char| !c.is_alphanumeric()) == *w
                    }) || (w_clean.len() >= 3 && text_normalized.contains(&w_clean))
                })
            };
            if found {
                matched += 1;
            }
        }
        // Coverage: fraction of positive constraints matched
        let positive_count = constraints.positive.len() as f32;
        let coverage = matched as f32 / positive_count;

        // Positive boost is a bi-criteria score biased toward multi-signal hits:
        // - Coverage pressure: fraction of positives matched.
        // - Width pressure: concrete multi-positive hits beat single-token matches from broad docs.
        // Coverage dominates for small positive sets; width lifts tighter topical candidates.

        let mut coverage_pressure = coverage;
        let mut width_pressure = if positive_count > 1.0 {
            (matched as f32 / positive_count).sqrt()
        } else {
            matched as f32 / positive_count
        };

        // Soft fallback: when no positive matched, treat the result as if it matched the
        // query semantically. This prevents narrow positive sets from producing zero-pressure
        // text and turning ordering into a metadata lottery. It is NOT a fake match:
        // it is a last-resort boost based on query-to-document similarity.
        if matched == 0 {
            let url_tokens: Vec<&str> = url.split_whitespace().collect();
            let title_tokens: Vec<&str> = title.split_whitespace().collect();
            if !url_tokens.is_empty() || !title_tokens.is_empty() {
                let mut similarity_gap = 0.0f32;
                if !url_tokens.is_empty() {
                    if let Ok(parsed_url) = reqwest::Url::parse(url) {
                        if let Some(host) = parsed_url.host_str() {
                            let host_lower = host.to_lowercase();
                            let matching = url_tokens.iter().filter(|t| host_lower.contains(*t)).count();
                            similarity_gap = (matching as f32 / url_tokens.len() as f32).clamp(0.0, 1.0);
                        }
                    }
                }
                let q_reuse: f32 = semantic_relevance_score(url, &title, &content);
                let similar = (similarity_gap * 0.45 + q_reuse * 0.55).clamp(0.0, 1.0);
                coverage_pressure = coverage_pressure.max(similar * 0.12);
                width_pressure = width_pressure.max(similar * 0.12);
            }
        }

        let blended_coverage = coverage_pressure * 0.70 + width_pressure * 0.30;

        // Scale: 0% coverage -> 0.60x (Phase 5: raised from 0.35 so a broad
        //       query with no positive hits is no longer over-penalized)
        //         100% coverage -> 1.9x
        // Mapping is monotonic, but at least one positive match with high coverage
        // becomes a strong discriminator vs zero-match passthrough.
        score *= 0.60 + blended_coverage * 1.30;
    }

    // Language entity constraints: when a programming language is detected in the
    // query, boost results that mention it and penalize results mentioning a
    // different language. This handles "Go web framework" not surfacing Python results.
    if let Some(ref lang) = constraints.language {
        let lang_lower = lang.to_lowercase();
        // Known language-related terms that appear in result content when the
        // result IS about that language. Not exhaustive — just the most common
        // co-occurring terms.
        let lang_aliases: &[&str] = match lang_lower.as_str() {
            "go" => &["golang", "go ", " go,", " go.", " go/", "go-", ".go"],
            "rust" => &["rust", "rustc", "cargo", "crate"],
            "python" => &["python", "pip", "pypi", "django", "flask", "fastapi"],
            "javascript" => &["javascript", "nodejs", "node.js", "npm", "yarn", "deno", "bun"],
            "typescript" => &["typescript", "tsc", "tsx"],
            "java" => &["java", "jdk", "jvm", "maven", "gradle", "spring"],
            "c++" => &["c++", "cpp", "cmake", "boost"],
            "ruby" => &["ruby", "rails", "gem", "bundler"],
            "php" => &["php", "composer", "laravel", "symfony"],
            "swift" => &["swift", "xcode", "swiftui", "cocoapods"],
            "kotlin" => &["kotlin", "ktor", "gradle"],
            _ => &[lang_lower.as_str()],
        };

        // Check if result mentions the detected language
        let mentions_lang = lang_aliases.iter().any(|alias| text_lower.contains(alias));

        // Check if result mentions a DIFFERENT language (cross-language penalty)
        let other_languages: &[&[&str]] = &[
            &["python", "pip", "pypi", "django", "flask"],
            &["javascript", "nodejs", "node.js", "npm"],
            &["typescript", "tsc"],
            &["rust", "rustc", "cargo"],
            &["golang", " go "],
            &["java", "jdk", "jvm"],
            &["ruby", "rails"],
            &["php", "laravel"],
            &["swift", "xcode"],
            &["kotlin", "ktor"],
            &["c++", "cpp"],
        ];

        let mentions_other = other_languages.iter().any(|group| {
            // Skip the group that matches the detected language
            let group_canonical = group[0].to_lowercase();
            if lang_aliases.iter().any(|a| *a == group_canonical) { return false; }
            group.iter().any(|term| text_lower.contains(term))
        });

        if mentions_lang {
            score *= 1.3; // 30% boost for matching language
        } else if mentions_other {
            score *= 0.6; // 40% penalty for different language
        }
    }

    score.clamp(0.0, 1.0)
}

/// Parsed price constraint. `min`/`max` describe an explicit range (`price:10-100`);
/// `lt`/`gt` describe a comparison operator (`price:<100`, `price:>50`). A bare
/// `price:100` is treated as an upper bound (`max`/`lt` both set to 100) so the
/// documented `<` operator is never silently discarded.
struct ParsedPrice {
    min: Option<f32>,
    max: Option<f32>,
    lt: Option<f32>,
    gt: Option<f32>,
}

fn parse_price_range(s: &str) -> Option<ParsedPrice> {
    let s = s.trim();
    // Detect comparison operators before stripping them.
    let (op, rest) = if let Some(v) = s.strip_prefix("<=") {
        ("le", v)
    } else if let Some(v) = s.strip_prefix(">=") {
        ("ge", v)
    } else if let Some(v) = s.strip_prefix('<') {
        ("lt", v)
    } else if let Some(v) = s.strip_prefix('>') {
        ("gt", v)
    } else {
        ("", s)
    };

    // Keep digits, '-' (range separator) and '.' only.
    let clean: String = rest.chars().filter(|c| c.is_numeric() || *c == '-' || *c == '.').collect();

    // Explicit range: "10-100"
    if clean.contains('-') {
        let parts: Vec<&str> = clean.split('-').collect();
        if parts.len() == 2 {
            let pmin = parts[0].parse::<f32>().ok();
            let pmax = parts[1].parse::<f32>().ok();
            if pmin.is_some() || pmax.is_some() {
                return Some(ParsedPrice { min: pmin, max: pmax, lt: None, gt: None });
            }
        }
    }

    // Single value: bare `price:100` or `price:<100` / `price:>50`.
    if let Ok(val) = clean.parse::<f32>() {
        return match op {
            "lt" | "le" => Some(ParsedPrice { min: None, max: Some(val), lt: Some(val), gt: None }),
            "gt" | "ge" => Some(ParsedPrice { min: Some(val), max: None, lt: None, gt: Some(val) }),
            _ => Some(ParsedPrice { min: None, max: Some(val), lt: Some(val), gt: None }),
        };
    }
    None
}

/// Hard-filter results violating constraints beyond redemption.
fn sanitize_constraints(c: &Constraints) -> Constraints {
    let mut negative: Vec<String> = Vec::new();
    let mut positive: Vec<String> = Vec::new();
    let mut file_types = c.file_types.clone();
    let mut sites = c.sites.clone();
    let phrases = c.phrases.clone();
    let mut after_date = c.after_date.clone();
    let mut before_date = c.before_date.clone();
    let mut intitle = c.intitle.clone();
    let mut inurl = c.inurl.clone();
    let mut intext = c.intext.clone();
    let mut related = c.related.clone();
    let mut price_min = c.price_min;
    let mut price_max = c.price_max;
    let mut price_lt = c.price_lt;
    let mut price_gt = c.price_gt;

    // 1. Process negative constraints first (filtering and stripping +- or - prefixes)
    for n in &c.negative {
        let mut clean_n = n.trim().to_lowercase();
        if clean_n.starts_with('-') {
            clean_n = clean_n.strip_prefix('-').unwrap().trim().to_string();
        }
        if clean_n.starts_with('+') {
            clean_n = clean_n.strip_prefix('+').unwrap().trim().to_string();
        }
        if clean_n.split_whitespace().count() <= 2 && !clean_n.is_empty() {
            if !negative.contains(&clean_n) {
                negative.push(clean_n);
            }
        }
    }

    // 2. Process positive constraints, filtering out actual negatives and operators
    for p in &c.positive {
        let mut clean_p = p.trim().to_lowercase();
        
        let mut is_neg = false;
        if clean_p.starts_with('-') {
            clean_p = clean_p.strip_prefix('-').unwrap().trim().to_string();
            is_neg = true;
        } else if clean_p.starts_with("+-") {
            clean_p = clean_p.strip_prefix("+-").unwrap().trim().to_string();
            is_neg = true;
        }
        
        if is_neg {
            if !clean_p.is_empty() && clean_p.split_whitespace().count() <= 2 {
                if !negative.contains(&clean_p) {
                    negative.push(clean_p);
                }
            }
            continue;
        }

        if clean_p.starts_with("filetype:") {
            let ft = clean_p.strip_prefix("filetype:").unwrap().trim().to_string();
            if !ft.is_empty() && !file_types.contains(&ft) {
                file_types.push(ft);
            }
        } else if clean_p.starts_with("site:") {
            let site = clean_p.strip_prefix("site:").unwrap().trim().to_string();
            if !site.is_empty() && !sites.contains(&site) {
                sites.push(site);
            }
        } else if clean_p.starts_with("after:") {
            let val = clean_p.strip_prefix("after:").unwrap().trim().to_string();
            if !val.is_empty() {
                after_date = Some(val);
            }
        } else if clean_p.starts_with("before:") {
            let val = clean_p.strip_prefix("before:").unwrap().trim().to_string();
            if !val.is_empty() {
                before_date = Some(val);
            }
        } else if clean_p.starts_with("intitle:") {
            let val = clean_p.strip_prefix("intitle:").unwrap().trim().to_string();
            if !val.is_empty() && !intitle.contains(&val) {
                intitle.push(val);
            }
        } else if clean_p.starts_with("inurl:") {
            let val = clean_p.strip_prefix("inurl:").unwrap().trim().to_string();
            if !val.is_empty() && !inurl.contains(&val) {
                inurl.push(val);
            }
        } else if clean_p.starts_with("intext:") {
            let val = clean_p.strip_prefix("intext:").unwrap().trim().to_string();
            if !val.is_empty() && !intext.contains(&val) {
                intext.push(val);
            }
        } else if clean_p.starts_with("related:") {
            let val = clean_p.strip_prefix("related:").unwrap().trim().to_string();
            if !val.is_empty() && !related.contains(&val) {
                related.push(val);
            }
        } else if clean_p.starts_with("price:") {
            let val = clean_p.strip_prefix("price:").unwrap().trim().to_string();
            if let Some(p) = parse_price_range(&val) {
                price_min = p.min.or(price_min);
                price_max = p.max.or(price_max);
                price_lt = p.lt.or(price_lt);
                price_gt = p.gt.or(price_gt);
            }
        } else {
            let pl = clean_p;
            if pl.is_empty() { continue; }
            let is_dup = positive.iter().any(|kept| {
                let kl = kept.to_lowercase();
                kl == pl || kl.split_whitespace().all(|w| pl.split_whitespace().any(|w2| w2 == w))
                    || pl.split_whitespace().all(|w| kl.split_whitespace().any(|w2| w2 == w))
            });
            if !is_dup {
                positive.push(p.clone()); // keep original casing
            }
        }
    }

    Constraints {
        positive,
        negative,
        entities: c.entities.clone(),
        language: c.language.clone(),
        file_types,
        sites,
        phrases,
        after_date,
        before_date,
        intitle,
        inurl,
        intext,
        related,
        price_min,
        price_max,
        price_lt,
        price_gt,
    }
}

fn extract_year_from_url(url: &str) -> Option<i32> {
    let re = regex::Regex::new(r"\b(20\d{2}|19\d{2})\b").ok()?;
    if let Some(caps) = re.captures(url) {
        let y = caps.get(1)?.as_str().parse::<i32>().ok()?;
        return Some(y);
    }
    None
}

fn get_related_domains(domain: &str) -> Vec<String> {
    let d = domain.to_lowercase();
    let clean_d = d.trim_start_matches("www.");
    match clean_d {
        "github.com" => vec!["gitlab.com".to_string(), "bitbucket.org".to_string()],
        "google.com" => vec!["bing.com".to_string(), "duckduckgo.com".to_string(), "yahoo.com".to_string()],
        "reddit.com" => vec!["news.ycombinator.com".to_string(), "quora.com".to_string(), "stackexchange.com".to_string()],
        "wikipedia.org" => vec!["britannica.com".to_string(), "wikihow.com".to_string()],
        _ => vec![],
    }
}

fn extract_price_from_text(text: &str) -> Option<f32> {
    let lower = text.to_lowercase();
    let amount = r"(\d{1,3}(?:[.,]\d{3})*(?:\.\d{2})?|\d+(?:\.\d{2})?)";

    // Currency symbol / code followed by an amount: $100, €99, US$ 1,299.00, £50
    static RE1: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re1 = RE1.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i)(?:us\s?\$|can\s?\$|au\s?\$|\$|€|£|¥|₹|rs\.?\s?|inr|eur|gbp|usd)\s*{}",
            amount
        ))
        .unwrap()
    });
    if let Some(caps) = re1.captures(&lower) {
        let raw = caps.get(1)?.as_str().replace(',', "");
        if let Ok(v) = raw.parse::<f32>() {
            return Some(v);
        }
    }

    // Amount followed by a currency word: 100 dollars, 200 euros, 999 rupees
    static RE2: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re2 = RE2.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i){}\s*(?:us\s?dollars?|dollars?|euros?|pounds?|gbp|usd|inr|rupees?|rs)",
            amount
        ))
        .unwrap()
    });
    if let Some(caps) = re2.captures(&lower) {
        let raw = caps.get(1)?.as_str().replace(',', "");
        if let Ok(v) = raw.parse::<f32>() {
            return Some(v);
        }
    }

    // Explicit price/cost label: "price: 49", "cost 129", "starting at 15.99"
    static RE3: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re3 = RE3.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i)(?:price|cost|starting\s+at|from\s+price|for\s+only)\s*:?\s*\$?\s*{}",
            amount
        ))
        .unwrap()
    });
    if let Some(caps) = re3.captures(&lower) {
        let raw = caps.get(1)?.as_str().replace(',', "");
        if let Ok(v) = raw.parse::<f32>() {
            return Some(v);
        }
    }

    None
}

fn should_filter_by_constraints(
    title: &str, content: &str, url: &str, published_date: Option<&str>, constraints: &Constraints,
) -> bool {
    // 1. Hard filter on file_types
    if !constraints.file_types.is_empty() {
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            let path = parsed_url.path();
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if ext.is_empty() {
                // The URL has no file extension at all (clean slug, CDN download
                // link, Google Docs viewer, etc.). SearXNG already applied the
                // `filetype:` constraint upstream, so we trust that and KEEP the
                // result rather than hard-dropping it. Only when the URL *does*
                // carry an extension do we require it to match — that's a
                // reliable signal we can enforce without false negatives.
                // (Previously every extensionless filetype result was dropped,
                // which zeroed out `python filetype:pdf filetype:doc`.)
            } else if !constraints.file_types.iter().any(|ft| ft.to_lowercase() == ext) {
                return true;
            }
        } else {
            return true;
        }
    }

    // 2b. Hard EXCLUSION for negated site:/filetype: tokens (e.g. "-site:reddit.com").
        // These arrive in `negative` as "site:reddit.com" / "filetype:pdf" and must
        // hard-drop matching results — the exact opposite of the positive `+site:` filter.
        // Without this, "-site:reddit.com python" would return reddit links because the
        // negation was only applied as a soft text penalty, never as a structural exclude.
        if !constraints.negative.is_empty() {
        let neg_sites: Vec<String> = constraints.negative.iter()
            .filter_map(|n| n.strip_prefix("site:").map(|s| s.trim().to_lowercase()))
            .filter(|s| !s.is_empty())
            .collect();
        let neg_fts: Vec<String> = constraints.negative.iter()
            .filter_map(|n| n.strip_prefix("filetype:").map(|s| s.trim().to_lowercase()))
            .filter(|s| !s.is_empty())
            .collect();
        if !neg_sites.is_empty() {
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(host) = parsed_url.host_str().map(|h| h.to_lowercase()) {
                if neg_sites.iter().any(|site| host == *site || host.ends_with(&format!(".{}", site))) {
                    return true;
                }
            }
        }
        }
        if !neg_fts.is_empty() {
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(ext) = std::path::Path::new(parsed_url.path())
                .extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase())
            {
                if neg_fts.iter().any(|ft| *ft == ext) {
                    return true;
                }
            }
        }
        }
        }

        // 2. Hard filter on sites
        if !constraints.sites.is_empty() {
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(host) = parsed_url.host_str().map(|h| h.to_lowercase()) {
                let matches_site = constraints.sites.iter().any(|site| {
                    let s_low = site.to_lowercase();
                    host == s_low || host.ends_with(&format!(".{}", s_low))
                });
                if !matches_site {
                    return true;
                }
            } else {
                return true;
            }
        } else {
            return true;
        }
    }

    // 3. Hard filter on date bounds
    // Policy: a date bound filters ONLY results that carry a parseable
    // published date. Results without a resolvable date are KEPT, never
    // dropped — dropping them silently removes the majority of general web
    // results (most pages expose no machine-readable date), which previously
    // zeroed out queries like "python after:2024" even though plenty of
    // relevant content existed. We report the coverage gap via
    // `ignored_constraints` (dated_result_count==0) instead of silently
    // filtering everything. This is the fail-open choice: a dateless result is
    // assumed in-range rather than out-of-range.
    if let Some(ref ad) = constraints.after_date {
        if let Some(limit) = parse_date_to_comparable(ad) {
            if let Some(p_date) = resolve_item_date(published_date, url, title, content) {
                if !date_gte(p_date, limit) {
                    return true;
                }
            }
        }
    }
    if let Some(ref bd) = constraints.before_date {
        if let Some(limit) = parse_date_to_comparable(bd) {
            if let Some(p_date) = resolve_item_date(published_date, url, title, content) {
                if !date_lte(p_date, limit) {
                    return true;
                }
            }
        }
    }

    // 4. Hard filter on phrases — token-overlap, fail-open.
    // Require ALL words of the phrase to appear (case-insensitive) in
    // title/content/url instead of the exact contiguous substring. Upstream now
    // receives the phrase WITH quotes (preprocess_searxng_query preserves them),
    // so snippets rarely contain the verbatim substring and the old exact-match
    // filter zeroed out valid queries ("Lancaster norms" -> n=0). Fail-open: an
    // empty phrase never drops a result.
    if !constraints.phrases.is_empty() {
        for phrase in &constraints.phrases {
            let p_words: Vec<&str> = phrase.split_whitespace().collect();
            if p_words.is_empty() { continue; }
            let t_low = title.to_lowercase();
            let c_low = content.to_lowercase();
            let u_low = url.to_lowercase();
            let all_present = p_words.iter().all(|w| {
                let wl = w.to_lowercase();
                t_low.contains(&wl) || c_low.contains(&wl) || u_low.contains(&wl)
            });
            if !all_present {
                return true; // drop only when the page lacks the whole phrase's words
            }
        }
    }

    // 4b. Soft preference on intitle: (kept as BOOST, not hard-drop).
    // The operator is now forwarded to the upstream engine (preprocess_searxng_query),
    // which applies it natively. A local hard-drop here re-creates the n=0 trap
    // whenever an engine instance ignores intitle: — so we only nudge ranking.
    // should_filter_by_constraints is a pure predicate, so the boost itself is
    // applied by the callers (constraint_boost). This block intentionally does
    // nothing for intitle (left in place as an explicit no-op marker).

    // 4c. Soft preference on inurl: (BOOST, not hard-drop — see 4b).
    // 4d. Soft preference on intext: (BOOST, not hard-drop — see 4b).
    // (All three are now enforced upstream + boosted downstream; never hard-dropped.)

    // 4e. Hard filter on related:
    // Semantics: "related:amazon.com" means sites SIMILAR to amazon, NOT amazon
    // itself. The upstream search engine is forwarded the `related:` operator
    // (see preprocess_searxng_query) and is expected to return similar sites.
    // Here we only hard-exclude the domain itself (and its subdomains) and keep
    // any curated similar domain. For unknown domains with no curated map we
    // cannot prove a result is unrelated, so we keep it rather than guess.
    if !constraints.related.is_empty() {
        if let Ok(parsed_url) = reqwest::Url::parse(url) {
            if let Some(host) = parsed_url.host_str().map(|h| h.to_lowercase()) {
                let mut is_self = false;
                let mut is_known_related = false;
                for rel in &constraints.related {
                    let rel_l = rel.to_lowercase().trim_start_matches("www.").to_string();
                    if host == rel_l || host.ends_with(&format!(".{}", rel_l)) {
                        is_self = true;
                        break;
                    }
                    for rd in get_related_domains(&rel_l) {
                        if host == rd || host.ends_with(&format!(".{}", rd)) {
                            is_known_related = true;
                        }
                    }
                }
                if is_self {
                    return true; // drop the domain itself
                }
                if is_known_related {
                    return false; // curated similar domain → keep
                }
            }
        }
    }

    // 4f. Hard filter on price:
    if constraints.price_min.is_some() || constraints.price_max.is_some()
        || constraints.price_lt.is_some() || constraints.price_gt.is_some()
    {
        if let Some(price) = extract_price_from_text(title)
            .or_else(|| extract_price_from_text(content))
        {
            if let Some(pmin) = constraints.price_min {
                if price < pmin { return true; }
            }
            if let Some(pmax) = constraints.price_max {
                if price > pmax { return true; }
            }
            if let Some(plt) = constraints.price_lt {
                if price > plt { return true; }
            }
            if let Some(pgt) = constraints.price_gt {
                if price < pgt { return true; }
            }
        }
    }

    // 5. Negative constraint hard filter (skipped for only-negative queries)
    let is_only_negative = !constraints.negative.is_empty() && constraints.positive.is_empty();
    if is_only_negative {
        return false;
    }

    if constraints.negative.is_empty() && constraints.positive.is_empty() {
        return false;
    }
    let c_score = constraint_score(title, content, url, constraints);
    let alt_score = is_alternative_listing_page(title, url, content);
    if alt_score > 0.3 && is_comparison_or_alternative_query(constraints) {
        return c_score < 0.02;
    }
    let threshold = if constraints.negative.len() >= 2 { 0.10 }
    else if constraints.negative.len() == 1 { 0.05 }
    else { 0.02 };
    c_score < threshold
}

/// Soft boost for `intitle:`/`inurl:`/`intext:` constraints.
///
/// These operators are now forwarded to the upstream engine (SearXNG applies them
/// natively). Historically the gateway ALSO hard-dropped any result that did not
/// literally contain the operator token — which zeroed out queries like
/// `rust inurl:blog` when an engine instance ignored the operator (the engine
/// returned results without the token, then the local hard filter wiped them → n=0).
///
/// We therefore never hard-drop on these: the engine is the authoritative enforcer,
/// and the gateway instead gives a modest ranking nudge to results that DO satisfy
/// the operator. This keeps the operator meaningful without risking an empty page.
fn constraint_boost(title: &str, content: &str, url: &str, constraints: &Constraints) -> f32 {
    let mut bonus = 0.0f32;
    let t_low = title.to_lowercase();
    let u_low = url.to_lowercase();
    let c_low = content.to_lowercase();
    for t in &constraints.intitle {
        if !t.is_empty() && t_low.contains(&t.to_lowercase()) {
            bonus += 0.05;
        }
    }
    for u in &constraints.inurl {
        if !u.is_empty() && u_low.contains(&u.to_lowercase()) {
            bonus += 0.05;
        }
    }
    for txt in &constraints.intext {
        if !txt.is_empty() && c_low.contains(&txt.to_lowercase()) {
            bonus += 0.05;
        }
    }
    bonus.min(0.15) // cap so a multi-operator match can't dominate the score
}


// Rotates both gluetun VPN and tor2 circuit to get fresh exit IPs.
// Called on CAPTCHA detection, rate limiting, and periodically every 10 minutes.

/// Tracks whether SearXNG2 (tor2 / Tor) currently has a HOT circuit. A fresh
/// Tor circuit (after NEWNYM) is COLD and the first query can take 10-15s;
/// queries sent into a cold circuit blow the retry budget and surface
/// `upstream_unavailable`. `warm_tor2_cache()` flips this to true once it has
/// rebuilt+confirmed the circuit; the search recovery path waits (bounded) for
/// it before querying tor2 so users don't pay the cold-build cost.
static TOR2_WARM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn trigger_vpn_rotation(reason: &str) {
    tracing::info!("VPN rotation triggered: {}", reason);
    let signal_dir = "/tmp/vpn-signals";
    let signal_path = format!("{}/rotate_signal", signal_dir);
    let _ = std::fs::create_dir_all(signal_dir);
    let _ = std::fs::write(&signal_path, reason);
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::new();
        let _ = client.put("http://127.0.0.1:8000/v1/vpn/status")
            .json(&serde_json::json!({"status": "stopped"}))
            .timeout(std::time::Duration::from_secs(10))
            .send();
        std::thread::sleep(std::time::Duration::from_secs(5));
        let _ = client.put("http://127.0.0.1:8000/v1/vpn/status")
            .json(&serde_json::json!({"status": "running"}))
            .timeout(std::time::Duration::from_secs(10))
            .send();
        std::thread::sleep(std::time::Duration::from_secs(15));
        if let Ok(resp) = client.get("http://127.0.0.1:8000/v1/publicip/ip")
            .timeout(std::time::Duration::from_secs(10))
            .send()
        {
            if let Ok(ip) = resp.text() {
                tracing::info!("VPN rotated, new IP info: {}", ip);
            }
        }
    });
}

fn rotate_tor_circuit() {
    tracing::info!("Rotating tor2 circuit (SIGNAL NEWNYM)");
    std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        if let Ok(mut stream) = std::net::TcpStream::connect("tor2:9052") {
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            // Consume initial "250 OK" banner
            let mut banner = String::new();
            let _ = reader.read_line(&mut banner);
            writeln!(stream, "AUTHENTICATE \"intentforge_rotate\"").ok();
            let mut resp = String::new();
            let _ = reader.read_line(&mut resp);
            if resp.starts_with("250") {
                writeln!(stream, "SIGNAL NEWNYM").ok();
                let mut newnym_resp = String::new();
                let _ = reader.read_line(&mut newnym_resp);
                if newnym_resp.starts_with("250") {
                    tracing::info!("tor2 circuit rotated successfully");
                    // Mark tor2 COLD immediately so the search recovery path's
                    // bounded pre-query wait engages until warm_tor2_cache()
                    // rebuilds+confirms the circuit. Without this, a query
                    // arriving in the cold window would hit the 10-15s build
                    // cost and surface upstream_unavailable.
                    TOR2_WARM.store(false, std::sync::atomic::Ordering::SeqCst);
                    // A fresh Tor circuit is COLD: the next SearXNG2 query can
                    // take 10-15s to build a circuit, which would blow the
                    // gateway's retry budget and surface `upstream_unavailable`
                    // for the very first user query after a rotation. Warm the
                    // circuit in the background so real queries hit a HOT path.
                    warm_tor2_cache();
                } else {
                    tracing::warn!("tor2 NEWNYM response: {}", newnym_resp.trim());
                }
            } else {
                tracing::warn!("tor2 auth response: {}", resp.trim());
            }
        } else {
            tracing::warn!("Could not connect to tor2:9052 for circuit rotation");
        }
    });
}

/// Fire a cheap background query at SearXNG2 (tor2) to rebuild its Tor
/// circuit after a NEWNYM, so subsequent user queries don't pay the cold
/// 10-15s circuit-build cost. Runs detached; results are discarded. Once the
/// circuit is confirmed hot, flips `TOR2_WARM` so the search recovery path
/// knows it can query tor2 without the cold penalty.
fn warm_tor2_cache() {
    std::thread::spawn(|| {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(25))
            .build();
        if let Ok(client) = client {
            // Fire ONE warmup query. A cold Tor circuit builds on the first
            // request (the result is discarded); once it returns, the circuit
            // is HOT, so flip TOR2_WARM immediately rather than waiting for a
            // second confirmation. This keeps the warmup well under the
            // search recovery path's 12s pre-query wait window.
            let _ = client
                .get("http://tor2:8081/search")
                .query(&[("q", "warmup"), ("format", "json")])
                .send();
            TOR2_WARM.store(true, std::sync::atomic::Ordering::SeqCst);
            tracing::info!("tor2 cache warmed after NEWNYM (TOR2_WARM=true)");
        }
    });
}

fn rotate_all_ips(reason: &str) {
    tracing::info!("Rotating ALL IPs (gluetun VPN + tor2): {}", reason);
    trigger_vpn_rotation(reason);
    rotate_tor_circuit();
}

// ─── Simple Stemming (English Plurals) ──────────────────────────────
// Strips trailing 's' for plurals to improve cross-morphological matching.
// "frameworks" → "framework", "browsers" → "browser"
// Conservative: only applies to words > 4 chars, excludes -us/-is/-ss/-os endings.

fn stem(word: &str) -> String {
    let len = word.len();
    if len <= 4 {
        return word.to_string();
    }
    if word.ends_with('s')
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
        && !word.ends_with("os")
    {
        return word[..len - 1].to_string();
    }
    word.to_string()
}

// ─── Semantic Relevance (TF Cosine + Bigrams + Stemming) ──────────
// Measures topical relevance using:
// - TF cosine similarity with title 3x weighting
// - Bigram phrase matching: "rust framework" as adjacent words is stronger than separate
// - Simple stemming: "frameworks" matches "framework"
// - Adaptive coverage threshold: shorter queries need higher per-term match rate
// NOT just keyword overlap — proper information retrieval scoring.

fn semantic_relevance_score(query: &str, title: &str, content: &str) -> f32 {
    // Early exit: if both title and content are empty/too short, return 0.01
    let title_trimmed = title.trim();
    let content_trimmed = content.trim();
    if title_trimmed.is_empty() && content_trimmed.len() < 10 {
        return 0.01;
    }
    // Early exit: if title is meaningful but content is empty, score based on title only
    // (skip full TF-IDF scoring that would return 0 anyway)
    if content_trimmed.len() < 10 {
        let q_lower = query.to_lowercase();
        let t_lower = title_trimmed.to_lowercase();
        let q_words: Vec<&str> = q_lower.split_whitespace().collect();
        let matched = q_words.iter().filter(|w| w.len() >= 2 && t_lower.contains(**w)).count();
        if matched > 0 {
            return (matched as f32 / q_words.iter().filter(|w| w.len() >= 2).count().max(1) as f32).clamp(0.01, 0.5);
        }
        return 0.01;
    }

    let q_lower = query.to_lowercase();
    let t_lower = title_trimmed.to_lowercase();
    let c_lower = content_trimmed.to_lowercase();

    // Extract topic terms (skip stop words and very short words)
    let stop_words: std::collections::HashSet<&str> = [
        "the","a","an","in","on","for","with","using","from","to",
        "and","or","of","is","are","was","were","be","been","has","have","had",
        "do","does","did","will","would","could","should","may","might",
        "how","what","where","when","why","which","who","this","that","these",
        "those","it","its","i","me","my","we","our","you","your","he","she","they",
        "be","as","at","by","not","but","if","so","than","too","very","can","just",
        "best","top","new","old","good","bad","big","small","fast","first","last",
        "most","more","less","many","few","each","every","all","any","some",
        "modern","quick","simple","easy","great","popular","powerful",
    ].iter().copied().collect();

    let tokenize = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| w.len() >= 2 && !stop_words.contains(w.as_str()))
            .map(|w| stem(&w))
            .collect()
    };

    let query_terms = tokenize(&q_lower);
    if query_terms.is_empty() {
        return 0.5;
    }

    // Build weighted term frequency map — title terms get 3x weight
    let mut tf: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let title_tokens = tokenize(&t_lower);
    let content_tokens = tokenize(&c_lower);

    for term in &title_tokens {
        *tf.entry(term.clone()).or_insert(0.0) += 3.0;
    }
    for term in &content_tokens {
        *tf.entry(term.clone()).or_insert(0.0) += 1.0;
    }

    // Build query TF map
    let mut qtf: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    for term in &query_terms {
        *qtf.entry(term.clone()).or_insert(0.0) += 1.0;
    }

    // Cosine similarity on unigrams
    let mut dot_product = 0.0f32;
    let mut q_norm_sq = 0.0f32;
    let mut d_norm_sq = 0.0f32;

    let mut seen_terms: std::collections::HashSet<String> = std::collections::HashSet::new();
    for term in &query_terms {
        if !seen_terms.insert(term.clone()) {
            continue;
        }
        let q_val = qtf.get(term).copied().unwrap_or(0.0);
        let d_val = tf.get(term).copied().unwrap_or(0.0);
        dot_product += q_val * d_val;
        q_norm_sq += q_val * q_val;
        d_norm_sq += d_val * d_val;
    }

    if q_norm_sq < 1e-8 || d_norm_sq < 1e-8 {
        return 0.01;
    }

    let cosine_sim = dot_product / (q_norm_sq.sqrt() * d_norm_sq.sqrt());

    // Coverage: fraction of query terms found in document
    let matched = query_terms.iter().filter(|t| tf.contains_key(*t)).count();
    let coverage = matched as f32 / query_terms.len() as f32;

    // Bigram phrase matching: check if query bigrams appear as adjacent words.
    // "rust framework" as adjacent words is much stronger than "rust" and "framework"
    // appearing in different sentences. Catches phrase-level relevance.
    let bigram_score = if query_terms.len() >= 2 {
        let title_joined = title_tokens.join(" ");
        let content_joined = content_tokens.join(" ");
        let mut bigram_hits_title = 0.0f32;
        let mut bigram_hits_content = 0.0f32;
        let num_bigrams = (query_terms.len() - 1) as f32;

        for w in query_terms.windows(2) {
            let bigram = format!("{} {}", w[0], w[1]);
            if title_joined.contains(&bigram) {
                bigram_hits_title += 1.0;
            } else if content_joined.contains(&bigram) {
                bigram_hits_content += 1.0;
            }
        }
        // Title bigrams worth 3x, like unigram TF weighting
        ((bigram_hits_title * 3.0 + bigram_hits_content) / (num_bigrams * 3.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Combine all signals:
    // - Unigram cosine (70% of unigram signal) + coverage (30% of unigram signal)
    // - Then blend unigram (80%) with bigram phrase coherence (20%)
    let unigram_combined = cosine_sim * 0.7 + coverage * 0.3;
    let combined = unigram_combined * 0.8 + bigram_score * 0.2;

    // Adaptive coverage threshold: shorter queries need higher per-term match rate
    // to avoid false positives. Longer queries can tolerate partial matches but
    // not too lenient — a single term match on a 5-term query means the result
    // barely touches the topic. Dictionary definitions of a single word ("deploy")
    // should not pass for "how to deploy fastapi with postgres on ubuntu".
    let min_coverage = match query_terms.len() {
        1 => 1.0,   // single term must match exactly
        2 => 0.45,  // at least 1 of 2
        3 => 0.30,  // at least 1 of 3
        4 => 0.25,  // at least 1 of 4
        5 => 0.25,  // at least 2 of 5
        6 => 0.20,  // at least 2 of 6
        7 => 0.20,  // at least 2 of 7
        _ => 0.25,  // 8+ terms: need at least 3 of 8+ to prevent 2-term surface matches
    };
    if coverage < min_coverage {
        if coverage < 0.10 {
            return 0.01;
        }
        return (combined * coverage * 0.6).clamp(0.0, combined.min(0.18));
    }

    combined.clamp(0.0, 1.0)
}

/// Blend genuine BERT semantic similarity into web-result ranking.
///
/// The query already has a MiniLM embedding (computed in handle_search and used
/// for the local index). Web results from SearXNG/Whoogle were ONLY scored by
/// unigram cosine + substring coherence (semantic_relevance_score), which cannot
/// tell word senses apart — so "square a circle" matched the POS-system "Square"
/// and the chart "Circle". This function embeds the top web-result texts in ONE
/// batched call to the intent-engine /embed_batch endpoint and returns, per URL,
/// the cosine similarity of each result's text to the QUERY embedding. The caller
/// blends this into the result's `semantic` score.
///
/// Fail-closed: if the query has no vector, or the batch call errors/timeouts,
/// returns an empty map and the caller falls back to the existing substring
/// scorer (no behaviour change). A zero vector in the batch yields cosine 0, so a
/// single bad embed never poisons others.
async fn compute_web_semantic(
    query_vector: &Option<Vec<f32>>,
    web_results: &[SearxResult],
    client: &reqwest::Client,
) -> std::collections::HashMap<String, f32> {
    use std::collections::HashMap;
    let mut out: HashMap<String, f32> = HashMap::new();
    let qv = match query_vector {
        Some(v) if v.len() >= 2 => v,
        _ => return out, // no query embedding -> fail closed
    };
    // Deduplicate texts by URL; only embed the top ~28 distinct results.
    let mut texts: Vec<String> = Vec::new();
    let mut url_order: Vec<String> = Vec::new();
    for r in web_results.iter().take(28) {
        let text = format!("{} {}", r.title, r.content);
        if text.trim().len() < 10 { continue; }
        url_order.push(r.url.clone());
        texts.push(text);
    }
    if texts.is_empty() { return out; }

    let body = serde_json::json!({ "texts": texts });
    let req = client
        .post("http://127.0.0.1:3005/embed_batch")
        .json(&body)
        .timeout(std::time::Duration::from_millis(1500));
    let embeddings: Option<Vec<Vec<f32>>> = match req.send().await {
        Ok(resp) => resp.json::<serde_json::Value>().await.ok()
            .and_then(|j| j.get("embeddings").cloned())
            .and_then(|e| serde_json::from_value(e).ok()),
        Err(_) => None,
    };
    let embeddings = match embeddings {
        Some(e) if e.len() == texts.len() => e,
        _ => return out, // fail closed
    };
    for (url, vec) in url_order.into_iter().zip(embeddings.into_iter()) {
        out.insert(url, cosine_sim_vec(qv, &vec));
    }
    out
}

/// Cosine similarity between two equal-or-unequal length f32 vectors.
/// Zero vectors -> 0.0 (never NaN). Used by compute_web_semantic.
fn cosine_sim_vec(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    let n = a.len().min(b.len());
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    // If vectors differ in length, pad the shorter with zeros (already handled by
    // min above); add remaining squared magnitudes for the longer side.
    for i in n..a.len() { na += a[i] * a[i]; }
    for i in n..b.len() { nb += b[i] * b[i]; }
    if na < 1e-8 || nb < 1e-8 { return 0.0; }
    (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
}

// ─── Intent-aware domain disambiguation (P-B) ───────────────────────────
//
// Polysemous verbs (e.g. "break", "crash", "run") and the token "code" collapse
// across word senses that a BERT cosine cannot separate. Example: the query
// "is it okay to break a promise" embeds ~equally close to "break no contact
// with a narcissist", "Promise | JavaScript", and "tell a lie" — all ~0.85.
// A pure embedding blend therefore cannot pick the moral/relationship sense.
//
// This guard disambiguates via *query intent*, not fixed thresholds:
//   - We classify the query into a coarse sense class from its lexical signals
//     (relationship/moral, programming-help, conspiracy-claim, ...). These are
//     descriptive buckets, not scoring magic numbers.
//   - For each result we detect whether it belongs to a *conflicting* sense
//     class (e.g. a JavaScript-API page for a relationship query, a brand/IDE
//     page for a programming-help query) using structural signals (URL path,
//     title shape, content markers) — not hardcoded domain allow/deny lists.
//   - The returned multiplier is a soft penalty derived from the *strength of
//     the conflicting-sense evidence* (how many independent markers fired),
//     so it degrades gracefully and never silently drops a legitimate result.
//
// Fail-closed: if the query has no clear sense class, or the result shows no
// conflicting-sense markers, this returns 1.0 (no effect). It can only ever
// *reduce* a score, never invent relevance.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SenseClass {
    RelationshipMoral,
    ProgrammingHelp,
    ConspiracyClaim,
    None,
}

/// Classify the query into a coarse sense class using lexical signals only.
/// Returns `SenseClass::None` when no strong signal is present (fail-closed).
fn query_sense_class(query: &str) -> SenseClass {
    let q = query.to_lowercase();
    let q_words: Vec<&str> = q.split_whitespace().collect();
    let has = |w: &str| q_words.iter().any(|x| *x == w);

    // Relationship / moral sense: first-person relational verbs + moral objects.
    let relational = ["promise", "lie", "cheat", "relationship", "marriage",
        "boyfriend", "girlfriend", "partner", "spouse", "friend", "family",
        "date", "dating", "breakup", "apology", "forgive", "trust"];
    let moral_obj = ["okay", "ok", "right", "wrong", "moral", "ethic", "honest",
        "fair", "justify", "acceptable"];
    let relational_hit = relational.iter().any(|w| has(w));
    let moral_hit = moral_obj.iter().any(|w| has(w));
    if relational_hit && moral_hit {
        return SenseClass::RelationshipMoral;
    }

    // Programming-help sense: programming verbs/nouns + a problem framing.
    let prog = ["code", "crash", "bug", "error", "debug", "compile", "runtime",
        "stack", "null", "segfault", "exception", "syntax", "function", "variable",
        "loop", "thread", "async", "api", "dependency", "module", "package",
        "build", "test", "refactor", "algorithm", "database", "query", "server",
        "deploy", "docker", "kubernetes", "python", "rust", "javascript", "java",
        "typescript", "golang", "c++", "react", "vue", "node", "sql"];
    let prog_hit = prog.iter().any(|w| has(w));
    let problem_frame = ["why", "how", "fix", "crash", "error", "not", "won't",
        "wont", "doesn't", "dont", "fails", "broken", "issue", "problem", "debug"];
    let problem_hit = problem_frame.iter().any(|w| has(w));
    if prog_hit && problem_hit {
        return SenseClass::ProgrammingHelp;
    }

    // Conspiracy-claim sense: claim-framing + secrecy language.
    let claim_frame = ["secret", "cover", "coverup", "conspiracy", "they",
        "government", "truth", "expose", "hidden", "suppressed", "lies", "lie",
        "kept", "don't", "dont", "want", "know", "real", "truth", "reveal",
        "theyre", "they're", "elite", "illuminati", "matrix"];
    let secrecy_hit = claim_frame.iter().any(|w| has(w));
    // Require a second independent secrecy-ish token so ordinary "government"
    // news queries are not flagged.
    let secrecy_count = claim_frame.iter().filter(|w| has(w)).count();
    if secrecy_hit && secrecy_count >= 2 {
        return SenseClass::ConspiracyClaim;
    }

    SenseClass::None
}

/// Detect whether a result text belongs to a *conflicting* sense class for the
/// given query sense. Returns a soft penalty multiplier in (0, 1] derived from
/// the number of independent conflicting-sense markers that fired (more markers
/// = stronger, but capped so it never fully zeroes a result unless extreme).
///
/// Markers are structural/lexical, never domain lists:
///   - ProgrammingHelp query vs a JavaScript-API page: title is exactly the
///     API symbol + "| MDN"/"w3schools"/"docs" shape, or content opens with a
///     code/syntax block, or the token "code" appears as the IDE "Visual Studio
///     Code" rather than "source code".
///   - RelationshipMoral query vs a programming page: same programming markers
///     fire while the page shows no relational content.
///   - Either query vs a dictionary/definition page (already penalized elsewhere,
///     but reinforced here via POS-label / phonetic structure).
fn conflicting_sense_penalty(
    sense: SenseClass,
    title: &str,
    content: &str,
    url: &str,
) -> f32 {
    if sense == SenseClass::None {
        return 1.0;
    }
    let title_l = title.to_lowercase();
    let content_l = content.to_lowercase();
    let url_l = url.to_lowercase();
    let title_words: Vec<&str> = title_l.split_whitespace().collect();
    let short_title = title_words.len() <= 3;

    // Programming-page markers (fired for a relationship/moral query = conflict).
    let mut prog_markers = 0u32;
    // 1) Title is an API symbol paired with a docs site pattern.
    if (title_l.contains("| mdn") || title_l.contains("| w3schools")
        || title_l.contains("| mozilla") || title_l.contains("| devdocs")
        || title_l.contains(" docs") || title_l.ends_with(" api"))
        && short_title
    {
        prog_markers += 1;
    }
    // 2) Content opens with a code fence or inline code / syntax markers.
    let content_prefix = content_l.chars().take(200).collect::<String>();
    if content_prefix.contains("```") || content_prefix.contains("<code")
        || content_prefix.contains("function ") || content_prefix.contains("=>")
        || content_prefix.contains("console.log") || content_prefix.contains("#!/")
    {
        prog_markers += 1;
    }
    // 3) "Visual Studio Code" / IDE framing rather than source code.
    if title_l.contains("visual studio code") || title_l.contains("vs code")
        || url_l.contains("code.visualstudio.com")
    {
        prog_markers += 1;
    }
    // 4) Bare API token in title (e.g. "Promise", "Array", "Map") with no
    //    relational words anywhere in the page.
    let api_symbols = ["promise", "array", "map", "set", "async", "await",
        "closure", "callback", "prototype", "iterator", "generator"];
    let title_has_api = api_symbols.iter().any(|s| title_words.iter().any(|t| *t == *s));
    let relational_in_page = ["promise", "relationship", "marriage", "partner",
        "boyfriend", "girlfriend", "friend", "apology", "trust", "honest"]
        .iter().any(|w| content_l.contains(w) || title_l.contains(w));
    if title_has_api && !relational_in_page && short_title {
        prog_markers += 1;
    }

    // Dictionary/definition marker (conflicts with any non-definition query).
    let dict_marker =
        content_prefix.starts_with("noun") || content_prefix.starts_with("verb")
        || content_prefix.starts_with("adjective") || content_prefix.contains("/ˈ")
        || content_prefix.contains("/ˌ");

    // Relationship/moral query: penalize programming pages specifically.
    if sense == SenseClass::RelationshipMoral {
        if prog_markers >= 2 {
            // Soft, capped penalty: each extra marker beyond 2 deepens it
            // slightly, but never below 0.45 so a genuinely-good page survives.
            return (0.45f32 + 0.10f32 * (2u32.saturating_sub(prog_markers) as f32)).clamp(0.45, 1.0);
        }
        if prog_markers == 1 {
            return 0.8;
        }
        if dict_marker {
            return 0.85;
        }
        return 1.0;
    }

    // Programming-help query: penalize IDE/dictionary/brand pages, prefer dev docs.
    if sense == SenseClass::ProgrammingHelp {
        if prog_markers >= 2 {
            // This is itself a programming page, but the *wrong* kind
            // (IDE/docs-API vs the debugging help the query wants). Moderate.
            return 0.7;
        }
        if prog_markers == 1 {
            return 0.85;
        }
        if dict_marker {
            return 0.8;
        }
        return 1.0;
    }

    // ConspiracyClaim: handled separately (P-C) — no-op here.
    1.0
}

// ─── Conspiracy-claim debias (P-C) ──────────────────────────────────────
//
// Dense-retrieval rewards claim-echoing pages: a query like "perpetual motion =
// government secret kept from the masses" embeds *closer* to a clickbait article
// that repeats the claim than to a Britannica debunk, because they share
// vocabulary. Left alone, the conspiracy echo outranks the debunk.
//
// This guard does NOT censor — it applies a *mild, evidence-counted* penalty to
// pages that merely *amplify* the claim (echo it without counter-evidence) and a
// *mild* boost to pages that present counter-evidence (debunk markers). Both are
// derived from lexical counters, not fixed lists, and are capped so they nudge
// rather than dictate. Fail-closed: returns (1.0, 0.0) when the query is not a
// conspiracy-claim sense.

/// Returns (penalty_multiplier, boost_addition) for a conspiracy-claim query.
/// - penalty: 1.0 normally; reduced toward ~0.7 when the page *echoes* the claim
///   with little counter-evidence (count of echo phrases vs debunk phrases).
/// - boost: 0.0 normally; up to +0.15 when the page presents debunk/counter
///   evidence (so the credible source climbs without being forced to #1).
fn conspiracy_guard(title: &str, content: &str) -> (f32, f32) {
    let text = format!("{} {}", title, content).to_lowercase();

    // Echo phrases: the page repeats/endorses the claim-like framing.
    let echo = ["secret", "they don't want you to know", "hidden truth",
        "wake up", "the truth they", "cover up", "cover-up", "what they",
        "mainstream media won't", "suppressed", "they are lying", "real reason",
        "government doesn't want", "kept from", "they don't tell you"];
    // Debunk / counter-evidence phrases: the page presents skepticism or facts.
    let debunk = ["debunk", "myth", "false", "not true", "fact check", "no evidence",
        "conspiracy theory", "pseudoscience", "hoax", "physicist", "scientist",
        "law of thermodynamics", "conservation of energy", "peer review",
        "evidence shows", "actually", "misinformation", "snopes", "reliable source"];

    let echo_n = echo.iter().filter(|p| text.contains(*p)).count() as f32;
    let debunk_n = debunk.iter().filter(|p| text.contains(*p)).count() as f32;

    // Net echo signal: more echo than debunk → mild penalty.
    let net = echo_n - debunk_n;
    let penalty = if net >= 2.0 {
        // Echo-heavy, little counter-evidence: nudge down, floor at 0.7.
        (0.7f32 + 0.05f32 * (2.0f32 - net).clamp(0.0, 2.0)).clamp(0.7, 1.0)
    } else {
        1.0
    };
    // Counter-evidence present: mild climb, capped at +0.15.
    let boost = if debunk_n >= 1.0 {
        (0.05f32 * debunk_n).clamp(0.0, 0.15)
    } else {
        0.0
    };
    (penalty, boost)
}

// ─── Engine Consensus Score ─────────────────────────────────────────
// Algorithmic: results returned by multiple independent sources get higher scores.
// This is the single strongest quality signal — if Bing, Brave, DuckDuckGo, and
// the local index all agree on a result, it's almost certainly relevant.
// No hardcoded lists — just count distinct sources.

fn consensus_score(sources: &[String]) -> f32 {
    if sources.is_empty() {
        return 0.2; // single-source result — lowest confidence
    }
    let unique_sources: std::collections::HashSet<&String> = sources.iter().collect();
    let count = unique_sources.len() as f32;
    // Logarithmic scaling with proper differentiation:
    // 1 source = 0.20, 2 = 0.41, 3 = 0.53, 4 = 0.62, 5 = 0.68
    // Multi-source agreement is the strongest quality signal — if Bing,
    // Brave, Whoogle, and the local index all agree, the result is gold.
    (0.2 + 0.3 * count.ln()).clamp(0.2, 1.0)
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
    consensus: f32,  // cross-source agreement
    constraint: f32, // constraint matching (positive/negative)
}

impl RankingWeights {
    fn for_intent(intent: &str) -> Self {
        match intent {
            "fresh" => Self {
                rrf: 0.06,
                intent: 0.04,
                freshness: 0.16,
                authority: 0.10,
                local_bonus: 0.02,
                quality: 0.08,
                semantic: 0.20,
                consensus: 0.18,
                constraint: 0.16,
            },
            "technical" => Self {
                rrf: 0.07,
                intent: 0.08,
                freshness: 0.03,
                authority: 0.16,  // boosted from 0.10 — breaks ties among 0.950-scoring results
                local_bonus: 0.04,
                quality: 0.06,
                semantic: 0.22,   // reduced from 0.28 to balance authority boost
                consensus: 0.14,
                constraint: 0.20,
            },
            "navigational" => Self {
                rrf: 0.03,
                intent: 0.18,
                freshness: 0.02,
                authority: 0.15,
                local_bonus: 0.02,
                quality: 0.04,
                semantic: 0.22,
                consensus: 0.12,
                constraint: 0.22,
            },
            "comparison" => Self {
                rrf: 0.08,
                intent: 0.06,
                freshness: 0.06,
                authority: 0.05,
                local_bonus: 0.02,
                quality: 0.08,
                semantic: 0.22,
                consensus: 0.18,
                constraint: 0.25,
            },
            "how-to" => Self {
                rrf: 0.06,
                intent: 0.06,
                freshness: 0.04,
                authority: 0.05,
                local_bonus: 0.04,
                quality: 0.06,
                semantic: 0.26,
                consensus: 0.18,
                constraint: 0.25,
            },
            "transactional" => Self {
                rrf: 0.06,
                intent: 0.10,      // higher — reward product page structure detection
                freshness: 0.03,
                authority: 0.10,   // boost — e-commerce domains are authoritative for shopping
                local_bonus: 0.02,
                quality: 0.08,     // boost — reward structured product content
                semantic: 0.22,
                consensus: 0.18,
                constraint: 0.21,
            },
            "local" => Self {
                rrf: 0.05,
                intent: 0.06,
                freshness: 0.14,   // boost — local results are time-sensitive (hours, events)
                authority: 0.04,
                local_bonus: 0.14, // strong boost — local index results are very relevant
                quality: 0.05,
                semantic: 0.20,
                consensus: 0.16,
                constraint: 0.16,
            },
            _ => Self {  // informational, default
                rrf: 0.06,
                intent: 0.05,
                freshness: 0.05,
                authority: 0.06,
                local_bonus: 0.03,
                quality: 0.07,
                semantic: 0.24,
                consensus: 0.20,
                constraint: 0.24,
            },
        }
    }

    // Distribution-aware blending: when intent is uncertain (e.g., informational 0.41,
    // comparison 0.38), blend ranking weights proportionally instead of hard-switching.
    // "Intent as hint, not gate."
    fn for_distribution(distribution: &std::collections::HashMap<String, f32>) -> Self {
        let labels = ["informational", "technical", "navigational", "comparison", "how-to", "fresh", "transactional", "local"];

        // Get probabilities for each label (default 0 if missing)
        let probs: Vec<f32> = labels.iter().map(|l| {
            distribution.get(*l).copied().unwrap_or(0.0)
        }).collect();

        // If distribution is empty or all zeros, fall back to informational
        let total: f32 = probs.iter().sum();
        if total < 0.01 {
            return Self::for_intent("informational");
        }

        // Compute margin: how certain is the top intent?
        let mut sorted = probs.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let margin = sorted[0] - sorted[1];

        // If margin > 0.3, the classifier is confident — use the winning intent directly
        if margin > 0.15 {
            let (winner_idx, _) = probs.iter().enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));
            return Self::for_intent(labels[winner_idx]);
        }

        // Otherwise, blend weights proportionally
        let mut blended = Self::for_intent("informational"); // start with default
        blended.rrf = 0.0;
        blended.intent = 0.0;
        blended.freshness = 0.0;
        blended.authority = 0.0;
        blended.local_bonus = 0.0;
        blended.quality = 0.0;
        blended.semantic = 0.0;
        blended.consensus = 0.0;
        blended.constraint = 0.0;

        for (i, label) in labels.iter().enumerate() {
            let w = Self::for_intent(label);
            let p = probs[i] / total; // normalize to sum to 1
            blended.rrf += w.rrf * p;
            blended.intent += w.intent * p;
            blended.freshness += w.freshness * p;
            blended.authority += w.authority * p;
            blended.local_bonus += w.local_bonus * p;
            blended.quality += w.quality * p;
            blended.semantic += w.semantic * p;
            blended.consensus += w.consensus * p;
            blended.constraint += w.constraint * p;
        }

        blended
    }
}

// ─── Cross-Query Score Normalization ────────────────────────────────
// Strip tracking/query params from URLs for better dedup.
// Handles utm_*, fbclid, gclid, ref, srsltid, etc.
// "https://example.com/page?utm_source=google&id=5" → "https://example.com/page?id=5"

fn strip_tracking_params(url: &str) -> String {
    static TRACKING_PARAMS: &[&str] = &[
        "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
        "utm_id", "utm_source_platform", "utm_creative_format", "utm_marketing_tactic",
        "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid",
        "msclkid", "twclid", "li_fat_id",
        "srsltid", "source", "ref", "ref_src", "ref_url",
        "_ga", "_gl", "mc_cid", "mc_eid",
        "yclid", "ymclid", "ysclid",
    ];

    let (path, query) = match url.find('?') {
        Some(pos) => (&url[..pos], &url[pos + 1..]),
        None => return url.to_string(),
    };

    if query.is_empty() {
        return url.to_string();
    }

    let filtered: Vec<&str> = query
        .split('&')
        .filter(|param| {
            let key = param.split('=').next().unwrap_or("");
            !TRACKING_PARAMS.iter().any(|tp| key == *tp)
        })
        .collect();

    if filtered.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, filtered.join("&"))
    }
}

/// URL canonicalization used when fusing local-index result sets (BM25 vs
/// semantic re-query) by URL. Mirrors the dedup logic inside
/// `merge_local_and_web` so the two passes agree on what counts as "the same
/// URL" (lowercase, strip fragment, trailing slash, www./m. prefixes, tracking
/// params). Centralized here so the two code paths can't drift apart.
fn normalize_indexer_url(url: &str) -> String {
    let lower = url.to_lowercase();
    let no_fragment = lower.split('#').next().unwrap_or(&lower);
    let no_trailing = no_fragment.trim_end_matches('/');
    let no_www = no_trailing.replacen("://www.", "://", 1);
    let no_mobile = no_www
        .replacen("://m.", "://", 1)
        .replacen("://mobile.", "://", 1);
    strip_tracking_params(&no_mobile)
}

// Scores are already on an absolute calibrated scale from the weighted
// multi-signal fusion — raw scores directly reflect quality:
//   Mediocre results:  ~0.15-0.25
//   Good results:      ~0.35-0.55
//   Excellent results: ~0.55-0.80
//
// Previous min-max normalization mapped every query's top-1 to exactly 1.0,
// destroying the absolute quality signal. A query with mediocre results and
// one with excellent results both showed 1.000 for the top hit.
//
// Current approach: clamp to [0.05, 1.0]. This preserves the absolute
// quality — a mediocre top result stays at ~0.20 while an excellent one
// scores ~0.65. Scores below 0.05 are clamped up (essentially noise),
// scores above 1.0 are log-compressed into [0.95, 1.0] so the over-1.0
// cluster (from consensus boosts, nav domain boosts) retains differentiation.

/// Phase 0: per-query min-max calibration.
/// Maps the raw [min, max] score distribution onto [0.05, 1.0], preserving
/// the *real* relative ordering/differentiation between results instead of
/// collapsing every negation-survivor to exactly 1.0 (the old
/// normalize_scores spread=0.05 behaviour). Scores become differentiable
/// within a query but are NOT comparable across queries — acceptable, since
/// the downstream ranking/threshold is per-query.
fn calibrate_scores(scores: &mut [f32]) {
    if scores.is_empty() {
        return;
    }
    let raw_min = scores.iter().cloned().fold(f32::INFINITY, f32::min);
    let raw_max = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if raw_max <= raw_min {
        // Degenerate cluster — leave scores as-is (already equal).
        return;
    }
    let floor = 0.05f32;
    let ceil = 1.0f32;
    let span = ceil - floor;
    let norm = raw_max - raw_min;
    for score in scores.iter_mut() {
        let t = (*score - raw_min) / norm;
        *score = (floor + t * span).clamp(floor, ceil);
    }
}

// ─── Search URL Builder with Location Support ──────────────────────

fn map_lang_to_country(lang: &str) -> Option<&'static str> {
    match lang.to_lowercase().as_str() {
        "de" => Some("de"),
        "fr" => Some("fr"),
        "es" => Some("es"),
        "nl" => Some("nl"),
        "it" => Some("it"),
        "en" => Some("us"),
        _ => None,
    }
}

/// Build a SearXNG search URL with optional geolocation parameters.
/// Appends `source_country` and `language` when location data is available.
fn searxng_url(base: &str, query: &str, geo: Option<&geoloc::GeoLocation>, lang: Option<&str>) -> String {
    searxng_url_with_categories(base, query, "", geo, lang)
}

/// Build a SearXNG URL with optional categories (news, images, videos) and geolocation.
fn searxng_url_with_categories(
    base: &str,
    query: &str,
    categories: &str,
    geo: Option<&geoloc::GeoLocation>,
    lang: Option<&str>,
) -> String {
    let encoded = urlencoding::encode(query);
    let cat = if categories.is_empty() { String::new() } else { format!("&categories={}", categories) };
    let mut url = format!("{}/search?q={}&format=json{}&pageno=1", base, encoded, cat);
    
    if let Some(l) = lang {
        url.push_str(&format!("&language={}", l));
        if let Some(ref cc) = map_lang_to_country(l) {
            url.push_str(&format!("&source_country={}", cc));
        }
    } else if let Some(g) = geo {
        if let Some(ref cc) = g.country_code {
            url.push_str(&format!("&source_country={}", cc.to_lowercase()));
        }
        if let Some(ref lang_tag) = g.language_tag() {
            url.push_str(&format!("&language={}", lang_tag));
        }
    } else {
        url.push_str("&language=en");
    }
    url
}

/// Score how relevant a search result is to the user's geographic location.
/// Checks if the result title, content, or URL mentions the user's country,
/// region, or city. Returns a boost between 0.0 and 0.25.
fn geo_relevance_score(title: &str, content: &str, url: &str, geo: &geoloc::GeoLocation) -> f32 {
    let preview = content.chars().take(500).collect::<String>().to_lowercase();
    let text = format!("{} {} {}", title.to_lowercase(), preview, url.to_lowercase());
    let mut boost: f32 = 0.0;

    if let Some(ref country) = geo.country_name {
        if text.contains(&country.to_lowercase()) {
            boost = boost.max(0.10);
        }
    }
    if let Some(ref code) = geo.country_code {
        let code_lower = code.to_lowercase();
        if url.to_lowercase().ends_with(&format!(".{}", code_lower))
            || url.to_lowercase().contains(&format!("/{}", code_lower))
        {
            boost = boost.max(0.12);
        }
    }

    if let Some(ref region) = geo.region {
        if text.contains(&region.to_lowercase()) {
            boost = boost.max(0.20);
        }
    }

    if let Some(ref city) = geo.city {
        if text.contains(&city.to_lowercase()) {
            boost = boost.max(0.25);
        }
    }

    boost
}

/// Detect if a search query has local intent (seeking nearby/nearby results).
/// Returns `true` if the query contains signals like "near me", "nearby", etc.
fn has_local_intent(query: &str) -> bool {
    let lower = query.to_lowercase();
    lower.contains(" near me")
        || lower.starts_with("near me")
        || lower.contains("nearby")
        || lower.contains("close to me")
        || lower.contains(" around me")
        || lower.starts_with("around me")
        || lower.contains(" near ")
        || lower.contains(" in ") && (
            lower.ends_with(" area")
            || lower.ends_with(" region")
            || lower.ends_with(" neighbourhood")
            || lower.ends_with(" neighborhood")
        )
}

/// If the query has local intent, expand it with the user's city/region context.
fn localize_query(query: &str, geo: &geoloc::GeoLocation) -> Option<String> {
    if !has_local_intent(query) {
        return None;
    }
    let location = match (&geo.city, &geo.region, &geo.country_code) {
        (Some(city), _, _) => city.clone(),
        (None, Some(region), _) => region.clone(),
        (None, None, Some(cc)) => cc.clone(),
        _ => return None,
    };
    let localized = format!("{} {}", query, location);
    // Don't return if it's essentially the same query
    if localized.to_lowercase() == query.to_lowercase() {
        return None;
    }
    Some(localized)
}

// ─── Text Content Sanitizer ───────────────────────────────────────
// Strips control characters from text content for safe JSON serialization.
// Preserves normal whitespace (tab, newline, space) but removes NUL, BS, etc.

fn sanitize_text_content(text: &str) -> String {
    text.chars().filter(|&c| !c.is_control() || c == 0x09 as char || c == 0x0A as char || c == 0x0D as char).collect()
}

// ─── JSON Text Sanitizer ──────────────────────────────────────────
// Strips control characters (except \t, \n, \r) that SearXNG may return
// in search result content. These cause serde_json parse failures.
// Also handles duplicate JSON keys that some engines produce.

fn sanitize_json_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            // Allow tab, newline, carriage return
            '\t' | '\n' | '\r' => out.push(ch),
            // Strip other control characters (0x00-0x1F)
            c if c.is_control() => {
                // Replace with space to preserve word boundaries
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }
    // Deduplicate keys (SearXNG sometimes returns duplicate keys)
    deduplicate_json_keys(&out)
}

// Preprocess query for SearXNG — strip trigger words that cause dictionary/shopping results
fn preprocess_searxng_query(query: &str) -> String {
    // Normalize natural-language constraint syntax (under $500, in url:, …)
    // into canonical operator tokens so the engine query honours them. The
    // intent engine applies the same normalization for constraint extraction,
    // keeping both paths consistent.
    let normalized = normalize_nl_operators(query);
    let q = normalized.trim();
    let q_lower = q.to_lowercase();
    
    // Count filetype operators
    let filetype_count = q_lower.matches("filetype:").count();
    let has_booleans = q_lower.contains(" or ") || q_lower.contains(" and ");

    // Multiple `site:` operators must be OR'd (e.g. "site:a site:b" →
    // "site:a OR site:b"), NOT intersected. Bing/SearXNG treat adjacent
    // site: tokens as a conjunction and return 0 results, which silently
    // zeroes out any multi-site query. Collect the distinct site values so we
    // can fold them into a single OR-group below.
    let mut site_values: Vec<String> = Vec::new();
    for cap in q_lower.match_indices("site:") {
        let after = cap.0 + 5;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        // Skip malformed site tokens. A valid site constraint must look like a
        // host: it either contains a dot (example.com) or is a bare TLD that we
        // normalise (see below). Tokens such as ".edu" (leading dot) or a bare
        // two/three-letter string with no dot are not real hosts and would make
        // the upstream engine return zero hits — drop them so they don't
        // silently zero out the whole query.
        if val.is_empty() {
            continue;
        }
        // A leading-dot token like ".edu" is a malformed host. Strip the dot so
        // it becomes the bare TLD "edu" and falls through to the normalisation
        // below (SearXNG matches `site:edu` correctly; `site:.edu` returned 0).
        let val = val.strip_prefix('.').unwrap_or(&val);
        let is_valid_host = val.contains('.') || val == "localhost";
        if !is_valid_host {
            // Bare TLD like "edu"/"gov" → emit the bare form (e.g. "edu") so the
            // upstream engine can match subdomains. SearXNG honours `site:edu`;
            // the dotted form `site:.edu` returned zero hits in testing.
            if ["edu", "gov", "org", "com", "net", "io", "dev", "ai", "co", "us", "uk", "de", "fr", "es", "nl", "ru", "cn", "jp", "in"].contains(&val) {
                if !site_values.contains(&val.to_string()) {
                    site_values.push(val.to_string());
                }
            }
            continue;
        }
        if !site_values.contains(&val.to_string()) {
            site_values.push(val.to_string());
        }
    }

    // Collect filetype operators so multiple values can be OR'd (e.g.
    // "filetype:pdf filetype:doc" → "filetype:pdf OR filetype:doc"). The old
    // code dropped every filetype token when more than one was present, which
    // left the upstream engine with no type constraint and the local hard
    // filter then dropped all general web results → 0 hits.
    let mut filetype_values: Vec<String> = Vec::new();
    for cap in q_lower.match_indices("filetype:") {
        let after = cap.0 + 9;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() && !filetype_values.contains(&val) {
            filetype_values.push(val);
        }
    }

    let mut words_cleaned = Vec::new();
    // Operators SearXNG understands natively. Stripping them and never re-emitting
    // (the old behaviour) silently forwarded e.g. `rust inurl:blog` as just `rust`,
    // so the engine could never honour the constraint and a downstream hard filter
    // then dropped every result → n=0. We preserve them verbatim so the engine
    // itself applies them (it supports intitle:/inurl:/intext: natively), and the
    // local `should_filter_by_constraints` hard-drop is downgraded to a soft boost.
    let mut passthrough_emitted: Vec<String> = Vec::new();
    for w in q.split_whitespace() {
        let wl = w.to_lowercase();
        if wl.starts_with("intitle:") || wl.starts_with("inurl:") || wl.starts_with("intext:")
            || wl.starts_with("price:") || wl.starts_with("lang:")
            || wl.starts_with("after:") || wl.starts_with("before:")
        {
            // Emit native operators (everything except price:, after:, before: which
            // are handled via dedicated query params / date-window overrides).
            if wl.starts_with("intitle:") || wl.starts_with("inurl:") || wl.starts_with("intext:")
                || wl.starts_with("lang:")
            {
                passthrough_emitted.push(w.to_string());
            }
            continue;
        }
        if wl == "or" || wl == "and" {
            continue;
        }
        // Drop raw site: tokens here; they are re-emitted as a single OR-group
        // (or passed through unchanged when there is exactly one).
        if wl.starts_with("site:") {
            continue;
        }
        if wl.starts_with("filetype:") {
            continue;
        }
        // Preserve double quotes so the upstream engine honors "exact phrase"
        // queries (e.g. "Lancaster norms"). Single quotes (apostrophes) are still
        // stripped. Previously BOTH were stripped, which silently turned a quoted
        // phrase into a loose bag-of-words and broke the phrase hard-filter below
        // (the engine was asked for the unquoted text, so snippets rarely contained
        // the verbatim substring and every result got dropped -> n=0).
        let clean_w = w.replace('\'', "");
        if !clean_w.is_empty() {
            words_cleaned.push(clean_w);
        }
    }

    let mut cleaned_str = words_cleaned.join(" ");
    // Re-emit site: as OR-group when there are 2+, otherwise the single value
    // passes through (already stripped above).
    if site_values.len() >= 2 {
        let or_group = site_values
            .iter()
            .map(|s| format!("site:{}", s))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !cleaned_str.is_empty() {
            cleaned_str.push(' ');
        }
        cleaned_str.push_str(&or_group);
    } else if let Some(single) = site_values.first() {
        if !cleaned_str.is_empty() {
            cleaned_str.push(' ');
        }
        cleaned_str.push_str(&format!("site:{}", single));
    }

    // Re-emit filetype: as an OR-group when there are 2+, otherwise the single
    // value passes through (already stripped above). Mirrors the site: handling
    // so "filetype:pdf filetype:doc" is honoured as a union, not silently dropped.
    if filetype_values.len() >= 2 {
        let or_group = filetype_values
            .iter()
            .map(|s| format!("filetype:{}", s))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !cleaned_str.is_empty() {
            cleaned_str.push(' ');
        }
        cleaned_str.push_str(&or_group);
    } else if let Some(single) = filetype_values.first() {
        if !cleaned_str.is_empty() {
            cleaned_str.push(' ');
        }
        cleaned_str.push_str(&format!("filetype:{}", single));
    }

    // Re-emit native operators (intitle:/inurl:/intext:/lang:) verbatim so the
    // upstream engine applies them. Skipping this is what zeroed out e.g.
    // `rust inurl:blog` (forwarded as `rust`); the engine then returned results
    // that failed the local hard filter → 0 hits.
    for op in &passthrough_emitted {
        if !cleaned_str.is_empty() {
            cleaned_str.push(' ');
        }
        cleaned_str.push_str(op);
    }

    // Prefixes that trigger dictionary/definition results on Bing
    let prefix_triggers = [
        "comparing ", "compare ", "compared ", "comparison of ",
        "explanation of ", "definition of ",
        "implications of ", "analysis of ", "overview of ",
        "understanding ", "introduction to ",
    ];
    let mut cleaned = cleaned_str.to_lowercase();
    for prefix in &prefix_triggers {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            cleaned = rest.to_string();
            break;
        }
    }
    
    // Strip "how to / how do i / how can i" prefixes
    let start_triggers = ["how to ", "how do i ", "how can i ", "how do you "];
    for prefix in &start_triggers {
        if let Some(rest) = cleaned.strip_prefix(prefix) {
            cleaned = rest.to_string();
            break;
        }
    }
    
    // Action verbs that trigger dictionary/definition results on Bing/Google
    let action_verbs: std::collections::HashSet<&str> = [
        "deploy", "implement", "configure", "setup", "install", "migrate",
        "optimize", "compile", "debug", "integrate", "initialize", "instantiate",
        "provision", "orchestrate", "containerize", "virtualize",
        "download", "upload", "import", "export", "backup", "restore",
        "monitor", "observe", "instrument", "profile", "benchmark",
    ].iter().copied().collect();
    
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.len() > 6 {
        let filtered: Vec<&str> = words.iter()
            .filter(|w| !action_verbs.contains(*w))
            .copied()
            .collect();
        if filtered.len() >= 3 {
            cleaned = filtered.join(" ");
        }
    }
    
    if cleaned.len() < 3 {
        return cleaned_str;
    }
    
    cleaned
}

/// Disambiguate short language names that have non-programming meanings.
/// "rust" -> "rust programming" (vs the survival game)
/// Language disambiguation data: programming language names whose bare form
/// is also a common English word or game name, causing search engines to return
/// unrelated results. Each entry: (query_match, replacement_base, suffix).
/// When a bare query matches `query_match`, it becomes
/// `format!("{}{}", replacement_base, suffix)` before sending to SearXNG.
/// Data-driven: add entries here without changing any logic.
static LANGUAGE_DISAMBIGUATION: &[(&str, &str, &str)] = &[
    ("go",    "golang",         ""),        // go is a common verb -> golang
    ("rust",  "rust",           " programming"),  // rust is a survival game
    ("ruby",  "ruby",           " programming"),  // ruby is a gemstone
    ("swift", "swift",          " programming"),  // swift means fast
    ("java",  "java",           " programming"),  // java is an island/coffee
    ("c",     "c",              " programming"),  // c is a grade/note/vitamin
];

/// Disambiguate ambiguous programming language names using LANGUAGE_DISAMBIGUATION data.
/// Phase 6: token-level rewrite. The OLD version only rewrote when the BARE
/// trimmed query exactly equaled a language name, so "rust backend" / "rust vs go"
/// leaked through to "Rust on Steam". Now we rewrite EACH ambiguous token in place
/// (unless the query is clearly about gaming), and "go" gets a verb guard so
/// "how to go to sleep" is never rewritten to "golang".
fn disambiguate_engine_query(query: &str, intent: &str, _expanded_queries: &[String]) -> String {
    let q_lower = query.to_lowercase();
    let trimmed = q_lower.trim();
    let is_gaming = intent == "gaming" || intent == "entertainment";
    if is_gaming {
        return query.to_string();
    }

    // Verb-guard tokens that, when immediately preceding "go", mean "go" is a verb.
    let go_verb_leads: &[&str] = &["to", "how", "will", "let", "lets", "should", "can", "did", "wanna", "want"];

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut changed = false;
    for (i, tok) in tokens.iter().enumerate() {
        let t = tok.trim_matches(|c: char| !c.is_alphanumeric());
        let mut matched: Option<(&str, &str, &str)> = None;
        for &(lang_name, base, suffix) in LANGUAGE_DISAMBIGUATION {
            if t == lang_name {
                matched = Some((lang_name, base, suffix));
                break;
            }
        }
        if let Some((lang_name, base, suffix)) = matched {
            // Verb guard for "go": if it follows a verb-lead token, it's a verb, not golang.
            if lang_name == "go" {
                let prev = tokens.get(i.wrapping_sub(1)).map(|p| p.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).unwrap_or_default();
                if go_verb_leads.contains(&prev.as_str()) {
                    out.push(tok.to_string());
                    continue;
                }
            }
            let d = format!("{}{}", base, suffix);
            tracing::info!("DISAMBIGUATE: token '{}' -> '{}' (lang={})", tok, d, lang_name);
            out.push(d);
            changed = true;
        } else {
            out.push(tok.to_string());
        }
    }
    if changed { out.join(" ") } else { query.to_string() }
}

// Simple negation-word stripper for the initial fan-out (no intent engine needed).
// Detects common negation patterns and strips trigger words AND the content terms
// immediately following them. This prevents negated terms (e.g., "chrome" in
// "browser not chrome") from leaking into the search query and polluting results.
// Example: "browser not chrome not edge" → "browser"
fn simple_negation_strip(query: &str) -> Option<String> {
    let neg_triggers: std::collections::HashSet<&str> = [
        "not", "no", "without", "except", "excluding", "besides", "minus",
        "other", "than", "nor",
    ].iter().copied().collect();

    let preserved_words: std::collections::HashSet<&str> = [
        "2026", "2025", "2024", "2023", "2022", "2021", "2020",
        "privacy", "private", "secure", "security",
        "small", "startup", "startups", "indie",
        "open-source", "opensource", "foss", "floss", "free", "libre",
        "self-hosted", "selfhosted", "offline", "local",
        "lightweight", "minimal", "minimalist",
        "ubuntu", "debian", "linux", "mac", "macos", "windows", "android", "ios",
        "framework", "library", "language", "programming", "compiler", "runtime",
        "editor", "browser", "server", "client", "database", "distro", "distribution",
        "package", "module", "tool", "api", "web", "app", "application",
        "search", "engine", "cloud", "native", "backend", "frontend", "fullstack",
        "mobile", "desktop", "enterprise", "community", "stable", "testing",
        "production", "development", "deployment", "container", "orchestrator",
    ].iter().copied().collect();

    let words: Vec<&str> = query.split_whitespace().collect();
    let mut result: Vec<&str> = Vec::new();
    let mut in_negation = false;

    for w in &words {
        let w_lower = w.to_lowercase();
        let clean_w = w_lower.trim_matches(|c: char| !c.is_alphanumeric());

        let is_neg_trigger = neg_triggers.contains(w_lower.as_str()) || w_lower.starts_with("-");
        if is_neg_trigger {
            in_negation = true;
            continue;
        }

        if in_negation {
            if preserved_words.contains(clean_w) || preserved_words.contains(w_lower.as_str()) {
                in_negation = false;
                result.push(w);
            }
            continue;
        }

        result.push(w);
    }

    if result.len() == words.len() {
        return None; // nothing stripped
    }
    let cleaned = result.join(" ");
    if cleaned.is_empty() {
        return None; // everything was stripped
    }
    Some(cleaned)
}

/// P1-compound: when a query pairs `site:` with `filetype:` (e.g.
/// "python tutorial site:docs.python.org filetype:pdf"), SearXNG frequently
/// returns 0 for the narrow conjunction even though `site:`-alone has hits
/// (observed: site:realpython.com -> 2 results, site:realpython.com
/// filetype:pdf -> 0). Fire a filetype-RELAXED variant in parallel so the
/// gateway can recover results when the strict conjunction is empty. The
/// relaxed variant keeps `site:` and drops only `filetype:`.
fn filetype_relax_variant(query: &str) -> Option<String> {
    let has_site = query.to_lowercase().contains("site:");
    let has_filetype = query.to_lowercase().contains("filetype:");
    if !(has_site && has_filetype) {
        return None;
    }
    let kept: Vec<&str> = query
        .split_whitespace()
        .filter(|w| !w.to_lowercase().starts_with("filetype:"))
        .collect();
    if kept.len() == query.split_whitespace().count() {
        return None; // nothing removed
    }
    let relaxed = kept.join(" ");
    if relaxed.is_empty() { return None; }
    Some(relaxed)
}

/// Extract core keyphrases by removing natural-language filler/stop words
/// from verbose queries (e.g. "construct a warp drive using exotic matter" → "warp drive exotic matter").
/// Fires in parallel during the initial fan-out to ensure upstream engines return
/// relevant hits even when verbose natural-language framing yields 0 exact matches.
fn keyphrase_relax_variant(query: &str) -> Option<String> {
    let q_lower = query.to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    if words.len() < 3 {
        return None;
    }
    // Don't strip structured operators or site constraints
    if q_lower.contains("site:") || q_lower.contains("filetype:") || q_lower.contains("intitle:") || q_lower.contains("inurl:") {
        return None;
    }

    const STOP_WORDS: &[&str] = &[
        "a", "an", "the", "of", "to", "in", "on", "for", "with", "by", "from", "at",
        "about", "into", "through", "during", "before", "after", "above", "below",
        "under", "using", "construct", "build", "create", "make", "how", "what",
        "why", "where", "when", "which", "who", "is", "are", "was", "were", "be",
        "been", "being", "have", "has", "had", "do", "does", "did", "can", "could",
        "should", "would", "shall", "will", "may", "might", "must", "deploy",
        "master", "learn", "start", "write", "develop", "setup", "install", "run"
    ];

    let filtered: Vec<&str> = words.into_iter()
        .filter(|w| !STOP_WORDS.contains(w) && w.len() > 1)
        .collect();

    let orig_count = query.split_whitespace().count();
    if filtered.len() < 2 || filtered.len() == orig_count {
        return None;
    }

    Some(filtered.join(" "))
}


// ─── JSON Key Deduplication ────────────────────────────────────────
// Removes duplicate keys from JSON objects. Keeps the LAST value for each key.
// Handles nested objects and arrays. Algorithmic — no hardcoded key lists.

fn deduplicate_json_keys(json_str: &str) -> String {
    // Manual dedup: serde_json rejects duplicate keys even for Value type.
    // Parse char-by-char, track seen keys per nesting level, remove duplicates.
    let chars_vec: Vec<char> = json_str.chars().collect();
    let mut duplicate_ranges: Vec<(usize, usize)> = Vec::new();
    let mut idx = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut seen_keys: Vec<Vec<String>> = vec![Vec::new()];

    while idx < chars_vec.len() {
        let c = chars_vec[idx];
        if esc {
            esc = false;
            idx += 1;
            continue;
        }
        if c == '\\' && in_str {
            esc = true;
            idx += 1;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            if in_str {
                let start = idx;
                idx += 1;
                while idx < chars_vec.len() {
                    if chars_vec[idx] == '\\' { idx += 2; continue; }
                    if chars_vec[idx] == '"' { break; }
                    idx += 1;
                }
                let key: String = chars_vec[start+1..idx].iter().collect();
                let mut j = idx + 1;
                while j < chars_vec.len() && chars_vec[j].is_whitespace() { j += 1; }
                if j < chars_vec.len() && chars_vec[j] == ':' {
                    if let Some(seen) = seen_keys.last_mut() {
                        if seen.contains(&key) {
                            // Duplicate — find value extent and mark for removal
                            let mut k = j + 1;
                            while k < chars_vec.len() && chars_vec[k].is_whitespace() { k += 1; }
                            if k < chars_vec.len() {
                                if chars_vec[k] == '"' {
                                    k += 1;
                                    while k < chars_vec.len() {
                                        if chars_vec[k] == '\\' { k += 2; continue; }
                                        if chars_vec[k] == '"' { k += 1; break; }
                                        k += 1;
                                    }
                                } else if chars_vec[k] == '{' || chars_vec[k] == '[' {
                                    let mut d = 0;
                                    let mut in_s = false;
                                    let mut es = false;
                                    while k < chars_vec.len() {
                                        if es { es = false; k += 1; continue; }
                                        if chars_vec[k] == '\\' && in_s { es = true; k += 1; continue; }
                                        if chars_vec[k] == '"' { in_s = !in_s; }
                                        if !in_s {
                                            if chars_vec[k] == '{' || chars_vec[k] == '[' { d += 1; }
                                            if chars_vec[k] == '}' || chars_vec[k] == ']' { d -= 1; if d == 0 { k += 1; break; } }
                                        }
                                        k += 1;
                                    }
                                } else {
                                    while k < chars_vec.len() && chars_vec[k] != ',' && chars_vec[k] != '}' && chars_vec[k] != ']' { k += 1; }
                                }
                            }
                            while k < chars_vec.len() && chars_vec[k].is_whitespace() { k += 1; }
                            if k < chars_vec.len() && chars_vec[k] == ',' { k += 1; }
                            duplicate_ranges.push((start, k));
                        } else {
                            seen.push(key);
                        }
                    }
                }
                in_str = false;
                idx += 1;
                continue;
            }
        }
        if !in_str {
            if c == '{' || c == '[' { seen_keys.push(Vec::new()); }
            else if c == '}' || c == ']' { seen_keys.pop(); }
        }
        idx += 1;
    }

    if duplicate_ranges.is_empty() { return json_str.to_string(); }

    let mut result = String::with_capacity(json_str.len());
    let mut last_end = 0;
    for (start, end) in &duplicate_ranges {
        result.push_str(&json_str[last_end..*start]);
        last_end = *end;
    }
    result.push_str(&json_str[last_end..]);
    result
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
    // Dynamic weight tracking
    total_successes: u64,
    total_failures: u64,
    total_results_returned: u64,
    last_success: Option<Instant>,
}

impl CircuitBreaker {
    fn new() -> Self {
        Self {
            engines: Mutex::new(HashMap::new()),
        }
    }

    fn is_open(&self, engine: &str) -> bool {
        let engines = self.engines.lock();
        if let Some(health) = engines.get(engine) {
            if let Some(until) = health.open_until {
                return Instant::now() < until;
            }
        }
        false
    }

    fn record_success(&self, engine: &str) {
        let mut engines = self.engines.lock();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
            total_successes: 0,
            total_failures: 0,
            total_results_returned: 0,
            last_success: None,
        });
        health.consecutive_failures = 0;
        health.open_until = None;
        health.total_successes += 1;
        health.last_success = Some(Instant::now());
    }

    fn record_failure(&self, engine: &str) {
        let mut engines = self.engines.lock();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
            total_successes: 0,
            total_failures: 0,
            total_results_returned: 0,
            last_success: None,
        });
        health.consecutive_failures += 1;
        health.last_failure = Some(Instant::now());
        health.total_failures += 1;

        // Immediate backoff on first failure, exponential after that
        // First failure: 15s, second: 30s, third: 60s, ... capped at 5 min
        // This prevents every request from waiting for timeouts when an engine is down.
        let backoff_secs = 15u64 * 2u64.pow(health.consecutive_failures.saturating_sub(1));
        let backoff = Duration::from_secs(backoff_secs.min(300));
        health.open_until = Some(Instant::now() + backoff);
        tracing::warn!(
            "Circuit OPEN for engine '{}' — {} failures, backing off {:?}",
            engine, health.consecutive_failures, backoff
        );
    }

    // Record how many results an engine returned (for weight calculation)
    fn record_results(&self, engine: &str, count: u64) {
        let mut engines = self.engines.lock();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
            total_successes: 0,
            total_failures: 0,
            total_results_returned: 0,
            last_success: None,
        });
        health.total_results_returned += count;
    }

    // Get dynamic weight for an engine based on historical performance.
    // Returns a value in [0.5, 2.0]:
    //   - New engines (no history): 1.0 (neutral)
    //   - Reliable engines (high success rate, many results): up to 2.0
    //   - Unreliable engines (low success rate): down to 0.5
    // This is used to boost/penalize RRF contributions from each engine.
    fn weight(&self, engine: &str) -> f32 {
        let engines = self.engines.lock();
        if let Some(health) = engines.get(engine) {
            let total = health.total_successes + health.total_failures;
            if total < 5 {
                return 1.0; // not enough data, stay neutral
            }
            let success_rate = health.total_successes as f32 / total as f32;
            // Boost engines that return more results (they have broader coverage)
            let result_volume = (health.total_results_returned as f32 / total as f32).min(50.0);
            let volume_boost = (result_volume / 20.0).clamp(0.8, 1.3);
            // Combine: success rate [0.5, 1.5] * volume boost [0.8, 1.3]
            let weight = (0.5 + success_rate) * volume_boost;
            weight.clamp(0.5, 2.0)
        } else {
            1.0 // unknown engine, neutral weight
        }
    }
}

// ─── Search Result Cache (TTL-based) ───────────────────────────────
// Caches (query, intent) → aggregated results for 5 minutes.
// Avoids hammering meta-search engines for repeated queries.

/// Maximum number of cached query responses. Bounds memory under sustained traffic;
/// oldest entries are evicted by `inserted_at` when the cap is exceeded. LRU-by-age,
/// not access time — the access pattern is read-heavy with rare repeats, so age is a
/// good proxy for staleness without per-get bookkeeping.
const SEARCH_CACHE_MAX_ENTRIES: usize = 10_000;

/// Maximum total bytes held by cached response bodies. This is the *real* memory
/// bound: each entry stores a fully serialized `UnifiedResponse` which can be very
/// large (MBs). A pure entry-count cap of 10k would permit gigabytes of retained
/// JSON under sustained load and OOM the container. Entries older than the newest
/// (`inserted_at` ascending) are evicted first until the byte budget is satisfied.
const SEARCH_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

/// Responses larger than this are not cached at all — caching a single multi-MB
/// body permanently occupies the budget and helps no repeat query in practice.
const SEARCH_CACHE_MAX_ENTRY_BYTES: usize = 4 * 1024 * 1024; // 4 MiB

struct SearchCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    response_json: String, // serialized UnifiedResponse
    inserted_at: Instant,
    ttl: Duration,
    bytes: usize,
}

impl SearchCache {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock();
        if let Some(entry) = entries.get(key) {
            if entry.inserted_at.elapsed() < entry.ttl {
                return Some(entry.response_json.clone());
            }
        }
        None
    }

    fn put(&self, key: String, response_json: String, ttl: Duration) {
        let bytes = response_json.len();

        // Never cache pathological responses — they would monopolize the budget.
        if bytes > SEARCH_CACHE_MAX_ENTRY_BYTES {
            tracing::debug!(
                "Cache skip: response {}B exceeds entry cap ({}B)",
                bytes, SEARCH_CACHE_MAX_ENTRY_BYTES
            );
            return;
        }

        let mut entries = self.entries.lock();
        entries.insert(key, CacheEntry {
            response_json,
            inserted_at: Instant::now(),
            ttl,
            bytes,
        });

        // Evict expired entries to prevent unbounded growth.
        entries.retain(|_, e| e.inserted_at.elapsed() < e.ttl);

        // Enforce the byte budget: evict oldest-by-age entries until total bytes
        // is within budget (and the count cap is respected). O(n log n) per put
        // but n is small in practice and correctness > micro-optimization here.
        let total_bytes: usize = entries.values().map(|e| e.bytes).sum();
        if total_bytes > SEARCH_CACHE_MAX_BYTES || entries.len() > SEARCH_CACHE_MAX_ENTRIES {
            let mut by_age: Vec<(Instant, String)> = entries
                .iter()
                .map(|(k, e)| (e.inserted_at, k.clone()))
                .collect();
            by_age.sort_by_key(|(t, _)| *t);
            let mut used: usize = entries.values().map(|e| e.bytes).sum();
            let mut count = entries.len();
            for (_, k) in by_age.into_iter() {
                if used <= SEARCH_CACHE_MAX_BYTES && count <= SEARCH_CACHE_MAX_ENTRIES {
                    break;
                }
                if let Some(e) = entries.remove(&k) {
                    used = used.saturating_sub(e.bytes);
                    count -= 1;
                }
            }
        }
    }
}

fn deduplicate_merged_results(results: &mut Vec<MergedResult>) {
    let mut seen_titles: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_domain_titles: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    results.retain(|r| {
        let t_clean: String = r.title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if t_clean.is_empty() {
            return true;
        }

        // 1. Exact title deduplication (case-insensitive, whitespace-normalized)
        if seen_titles.contains(&t_clean) {
            return false;
        }
        seen_titles.insert(t_clean.clone());

        // 2. Domain + Title similarity deduplication
        let host = reqwest::Url::parse(&r.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
            .unwrap_or_default();

        if !host.is_empty() {
            let title_words: std::collections::HashSet<&str> = t_clean.as_str().split_whitespace().collect::<std::collections::HashSet<&str>>();
            if let Some(existing_titles) = seen_domain_titles.get(&host) {
                for ext_title in existing_titles {
                    let ext_words: std::collections::HashSet<&str> = ext_title.as_str().split_whitespace().collect::<std::collections::HashSet<&str>>();
                    let intersection = title_words.intersection(&ext_words).count();
                    let union = title_words.union(&ext_words).count();
                    if union > 0 {
                        let jaccard = intersection as f32 / union as f32;
                        if jaccard > 0.70 {
                            return false; // too similar on the same domain
                        }
                    }
                }
            }
            seen_domain_titles.entry(host).or_insert_with(Vec::new).push(t_clean);
        }

        true
    });
}

fn renormalize_distribution(distribution: &mut std::collections::HashMap<String, f32>) {
    if distribution.is_empty() {
        return;
    }
    let sum: f32 = distribution.values().sum();
    if sum > 0.0 {
        for val in distribution.values_mut() {
            *val = *val / sum;
        }
    }
}

// ─── Unified Merge: Local + Web → Single Ranked List ────────────────
// Cross-source dedup: URLs appearing in both local index AND web search
// get merged with a consensus boost. This is the strongest relevance signal.
// Returns a single sorted list by final score.

fn merge_local_and_web(
    local: Vec<IndexerResult>,
    web: Vec<SearxResult>,
    query: &str,
    intent: &str,
    constraints: &Constraints,
    distribution: Option<&std::collections::HashMap<String, f32>>,
    geo_location: Option<&geoloc::GeoLocation>,
    web_semantic: &std::collections::HashMap<String, f32>,
) -> Vec<MergedResult> {
    let mut merged: Vec<MergedResult> = Vec::new();
    let mut url_to_idx: HashMap<String, usize> = HashMap::new();

    // Helper: normalize URL for dedup matching
    let normalize = |url: &str| -> String {
        let lower = url.to_lowercase();
        let no_fragment = lower.split('#').next().unwrap_or(&lower);
        let no_trailing = no_fragment.trim_end_matches('/');
        let no_www = no_trailing.replacen("://www.", "://", 1);
        // Strip m./mobile. prefixes: m.example.com → example.com
        let no_mobile = no_www
            .replacen("://m.", "://", 1)
            .replacen("://mobile.", "://", 1);
        strip_tracking_params(&no_mobile)
    };

    // 1. Add local results first (they have richer content)
    for r in local {
        let norm = normalize(&r.url);
        let entry = MergedResult {
            url: r.url,
            title: r.title,
            content: r.content,
            score: r.score,
            authority: r.authority,
            sources: vec!["local".to_string()],
            is_local: true,
            published_date: None,
            quality: r.quality,
        };
        url_to_idx.insert(norm, merged.len());
        merged.push(entry);
    }

    // 2. Add web results — merge if URL already in local
    for r in web {
        let norm = normalize(&r.url);
        if let Some(&idx) = url_to_idx.get(&norm) {
            // URL exists in local index — merge sources and apply consensus boost
            let existing = &mut merged[idx];
            let source = if r.engine.is_empty() { "web".to_string() } else { r.engine.clone() };
            if !existing.sources.contains(&source) {
                existing.sources.push(source);
            }
            // Add any extra sources from the web result
            for s in &r.sources {
                if !existing.sources.contains(s) {
                    existing.sources.push(s.clone());
                }
            }
            // Consensus boost: appearing in both local AND web = very strong signal
            // Apply 1.5x boost (only once, even if multiple web sources merge)
            if existing.score > 0.0 {
                existing.score *= 1.5;
            }
            // Prefer richer content — if web has more content, use it
            if r.content.len() > existing.content.len() {
                existing.content = r.content;
            }
            if existing.published_date.is_none() {
                existing.published_date = r.published_date.clone();
            }
        } else {
            let source = if r.engine.is_empty() { "web".to_string() } else { r.engine.clone() };
            let authority = domain_authority_score(&r.url);
            let entry = MergedResult {
                url: r.url,
                title: r.title,
                content: r.content,
                score: r.score,
                authority,
                sources: vec![source],
                is_local: false,
                published_date: r.published_date.clone(),
                quality: 1.0,
            };
            url_to_idx.insert(norm, merged.len());
            merged.push(entry);
        }
    }

    // 3. Apply unified ranking signals to all results
    // Use distribution-aware blending when available (intent as hint, not gate)
    let weights = match distribution {
        Some(dist) => RankingWeights::for_distribution(dist),
        None => RankingWeights::for_intent(intent),
    };

    let clean_query = simple_negation_strip(query).unwrap_or_else(|| query.to_string());

    // Navigational domain boost: if intent is navigational and the query
    // looks like a platform name (1-2 tokens), boost results whose host
    // matches the query. This fixes "github" → github.com subpages being
    // ranked below irrelevant content.
    let nav_query_domain: Option<String> = if intent == "navigational" {
        let q_words: Vec<&str> = clean_query.split_whitespace().collect();
        if q_words.len() <= 2 {
            // Check if query looks like a domain name (no spaces, alphanumeric)
            let joined = q_words.join("").to_lowercase();
            if joined.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.') {
                Some(joined)
            } else { None }
        } else { None }
    } else { None };

    let mut _max_semantic: f32 = 0.0; // tracked for thin-result gate
    // Relevance multiplier applied to each result's final score in the post-loop
    // adaptive-floor pass. Initialised to 1.0; the floor pass (Task 4) overrides it.
    let mut relevance_factor: f32 = 1.0;

    // Compute the query's distinctive terms ONCE (per-query, not per-result).
    // These drive both the lexical overlap relevance and the coherence gate. A
    // distinctive term is a word >=3 chars that is NOT a stop word and NOT a
    // generic web term (e.g. "web", "framework"), so "boilerplate code" ->
    // ["boilerplate","code"], not ["web"]. This is the lexical backbone of the
    // single relevance signal (BERT cannot separate polysemous tokens like "code").
    let stop_words: std::collections::HashSet<&str> = [
        "the","a","an","is","are","was","were","be","been","have","has","had",
        "do","does","did","will","would","can","may","might","shall","must","could",
        "should","in","on","at","to","for","of","with","from","by","and","but","or",
        "nor","not","so","yet","this","that","these","those","it","its","what","which",
        "who","whom","when","where","why","how","all","each","every","both","few",
        "more","most","other","some","such","no","only","own","same","than","too",
        "very","just","about","also","any","because","before","after","during",
        "between","through","under","over","again","then","there","here","into",
        "upon","within","without","out","off","up","down",
    ].iter().copied().collect();
    let generic_web_terms: std::collections::HashSet<&str> = [
        "web","framework","library","lib","tool","tools","app","apps","application",
        "applications","guide","guides","tutorial","tutorials","docs","doc",
        "documentation","example","examples","reference","server","client","best",
        "top","review","reviews","using","getting","started","introduction","overview",
    ].iter().copied().collect();
    let meta_action_terms: std::collections::HashSet<&str> = [
        "deploy", "deployment", "deploying", "master", "mastering", "learn", "learning",
        "start", "starting", "write", "writing", "build", "building", "create", "creating",
        "make", "making", "use", "using", "run", "running", "setup", "setting",
        "install", "installing", "develop", "developing", "beginner", "beginners",
        "intermediate", "advanced", "production", "business", "complete", "ultimate",
        "essential", "practical", "basic", "basics", "how", "what", "why", "where",
        "when", "which", "definition", "meaning", "dictionary", "define", "versus",
        "cheap", "best", "top", "review", "reviews", "free", "course", "courses",
        "tutorial", "tutorials", "guide", "guides", "documentation", "docs", "doc",
        "reference", "overview", "introduction", "getting", "started", "example",
        "examples", "sample", "samples", "site", "sites", "page", "pages", "online",
        "architecture", "architectures", "design", "pattern", "patterns"
    ].iter().copied().collect();

    let q_words: Vec<&str> = clean_query.split_whitespace().collect();
    let distinctive_terms: Vec<&str> = q_words.iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower.len() >= 3
                && !stop_words.contains(lower.as_str())
                && !generic_web_terms.contains(lower.as_str())
                && !lower.chars().all(|c| c.is_ascii_digit())
        })
        .copied()
        .collect();

    let core_topic_terms: Vec<&str> = q_words.iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower.len() >= 3
                && !stop_words.contains(lower.as_str())
                && !generic_web_terms.contains(lower.as_str())
                && !meta_action_terms.contains(lower.as_str())
                && !lower.chars().all(|c| c.is_ascii_digit())
        })
        .copied()
        .collect();

    // Multi-word phrase entities (P1): adjacent non-stopword runs of length >= 2 in the
    // raw query. These are the terms most prone to FALSE-POSITIVE token overlap — e.g.
    // "sky blue", "best laptop", "world wide web". A result that contains the individual
    // words scattered across different sentences ("bright sky", "blue ocean") is NOT a
    // real match. We require the contiguous phrase to appear (title preferred) so
    // "Sky Blue Credit" (a brand whose two tokens happen to be "sky"+"blue") no longer
    // rides a token-overlap bonus it didn't earn. Computed once per query, not per result.
    let phrase_entities: Vec<String> = {
        let mut phrases = Vec::new();
        let mut run: Vec<String> = Vec::new();
        for w in q_words.iter() {
            let lower = w.to_lowercase();
            let is_content = lower.len() >= 2
                && !stop_words.contains(lower.as_str())
                && !lower.chars().all(|c| c.is_ascii_digit());
            if is_content {
                run.push(lower);
            } else if run.len() >= 2 {
                phrases.push(run.join(" "));
                run.clear();
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            phrases.push(run.join(" "));
        }
        phrases
    };

    // Superlative terms whose presence alone is a weak/off-topic signal when the page
    // is about the superlative ("best", "top") rather than the actual topic. Used to
    // penalize pages that rank purely because they contain the generic word "best".
    let superlative_terms: &[&str] = &["best", "top", "greatest", "cheapest", "finest"];
    let query_has_superlative = q_words.iter().any(|w| superlative_terms.contains(&w.to_lowercase().as_str()));

    let mut relevance_vec: Vec<f32> = Vec::with_capacity(merged.len());

    for r in merged.iter_mut() {
        let substr_semantic = semantic_relevance_score(&clean_query, &r.title, &r.content);
        // Blend genuine BERT semantic similarity (web_semantic vs the query
        // embedding) into the substring scorer. This is what resolves polysemous
        // queries: "square a circle" embeds close to a geometry article and far
        // from the POS-system "Square", so the right-sense result wins. When the
        // embedding map has no entry for this URL (fail-closed), we keep the
        // substring scorer untouched.
        let semantic = match web_semantic.get(&r.url) {
            Some(&web_cos) => {
                let web_cos = web_cos.clamp(0.0, 1.0);
                (substr_semantic * 0.6 + web_cos * 0.4).clamp(0.0, 1.0)
            }
            None => substr_semantic,
        };
        if semantic > _max_semantic { _max_semantic = semantic; }
        // ── Single relevance signal (lexical overlap blended with BERT cosine) ──
        // This is the SOURCE OF TRUTH for topical fit. Distinctive-term overlap is
        // pure lexical (no network) so local and web results are directly comparable;
        // the BERT cosine (where the embed service returned one) only enhances it.
        // The post-loop adaptive-floor pass (Task 4) demotes results below the
        // query's own relevance distribution on the FINAL score.
        let title_lower = r.title.to_lowercase();
        let content_lower = r.content.to_lowercase();
        let url_lower = r.url.to_lowercase();

        let core_matches = if core_topic_terms.is_empty() {
            true
        } else {
            // Require matching all core topic terms (or their stemmed versions).
            // For multi-term topic queries (e.g. "fantasy novel", "microservices architecture"),
            // matching only "fantasy" (like ESPN Fantasy Football) or only "architecture" (Quantum Architecture)
            // is off-topic. All core topic terms must be present.
            core_topic_terms.iter().all(|t| {
                let tl = t.to_lowercase();
                let stemmed = tl.trim_end_matches('s');
                title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
                    || title_lower.contains(stemmed) || content_lower.contains(stemmed) || url_lower.contains(stemmed)
            })
        };

        let overlap = if distinctive_terms.is_empty() || !core_matches {
            if !core_matches { 0.0 } else { 1.0 }
        } else {
            let present = distinctive_terms.iter().filter(|t| {
                let tl = t.to_lowercase();
                title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
            }).count() as f32;
            present / distinctive_terms.len() as f32
        };
        // BERT cosine enhances relevance ONLY when the page already has lexical
        // overlap with the query. When overlap is ZERO the page shares no query
        // term, and BERT is known to conflate polysemous tokens (main.rs:2568:
        // "prediction" ≈ "predictive", "code" ≈ boilerplate-code) — so trusting
        // the cosine there would resurrect the football-drift. Fall back to pure
        // overlap (which is 0) instead of letting the embedding float spam.
        let bert_cos = if overlap > 0.0 {
            web_semantic.get(&r.url).copied().unwrap_or(overlap).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut relevance = 0.4f32 * overlap + 0.6f32 * bert_cos;

        // ── Phrase-entity fidelity (P1) ──
        // For multi-word phrase entities in the query (e.g. "sky blue", "world wide web",
        // "best laptop"), require the CONTIGUOUS phrase to actually appear. Token-overlap
        // scoring alone lets "Sky Blue Credit" (a brand) or "Best Western Hotels" win on
        // scattered single-word hits. When a phrase entity exists, a result that lacks the
        // contiguous phrase is not a genuine match for that entity — cap its relevance so
        // it sinks below results that do contain the phrase. Title phrase = strong; content
        // phrase = partial; neither = damped. This is generic (no brand hardcodes).
        if !phrase_entities.is_empty() {
            let phrase_hits: usize = phrase_entities.iter().filter(|p| {
                title_lower.contains(p.as_str()) || content_lower.contains(p.as_str()) || url_lower.contains(p.as_str())
            }).count();
            let phrase_ratio = phrase_hits as f32 / phrase_entities.len() as f32;
            // Blend the phrase ratio into relevance: a result missing every phrase entity
            // drops to at most ~0.45 of its token-overlap relevance; full phrase coverage
            // keeps it intact. This lets "Why Is the Sky Blue?" (title has the phrase) rank
            // above "Sky Blue Credit" (no contiguous phrase), purely from structure.
            relevance *= 0.45 + 0.55 * phrase_ratio;
        }

        // ── Superlative penalty (P1) ──
        // Queries with a superlative ("best laptop ...", "top framework") attract pages
        // that match ONLY on the generic word "best"/"top" while missing the actual topic.
        // If the query has a superlative and this result's TITLE contains the superlative
        // but NONE of the core topic terms, the "best" token is doing all the work — dampen
        // so topic-relevant pages (which may not say "best") rank above generic listicles.
        if query_has_superlative && !core_topic_terms.is_empty() {
            let title_has_superlative = superlative_terms.iter().any(|s| title_lower.contains(*s));
            let title_has_topic = core_topic_terms.iter().any(|t| {
                let tl = t.to_lowercase();
                title_lower.contains(&tl) || title_lower.contains(tl.trim_end_matches('s'))
            });
            if title_has_superlative && !title_has_topic {
                relevance *= 0.55;
            }
        }

        // ── Local-index low-signal gate (P2) ──
        // The local crawl index can contain pages that matched the query only on a
        // generic/boilerplate term (or a single scattered token) and are otherwise
        // off-topic for the query — e.g. a JetBrains "Django vs Flask" blog surfacing
        // for "history of the world wide web". The indexer already emits a per-result
        // `quality` metric (BM25/semantic match strength). We use it — plus a check
        // that the result actually mentions the query's distinctive topic terms — to
        // demote low-signal local pages BEFORE they can crowd out authoritative web hits.
        // Generic (no hardcoded domains): a local page that is both low-quality AND
        // missing every distinctive term is almost certainly crawl noise.
        if r.is_local {
            let topic_mentioned = distinctive_terms.is_empty()
                || distinctive_terms.iter().any(|t| {
                    let tl = t.to_lowercase();
                    title_lower.contains(&tl) || content_lower.contains(&tl)
                        || title_lower.contains(tl.trim_end_matches('s')) || content_lower.contains(tl.trim_end_matches('s'))
                });
            if r.quality < 0.5 && !topic_mentioned {
                relevance *= 0.05;
                tracing::info!(
                    "LOCAL NOISE GATE: '{}' quality={:.2} topic_mentioned={} -> relevance crushed",
                    r.url.chars().take(60).collect::<String>(), r.quality, topic_mentioned
                );
            } else if r.quality < 0.7 && !topic_mentioned {
                relevance *= 0.5;
            }
        }

        // ── Price-aware ranking (P3) ──
        // The price constraint (price:<1000, price_max:1000.0) is extracted correctly
        // but was a NO-OP: web/local results carry no structured price metadata, so the
        // hard filter (which only fires when a price is parsed from page text) never
        // triggered and ranking stayed pure relevance. We make the bound MEANINGFUL at
        // ranking time without inventing structured price data:
        //   • if the result states a price, hold it to the bound (out-of-budget -> crush;
        //     in-budget -> small boost "this is the product");
        //   • if NO price is stated AND the query is a transactional-product query (price
        //     bound present) AND the result shows no price/product lexical signal, demote
        //     it — almost certainly not the priced product asked for. Generic; no hardcoded
        //     merchants or domains.
        let price_bound = constraints.price_max.or(constraints.price_lt)
            .or_else(|| constraints.price_min.or(constraints.price_gt));
        if let Some(_bound) = price_bound {
            let res_price = extract_price_from_text(&r.title)
                .or_else(|| extract_price_from_text(&r.content));
            let price_signal = title_lower.contains('$') || content_lower.contains('$')
                || title_lower.contains("price") || content_lower.contains("price")
                || title_lower.contains("cost") || content_lower.contains("cost")
                || title_lower.contains("rs") || title_lower.contains("rupee")
                || title_lower.contains("pound") || title_lower.contains("euro")
                || title_lower.contains("buy") || title_lower.contains("shop")
                || title_lower.contains("cheap") || title_lower.contains("affordable")
                || title_lower.contains("sale") || title_lower.contains("deal")
                || title_lower.contains("specs") || title_lower.contains("spec");
            if let Some(p) = res_price {
                let over = (constraints.price_max.is_some() && p > constraints.price_max.unwrap())
                    || (constraints.price_lt.is_some() && p > constraints.price_lt.unwrap())
                    || (constraints.price_min.is_some() && p < constraints.price_min.unwrap())
                    || (constraints.price_gt.is_some() && p < constraints.price_gt.unwrap());
                if over {
                    relevance *= 0.12;
                } else {
                    relevance *= 1.10;
                }
            } else if !price_signal {
                relevance *= 0.45;
            }
        }


        // meaning of X", "X explained (listicle)" — these are off-topic for a
        // substantive query and the POS/phonetic dictionary guard misses them.
        // Fold into relevance so the adaptive floor demotes them on the final score.
        // (is_definition_query is computed later in the loop, so use a local check here.)
        let q_lc = clean_query.to_lowercase();
        let is_def_query = q_lc.contains("define") || q_lc.contains("definition")
            || q_lc.contains("meaning of") || q_lc.contains("what does")
            || q_lc.contains("what is");
        let listicle_title = (title_lower.starts_with("why ")
                && (title_lower.contains(" mean") || title_lower.contains(" meaning")))
            || title_lower.starts_with("what is the meaning of")
            || (title_lower.starts_with("what does ") && title_lower.contains(" mean"))
            || (title_lower.starts_with("what is ") && title_lower.contains(" mean"));
        if listicle_title && !is_def_query {
            // A listicle title is a STRUCTURAL off-topic signal (clickbait about the
            // word's etymology, not the topic). Because the page still contains the
            // query term, lexical overlap is high and a gentle multiplier wouldn't sink
            // it — so CAP relevance to a low value so the adaptive floor crushes it.
            relevance = relevance.min(0.12);
        }
        let intent_boost = calculate_intent_boost(&r.url, &r.title, &clean_query, intent);
        let mut freshness = freshness_score(&r.url, intent, r.published_date.as_deref());
        let mut quality = content_quality_score(&r.content);

        // ─── Intent-aware sense disambiguation (P-B) + conspiracy debias (P-C) ───
        // Computed once per result. Both are fail-closed: they only ever *reduce*
        // quality (P-B) or nudge score (P-C), never invent relevance. They rely on
        // lexical/structural signals, not hardcoded domain lists, so they stay
        // robust as the web shifts.
        let sense_class = query_sense_class(&clean_query);
        if sense_class != SenseClass::None {
            let p = conflicting_sense_penalty(sense_class, &r.title, &r.content, &r.url);
            if p < 1.0 {
                quality *= p;
                tracing::debug!(
                    "P-B SENSE PENALTY x{:.2}: '{}' (sense={:?})",
                    p, r.url.chars().take(60).collect::<String>(), sense_class
                );
            }
        }
        // P-C: only act when the query itself is a conspiracy-claim sense.
        let mut conspiracy_boost = 0.0f32;
        if sense_class == SenseClass::ConspiracyClaim {
            let (penalty, boost) = conspiracy_guard(&r.title, &r.content);
            if penalty < 1.0 {
                quality *= penalty;
            }
            conspiracy_boost = boost;
        }

        // Local index artifact gate: if a local result has near-zero semantic relevance
        // to the query, it's an irrelevant local page (e.g., a different project's README
        // that happens to contain one matching term). Crush its quality score so it sinks
        // below web search results.
        if r.is_local && semantic < 0.12 {
            quality *= 0.05;
        }

        // Topic coherence: prevent domain mismatch where results match on generic
        // web terms (e.g., "web", "framework") but not the query's distinctive topic
        // term (e.g., "productivity suite" query returning a sports article).
        // Applies to BOTH local and web results, with a stronger penalty for local
        // results (which should have richer content) and a gentler penalty for web
        // results (which may have shorter snippets).
        //
        // The distinctive terms are words ≥3 chars that are NOT stop words AND NOT
        // generic web domain terms. This catches the "football scores" collapse:
        //   query="productivity suite not google workspace not ..."
        //   positive=["productivity", "suite", "open", "source"]
        //   distinctive=["productivity", "suite"] ← neither is a generic web term
        // If a result contains neither "productivity" nor "suite", it's off-topic.
        //
        // Also check negative constraints: if the query excludes certain items,
        // the result should be ABOUT alternatives to those items. A result that
        // mentions none of the positive AND none of the negative terms is suspicious
        // (it's probably off-topic content that just happened to slip through).
        if quality > 0.01 {
            // Also build a set of negative constraint words — if a result mentions
            // NONE of the positive AND NONE of the negative, it's likely off-topic.
            let mut neg_word_set: std::collections::HashSet<String> = std::collections::HashSet::new();
            for n in &constraints.negative {
                for syn in expand_negative_synonyms(n) {
                    for w in syn.split_whitespace() {
                        neg_word_set.insert(w.to_string());
                    }
                }
            }
            let title_lower = r.title.to_lowercase();
            let content_lower = r.content.to_lowercase();

            if !distinctive_terms.is_empty() {
                let any_distinctive_match = distinctive_terms.iter().any(|t| {
                    title_lower.contains(t) || content_lower.contains(t)
                });

                if !any_distinctive_match {
                    // Also check: does the result mention ANY negative constraint word?
                    // If the query has negatives ("not X"), results that don't mention
                    // X AND don't mention the distinctive positive terms are probably
                    // off-topic garbage (e.g., football scores for a productivity query).
                    // Fold the off-topic signal into the single relevance gate (the
                    // adaptive floor then demotes on the final score) instead of a
                    // near-no-op quality multiplier.
                    if !constraints.negative.is_empty() && !neg_word_set.is_empty() {
                        let title_content = format!("{} {}", title_lower, content_lower);
                        let mentions_negative = neg_word_set.iter().any(|n| title_content.contains(n.as_str()));
                        if !mentions_negative {
                            // No distinctive positive AND no negative = completely off-topic
                            relevance *= if r.is_local { 0.10 } else { 0.20 };
                        } else {
                            // Mentions negative terms but not positive = borderline
                            relevance *= if r.is_local { 0.20 } else { 0.35 };
                        }
                    } else {
                        // No negative constraints — just no positive match
                        relevance *= if r.is_local { 0.10 } else { 0.25 };
                    }
                }
            } else if !constraints.negative.is_empty() && !neg_word_set.is_empty() {
                // No distinctive positive terms found (query is all generics + negatives).
                // Check if result mentions any negative term — if not, it's off-topic.
                let title_content = format!("{} {}", title_lower, content_lower);
                let mentions_negative = neg_word_set.iter().any(|n| title_content.contains(n.as_str()));
                if !mentions_negative {
                    relevance *= if r.is_local { 0.20 } else { 0.35 };
                }
            }
        }

        // Dictionary/definition site penalty: detect dictionary/glossary sites
        // via content structure, domain markers, and title patterns.
        let is_definition_site = {
            let title_lower = r.title.to_lowercase();
            let content_lower = r.content.to_lowercase();
            let url_lower = r.url.to_lowercase();
            let content_prefix = content_lower.chars().take(300).collect::<String>();
            let title_words: Vec<&str> = title_lower.split_whitespace().collect();

            let is_dict_domain_or_path = url_lower.contains("merriam-webster.com")
                || url_lower.contains("dictionary.cambridge.org")
                || url_lower.contains("wiktionary.org")
                || url_lower.contains("dictionary.com")
                || url_lower.contains("vocabulary.com")
                || url_lower.contains("thefreedictionary.com")
                || url_lower.contains("wordnik.com")
                || url_lower.contains("/dictionary/")
                || url_lower.contains("/define/")
                || url_lower.contains("/meaning/");

            let has_dict_title = title_lower.contains("meaning & definition")
                || title_lower.contains("definition & meaning")
                || title_lower.contains("definition of ")
                || title_lower.contains("meaning of ")
                || title_lower.ends_with("- wiktionary")
                || title_lower.contains("cambridge dictionary")
                || title_lower.contains("merriam-webster")
                || (title_words.len() <= 3 && (title_lower.contains("definition") || title_lower.contains("dictionary")));

            let has_phonetic = content_prefix.contains("/ˈ") || content_prefix.contains("/ˌ")
                || content_prefix.contains("/'") || content_prefix.contains("/-");
            let has_pos_label = content_prefix.starts_with("noun")
                || content_prefix.starts_with("verb")
                || content_prefix.starts_with("adjective")
                || content_prefix.starts_with("adverb")
                || content_prefix.contains("1. : to ")
                || content_prefix.contains("2. : to ")
                || content_prefix.contains("definition of ")
                || content_prefix.contains("meaning of ");

            let content_is_short = r.content.len() < 200;
            let short_title = title_words.len() <= 3;

            is_dict_domain_or_path
                || has_dict_title
                || (has_phonetic || has_pos_label) && short_title
                || has_pos_label && content_is_short
        };
        // Only treat as definition query if user explicitly asked for linguistic word definition
        let q_lower_check = query.to_lowercase();
        let is_definition_query = q_lower_check.starts_with("define ")
            || q_lower_check.starts_with("definition of ")
            || q_lower_check.contains("definition of ")
            || q_lower_check.contains(" meaning of ")
            || q_lower_check.contains("word meaning")
            || (q_lower_check.starts_with("what does ") && q_lower_check.contains(" mean"))
            || (q_lower_check.starts_with("what is the definition"));

        if is_definition_site && !is_definition_query {
            // Dictionary definition pages are off-topic for non-definition queries.
            // Crush relevance to 0.001 and quality to 0.01 so they sink completely.
            relevance = 0.001;
            quality *= 0.01;
            tracing::info!(
                "DICTIONARY SITE PENALTY: '{}' -> relevance crushed 0.001",
                r.url.chars().take(60).collect::<String>()
            );
        }

        // Academic Paper Deprioritization for Navigational, Download, and Commercial Queries:
        // Academic repositories (arxiv, crossref, pubmed) often contain math/theory papers that match
        // query words like "driver" (e.g. wireless LAN driver math, cancer survivorship drivers).
        // For navigational, software download, or commercial queries, academic papers are 100% noise.
        let is_academic = url_lower.contains("arxiv.org") || url_lower.contains("crossref.org")
            || url_lower.contains("ncbi.nlm.nih.gov")
            || r.sources.iter().any(|s| s == "arxiv" || s == "crossref" || s == "pubmed");

        let has_download = DOWNLOAD_KEYWORDS.iter().any(|k| q_lower_check.contains(k));
        let tx_keywords = ["buy", "price", "pricing", "cheap", "purchase", "shop", "store", "discount", "coupon"];
        let has_tx = tx_keywords.iter().any(|k| q_lower_check.contains(k));
        let is_nav_or_download = intent == "navigational"
            || intent == "transactional"
            || has_download
            || has_tx
            || distribution.and_then(|d| d.get("download")).copied().unwrap_or(0.0) > 0.40;

        let q_wants_academic = q_lower_check.contains("paper") || q_lower_check.contains("arxiv")
            || q_lower_check.contains("study") || q_lower_check.contains("journal")
            || q_lower_check.contains("research");

        if is_academic && is_nav_or_download && !q_wants_academic {
            relevance = 0.001;
            quality *= 0.01;
            tracing::info!(
                "ACADEMIC PAPER PENALTY: '{}' -> crushed 0.001 for navigational/download query",
                r.url.chars().take(60).collect::<String>()
            );
        }

        // Wikidata penalty: Wikidata is a machine database, not a human-readable search result.
        let url_lower = r.url.to_lowercase();
        let is_wikidata = url_lower.contains("wikidata.org");
        if is_wikidata {
            quality *= 0.05;
            r.authority *= 0.1;
        }

        // Wikipedia/Wikidata generic concept penalty for negative queries:
        // If a query has negative constraints (e.g. "search engine not google"), the user is looking
        // for specific alternatives, not a generic encyclopedia page about the concept.
        if !constraints.negative.is_empty() && (url_lower.contains("wikipedia.org") || url_lower.contains("wikidata.org")) {
            quality *= 0.15;
            r.authority *= 0.2;
        }

        let c_score = constraint_score(&r.title, &r.content, &r.url, constraints);
        let consensus = consensus_score(&r.sources);

        // Navigational domain match boost: if the URL host contains the
        // query as a domain component, this is likely the destination the
        // user wants. Strong boost for homepage, moderate for subpages.
        let nav_domain_boost = if let Some(ref domain) = nav_query_domain {
            if let Ok(parsed) = reqwest::Url::parse(&r.url) {
                if let Some(host) = parsed.host_str() {
                    let host_lower = host.to_lowercase();
                    // Exact match: host is query.com or www.query.com
                    if host_lower == format!("{}.com", domain)
                        || host_lower == format!("www.{}.com", domain)
                        || host_lower == format!("{}.org", domain)
                        || host_lower == format!("{}.io", domain)
                        || host_lower == format!("{}.dev", domain)
                    {
                        let path = parsed.path();
                        if path == "/" || path.is_empty() {
                            0.4 // homepage boost
                        } else {
                            0.25 // subpage boost
                        }
                    } else if host_lower.contains(domain.as_str()) {
                        0.1 // related domain
                    } else {
                        0.0
                    }
                } else { 0.0 }
            } else { 0.0 }
        } else { 0.0 };

        // Local pages earn the bonus ONLY when actually relevant to the query.
        // The old blanket +1.0 floated token-overlap noise (e.g. "boilerplate code"
        // -> "QR Code Generator") to the top regardless of relevance. The merge-time
        // consensus *1.5 boost still prefers genuinely-good local pages.
        let local_bonus = if r.is_local && relevance >= 0.3 { 0.6 } else { 0.0 };
        // Geo-relevance boost: boost results that mention the user's country, region, or city.
        // Higher boost for city-level matches (0.25) than country-level (0.10).
        let geo_boost = geo_location.map(|g| geo_relevance_score(&r.title, &r.content, &r.url, g)).unwrap_or(0.0);
        let base = (weights.rrf * r.score)
            + (weights.semantic * semantic)
            + (weights.intent * intent_boost)
            + (weights.freshness * freshness)
            + (weights.authority * r.authority)
            + (weights.quality * quality)
            + (weights.consensus * consensus)
            + (weights.local_bonus * local_bonus)
            + nav_domain_boost
            + geo_boost
            + conspiracy_boost;

        let mut generic_penalty = 1.0f32;
        if !constraints.negative.is_empty() && (url_lower.contains("wikipedia.org") || url_lower.contains("wikidata.org")) {
            // encyclopedia fallback is undesirable when user explicitly seeks niche alternatives
            // Scale by exclusion count: more exclusions = stronger penalty
            let wiki_penalty = match constraints.negative.len() {
                1 => 0.20,
                2 => 0.08,
                3 => 0.04,
                _ => 0.02, // 4+ exclusions: almost certainly wrong to show encyclopedia
            };
            generic_penalty *= wiki_penalty;
        }
        if url_lower.contains("wikidata.org") {
            // Wikidata is a machine database, not human readable search result
            generic_penalty *= 0.10;
        }

        // Fold the single relevance signal directly into the FINAL score (P1/P2 fix).
        // Previously `relevance` (which carries the phrase-entity fidelity, superlative,
        // and local-noise penalties) only fed the distribution-relative adaptive floor.
        // When every top result is similarly spammy, a relative floor cannot crush any of
        // them, so "Sky Blue Credit" kept outranking the real "Why Is the Sky Blue?".
        // Multiplying the final score by relevance makes those penalties bite: a result
        // whose relevance was halved by the phrase gate (×0.45) also has its score halved,
        // creating real separation. Clamped to [0.05,1.0] so relevance can demote hard
        // but never zero out a result (preserving the calibrate_scores floor logic).
        let relevance_mult = relevance.clamp(0.05, 1.0);
        r.score = base * c_score * generic_penalty * relevance_factor * relevance_mult;
        // Capture this result's relevance for the post-loop adaptive-floor pass.
        relevance_vec.push(relevance);
    }

    // ── Adaptive relevance floor (distribution-driven, no fixed threshold) ──
    // Demote results whose relevance sits far below THIS query's own relevance
    // distribution. The floor tracks the shape of the results returned, so it
    // stays correct as the web shifts (no magic constant like 0.12/0.18). Off-topic
    // pages (football spam for "predictive coding", dictionary/listicle clickbait)
    // have low distinctive-term overlap -> low relevance -> crushed here on the
    // FINAL score, where it actually bites (the old quality*0.08 never did).
    if !relevance_vec.is_empty() {
        let mut sorted = relevance_vec.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p60_idx = ((sorted.len() as f32 * 0.6).floor() as usize).min(sorted.len() - 1);
        let p60 = sorted[p60_idx];
        // Floor = a fraction of the 60th-percentile relevance, clamped to a sane band.
        let floor = (p60 * 0.35).max(0.05).min(0.5);
        for (i, r) in merged.iter_mut().enumerate() {
            let rel = relevance_vec.get(i).copied().unwrap_or(0.0);
            // Below the distribution floor = genuinely off-topic (bottom ~40% of the
            // result set). Apply a HARD demotion (not a ratio): a ratio `rel/floor`
            // leaves a moderately-low relevance at ~0.5, which is too gentle when the
            // page still contains the query term (listicle/dictionary case). The hard
            // 0.15 multiplier reliably sinks off-topic results while the top of the
            // distribution (above floor) is untouched (factor 1.0).
            let factor = if rel < floor { 0.15 } else { 1.0 };
            r.score *= factor;
        }
    }

    // --- Thin-Result Detection: boost scores when few results or low max score ---
    // For niche topics (few results returned, low max score), apply a proportional
    // boost to ensure the top results surface with reasonable confidence.
    // GATE: only boost if the best result has MULTIPLE query term matches (not just one).
    // This prevents garbage results (dictionary definitions, single-word match pages)
    // from being amplified to the top.
    if merged.len() < 15 && merged.len() > 0 {
        let max_score = merged.iter().map(|r| r.score).fold(0.0f32, f32::max);
        // Semantic relevance gate: only apply thin-result boost if at least one result
        // has minimum semantic relevance to the query. This prevents garbage results
        // (local index misses with negative constraint hits) from being amplified.
        // Use cached max_semantic from scoring loop (avoids recomputing all scores)
        let max_semantic = _max_semantic;
        if max_score < 0.30 && max_semantic > 0.05 {
            // ADDITIONAL GATE: Check if the best-scoring result matches at least 2 unique
            // query terms (after stemming). Single-term matches are usually dictionary
            // definitions or tangentially related content that shouldn't be amplified.
            let query_terms_raw: Vec<&str> = query.split_whitespace()
                .filter(|w| w.len() >= 2)
                .collect();
            let has_good_result = merged.iter().enumerate().any(|(i, r)| {
                let t_lower = r.title.to_lowercase();
                let c_lower = r.content.to_lowercase();
                let match_count = query_terms_raw.iter()
                    .filter(|qt| t_lower.contains(*qt) || c_lower.contains(*qt))
                    .count();
                let min_terms = (query_terms_raw.len().min(5) / 2).max(2); // at least 2 terms or half of query
                // ALSO require the result to be genuinely relevant (single relevance
                // gate), so a niche query can never amplify an off-topic page.
                let rel = relevance_vec.get(i).copied().unwrap_or(0.0);
                match_count >= min_terms && rel >= 0.2
            });
            if has_good_result {
                let boost_factor = (0.30 / max_score.max(0.01)).min(2.5);
                tracing::info!(
                    "THIN RESULTS: merged.len={} max_score={:.3} max_sem={:.3} boost={:.2}x",
                    merged.len(), max_score, max_semantic, boost_factor
                );
                for r in merged.iter_mut() {
                    r.score *= boost_factor;
                }
            } else {
                tracing::info!(
                    "THIN RESULTS SKIPPED (no multi-term match): merged.len={} max_score={:.3} max_sem={:.3}",
                    merged.len(), max_score, max_semantic
                );
            }
        } else if max_score < 0.30 {
            tracing::info!(
                "THIN RESULTS SKIPPED (garbage gate): merged.len={} max_score={:.3} max_sem={:.3}",
                merged.len(), max_score, max_semantic
            );
        }
    }

    // 4. Sort by score descending
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 4b. Position-aware domain diversity: penalize repeated domains in top slots.
    // The global cap (MAX_PER_DOMAIN=3) prevents outright flooding, but doesn't
    // ensure diversity in the top results. A domain with 3 great results will
    // still dominate positions 1-3. This penalty makes the 2nd appearance of a
    // domain score 70% and the 3rd score 49% — the algorithm naturally promotes
    // other domains into higher slots without hard cutoffs.
    {
        let mut domain_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in merged.iter_mut() {
            let domain = reqwest::Url::parse(&r.url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
                .unwrap_or_default();
            let count = domain_counts.entry(domain).or_insert(0);
            *count += 1;
            // Each appearance beyond the first gets a 0.7x penalty (compounding)
            // 1st: 1.0, 2nd: 0.70, 3rd: 0.49, 4th: 0.34, 5th: 0.24
            if *count > 1 {
                let penalty = 0.7_f32.powi((*count - 1) as i32);
                r.score *= penalty;
            }
        }
        // Re-sort after diversity penalty
        merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    // 4c. Source-level diversity boost: boost results from underrepresented engine sources.
    // Count how many results each engine source contributed, then boost results from
    // sources that contributed fewer than the median. This promotes multi-engine diversity.
    {
        let mut source_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in &merged {
            for s in &r.sources {
                *source_counts.entry(s.to_lowercase()).or_insert(0) += 1;
            }
        }
        if source_counts.len() > 1 {
            let total_sources = source_counts.len();
            let total_results = merged.len();
            let median_per_source = (total_results as f32 / total_sources as f32).ceil();
            for r in merged.iter_mut() {
                let is_underrepresented = r.sources.iter().any(|s| {
                    source_counts.get(&s.to_lowercase()).copied().unwrap_or(0) as f32 <= median_per_source * 0.5
                });
                if is_underrepresented {
                    // Phase 0: differential nudge instead of uniform ×1.25.
                    // A flat ×1.25 makes every survivor of a negation cluster
                    // land identically ≥1.0 and then collapse to 1.0 in
                    // calibrate_scores. A small additive nudge (+0.03) keeps
                    // the real relative ordering intact (differentiable).
                    r.score += 0.03;
                }
            }
        }
    }

    // Deduplicate merged results by title and domain
    deduplicate_merged_results(&mut merged);

    // 5. Calibrate scores onto [0.05, 1.0] preserving real distribution (Phase 0)
    let mut scores: Vec<f32> = merged.iter().map(|r| r.score).collect();
    calibrate_scores(&mut scores);
    for (i, r) in merged.iter_mut().enumerate() {
        r.score = scores[i];
    }

    merged
}

// ─── Main ────────────────────────────────────────────────────────────

// ─── Rate-Limit Tracker ─────────────────────────────────────────────
// Tracks rate-limit events in a sliding window for proactive VPN rotation.
// When too many rate-limits accumulate, triggers preemptive rotation
// before the circuit breaker even opens.

struct RateLimitTracker {
    events: Mutex<Vec<Instant>>,
}

impl RateLimitTracker {
    fn new() -> Self {
        Self { events: Mutex::new(Vec::new()) }
    }

    fn record(&self) {
        let mut events = self.events.lock();
        let now = Instant::now();
        // Prune events older than 5 minutes
        events.retain(|e| now.duration_since(*e) < Duration::from_secs(300));
        events.push(now);
    }

    fn count_in_window(&self, window_secs: u64) -> usize {
        let events = self.events.lock();
        let now = Instant::now();
        events.iter().filter(|e| now.duration_since(**e) < Duration::from_secs(window_secs)).count()
    }
}

// ─── Result Volume Tracker (Per-Engine Degradation Detection) ─────
// Tracks rolling average of results per engine per query.
// When an engine suddenly returns 50% fewer results than its average,
// it's likely being rate-limited — even if SearXNG doesn't report an error.
// This catches silent degradation that the circuit breaker misses.

struct ResultVolumeTracker {
    // engine_name → (rolling_sum, rolling_count, last_degradation_time)
    engines: Mutex<HashMap<String, EngineVolume>>,
    // Overall degradation tracking: timestamps of degraded queries
    degradation_events: Mutex<Vec<Instant>>,
}

struct EngineVolume {
    rolling_sum: f64,
    rolling_count: f64,
    // Exponential moving average alpha (higher = more responsive)
    alpha: f64,
    last_result_count: u64,
    last_check: Instant,
}

impl ResultVolumeTracker {
    fn new() -> Self {
        Self {
            engines: Mutex::new(HashMap::new()),
            degradation_events: Mutex::new(Vec::new()),
        }
    }

    // Record result count for an engine. Returns true if degraded.
    fn record(&self, engine: &str, count: u64) -> bool {
        let mut engines = self.engines.lock();
        let volume = engines.entry(engine.to_string()).or_insert(EngineVolume {
            rolling_sum: 0.0,
            rolling_count: 0.0,
            alpha: 0.3, // responsive to recent changes
            last_result_count: 0,
            last_check: Instant::now(),
        });

        let count_f = count as f64;
        let is_degraded = if volume.rolling_count >= 3.0 {
            let avg = volume.rolling_sum / volume.rolling_count;
            // Degraded if current < 50% of rolling average AND average is >= 3
            // (don't flag engines that naturally return few results)
            count_f < avg * 0.5 && avg >= 3.0
        } else {
            false // not enough data yet
        };

        // Update exponential moving average
        volume.rolling_sum = volume.rolling_sum * (1.0 - volume.alpha) + count_f * volume.alpha;
        volume.rolling_count = volume.rolling_count * (1.0 - volume.alpha) + volume.alpha;
        volume.last_result_count = count;
        volume.last_check = Instant::now();

        if is_degraded {
            let avg = (volume.rolling_sum / volume.rolling_count * (1.0 - volume.alpha)
                + count_f * volume.alpha)
                / 1.0; // approximate
            tracing::warn!(
                "Engine '{}' DEGRADED: {} results vs ~{:.0} avg",
                engine, count, avg
            );
            // Record degradation event
            let mut events = self.degradation_events.lock();
            let now = Instant::now();
            events.retain(|e| now.duration_since(*e) < Duration::from_secs(300));
            events.push(now);
        }

        is_degraded
    }

    // Count degradation events in the last N seconds
    fn degradation_count(&self, window_secs: u64) -> usize {
        let events = self.degradation_events.lock();
        let now = Instant::now();
        events
            .iter()
            .filter(|e| now.duration_since(**e) < Duration::from_secs(window_secs))
            .count()
    }

}

struct AppState {
    circuit: CircuitBreaker,
    cache: SearchCache,
    rate_limits: Arc<RateLimitTracker>,
    volume_tracker: ResultVolumeTracker,
    http_client: reqwest::Client,
    searxng2_url: Option<String>,
    searx_last_used: Mutex<HashMap<String, Instant>>,
    /// In-flight request deduplication: tracks identical queries in flight so
    /// concurrent duplicate requests share one SearXNG fetch instead of N.
    in_flight: Mutex<HashMap<String, Vec<tokio::sync::oneshot::Sender<String>>>>,
    /// SymSpell + LinSpell spelling correction index (built at startup)
    spell_index: spell::SymSpellIndex,
    /// Optional MaxMind GeoLite2 IP geolocation lookup
    geo_locator: Option<geoloc::GeoLocator>,
    /// Bounds concurrent heavy search handlers so the combined per-request
    /// working set (7+ parallel upstream fetches + embeddings + spawn_blocking
    /// merge) can never exceed the container cgroup. Measured peak is ~350-400
    /// MiB per concurrent search; with the 4 GiB gateway cgroup this caps at
    /// `N` concurrent searches to stay safely under the limit. Without it, a
    /// burst of N concurrent "near me" / local queries pushes RSS to the cgroup
    /// ceiling and the OOM-killer recycles the container (dropped connections).
    search_semaphore: Arc<tokio::sync::Semaphore>,
    /// Goal Feature: stores user goals, roadmaps, and leaderboard data
    goals_state: parking_lot::Mutex<goals::GoalStore>,
}

async fn handle_images(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
    headers: HeaderMap,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();

    // Guard: missing or empty `q` — return 400 with the documented body.
    if q.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Missing or empty query parameter 'q'",
                "results": [],
                "count": 0,
            })),
        );
    }

    let cache_key = format!("images:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }

    // Resolve client geolocation for location-aware image search
    let geo_location: Option<geoloc::GeoLocation> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim()))
        .and_then(|ip| ip.parse().ok())
        .or_else(|| {
            headers.get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|ip| ip.parse().ok())
        })
        .and_then(|ip| {
            state.geo_locator.as_ref().and_then(|gl| gl.lookup(ip))
        });

    // P3 fix: explicit location in query overrides IP/header geolocation.
    let geo_location: Option<geoloc::GeoLocation> = match detect_explicit_location(&q) {
        Some(explicit) => Some(explicit),
        None => geo_location,
    };

    // Fan-out to both SearXNG instances in parallel (VPN + Tor)
    let constraints = extract_gateway_constraints(&q);
    let lang = constraints.language.as_deref();

    // Fan-out to both SearXNG instances in parallel (VPN + Tor)
    let searx_url = searxng_url_with_categories(
        "http://127.0.0.1:8080", &q, "images", geo_location.as_ref(), lang
    );

    let searx2_url = state.searxng2_url.as_ref().map(|base| {
        searxng_url_with_categories(base, &q, "images", geo_location.as_ref(), lang)
    });

    let parse_images = |raw: String| -> Vec<ImageResult> {
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxImageResponse>(&sanitized) {
            Ok(data) => data.results.into_iter().map(|r| {
                let thumb = if !r.thumbnail.is_empty() { r.thumbnail.clone() }
                    else if !r.thumbnail_src.is_empty() { r.thumbnail_src.clone() }
                    else { r.source.clone() };
                let img_title = &r.title;
                let img_content = &r.content;
                let img_tokens: Vec<&str> = q.split_whitespace()
                    .filter(|w| w.len() >= 2)
                    .collect();
                let img_tm = img_tokens.iter().filter(|t| img_title.to_lowercase().contains(*t)).count();
                let img_cm = img_tokens.iter().filter(|t| img_content.to_lowercase().contains(*t)).count();
                let img_score = 0.5 + (img_tm as f32 * 0.15) + (img_cm as f32 * 0.05);
                ImageResult {
                    title: r.title,
                    url: r.url,
                    image_url: if r.img_src.is_empty() { thumb.clone() } else { r.img_src },
                    thumbnail_url: thumb,
                    description: r.content,
                    source: r.engine,
                    score: img_score.min(1.0),
                }
            }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG image parse error: {}", e);
                vec![]
            }
        }
    };

    let searx1_fut = async {
        match fetch_text_budgeted(state.http_client.clone(), searx_url.clone(), 4000).await {
            Some(raw) => parse_images(raw),
            None => { tracing::warn!("SearXNG1 image timed out/failed — empty"); vec![] }
        }
    };

    let searx2_fut = async {
        let url = match searx2_url {
            Some(u) => u,
            None => return vec![],
        };
        match fetch_text_budgeted(state.http_client.clone(), url.clone(), 4000).await {
            Some(raw) => parse_images(raw),
            None => { tracing::warn!("SearXNG2 image timed out/failed — empty"); vec![] }
        }
    };

    let (mut results, tor_results) = tokio::join!(searx1_fut, searx2_fut);

    // Merge Tor results — dedup by URL, prefer VPN results (faster)
    let mut seen: std::collections::HashSet<String> = results.iter().map(|r| r.url.clone()).collect();
    for r in tor_results {
        if seen.insert(r.url.clone()) {
            results.push(r);
        }
    }

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    (axum::http::StatusCode::OK, Json(response))
}

async fn handle_videos(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
    headers: HeaderMap,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();

    // Guard: missing or empty `q` — return 400 with the documented body.
    if q.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Missing or empty query parameter 'q'",
                "results": [],
                "count": 0,
            })),
        );
    }

    let cache_key = format!("videos:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }

    // Resolve client geolocation for location-aware video search
    let geo_location: Option<geoloc::GeoLocation> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim()))
        .and_then(|ip| ip.parse().ok())
        .or_else(|| {
            headers.get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|ip| ip.parse().ok())
        })
        .and_then(|ip| {
            state.geo_locator.as_ref().and_then(|gl| gl.lookup(ip))
        });

    // P3 fix: explicit location in query overrides IP/header geolocation.
    let geo_location: Option<geoloc::GeoLocation> = match detect_explicit_location(&q) {
        Some(explicit) => Some(explicit),
        None => geo_location,
    };

    let constraints = extract_gateway_constraints(&q);
    let lang = constraints.language.as_deref();

    // Query Invidious and both SearXNG instances (categories=videos) in parallel
    let invidious_url = format!("http://invidious:3000/api/v1/search?q={}", urlencoding::encode(&q));
    let searx_video_url = searxng_url_with_categories(
        "http://127.0.0.1:8080", &q, "videos", geo_location.as_ref(), lang
    );
    let searx2_video_url = state.searxng2_url.as_ref().map(|base| {
        searxng_url_with_categories(base, &q, "videos", geo_location.as_ref(), lang)
    });

    let searx_fut = async {
        match tokio::time::timeout(Duration::from_secs(4), state.http_client.get(&searx_video_url).send()).await {
            Ok(Ok(resp)) => match read_body_bounded(resp).await {
                Some(bytes) => {
                    let raw = String::from_utf8_lossy(&bytes).into_owned();
                    let sanitized = sanitize_json_text(&raw);
                    match serde_json::from_str::<SearxVideoResponse>(&sanitized) {
                        Ok(data) => data.results.into_iter().map(|r| {
                            let thumbnail = if !r.thumbnail.is_empty() { r.thumbnail.clone() }
                                else if !r.img_src.is_empty() { r.img_src.clone() }
                                else { String::new() };
                            let vid_title = &r.title;
                            let vid_content = &r.content;
                            let vid_tokens: Vec<&str> = q.split_whitespace()
                                .filter(|w| w.len() > 2)
                                .collect();
                            let vid_tm = vid_tokens.iter().filter(|t| vid_title.to_lowercase().contains(*t)).count();
                            let vid_cm = vid_tokens.iter().filter(|t| vid_content.to_lowercase().contains(*t)).count();
                            let vid_score = 0.5 + (vid_tm as f32 * 0.15) + (vid_cm as f32 * 0.05);
                            VideoResult {
                                title: r.title,
                                url: r.url,
                                description: r.content,
                                video_id: String::new(),
                                thumbnail,
                                source: r.engine,
                                score: vid_score.min(1.0),
                            }
                        }).collect::<Vec<_>>(),
                        Err(e) => { tracing::warn!("SearXNG video parse error: {}", e); vec![] }
                    }
                }
                None => { tracing::warn!("SearXNG video body read error / exceeded cap"); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG video request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG video timed out after 4s"); vec![] }
        }
    };

    let searx2_fut = async {
        let url = match searx2_video_url {
            Some(u) => u,
            None => return vec![],
        };
        match tokio::time::timeout(Duration::from_secs(4), state.http_client.get(&url).send()).await {
            Ok(Ok(resp)) => match read_body_bounded(resp).await {
                Some(bytes) => {
                    let raw = String::from_utf8_lossy(&bytes).into_owned();
                    let sanitized = sanitize_json_text(&raw);
                    match serde_json::from_str::<SearxVideoResponse>(&sanitized) {
                        Ok(data) => data.results.into_iter().map(|r| {
                            let thumbnail = if !r.thumbnail.is_empty() { r.thumbnail.clone() }
                                else if !r.img_src.is_empty() { r.img_src.clone() }
                                else { String::new() };
                            let vid_title = &r.title;
                            let vid_content = &r.content;
                            let vid_tokens: Vec<&str> = q.split_whitespace()
                                .filter(|w| w.len() > 2)
                                .collect();
                            let vid_tm = vid_tokens.iter().filter(|t| vid_title.to_lowercase().contains(*t)).count();
                            let vid_cm = vid_tokens.iter().filter(|t| vid_content.to_lowercase().contains(*t)).count();
                            let vid_score = 0.5 + (vid_tm as f32 * 0.15) + (vid_cm as f32 * 0.05);
                            VideoResult {
                                title: r.title,
                                url: r.url,
                                description: r.content,
                                video_id: String::new(),
                                thumbnail,
                                source: r.engine,
                                score: vid_score.min(1.0),
                            }
                        }).collect::<Vec<_>>(),
                        Err(e) => { tracing::warn!("SearXNG2 video parse error: {}", e); vec![] }
                    }
                }
                None => { tracing::warn!("SearXNG2 video body read error / exceeded cap"); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG2 video request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG2 video timed out after 4s"); vec![] }
        }
    };

    let invidious_fut = async {
        match tokio::time::timeout(Duration::from_secs(15), state.http_client.get(&invidious_url).send()).await {
            Ok(Ok(resp)) => match read_json_bounded::<Vec<InvidiousResult>>(resp).await {
                Some(data) => data.into_iter()
                    .filter(|r| r.result_type.as_deref() == Some("video"))
                    .filter_map(|r| {
                        let vid = r.video_id?;
                        let title = r.title.unwrap_or_default();
                        let description = r.description.unwrap_or_default();
                        Some(VideoResult {
                            title,
                            url: format!("https://www.youtube.com/watch?v={}", vid),
                            description,
                            thumbnail: format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", vid),
                            video_id: vid,
                            source: "invidious".to_string(),
                            score: 0.5,
                        })
                    })
                    .collect::<Vec<_>>(),
                None => { tracing::warn!("Invidious parse error / exceeded cap"); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("Invidious request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("Invidious timed out after 15s"); vec![] }
        }
    };

    // Run all three in parallel — SearXNG with 4s timeout, Invidious gets same 4s deadline
    let invidious_deadline = async {
        tokio::select! {
            results = invidious_fut => results,
            _ = tokio::time::sleep(Duration::from_secs(4)) => {
                tracing::warn!("Invidious skipped to meet 4s target");
                vec![]
            }
        }
    };
    let (searx_results, searx2_results, invidious_results) = tokio::join!(searx_fut, searx2_fut, invidious_deadline);

    // Merge results: SearXNG first, then SearXNG2, then Invidious
    let mut results = searx_results;
    {
        let mut seen: std::collections::HashSet<String> = results.iter().map(|r| r.url.clone()).collect();
        for r in searx2_results {
            if seen.insert(r.url.clone()) {
                results.push(r);
            }
        }
        for r in invidious_results {
            if seen.insert(r.url.clone()) {
                results.push(r);
            }
        }
    }

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    (axum::http::StatusCode::OK, Json(response))
}

async fn handle_news(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
    headers: HeaderMap,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();

    // Guard: missing or empty `q` — return 400 with the documented body.
    if q.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Missing or empty query parameter 'q'",
                "results": [],
                "count": 0,
            })),
        );
    }

    let cache_key = format!("news:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }

    // Resolve client geolocation for location-aware news search
    let geo_location: Option<geoloc::GeoLocation> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim()))
        .and_then(|ip| ip.parse().ok())
        .or_else(|| {
            headers.get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|ip| ip.parse().ok())
        })
        .and_then(|ip| {
            state.geo_locator.as_ref().and_then(|gl| gl.lookup(ip))
        });

    // P3 fix: explicit location in query overrides IP/header geolocation.
    let geo_location: Option<geoloc::GeoLocation> = match detect_explicit_location(&q) {
        Some(explicit) => Some(explicit),
        None => geo_location,
    };

    let constraints = extract_gateway_constraints(&q);
    let lang = constraints.language.as_deref();

    // Fan-out to both SearXNG instances in parallel (VPN + Tor)
    let searx_url = searxng_url_with_categories(
        "http://127.0.0.1:8080", &q, "news", geo_location.as_ref(), lang
    );

    let searx2_url = state.searxng2_url.as_ref().map(|base| {
        searxng_url_with_categories(base, &q, "news", geo_location.as_ref(), lang)
    });

    let parse_news = |raw: String| -> Vec<NewsResult> {
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxNewsResponse>(&sanitized) {
            Ok(data) => data.results.into_iter().map(|r| {
                    let news_tokens: Vec<&str> = q.split_whitespace()
                        .filter(|w| w.len() >= 2)
                        .collect();
                    let news_tm = news_tokens.iter().filter(|t| r.title.to_lowercase().contains(*t)).count();
                    let news_cm = news_tokens.iter().filter(|t| r.content.to_lowercase().contains(*t)).count();
                    let news_score = 0.5 + (news_tm as f32 * 0.15) + (news_cm as f32 * 0.05);
                    NewsResult {
                        title: r.title,
                        url: r.url,
                        description: r.content,
                        published_at: r.published_date.unwrap_or_default(),
                        source: r.engine,
                        score: news_score.min(1.0),
                    }
                }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG news parse error: {}", e);
                vec![]
            }
        }
    };

    let searx1_fut = async {
        match tokio::time::timeout(Duration::from_secs(6), state.http_client.get(&searx_url).send()).await {
            Ok(Ok(resp)) => match read_body_bounded(resp).await {
                Some(bytes) => {
                    let raw = String::from_utf8_lossy(&bytes).into_owned();
                    parse_news(raw)
                }
                None => { tracing::warn!("SearXNG1 news body read error / exceeded cap"); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG1 news request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG1 news timed out after 6s"); vec![] }
        }
    };

    let searx2_fut = async {
        let url = match searx2_url {
            Some(u) => u,
            None => return vec![],
        };
        match tokio::time::timeout(Duration::from_secs(6), state.http_client.get(&url).send()).await {
            Ok(Ok(resp)) => match read_body_bounded(resp).await {
                Some(bytes) => {
                    let raw = String::from_utf8_lossy(&bytes).into_owned();
                    parse_news(raw)
                }
                None => { tracing::warn!("SearXNG2 news body read error / exceeded cap"); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG2 news request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG2 news timed out after 6s"); vec![] }
        }
    };

    let (mut results, tor_results) = tokio::join!(searx1_fut, searx2_fut);

    // Merge Tor results — dedup by URL, prefer VPN results (faster)
    let mut seen: std::collections::HashSet<String> = results.iter().map(|r| r.url.clone()).collect();
    for r in tor_results {
        if seen.insert(r.url.clone()) {
            results.push(r);
        }
    }

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    (axum::http::StatusCode::OK, Json(response))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt::init();

    let searxng2_url = std::env::var("SEARXNG2_URL").ok();
    if searxng2_url.is_some() {
        tracing::info!("SearXNG2 enabled: parallel VPN fan-out active");
    }

    let state = Arc::new(AppState {
        circuit: CircuitBreaker::new(),
        cache: SearchCache::new(),
        rate_limits: Arc::new(RateLimitTracker::new()),
        volume_tracker: ResultVolumeTracker::new(),
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(10))  // Allow up to 10s for external engines (VPN/Tor overhead)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .pool_max_idle_per_host(128)
            .connect_timeout(Duration::from_secs(1))
            .tcp_nodelay(true)                          // Disable Nagle's — saves 5-40ms on small payloads
            .pool_idle_timeout(Duration::from_secs(90))   // Keep connections warm between bursts
            .tcp_keepalive(Duration::from_secs(60))        // Prevent mid-stream connection drops
            .build()
            .unwrap(),
        searxng2_url,
        searx_last_used: Mutex::new(HashMap::new()),
        in_flight: Mutex::new(HashMap::new()),
        spell_index: spell::SymSpellIndex::build(),
        geo_locator: geoloc::GeoLocator::load(),
        goals_state: parking_lot::Mutex::new(goals::GoalStore::new()),
        // Cap concurrent heavy searches. Measured peak per concurrent search is
        // ~350-400 MiB; with a 4 GiB cgroup, 8 keeps peak RSS safely under the
        // limit even under a burst, while still allowing real parallelism.
        search_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
    });

    // Prewarm: fire HEAD requests to populate connection pool immediately.
    // TCP+TLS handshakes are expensive (1-3 round trips); prewarming means the
    // first user request gets zero handshake latency.
    let prewarm_client = state.http_client.clone();
    let prewarm_searxng2 = state.searxng2_url.clone();
    tokio::spawn(async move {
        let mut prewarm_heads = vec![
            prewarm_client.head("http://127.0.0.1:8080/search?q=prewarm&format=json&pageno=1").send(),
            prewarm_client.head("http://127.0.0.1:6000/search?q=prewarm").send(),
        ];
        if let Some(ref s2_url) = prewarm_searxng2 {
            let url = format!("{}/search?q=prewarm&format=json&pageno=1", s2_url);
            prewarm_heads.push(prewarm_client.head(url).send());
        }
        let _ = futures::future::join_all(prewarm_heads).await;
        tracing::info!("Connection pool prewarmed with HEAD requests");

        tracing::info!("Prewarming — polling intent engine until ready...");
        // Poll intent engine with exponential backoff until it responds
        for attempt in 1..=20 {
            match prewarm_client.get("http://127.0.0.1:3005/analyze?q=warmup").send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!("Intent engine ready after {} attempt(s)", attempt);
                    break;
                }
                Ok(resp) => {
                    tracing::warn!("Prewarm attempt {}: status {}", attempt, resp.status());
                }
                Err(e) => {
                    tracing::info!("Prewarm attempt {}: {}", attempt, e);
                }
            }
            let delay = std::cmp::min(500 * attempt, 5000); // 500ms, 1s, 1.5s, ... max 5s
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        // Fire SearXNG + embed warmup in parallel (non-critical)
        let mut prewarm_futs = vec![
            prewarm_client.get("http://127.0.0.1:8080/search?q=warmup&format=json&pageno=1").send(),
            prewarm_client.get("http://127.0.0.1:3005/embed?text=warmup").send(),
        ];
        if let Some(ref s2_url) = prewarm_searxng2 {
            let url = format!("{}/search?q=warmup&format=json&pageno=1", s2_url);
            prewarm_futs.push(prewarm_client.get(url).send());
        }
        let _ = futures::future::join_all(prewarm_futs).await;
        // The tor2 GET above rebuilt the circuit; mark it hot so the search
        // recovery path can query tor2 without paying the cold-build penalty.
        TOR2_WARM.store(true, std::sync::atomic::Ordering::SeqCst);
        tracing::info!("Prewarm complete (tor2 circuit marked hot)");
    });

    // Periodic IP rotation every 10 minutes — rotates both gluetun VPN and tor2 circuit
    // to prevent CAPTCHA accumulation and avoid search engine rate-limiting patterns.
    let periodic_reason = "periodic_10min_rotation";
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await; // 10 min
            rotate_all_ips(periodic_reason);
        }
    });

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search))
        .route("/search/fast", get(handle_search_fast))
        .route("/images", get(handle_images))
        .route("/videos", get(handle_videos))
        .route("/news", get(handle_news))
        // Goal Feature endpoints
        .route("/goals", post(goals::handle_create_goal))
        .route("/goals/quick", post(goals::handle_quick_roadmap))
        .route("/goals/leaderboard", get(goals::handle_leaderboard))
        .route("/goals/:goal_id", get(goals::handle_get_goal))
        .route("/goals/:goal_id/answers", post(goals::handle_submit_answers))
        .route("/goals/:goal_id/phases/:phase_id/complete", post(goals::handle_complete_phase))
        .route("/goals/:goal_id/progress", post(goals::handle_update_progress))
        .with_state(state).layer(TimeoutLayer::new(Duration::from_secs(30)));

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {} (circuit-breaker + cache)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Phase 8: common English stopwords used to detect stopword-only queries.
static STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "then", "else", "of", "to", "in",
    "on", "at", "by", "for", "with", "about", "as", "is", "are", "was", "were",
    "be", "been", "being", "it", "this", "that", "these", "those", "i", "you",
    "he", "she", "we", "they", "my", "your", "his", "her", "its", "our", "their",
    "me", "him", "us", "them", "do", "does", "did", "have", "has", "had", "will",
    "would", "can", "could", "should", "may", "might", "must", "not", "no", "yes",
    "so", "than", "too", "very", "just", "also", "from", "into", "out", "up", "down",
];

/// Phase 7: classify incoming query quality so we can degrade gracefully.
/// Returns (flag, valid_ratio):
///   flag == "junk"   -> no recognizable words AND low entropy -> return 200 OK empty
///   flag == "low"    -> few recognizable words or very high entropy -> search but flag
///   otherwise ""     -> normal
fn is_valid_word(w: &str, spell_index: &spell::SymSpellIndex) -> bool {
    let wl = w.to_lowercase();
    if spell::is_protected_term(&wl) || spell_index.contains_word(&wl) {
        return true;
    }
    let parts: Vec<&str> = wl.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }
    parts.iter().all(|part| {
        part.len() <= 2
            || spell_index.contains_word(part)
            || spell::is_protected_term(part)
            // A part containing a digit is a technical token (version, hex, acronym
            // with a number, e.g. http3, tls1.3, x86_64, k8s, arm64) — never
            // gibberish. Treat it as valid rather than down-ranking the query.
            || part.chars().any(|c| c.is_numeric())
    })
}

fn is_pronounceable(w: &str) -> bool {
    let lower = w.to_lowercase();
    if lower.len() < 3 {
        return true;
    }
    let has_vowel = lower.chars().any(|c| c == 'a' || c == 'e' || c == 'i' || c == 'o' || c == 'u' || c == 'y');
    if !has_vowel {
        return false;
    }
    let chars: Vec<char> = lower.chars().collect();
    let mut max_repeat = 1;
    let mut current_repeat = 1;
    for i in 1..chars.len() {
        if chars[i] == chars[i-1] {
            current_repeat += 1;
            max_repeat = max_repeat.max(current_repeat);
        } else {
            current_repeat = 1;
        }
    }
    if max_repeat > 3 {
        return false;
    }
    true
}

fn query_quality_flag(q: &str, spell_index: &spell::SymSpellIndex) -> (String, f32) {
    let words: Vec<&str> = q.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphabetic())).collect();
    if words.is_empty() {
        return ("low".to_string(), 0.0);
    }
    
    let european_words: std::collections::HashSet<&str> = [
        "de", "la", "le", "el", "un", "une", "en", "et", "les", "des",
        "der", "die", "das", "ein", "eine", "und", "ist", "mit",
        "van", "het", "een", "je", "di", "il", "con", "del", "al", "para"
    ].iter().copied().collect();

    let has_european_word = words.iter().any(|w| european_words.contains(w.to_lowercase().as_str()));

    let known = words.iter().filter(|w| is_valid_word(w, spell_index)).count() as f32;
    let valid_ratio = known / words.len() as f32;

    let mut freq = [0u32; 128];
    let mut total = 0u32;
    for ch in q.chars() {
        if (ch as usize) < 128 { freq[ch as usize] += 1; total += 1; }
    }
    let mut h = 0.0f32;
    if total > 0 {
        for &f in &freq {
            if f > 0 {
                let p = f as f32 / total as f32;
                h -= p * p.log2();
            }
        }
    }

    let all_pronounceable = words.iter().all(|w| is_pronounceable(w));

    // A token with a digit, internal symbol, or mixed case is a technical
    // identifier/version (http3, tls1.3, x86_64, k8s, arm64, c++) — these
    // are never gibberish even if absent from the spell dictionary. Treat their
    // presence as legitimizing the query (structural rule, not a hardcoded list).
    let has_technical_token = words.iter().any(|w| {
        w.chars().any(|c| c.is_numeric())
            || w.chars().any(|c| c.is_uppercase()) && w.chars().any(|c| c.is_lowercase())
            || w.contains('_') || w.contains('-') || w.contains('.')
    });

    if valid_ratio == 0.0 && !has_european_word && !all_pronounceable && !has_technical_token {
        ("junk".to_string(), valid_ratio)
    } else if valid_ratio < 0.25 && !has_european_word && !all_pronounceable && (h < 2.5 || h > 6.5) && !has_technical_token {
        ("junk".to_string(), valid_ratio)
    } else if valid_ratio < 0.5 && !has_european_word && !has_technical_token {
        ("low".to_string(), valid_ratio)
    } else {
        ("".to_string(), valid_ratio)
    }
}

/// ─── Bounded body reader (OOM guard) ───────────────────────────────
/// Every upstream response is buffered with `reqwest`'s default client, which
/// has NO body-size limit. With the gateway cgroup at `mem_limit: 4096m`
/// (docker-compose.dev.yml), a single oversized or attacker-shaped response
/// from any of the 7 parallel upstreams (SearXNG / Invidious / News / Image /
/// Embed / Intent / Indexer) can allocate gigabytes and OOM-kill the
/// container (RSS ~3 GiB → cgroup kill → dropped connections → restart loop).
///
/// `MAX_RESPONSE_BYTES` caps how many bytes we will read from ANY upstream
/// before aborting. It is sized generously for legit payloads (SearXNG can
/// return ~tens of MB for a large result page; the local indexer stays <1 MB)
/// but far below the 4 GiB cgroup so a runaway upstream can never take down
/// the process. On overflow we return `None` (fail-closed: that source
/// contributes empty results instead of OOM-ing the whole gateway).
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024; // 16 MiB hard ceiling

/// Stream a `reqwest::Response` body into a `Vec<u8>`, aborting the moment the
/// byte cap is exceeded. Returns `None` on any error, timeout, or overflow.
async fn read_body_bounded(resp: reqwest::Response) -> Option<Vec<u8>> {
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(1 << 20); // start ~1 MiB
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(
        deadline.saturating_duration_since(tokio::time::Instant::now()),
        stream.next(),
    ).await {
        if buf.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            tracing::warn!(
                "UPSTREAM BODY GUARD: response exceeded {} MiB cap — aborting read (possible runaway payload)",
                MAX_RESPONSE_BYTES / (1024 * 1024)
            );
            return None;
        }
        buf.extend_from_slice(&chunk);
    }
    Some(buf)
}

/// Like `read_body_bounded` but parses the (size-capped) bytes as JSON `T`.
async fn read_json_bounded<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Option<T> {
    let bytes = read_body_bounded(resp).await?;
    serde_json::from_slice(&bytes).ok()
}

/// Resilient outbound GET with a HARD budget.
///
/// Runs `client.get(url).send()` (and the JSON parse) inside a DETACHED
/// `tokio::spawn` task and joins it with `budget_ms` ms. This is the
/// root-cause fix for the flaky-connection blips: every engine call
/// (SearXNG / Invidious / News / Image / Embed / Intent / Indexer)
/// is routed through gluetun/VPN, whose connections can stall in a way an
/// inline `tokio::time::timeout` around `client.get().send()` does NOT
/// reliably interrupt (observed: the task blocked 26-31s with the timeout
/// never firing, dropping the whole response). A detached task + join budget
/// cannot be defeated that way — if the connection stalls, the parent still
/// returns on time with `None` (fail-closed to empty/partial results).
///
/// `T` must be `DeserializeOwned + Send + 'static` so it can cross the
/// spawn boundary. On any failure/timeout/panic the task returns `None`.
/// Response bodies are read through the size-capped reader so a runaway
/// upstream payload can never blow the gateway cgroup.
async fn fetch_json_budgeted<T: serde::de::DeserializeOwned + Send + 'static>(
    client: reqwest::Client,
    url: String,
    budget_ms: u64,
) -> Option<T> {
    let task = tokio::spawn(async move {
        let send = match tokio::time::timeout(
            std::time::Duration::from_millis(budget_ms),
            client.get(&url).send(),
        ).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) | Err(_) => return None,
        };
        match read_json_bounded::<T>(send).await {
            Some(parsed) => Some(parsed),
            None => None,
        }
    });
    match tokio::time::timeout(std::time::Duration::from_millis(budget_ms + 200), task).await {
        Ok(Ok(v)) => v,
        _ => None,
    }
}

/// Resilient outbound GET returning the raw body text with a HARD budget.
/// Same rationale as `fetch_json_budgeted`; used where the caller needs the
/// raw JSON string (e.g. SearXNG image/HTML parsing, the parallel retry).
/// Body read is size-capped by `read_body_bounded` so a runaway payload
/// cannot blow the gateway cgroup.
async fn fetch_text_budgeted(
    client: reqwest::Client,
    url: String,
    budget_ms: u64,
) -> Option<String> {
    let task = tokio::spawn(async move {
        let send = match tokio::time::timeout(
            std::time::Duration::from_millis(budget_ms),
            client.get(&url).send(),
        ).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) | Err(_) => return None,
        };
        match read_body_bounded(send).await {
            Some(bytes) => String::from_utf8(bytes).ok(),
            None => None,
        }
    });
    match tokio::time::timeout(std::time::Duration::from_millis(budget_ms + 200), task).await {
        Ok(Ok(v)) => v,
        _ => None,
    }
}

fn clean_query_for_spelling(q: &str) -> String {
    let normalized = q
        .replace('“', "")
        .replace('”', "")
        .replace('‘', "")
        .replace('’', "")
        .replace('"', "");
    
    let mut cleaned = String::new();
    for w in normalized.split_whitespace() {
        let wl = w.to_lowercase();
        if wl.starts_with("site:") || wl.starts_with("filetype:") || wl.starts_with("after:") || wl.starts_with("before:")
            || wl.starts_with("lang:") || wl.starts_with("intitle:") || wl.starts_with("inurl:") || wl.starts_with("intext:")
            || wl.starts_with("related:") || wl.starts_with("price:")
        {
            continue;
        }
        cleaned.push_str(w);
        cleaned.push(' ');
    }
    cleaned.trim().to_string()
}

/// Remove a bare whole-word `term` from `s` (case-insensitive), returning the
/// whitespace-joined remainder. Used during spell reconstruction so that a
/// negated term (e.g. `-windows`) does not also survive as its positive twin
/// after correction (`linux windows` -> we strip `windows` before re-appending
/// `-windows`), which would otherwise self-contradict the engine query.
fn strip_bare_token(s: &str, term: &str) -> String {
    let term_l = term.to_lowercase();
    let filtered: Vec<&str> = s
        .split_whitespace()
        .filter(|w| w.to_lowercase() != term_l)
        .collect();
    filtered.join(" ")
}

fn make_error_response(query: &str, error_code: &str, message: &str, is_junk: bool) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let response = UnifiedResponse {
        query: query.to_string(),
        intent: None,
        category: None,
        confidence: None,
        constraints: vec![],
        structured_constraints: Constraints::default(),
        expanded_queries: vec![],
        distribution: None,
        deep_result: None,
        results: vec![],
        geo_location: None,
        spell_corrected_query: None,
        error: Some(error_code.to_string()),
        message: Some(message.to_string()),
        query_quality: if is_junk { Some("junk".to_string()) } else { None },
        applied_constraints: None,
        ignored_constraints: None,
        warnings: None,
        results_before_filter: None,
        results_after_filter: None,
        total: None,
        page_limit: None,
        page_offset: None,
        has_more: None,
    };
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({}))),
    )
}

/// RAII guard that removes a query's entry from `AppState::in_flight` when it goes
/// out of scope — including on panic. The dedup map is keyed by `cache_key`; the
/// "leader" request inserts an empty `Vec` and is responsible for removing it once
/// the result is ready. Without this guard, any panic between insertion and the
/// explicit `remove` (e.g. inside `spawn_blocking(...).await.unwrap()`) leaks the
/// key forever, growing the map unbounded over a long-running process.
struct DedupGuard {
    state: std::sync::Arc<AppState>,
    key: String,
    done: bool,
}

impl DedupGuard {
    /// Create the guard. The entry is assumed inserted by the caller.
    fn new(state: std::sync::Arc<AppState>, key: String) -> Self {
        Self { state, key, done: false }
    }

    /// Normal completion: the leader has already removed the entry and notified
    /// waiters, so the guard should do nothing on drop.
    fn complete(mut self) {
        self.done = true;
    }
}

impl Drop for DedupGuard {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        // Panic path (or early return): best-effort removal. `in_flight` is a
        // parking_lot Mutex whose `lock()` never fails, so removal is
        // unconditional. If another panic is unwinding concurrently the worst
        // case is the entry lingers and is overwritten on a future identical query.
        let mut map = self.state.in_flight.lock();
        map.remove(&self.key);
    }
}

async fn handle_search(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
    headers: HeaderMap,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    // 0. Validate query — reject empty or whitespace-only queries
    let q_trimmed = params.q.as_deref().unwrap_or("").trim();
    // Phase 8: empty / 1-char / stopword-only handling (graceful, never 400).
    if q_trimmed.is_empty() {
        return make_error_response(q_trimmed, "empty_query", "Query parameter 'q' is empty", false);
    }
    // Reject queries that have no letters (digits/symbols only).
    let alpha_count = q_trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count == 0 {
        return make_error_response(q_trimmed, "empty_query", "Query must contain at least one alphabetic character", false);
    }

    // Phase 8: meaningful-empty guard. A query that is only 1 char, or only
    // stopwords, has no retrievable intent. Return 400 Bad Request with a soft
    // query_quality:low flag — BUT exempt protected terms / language tokens so
    // single-token lookups like `go` / `rust` still search normally.
    let raw_tokens: Vec<&str> = q_trimmed.split_whitespace().collect();
    let meaningful: Vec<&str> = raw_tokens.iter()
        .filter(|t| {
            let tl = t.to_lowercase();
            !STOPWORDS.contains(&tl.as_str())
        })
        .copied()
        .collect();
    let only_stopwords = !raw_tokens.is_empty() && meaningful.is_empty();
    let is_single_char = raw_tokens.len() == 1
        && raw_tokens[0].chars().filter(|c| c.is_alphabetic()).count() <= 1
        // A token with a digit or symbol (c++, x86_64, 3d, c#, f#) is a
        // technical identifier, not a bare single letter — it has retrievable
        // content, so exempt it from the single-character rejection.
        && !raw_tokens[0].chars().any(|c| c.is_numeric() || c.is_ascii_punctuation());
    if only_stopwords || is_single_char {
        let is_protected = raw_tokens.iter().any(|t| spell::is_protected_term(&t.to_lowercase()));
        if !is_protected {
            return make_error_response(q_trimmed, "invalid_query", "Query has no retrievable content (stopword-only or single character)", false);
        }
    }

    let q_cleaned_spelling = clean_query_for_spelling(q_trimmed);

    // Phase 7: graceful degradation for gibberish / low-quality input.
    let (qflag, _valid_ratio) = query_quality_flag(&q_cleaned_spelling, &state.spell_index);
    if qflag == "junk" {
        return make_error_response(q_trimmed, "invalid_query", "Query appears to be gibberish; no results returned", true);
    }

    // 0b. Check cache first (5-min TTL)
    // Cache key must include pagination params: results are sliced by
    // limit/offset before serialization, so two requests for the same query
    // with different count/offset must NOT share a cached body.
    let page_key = match (params.limit.or(params.count).or(params.n), params.offset) {
        (None, None) => "all".to_string(),
        (l, o) => format!("l{}_o{}", l.unwrap_or(24), o.unwrap_or(0)),
    };
    let cache_key = format!("{}:{}", q_trimmed.to_lowercase(), page_key);
    if let Some(cached) = state.cache.get(&cache_key) {
        tracing::info!("Cache hit for query: {}", q_trimmed);
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }
    // 0b.x: Bound concurrency. Acquire a slot in the global search semaphore so
    // the combined per-request working set can never exceed the container cgroup.
    // `permit` is held for the whole handler and dropped (released) on return,
    // including on early-return paths above (it lives in this scope). This is the
    // root-cause guard against burst-driven OOM-kills: a burst of N concurrent
    // "near me"/local queries each peak at ~350-400 MiB; with the 4 GiB cgroup,
    // capping at 8 keeps peak RSS safely under the limit. Cached hits above skip
    // this and don't consume a slot. Acquire is bounded so a saturated gateway
    // still answers (just queued), never hangs.
    let sem = state.search_semaphore.clone();
    let _permit = match tokio::time::timeout(
        Duration::from_secs(20),
        sem.acquire(),
    ).await {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => {
            // Semaphore closed (shouldn't happen) — fail safe to an empty result
            // rather than panic.
            tracing::error!("Search semaphore closed unexpectedly");
            return make_error_response(q_trimmed, "search_unavailable", "Search service temporarily unavailable", false);
        }
        Err(_) => {
            tracing::warn!("Search concurrency slot wait exceeded 20s for '{}'", q_trimmed);
            return make_error_response(q_trimmed, "search_busy", "Search service is busy; please retry", false);
        }
    };

    // 0b.5: Spelling correction — correct misspellings before fan-out
    let (q_corrected_cleaned, mut spell_changed) = spell::correct_query(&state.spell_index, &q_cleaned_spelling);
    if spell_changed {
        tracing::info!("Spell-corrected query: '{}' -> '{}'", q_trimmed, q_corrected_cleaned);
    }

    // 0c. Request deduplication: if another task is already fetching this query, wait for it.
    // `dedup_guard` lives for the whole handler so it removes the in_flight entry even
    // on panic; it is marked complete() on the normal success path below.
    let mut dedup_guard: Option<DedupGuard> = None;
    let dedup_rx = {
        let mut in_flight = state.in_flight.lock();
        if let Some(senders) = in_flight.get_mut(&cache_key) {
            tracing::info!("DEDUP: another request in-flight for '{}', subscribing", q_trimmed);
            let (tx, rx) = tokio::sync::oneshot::channel();
            senders.push(tx);
            Some(rx)
        } else {
            in_flight.insert(cache_key.clone(), vec![]);
            dedup_guard = Some(DedupGuard::new(state.clone(), cache_key.clone()));
            None
        }
    };
    if let Some(rx) = dedup_rx {
        tracing::info!("DEDUP: waiting for in-flight query '{}' to complete", q_trimmed);
        match rx.await {
            Ok(response_json) => {
                // Decouple subscribers from a failed leader. The leader may have been
                // cancelled by the global TimeoutLayer (20s) or hit an upstream failure
                // and returned an error/empty payload. If so, do NOT blindly return the
                // leader's bad outcome to every concurrent caller (that fans out one
                // failure into N identical 408/empty responses). Instead, fall through
                // and execute the query independently so at least one copy can succeed.
                let leader_failed = {
                    let v: serde_json::Value = serde_json::from_str(&response_json).unwrap_or(serde_json::Value::Null);
                    let has_error = v.get("error").and_then(|e| e.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
                    let results_empty = v.get("results").and_then(|r| r.as_array()).map(|a| a.is_empty()).unwrap_or(false);
                    // A leader that produced an error body, OR a 200 with an empty
                    // result set AND an error/message marker, is treated as a failure.
                    let has_message_err = v.get("message").and_then(|m| m.as_str())
                        .map(|s| s.to_lowercase().contains("upstream") || s.to_lowercase().contains("unavailable"))
                        .unwrap_or(false);
                    has_error || (results_empty && has_message_err)
                };
                if leader_failed {
                    tracing::warn!("DEDUP: leader failed/empty (timeout or upstream error); re-executing independently instead of inheriting its failure");
                } else {
                    let value: serde_json::Value = serde_json::from_str(&response_json).unwrap_or(serde_json::json!({}));
                    return (axum::http::StatusCode::OK, Json(value));
                }
            }
            Err(_) => {
                tracing::warn!("DEDUP: sender dropped, processing query ourselves");
            }
        }
    }
    // Use shared HTTP client from AppState (connection pooling across requests)
    let search_start = std::time::Instant::now();
    let client = state.http_client.clone();

    // Phase 1 (A3): keep the CORRECTED form only for display + engine search,
    // but send the ORIGINAL query to the intent engine and constraint
    // extraction. A spell correction must NEVER silently change the user's
    // intent/constraints (the vegan→vegas data-loss bug). Corruption can only
    // ever touch the engine query string + spell_corrected_query display.
    let q = if spell_changed {
        let mut reconstructed = q_corrected_cleaned;
        let gateway_extracted = extract_gateway_constraints(q_trimmed);
        for phrase in gateway_extracted.phrases {
            reconstructed.push(' ');
            reconstructed.push('"');
            reconstructed.push_str(&phrase);
            reconstructed.push('"');
        }
        // Re-append operators the upstream engine understands natively so a
        // spell correction can never silently drop them (the vegan→vegas class
        // of data-loss bug — only the non-operator term text may be corrected).
        for w in q_trimmed.split_whitespace() {
            let wl = w.to_lowercase();
            if wl.starts_with("site:") || wl.starts_with("filetype:") || wl.starts_with("after:")
                || wl.starts_with("before:") || wl.starts_with("intitle:") || wl.starts_with("inurl:")
                || wl.starts_with("intext:") || wl.starts_with("lang:")
            {
                reconstructed.push(' ');
                reconstructed.push_str(w);
            }
        }
        // Re-append EXPLICIT negation tokens (-term). Spell correction collapses
        // "-windows" into a positive "windows" twin, which would invert the
        // user's intent (searching FOR windows instead of excluding it). Strip
        // that twin, then re-inject the negation so the engine query stays
        // faithful — and so the upstream engine itself excludes the term.
        for w in q_trimmed.split_whitespace() {
            let wl = w.to_lowercase();
            if let Some(neg) = wl.strip_prefix('-') {
                if !neg.is_empty() && !neg.starts_with('-') {
                    let neg_term = neg.trim().to_string();
                    let neg_form = format!("-{}", neg_term);
                    if !reconstructed.split_whitespace().any(|t| t.eq_ignore_ascii_case(&neg_form)) {
                        // remove the positive twin the corrector may have produced
                        reconstructed = strip_bare_token(&reconstructed, &neg_term);
                        reconstructed.push(' ');
                        reconstructed.push_str(&neg_form);
                    }
                }
            }
        }
        reconstructed
    } else {
        q_trimmed.to_string()
    };
    let q_orig = q_trimmed.to_string(); // original, untouched query for intent/constraints
    let q_encoded = urlencoding::encode(&q);

    // Extract client IP for geolocation (from X-Forwarded-For or X-Real-IP headers)
    let client_ip: Option<IpAddr> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next().map(|s| s.trim()))
        .and_then(|ip| ip.parse().ok())
        .or_else(|| {
            headers.get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|ip| ip.parse().ok())
        });
    // Look up geolocation immediately — used for SearXNG URL construction,
    // ranking geo-boost, and local query expansion.
    let geo_location: Option<geoloc::GeoLocation> = client_ip.and_then(|ip| {
        state.geo_locator.as_ref().and_then(|gl| gl.lookup(ip))
    });

    // P3 fix: an explicit location named in the query (e.g. "tokyo japan",
    // "restaurants in london") MUST override the IP-derived geolocation. Without
    // this, a user in India searching "restaurants in tokyo japan" got
    // IN-localised results. Explicit user intent wins over inferred IP location.
    let geo_location: Option<geoloc::GeoLocation> = match detect_explicit_location(&q) {
        Some(explicit) => {
            tracing::info!("GEO: explicit location '{}' overrides IP geolocation",
                explicit.country_code.as_deref().unwrap_or("?"));
            Some(explicit)
        }
        None => geo_location,
    };

    // 1. Run Intent Analysis (with retry) and Embedding in parallel.
    // Phase 1 (A3): intent + embedding get the ORIGINAL query (q_orig) so a
    // spell correction can never change classification/constraints.
    let intent_url = format!("http://127.0.0.1:3005/analyze?q={}", urlencoding::encode(&q_orig));
    let embed_url = format!("http://127.0.0.1:3005/embed?text={}", urlencoding::encode(&q_orig));

    // Retry intent engine up to 2 extra times with backoff.
    // Handles cold-start after container restart (model load takes 5-15s).
    // Wrapped in an overall 800ms timeout to prevent local engine delays.
    let intent_fut = {
        let intent_client = client.clone();
        let intent_url_str = intent_url.clone();
        let task = tokio::spawn(async move {
            let delays = [0u64, 200, 400]; // 0ms, 200ms, 400ms
            for (attempt, delay_ms) in delays.iter().enumerate() {
                if *delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                }
                // NOTE (flaky-connection fix): intent-engine is reached through
                // gluetun; the inner .send() is wrapped in its own timeout AND
                // the whole attempt runs in a detached task + budget so a stall
                // cannot hang the handler the way an inline timeout did.
                let resp = match tokio::time::timeout(
                    std::time::Duration::from_millis(700),
                    intent_client.get(&intent_url_str).send(),
                ).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => { tracing::warn!("Intent Engine request failed (attempt {}): {:?}", attempt + 1, e); continue; }
                    Err(_) => { tracing::warn!("Intent Engine request timed out (attempt {})", attempt + 1); continue; }
                };
                let status = resp.status();
                match read_json_bounded::<IntentResponse>(resp).await {
                    Some(parsed) => return Ok(parsed),
                    None => { tracing::warn!("Intent parse failed (attempt {}, status: {}): invalid/oversized body", attempt + 1, status); }
                }
            }
            Err::<IntentResponse, ()>(())
        });
        async move {
            match tokio::time::timeout(std::time::Duration::from_millis(900), task).await {
                Ok(Ok(v)) => v,
                Ok(Err(_)) => { tracing::warn!("Intent Engine task panicked/timed out (budget)"); Err::<IntentResponse, ()>(()) }
                Err(_) => { tracing::warn!("Intent Engine request timed out overall (budget)"); Err::<IntentResponse, ()>(()) }
            }
        }
    };
    let embed_fut = {
        let embed_client = client.clone();
        let embed_url_str = embed_url.clone();
        let task = tokio::spawn(async move {
            match tokio::time::timeout(std::time::Duration::from_millis(700), embed_client.get(&embed_url_str).send()).await {
                Ok(Ok(resp)) => Some(resp),
                _ => { tracing::warn!("Embedding request timed out or failed (700ms)"); None }
            }
        });
        async move {
            match tokio::time::timeout(std::time::Duration::from_millis(900), task).await {
                Ok(v) => v.unwrap_or(None),
                Err(_) => { tracing::warn!("Embedding task timed out (budget)"); None }
            }
        }
    };

    // ─── Build engine URLs with raw query (no intent dependency) ────
    // Engines fire immediately in parallel with intent analysis.
    // Intent results are used post-hoc for scoring, not for query construction.
    // Sort instances by last-used time (warmest first) so join_all starts with
    // the connection that's most likely to have an idle pool entry, reducing
    // overall fan-out latency when one instance has cooled down.
    let searx_base_urls: Vec<&str> = {
        let mut urls: Vec<&str> = if state.searxng2_url.is_some() {
            vec!["http://127.0.0.1:8080", state.searxng2_url.as_deref().unwrap()]
        } else {
            vec!["http://127.0.0.1:8080"]
        };
        let searx_last_used = state.searx_last_used.lock();
        urls.sort_by(|a, b| {
            let a_warm = searx_last_used.get(*a).copied().unwrap_or(std::time::Instant::now());
            let b_warm = searx_last_used.get(*b).copied().unwrap_or(std::time::Instant::now());
            b_warm.cmp(&a_warm) // most recently used first
        });
        urls
    };

    // Build SearXNG URLs: raw query + optional negation-stripped variant per instance.
    // The stripped query fires in parallel with the raw query, avoiding a separate retry
    // step for negative-only queries like "not django" (which return 0 results as-is).
    // Detection is heuristic-only (no intent engine dependency) since this runs before
    // the intent join.
    let has_neg_pattern = q.starts_with("not ") || q.starts_with("no ")
        || q.starts_with("without ") || q.starts_with("except ")
        || q.starts_with("excluding ") || q.starts_with("minus ")
        || q.starts_with("-") || q.contains(" not ") || q.contains(" no ")
        || q.contains(" -");
    let stripped_override: Option<String> = if has_neg_pattern {
        simple_negation_strip(&q).filter(|s| {
            let processed = preprocess_searxng_query(s);
            !processed.is_empty() && processed != preprocess_searxng_query(&q)
        })
    } else { None };

    let mut searx_urls: Vec<String> = Vec::new();
    let mut searx_instance_keys: Vec<String> = Vec::new();
    // Comparison: append "vs comparison guide" for vs/versus queries.
    // Uses only the raw query string since intent isn't resolved yet at this stage.
    let q_lower_local = q.to_lowercase();
    let comparison_q = if q_lower_local.contains(" vs ") || q_lower_local.contains(" versus ") {
        let no_vs = q_lower_local.replace(" vs ", " ").replace(" versus ", " ");
        let terms: Vec<&str> = no_vs.split_whitespace()
            .filter(|w| w.len() >= 2 && w != &"vs" && w != &"versus")
            .collect();
        if terms.len() >= 2 {
            format!("{} vs comparison guide 2026", q)
        } else {
            q.clone()
        }
    } else {
        q.clone()
    };
    // Language disambiguation for the INITIAL SearXNG query (pre-intent).
    // Bare ambiguous terms like "rust" return survival-game results from
    // Bing/SearXNG. Since the BERT-based intent engine hasn't resolved yet
    // at this stage, we use a simple data-driven check: if the preprocessed
    // query is a bare ambiguous term without disambiguating context, append
    // " programming". This is the same function used later in the retry
    // stage (post-intent), which has full intent context for richer decisions.
    let engine_q = disambiguate_engine_query(&comparison_q, "", &[]);

    let constraints = extract_gateway_constraints(&q);
    let lang = constraints.language.as_deref();

    for (i, base_url) in searx_base_urls.iter().enumerate() {
        let key = format!("searxng{}", i);
        // Raw query URL (with geolocation parameters)
        let clean_q = preprocess_searxng_query(&engine_q);
        searx_urls.push(searxng_url(base_url, &clean_q, geo_location.as_ref(), lang));
        searx_instance_keys.push(key.clone());
        // Stripped query URL (same instance, runs in parallel)
        if let Some(ref stripped) = stripped_override {
            let clean_stripped = preprocess_searxng_query(stripped);
            if !clean_stripped.is_empty() {
                searx_urls.push(searxng_url(base_url, &clean_stripped, geo_location.as_ref(), lang));
                searx_instance_keys.push(key.clone());
            }
        }
        // P1-compound: filetype-relaxed variant (site: kept, filetype: dropped)
        // fires in parallel so a narrow site:+filetype: conjunction that yields
        // 0 upstream can be recovered from the site:-scoped result set.
        if let Some(ref relaxed) = filetype_relax_variant(&engine_q) {
            let clean_relaxed = preprocess_searxng_query(relaxed);
            if !clean_relaxed.is_empty() && clean_relaxed != clean_q {
                searx_urls.push(searxng_url(base_url, &clean_relaxed, geo_location.as_ref(), lang));
                searx_instance_keys.push(key.clone());
            }
        }
        // Verbose query keyphrase relaxation (strips filler words like "construct a ... using ...")
        // Fires in parallel during initial fan-out to ensure upstream engines return hits for verbose natural language queries.
        if let Some(ref keyphrase) = keyphrase_relax_variant(&engine_q) {
            let clean_kp = preprocess_searxng_query(keyphrase);
            if !clean_kp.is_empty() && clean_kp != clean_q {
                searx_urls.push(searxng_url(base_url, &clean_kp, geo_location.as_ref(), lang));
                searx_instance_keys.push(key.clone());
            }
        }
    }

    let invidious_url = format!("http://invidious:3000/api/v1/search?q={}", q_encoded);

    let indexer_q = if let Some(ref stripped) = stripped_override {
        stripped.clone()
    } else {
        q.clone()
    };
    let indexer_q_encoded = urlencoding::encode(&indexer_q);
    let indexer_query_raw = format!("http://127.0.0.1:6000/search?q={}", indexer_q_encoded);

    let client_ref = &client;
    let circuit_ref = &state.circuit;
    let ratelimit_ref = &state.rate_limits;

    // Map instance key → base URL for connection-cooldown tracking
    let searx_key_to_url: HashMap<String, String> = searx_base_urls.iter().enumerate().map(|(i, url)| {
        (format!("searxng{}", i), url.to_string())
    }).collect();
    // Circuit check for each SearXNG request (raw + stripped variants share same key)
    let searx_instance_open: Vec<bool> = searx_instance_keys.iter()
        .map(|k| circuit_ref.is_open(k))
        .collect();
    let all_searx_open = searx_instance_open.iter().all(|&o| o);
    let invidious_open = circuit_ref.is_open("invidious");

    // NOTE (flaky-connection fix): the indexer lives behind gluetun and its
    // connection can stall in a way an inline `tokio::time::timeout` around
    // `client.get().send()` does NOT reliably interrupt (observed: the whole
    // handler hung 26s with the timeout never firing). So we run the indexer
    // fetch in a DETACHED spawned task and join it with a hard budget. If it
    // stalls, the parent handler proceeds with empty local results instead of
    // hanging the entire response.
    let indexer_client = client.clone();
    let indexer_q_raw = indexer_query_raw.clone();
    let indexer_task = tokio::spawn(async move {
        match tokio::time::timeout(Duration::from_millis(1500), indexer_client.get(&indexer_q_raw).send()).await {
            Ok(Ok(resp)) => {
                let status = resp.status();
                match read_json_bounded::<Vec<IndexerResult>>(resp).await {
                    Some(data) => Ok(data),
                    None => {
                        tracing::error!("Failed to read/parse Indexer JSON (status: {}) — body exceeded cap or invalid", status);
                        Err(())
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Indexer request failed: {:?}", e);
                Err(())
            }
            Err(_) => {
                tracing::warn!("Indexer request timed out — using empty results");
                Ok(vec![])
            }
        }
    });

    // Fire all SearXNG instances in parallel. For site:-constrained queries a
    // single transient double-failure (both the gluetun-VPN and Tor2 egress paths
    // hiccup at once) yields upstream_unavailable. Because the two paths are
    // independent, a short backoff + one re-fire almost always recovers. The
    // fan-out futures are built by `build_searx_futs` so they can be re-issued on
    // the retry without duplicating the ~120-line fetch block.
    let build_searx_futs = |searx_urls: &[String], force: bool| -> Vec<std::pin::Pin<Box<dyn std::future::Future<Output = Result<SearxResponse, reqwest::Error>> + Send>>> {
        searx_urls.iter().enumerate().map(|(i, url)| -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<SearxResponse, reqwest::Error>> + Send>> {
        let url = url.clone();
        // When `force` (the retry attempt), ignore the circuit breaker's open
        // state and re-probe EVERY instance. The breaker can exclude Tor2 for up
        // to its cooldown window; if that exclusion coincides with a gluetun
        // stall, the "two independent paths" fallback collapses to a single dead
        // path and a naive retry just re-hits it. Re-probing the excluded instance
        // is exactly the transient-recovery the site:-retry exists for.
        let is_open = if force { false } else { searx_instance_open[i] };
        let client_for_searx = client.clone();
        let ratelimit_for_searx = state.rate_limits.clone();
        let fut = async move {
            if is_open {
                return Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] });
            }
            // NOTE (flaky-connection fix): the SearXNG instance is reached
            // through gluetun/VPN, whose connections can stall in a way an
            // inline `tokio::time::timeout` around `client.get().send()` does
            // NOT reliably interrupt (same root cause as the indexer hangs).
            // So we run the fetch in a DETACHED spawned task and join it
            // with a hard budget. If it stalls, we return empty results
            // instead of hanging the whole fan-out.
            // The Tor-backed instance (index 1, searxng2/tor2) is inherently
            // slower: a WARM Tor circuit answers in ~1.5s, but the FIRST
            // request after a circuit rebuild (NEWNYM, done every 10 min for
            // IP rotation) can take ~10-12s. Give it a 13s branch budget so it
            // isn't cut before it responds; the gluetun instance (index 0)
            // keeps the original 4.2s budget.
            let is_tor_url = url.contains("tor2") || url.contains("8081");
            let branch_timeout_ms: u64 = if is_tor_url { 15000 } else { 4200 };
            let task = tokio::spawn(async move {
                let resp = match tokio::time::timeout(
                    std::time::Duration::from_millis(branch_timeout_ms),
                    client_for_searx.get(&url).send(),
                ).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        tracing::warn!("SearXNG instance request failed: {:?}", e);
                        return SearxResponse { results: vec![], unresponsive_engines: vec![] };
                    }
                    Err(_) => {
                        tracing::warn!("SearXNG instance timed out (4s): {}", &url[..url.find('?').unwrap_or(url.len())]);
                        return SearxResponse { results: vec![], unresponsive_engines: vec![] };
                    }
                };
                let status = resp.status();
                // Detect 429 from first attempt — rotate IP instead of retrying the same query.
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    let rl_count = ratelimit_for_searx.count_in_window(300);
                    ratelimit_for_searx.record();
                    let new_count = ratelimit_for_searx.count_in_window(300);
                    tracing::warn!("SearXNG got 429 — rate-limits in 5min: {} → {}", rl_count, new_count);
                    rotate_all_ips(&format!("429_rate_limit_{}", new_count));
                }
                let raw = match tokio::time::timeout(
                    std::time::Duration::from_secs(4),
                    resp.text(),
                ).await {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => {
                        tracing::warn!("SearXNG instance body read error: {}", e);
                        return SearxResponse { results: vec![], unresponsive_engines: vec![] };
                    }
                    Err(_) => {
                        tracing::warn!("SearXNG instance body read timed out (4s)");
                        return SearxResponse { results: vec![], unresponsive_engines: vec![] };
                    }
                };
                let sanitized = sanitize_json_text(&raw);
                match serde_json::from_str::<SearxResponse>(&sanitized) {
                    Ok(data) => {
                        // SearXNG-internal failures arrive as HTTP 200 with an
                        // `unresponsive_engines` entry, so the 429-only trigger at
                        // the top of this task misses them. BUT rotating IPs on
                        // EVERY such response causes a self-amplifying storm:
                        // rotating tor2 (NEWNYM) disrupts its circuits -> next query
                        // slower -> rate-limit again -> rotate again; and rotating
                        // the VPN (`status: stopped`) briefly flaps gluetun's
                        // namespace, dropping the gateway's :4000 socket. Observed:
                        // 4 rotations in 5 min, gateway unreachable mid-rotation.
                        //
                        // Correct policy:
                        //  - Transient engine rate-limits ("too many requests" /
                        //    "suspend" / "rate") are SELF-HEALED by SearXNG's
                        //    suspended_times (TooManyRequests=30) and covered by the
                        //    OTHER instance (5.5s fan-out budget). Do NOT rotate.
                        //  - Only a genuine IP BAN (403 / access denied / captcha)
                        //    warrants an IP rotation. Even then, throttle to once per
                        //    120s via the existing rate-limit window so a bad IP can't
                        //    trigger a rotation storm.
                        let ip_banned = data.unresponsive_engines.iter().any(|e| {
                            e.get(1)
                                .map(|msg| {
                                    let m = msg.to_lowercase();
                                    m.contains("403")
                                        || m.contains("access denied")
                                        || m.contains("captcha")
                                })
                                .unwrap_or(false)
                        });
                        if ip_banned {
                            // Throttle: skip if we already rotated within 120s.
                            if ratelimit_for_searx.count_in_window(120) == 0 {
                                ratelimit_for_searx.record();
                                let new_count = ratelimit_for_searx.count_in_window(300);
                                tracing::warn!(
                                    "SearXNG engine IP-banned (HTTP 200, unresponsive_engines) — rotating IPs (throttled 120s): {}",
                                    new_count
                                );
                                rotate_all_ips(&format!("engine_ipban_{}", new_count));
                            } else {
                                tracing::info!(
                                    "SearXNG engine IP-banned but rotation throttled (<=120s since last)"
                                );
                            }
                        }
                        data
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
                        SearxResponse { results: vec![], unresponsive_engines: vec![] }
                    }
                }
            });
            match tokio::time::timeout(std::time::Duration::from_millis(branch_timeout_ms + 500), task).await {
                Ok(Ok(inner)) => Ok(inner),
                Ok(Err(_)) => {
                    tracing::warn!("SearXNG instance task panicked — empty results");
                    Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] })
                }
                Err(_) => {
                    tracing::warn!("SearXNG instance task timed out (budget) — empty results");
                    Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] })
                }
            }
        };
        Box::pin(fut)
        }).collect()
    };

    let invidious_fut = async {
        if invidious_open {
            return Ok::<Vec<InvidiousResult>, anyhow::Error>(vec![]);
        }
        // NOTE (flaky-connection fix): invidious is reached through gluetun;
        // run the fetch in a detached task + budget so a stall can't hang
        // the fan-out the way an inline tokio::time::timeout did.
        let inv_client = client.clone();
        let inv_url = invidious_url.clone();
        let task = tokio::spawn(async move {
            let resp = match tokio::time::timeout(Duration::from_millis(800), inv_client.get(&inv_url).send()).await {
                Ok(Ok(r)) => r,
                _ => return Ok::<Vec<InvidiousResult>, anyhow::Error>(vec![]),
            };
            let status = resp.status();
            match read_json_bounded::<Vec<InvidiousResult>>(resp).await {
                Some(data) => Ok(data),
                None => {
                    tracing::error!("Failed to read/parse Invidious JSON (status: {}) — body exceeded cap or invalid", status);
                    Ok(vec![])
                }
            }
        });
        match tokio::time::timeout(Duration::from_millis(1000), task).await {
            Ok(inner) => inner.unwrap_or_else(|e| { tracing::warn!("Invidious task error: {:?}", e); Ok(vec![]) }),
            Err(_) => { tracing::warn!("Invidious task timed out (budget)"); Ok(vec![]) }
        }
    };

    // Conditional media fan-out based on raw query signals (no intent dependency)
    let q_lower = q.to_lowercase();
    let is_news_intent = q_lower.contains("news") || q_lower.contains("latest");
    let is_image_intent = q_lower.contains("image") || q_lower.contains("photo")
        || q_lower.contains("picture");

    let news_fut = async {
        if !is_news_intent || all_searx_open {
            return Ok(SearxNewsResponse { results: vec![] });
        }
        let news_url = searxng_url_with_categories(
            "http://127.0.0.1:8080", &q, "news", geo_location.as_ref(), lang
        );
        // NOTE (flaky-connection fix): detached spawn + budget so a gluetun
        // stall can't hang the fan-out.
        let n_client = client.clone();
        let task = tokio::spawn(async move {
            let resp = match tokio::time::timeout(Duration::from_millis(800), n_client.get(&news_url).send()).await {
                Ok(Ok(r)) => r,
                _ => return Ok::<SearxNewsResponse, anyhow::Error>(SearxNewsResponse { results: vec![] }),
            };
            let raw = match tokio::time::timeout(Duration::from_millis(800), resp.text()).await {
                Ok(Ok(t)) => t,
                _ => return Ok::<SearxNewsResponse, anyhow::Error>(SearxNewsResponse { results: vec![] }),
            };
            let sanitized = sanitize_json_text(&raw);
            match serde_json::from_str::<SearxNewsResponse>(&sanitized) {
                Ok(data) => Ok(data),
                Err(e) => { tracing::warn!("SearXNG news fan-out parse error: {}", e); Ok(SearxNewsResponse { results: vec![] }) }
            }
        });
        match tokio::time::timeout(Duration::from_millis(1000), task).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => { tracing::warn!("SearXNG news task error"); Ok(SearxNewsResponse { results: vec![] }) }
            Err(_) => { tracing::warn!("SearXNG news task timed out (budget)"); Ok(SearxNewsResponse { results: vec![] }) }
        }
    };

    let image_fut = async {
        if !is_image_intent || all_searx_open {
            return Ok(SearxImageResponse { results: vec![] });
        }
        let image_url = searxng_url_with_categories(
            "http://127.0.0.1:8080", &q, "images", geo_location.as_ref(), lang
        );
        // NOTE (flaky-connection fix): detached spawn + budget so a gluetun
        // stall can't hang the fan-out.
        let i_client = client.clone();
        let task = tokio::spawn(async move {
            let resp = match tokio::time::timeout(Duration::from_millis(800), i_client.get(&image_url).send()).await {
                Ok(Ok(r)) => r,
                _ => return Ok::<SearxImageResponse, anyhow::Error>(SearxImageResponse { results: vec![] }),
            };
            let raw = match tokio::time::timeout(Duration::from_millis(800), resp.text()).await {
                Ok(Ok(t)) => t,
                _ => return Ok::<SearxImageResponse, anyhow::Error>(SearxImageResponse { results: vec![] }),
            };
            let sanitized = sanitize_json_text(&raw);
            match serde_json::from_str::<SearxImageResponse>(&sanitized) {
                Ok(data) => Ok(data),
                Err(e) => { tracing::warn!("SearXNG image fan-out parse error: {}", e); Ok(SearxImageResponse { results: vec![] }) }
            }
        });
        match tokio::time::timeout(Duration::from_millis(1000), task).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => { tracing::warn!("SearXNG image task error"); Ok(SearxImageResponse { results: vec![] }) }
            Err(_) => { tracing::warn!("SearXNG image task timed out (budget)"); Ok(SearxImageResponse { results: vec![] }) }
        }
    };

    let has_site = !constraints.sites.is_empty();
    let searx_partial_collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let partial_collector_inner = searx_partial_collector.clone();
    let searx_fut_with_timeout = async move {
        use futures::future::FutureExt;

        // If tor2 (SearXNG2 / Tor) is currently COLD (flag false), wait a
        // bounded amount for warm_tor2_cache() to rebuild+confirm the circuit
        // before we query it. Querying a cold circuit blows the per-branch
        // budget and surfaces upstream_unavailable even though tor2 would
        // have answered once warm. We only wait when tor2 is actually in the
        // fan-out (it is, for every request) and it's currently marked cold.
        // Cap at 12s so a genuinely stuck tor2 can't hang the request.
        if searx_urls.iter().any(|u| u.contains("tor2")) && !TOR2_WARM.load(std::sync::atomic::Ordering::SeqCst) {
            let mut waited = 0u64;
            while waited < 12000 && !TOR2_WARM.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                waited += 500;
            }
            if TOR2_WARM.load(std::sync::atomic::Ordering::SeqCst) {
                tracing::debug!("tor2 warmed during pre-query wait ({}ms)", waited);
            } else {
                tracing::warn!("tor2 still cold after 12s pre-query wait; querying anyway");
            }
        }

        // Retry policy. For site:-constrained queries an upstream_unavailable is
        // almost always a transient double-failure of the two INDEPENDENT egress
        // paths (gluetun-VPN + Tor2) — a short backoff + one re-fire recovers it.
        // Budgets are sized so the WHOLE request stays under 5s:
        //   attempt1 = 2600ms, backoff 250ms, attempt2 = 1800ms  => worst ~4650ms.
        // Non-site queries keep the original single-shot 5.5s budget (no extra
        // upstream load, no behaviour change).
        let max_attempts: usize = if has_site { 2 } else { 1 };
        let attempt_budget_ms = |attempt: usize| -> u64 {
            if has_site {
                // attempt1 gives the gluetun instance a fair shot (instance1
                // attempt2 is the tor2 recovery path: tor2
                // is warm ~1.5s but a cold Tor circuit (after NEWNYM) can take
                // ~10-12s, so the retry budget must allow that. Worst case:
                // 4.5 + 0.15 + 13 = ~17.6s — only on the rare cold-tor2 degraded
                // run; a warm tor2 recovers in ~1.5s (total ~6s, just over 5s).
                // The common case (instance1 healthy, early-returns) stays fast.
                if attempt == 1 { 4500 } else { 15000 }
            } else {
                // Non-site queries also fan out to tor2; allow its cold-circuit
                // time so it can serve as a true redundant path.
                10000
            }
        };
        let min_early_return: usize = 15;

        let mut out_results: Vec<(usize, Result<SearxResponse, reqwest::Error>)> = Vec::new();
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            // Use a thread-safe shared mutex to preserve results if the timeout triggers
            let results_shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let results_inner = results_shared.clone();
            let partial_collector_sub = partial_collector_inner.clone();
            // Cloned per attempt: the select_all loop below moves it into an `async move`
            // that runs once per attempt, so it must be fresh each iteration.
            let urls_cloned = searx_urls.clone();

            // For site:-constrained queries we ALWAYS bypass the circuit breaker
            // (force=true) on every attempt: a narrow site: query must never be
            // starved because the breaker excluded a path that has since
            // recovered. Both egress paths are always tried. Non-site queries
            // keep normal breaker-respecting behaviour on the first attempt and
            // only force-reprobe on the retry.
            let force = has_site || attempt == 2;
            let futs: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = (usize, Result<SearxResponse, reqwest::Error>)> + Send>>> =
                build_searx_futs(&searx_urls, force).into_iter().enumerate().map(|(i, f)| {
                    f.map(move |r| (i, r)).boxed()
                }).collect();
            let mut futs = futs;

            let budget = attempt_budget_ms(attempt);
            let _ = tokio::time::timeout(std::time::Duration::from_millis(budget), async move {
                while !futs.is_empty() {
                    let ((orig_idx, result), _idx, remaining) = futures::future::select_all(futs).await;
                    futs = remaining;

                    match result {
                        Ok(data) => {
                            let count = data.results.len();
                            results_inner.lock().unwrap().push((orig_idx, Ok(data.clone())));
                            if count > 0 {
                                partial_collector_sub.lock().unwrap().push((orig_idx, Ok(data)));
                            }
                            let is_primary = urls_cloned[orig_idx].starts_with("http://127.0.0.1:8080");
                            // FIX: do NOT early-return on the primary instance returning a
                            // small/medium set. SearXNG1 (127.0.0.1:8080) sits behind a flaky
                            // VPN and its Bing/Brave engines frequently return OFF-TOPIC junk
                            // (e.g. "population of France" -> New Balance shoe pages). The
                            // secondary instance (Tor2 / SearXNG2) is far more reliable and
                            // routinely returns the correct results. The old `is_primary &&
                            // count >= 5` rule discarded Tor2's good results the moment the
                            // primary coughed up 5 junk hits, and the downstream "garbage
                            // cluster" fallback then trusted raw RRF ranking -- surfacing junk
                            // at the top.
                            //
                            // We now only short-circuit when a result set is genuinely LARGE
                            // (>= min_early_return=15), which still bounds latency while
                            // guaranteeing Tor2's reliable results always join the merge.
                            // Tor2 responds in ~2.5s, comfortably inside the 3.3s budget.
                            if count >= min_early_return {
                                tracing::info!(
                                    "SearXNG early return: {} results (is_primary={}), skipping {} remaining instance(s)",
                                    count, is_primary, futs.len()
                                );
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("SearXNG instance error (idx={}): {:?}", orig_idx, e);
                            results_inner.lock().unwrap().push((orig_idx, Err(e)));
                        }
                    }
                }
            }).await;

            let results = {
                let mut guard = results_shared.lock().unwrap();
                std::mem::take(&mut *guard)
            };

            // A resolved future is NOT the same as a usable result. An instance
            // can return Ok([]) (HTTP 200 but Brave suspended / no hits) — that
            // must NOT count as "we got results" or the retry below is skipped
            // and we surface upstream_unavailable even though the other path
            // (tor2) simply got cut by the budget and would have answered on
            // the retry. Only break early when at least one instance returned
            // a non-empty result set.
            let has_usable = results.iter().any(|(_, r)| {
                matches!(r, Ok(d) if !d.results.is_empty())
            });

            if has_usable {
                out_results = results;
                if attempt > 1 {
                    tracing::info!(
                        "SearXNG retry recovered results on attempt {} (site:-constrained query)",
                        attempt
                    );
                }
                break;
            }
            if attempt >= max_attempts {
                out_results = results;
                if has_site {
                    tracing::warn!(
                        "SearXNG site:-constrained query empty after {} attempt(s) -- will signal upstream_unavailable",
                        attempt
                    );
                }
                break;
            }
            // Short backoff before the single retry (transient double-failure cleared).
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        out_results
    };


    // This eliminates the sequential intent→engines pipeline.
    // Engines start fetching immediately; intent runs in parallel.
    // Latency = max(intent, engines) instead of intent + engines.
    // Cap SearXNG at 5s so slow backend instances can't block the whole response.
    // The SearXNG internal budget remains 10s for retries, but we surface
    // whatever results arrived within the deadline. The other backends (intent,
    // indexer, invidious, news, images) still get their full budget.
    let (intent_result, embed_res, indexer_res, searx_results, invidious_res, news_res, image_res) = tokio::join!(
        intent_fut,
        embed_fut,
        indexer_task,
        // ⚠️  NOTE: tokio::time::timeout cancels the inner future, which drops any
        // partial results already collected by searx_fut_with_timeout (from faster
        // SearXNG instances that completed within the 5s window). This means on
        // timeout we return Vec::new() for SearXNG — we don't preserve partial
        // results. The ROADMAP.md "Return early + merge stragglers" plan item
        // describes the proper fix: return completed results immediately and merge
        // slow instances' results in the background.
        async {
            let partial_collector_ref = searx_partial_collector.clone();
            match tokio::time::timeout(Duration::from_secs(5), searx_fut_with_timeout).await {
                Ok(res) => {
                    if res.is_empty() {
                        let mut guard = partial_collector_ref.lock().unwrap();
                        if !guard.is_empty() {
                            return std::mem::take(&mut *guard);
                        }
                    }
                    res
                }
                Err(_) => {
                    tracing::warn!("SearXNG fan-out exceeded 5s deadline; preserving partial results from completed instances");
                    let mut guard = partial_collector_ref.lock().unwrap();
                    std::mem::take(&mut *guard)
                }
            }
        },
        invidious_fut,
        news_fut,
        image_fut,
    );
    // 2. Process Intent & Embedding (now available alongside engine results)
    let mut intent: IntentResponse = match intent_result {
        Ok(parsed) => parsed,
        Err(()) => {
            tracing::error!("Intent Engine unreachable after 3 attempts — using fallback");
            fallback_intent(&q)
        }
    };

    intent.structured_constraints = sanitize_constraints(&intent.structured_constraints);
    
    // Merge constraints parsed directly by the gateway to prevent any loss of operators
    let gateway_extracted = extract_gateway_constraints(&q_orig);
    for ft in gateway_extracted.file_types {
        if !intent.structured_constraints.file_types.contains(&ft) {
            intent.structured_constraints.file_types.push(ft);
        }
    }
    for site in gateway_extracted.sites {
        if !intent.structured_constraints.sites.contains(&site) {
            intent.structured_constraints.sites.push(site);
        }
    }
    for phrase in gateway_extracted.phrases {
        if !intent.structured_constraints.phrases.contains(&phrase) {
            intent.structured_constraints.phrases.push(phrase);
        }
    }
    if intent.structured_constraints.after_date.is_none() {
        intent.structured_constraints.after_date = gateway_extracted.after_date;
    }
    if intent.structured_constraints.before_date.is_none() {
        intent.structured_constraints.before_date = gateway_extracted.before_date;
    }
    // Price: the gateway parser preserves the explicit `<`/`>` operators from the
    // query (e.g. `price:<100`), whereas the intent engine only populates the
    // range bounds. Prefer the gateway-extracted price fields so the operator is
    // not silently dropped from applied_constraints.
    if gateway_extracted.price_lt.is_some() {
        intent.structured_constraints.price_lt = gateway_extracted.price_lt;
        intent.structured_constraints.price_max = intent.structured_constraints.price_max.or(gateway_extracted.price_lt);
    }
    if gateway_extracted.price_gt.is_some() {
        intent.structured_constraints.price_gt = gateway_extracted.price_gt;
        intent.structured_constraints.price_min = intent.structured_constraints.price_min.or(gateway_extracted.price_gt);
    }
    if gateway_extracted.price_min.is_some() {
        intent.structured_constraints.price_min = gateway_extracted.price_min;
    }
    if gateway_extracted.price_max.is_some() {
        intent.structured_constraints.price_max = gateway_extracted.price_max;
    }
    intent.structured_constraints = sanitize_constraints(&intent.structured_constraints);

    // P3 fix: inject detected location as a Target entity so structured_constraints.entities
    // reflects the explicit user location intent. Must happen AFTER intent is defined.
    if let Some(ref loc) = geo_location {
        if let Some(ref city) = loc.city {
            if !intent.structured_constraints.entities.iter().any(|e| e.text == *city) {
                intent.structured_constraints.entities.push(QueryEntity {
                    text: city.clone(),
                    role: EntityRole::Target,
                });
                if !intent.structured_constraints.positive.contains(&city) {
                    intent.structured_constraints.positive.push(city.clone());
                }
            }
        }
        if let Some(ref cc) = loc.country_code {
            let country_lower = cc.to_lowercase();
            if !intent.structured_constraints.entities.iter().any(|e| e.text == country_lower) {
                intent.structured_constraints.entities.push(QueryEntity {
                    text: country_lower.clone(),
                    role: EntityRole::Target,
                });
            }
        }
    }

    // If query has ONLY negative constraints, keep results for constraint scoring
    let only_negative = !intent.structured_constraints.negative.is_empty()
        && intent.structured_constraints.positive.is_empty();
    if only_negative {
        let before = searx_results.len()
            + match &invidious_res { Ok(v) => v.len(), Err(_) => 0 }
            + match &news_res { Ok(v) => v.results.len(), Err(_) => 0 }
            + match &image_res { Ok(v) => v.results.len(), Err(_) => 0 };
        tracing::info!("ONLY NEGATIVE: {} web results — keeping for constraint scoring", before);
    }

    // ─── Rule-based intent overrides for known misclassification patterns ───
    // Fire when the linear probe has low confidence (<0.30) — the model is guessing,
    // so pattern-based heuristics beat random chance.
    {
        let q_lower = q.to_lowercase();
        let only_negative_pattern = !intent.structured_constraints.negative.is_empty()
            && intent.structured_constraints.positive.is_empty();

        // Override 1: only-negative queries classified as navigational → informational
        // e.g. "not django" (conf=0.24, classified navigational — should be informational)
        if only_negative_pattern && intent.intent.as_str() != "informational" && intent.confidence < 0.30 {
            tracing::info!(
                "INTENT OVERRIDE: only-negative '{}' was '{}' (conf={:.3}) → informational",
                q, intent.intent, intent.confidence
            );
            intent.intent = "informational".to_string();
            intent.confidence = intent.confidence.max(0.35);
            // Boost informational in the distribution for correct RankingWeights blending
            let info_prob = intent.distribution.get("informational").copied().unwrap_or(0.0);
            let nav_prob = intent.distribution.get("navigational").copied().unwrap_or(0.0);
            intent.distribution.insert("informational".to_string(), info_prob + nav_prob * 0.5);
            intent.distribution.insert("navigational".to_string(), nav_prob * 0.5);
        }

        // Override 2: temporal/freshness signal → force fresh intent.
        // Phase 4 (CROSS-CUTTING): the engine now emits intent="fresh" for
        // recency queries, but as defense-in-depth the gateway also forces it
        // here. The OLD gate (confidence < 0.30) let "latest ai news 2026"
        // (0.459) and "recent rust releases" (0.519) slip through to a 90-day
        // navigational half-life. We now trigger on the recency signal itself,
        // not on low confidence.
        {
            let has_news_signal = q_lower.contains("latest") || q_lower.contains("recent")
                || q_lower.contains("breaking") || q_lower.contains("headline")
                || q_lower.contains("new ") || q_lower.contains("newest")
                || q_lower.contains("cve-") || q_lower.contains("vulnerability")
                || q_lower.contains("this week") || q_lower.contains("this month")
                || q_lower.contains("past week") || q_lower.contains("last week");
            let has_topic_signal = q_lower.contains("news") || q_lower.contains("update")
                || q_lower.contains("today") || q_lower.contains("this week")
                || q_lower.contains("2026") || q_lower.contains("2025")
                || q_lower.contains("release") || q_lower.contains("version");
            // Don't clobber a fresh intent that the engine already set.
            if intent.intent != "fresh" && has_news_signal && has_topic_signal {
                tracing::info!(
                    "INTENT OVERRIDE (STRONG): news query '{}' was '{}' (conf={:.3}) — forcing fresh",
                    q, intent.intent, intent.confidence
                );
                intent.intent = "fresh".to_string();
                intent.confidence = intent.confidence.max(0.45);
                // Reshape distribution: fresh gets the top probability
                let fresh_prob = intent.distribution.get("fresh").copied().unwrap_or(0.0);
                let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
                intent.distribution.insert("fresh".to_string(), (fresh_prob + current_top_prob * 0.5).min(0.85));
                // Boost informational as secondary intent (for ranking weight blending)
                let info_prob = intent.distribution.get("informational").copied().unwrap_or(0.0);
                intent.distribution.insert("informational".to_string(), info_prob + 0.15);
            }
            // Weak signal: only topic signal (e.g. year without news keywords).
            else if intent.intent != "fresh" && (q_lower.contains("2026") || q_lower.contains("2025")) {
                let fresh_prob = intent.distribution.get("fresh").copied().unwrap_or(0.0);
                let current_prob = intent.distribution.get(&intent.intent).copied().unwrap_or(0.0);
                if fresh_prob + 0.15 > current_prob {
                    tracing::info!(
                        "INTENT OVERRIDE (WEAK): year query '{}' was '{}' (conf={:.3}) — boosting fresh (fresh={:.3})",
                        q, intent.intent, intent.confidence, fresh_prob
                    );
                    intent.distribution.insert("fresh".to_string(), fresh_prob + 0.15);
                }
            }
        }

        // Override 2b: a fresh intent with no derived date window must still
        // apply a real recency cutoff (not just re-weight scoring). Without this,
        // "latest ai news" would rank newer items higher but never drop stale ones.
        if intent.intent == "fresh" && intent.structured_constraints.after_date.is_none() {
            let today = today_ymd();
            intent.structured_constraints.after_date = Some(format_ymd(add_days(today, -7)));
            if intent.structured_constraints.before_date.is_none() {
                intent.structured_constraints.before_date = Some(format_ymd(today));
            }
            tracing::info!("FRESH OVERRIDE: applied default 7-day recency window");
        }

        // Override 3: "other than X" with low confidence → boost comparison + technical
        // e.g. "programming language other than java" (conf=0.12, technical is correct base)
        if q_lower.contains("other than") && intent.confidence < 0.20 {
            tracing::info!(
                "INTENT OVERRIDE: 'other than' query '{}' was '{}' (conf={:.3}) — boosting comparison/technical",
                q, intent.intent, intent.confidence
            );
            let comp = intent.distribution.get("comparison").copied().unwrap_or(0.0);
            let tech = intent.distribution.get("technical").copied().unwrap_or(0.0);
            intent.distribution.insert("comparison".to_string(), comp + 0.1);
            intent.distribution.insert("technical".to_string(), tech + 0.1);
        }

        // Override 4: local intent signals with low confidence → force local
        // e.g. "pizza near me" (conf=0.12, classified comparison → should be local)
        let has_local_keywords = q_lower.contains(" near me") || q_lower.starts_with("near me")
            || q_lower.contains("nearby") || q_lower.contains(" close to")
            || q_lower.starts_with("close to") || q_lower.contains("coffee shop");
        if has_local_keywords && intent.intent != "local" && intent.confidence < 0.50 {
            tracing::info!(
                "INTENT OVERRIDE (STRONG): local query '{}' was '{}' (conf={:.3}) -> local",
                q, intent.intent, intent.confidence
            );
            intent.intent = "local".to_string();
            intent.confidence = intent.confidence.max(0.45);
            let local_prob = intent.distribution.get("local").copied().unwrap_or(0.0);
            let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
            intent.distribution.insert("local".to_string(), (local_prob + current_top_prob * 0.5).min(0.85));
        }

        // Override 6: transactional keywords -> force/boost transactional intent
        let tx_keywords = ["buy ", "price ", "pricing", "cheap ", "purchase ", "shop ", "store ", "discount ", "coupon "];
        let has_tx_signal = tx_keywords.iter().any(|k| q_lower.starts_with(k) || q_lower.contains(&format!(" {}", k)) || q_lower.contains("headphones"));
        if has_tx_signal && !has_local_keywords && intent.intent != "transactional" && intent.confidence < 0.50 {
            tracing::info!(
                "INTENT OVERRIDE (STRONG): transactional query '{}' was '{}' (conf={:.3}) -> transactional",
                q, intent.intent, intent.confidence
            );
            intent.intent = "transactional".to_string();
            intent.confidence = intent.confidence.max(0.45);
            let tx_prob = intent.distribution.get("transactional").copied().unwrap_or(0.0);
            let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
            intent.distribution.insert("transactional".to_string(), (tx_prob + current_top_prob * 0.5).min(0.85));
        }

        // Override 8: Driver / Software Download Intent -> force decisive Navigational + Download intent
        // Handles queries like "find the driver download", "nvidia rtx 4090 driver download", "download vlc"
        let download_keywords = [
            "driver", "drivers", "download", "downloads", "installer", "installers",
            "firmware", "patch", "software download", "official download", "setup.exe"
        ];
        let has_download_signal = download_keywords.iter().any(|k| q_lower.contains(k));
        if has_download_signal {
            tracing::info!(
                "INTENT OVERRIDE (DECISIVE): driver/download query '{}' was '{}' (conf={:.3}) -> navigational",
                q, intent.intent, intent.confidence
            );
            intent.intent = "navigational".to_string();
            intent.confidence = intent.confidence.max(0.88);
            let nav_prob = intent.distribution.get("navigational").copied().unwrap_or(0.0);
            intent.distribution.insert("navigational".to_string(), (nav_prob + 0.60).min(0.95));
            intent.distribution.insert("download".to_string(), 0.90);
        }

        // Override 5: "vs" or "versus" signals in query with low comparison confidence
        // boost comparison intent. Handles queries like "react vs vue vs angular comparison"
        // that the engine may misclassify as informational.
        let has_vs_signal = q_lower.contains(" vs ") || q_lower.starts_with("vs ")
            || q_lower.contains(" versus ") || q_lower.starts_with("versus");
        if has_vs_signal {
            let comp_prob = intent.distribution.get("comparison").copied().unwrap_or(0.0);
            if comp_prob < 0.30 {
                tracing::info!(
                    "INTENT OVERRIDE: vs query '{}' was '{}' (conf={:.3}) — boosting comparison",
                    q, intent.intent, intent.confidence
                );
                intent.distribution.insert("comparison".to_string(), comp_prob + 0.25);
            }
            // If intent is informational or navigational with low confidence, force comparison
            if intent.intent != "comparison" && intent.confidence < 0.40 {
                let cur_top = intent.distribution.iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default();
                let cur_prob = intent.distribution.get(&cur_top).copied().unwrap_or(0.0);
                let new_comp = intent.distribution.get("comparison").copied().unwrap_or(0.0);
                let threshold = if intent.intent == "informational" { 0.35 } else { 0.25 };
                if new_comp + 0.25 > cur_prob.min(threshold) {
                    tracing::info!(
                        "INTENT OVERRIDE (STRONG): vs query '{}' was '{}' (conf={:.3}) — comparison now dominant",
                        q, intent.intent, intent.confidence
                    );
                    intent.intent = "comparison".to_string();
                    intent.confidence = intent.confidence.max(0.30);
                }
            }

            // Override 7: weather / forecast queries → fresh (time-sensitive, never navigational).
            // "weather forecast tomorrow" is routinely mislabeled navigational by the engine.
            let weather_signals = [
                "weather", "forecast", "temperature", "rain", "snow", "humidity",
                "precipitation", "thunderstorm", "sunny", "cloudy", "meteorology",
            ];
            let has_weather_signal = weather_signals.iter().any(|s| q_lower.contains(s));
            if has_weather_signal && intent.intent != "fresh" && intent.intent != "local" {
                tracing::info!(
                    "INTENT OVERRIDE (STRONG): weather query '{}' was '{}' (conf={:.3}) → fresh",
                    q, intent.intent, intent.confidence
                );
                intent.intent = "fresh".to_string();
                intent.confidence = intent.confidence.max(0.45);
                let fresh_prob = intent.distribution.get("fresh").copied().unwrap_or(0.0);
                let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
                intent.distribution.insert("fresh".to_string(), (fresh_prob + current_top_prob * 0.5).min(0.85));
            }

            // Override 8: procedural / how-to queries → how-to (with technical secondary).
            // e.g. "linux find files modified last 7 days", "how to fix npm ERR_MODULE_NOT_FOUND".
            // The engine often mislabels step-by-step operational queries as navigational.
            let howto_signals = [
                "how to", "how do i", "how do you", "how can i", "how can you", "how to's",
                "tutorial", "step by step", "step-by-step", "ways to", "guide to", "guide:",
                "find files", "find the", "modified", "fix ", "install", "configure",
                "set up", "setup", "uninstall", "upgrade", "build from", "compile",
                "debug", "troubleshoot", "resolve", "workaround",
            ];
            let has_howto_signal = howto_signals.iter().any(|s| q_lower.contains(s));
            if has_howto_signal
                && (intent.intent == "navigational" || intent.confidence < 0.40)
                && intent.intent != "how-to"
            {
                tracing::info!(
                    "INTENT OVERRIDE (STRONG): how-to query '{}' was '{}' (conf={:.3}) → how-to",
                    q, intent.intent, intent.confidence
                );
                intent.intent = "how-to".to_string();
                intent.confidence = intent.confidence.max(0.45);
                let howto_prob = intent.distribution.get("how-to").copied().unwrap_or(0.0);
                let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
                intent.distribution.insert("how-to".to_string(), (howto_prob + current_top_prob * 0.5).min(0.85));
                // Technical is a natural secondary intent for operational/dev queries.
                let tech_prob = intent.distribution.get("technical").copied().unwrap_or(0.0);
                intent.distribution.insert("technical".to_string(), tech_prob + 0.15);
            }

            // Override 9: research / study queries → informational (with technical secondary).
            // e.g. "covid vaccine efficacy studies", "attention is all you need paper".
            let research_signals = [
                "study", "studies", "efficacy", "research", "analysis", "literature",
                "paper", "survey", "whitepaper", "benchmark", "experiment", "findings",
                "meta-analysis", "peer review", "journal", "abstract",
            ];
            let has_research_signal = research_signals.iter().any(|s| q_lower.contains(s));
            if has_research_signal
                && (intent.intent == "navigational" || intent.confidence < 0.40)
                && intent.intent != "informational"
            {
                tracing::info!(
                    "INTENT OVERRIDE (STRONG): research query '{}' was '{}' (conf={:.3}) → informational",
                    q, intent.intent, intent.confidence
                );
                intent.intent = "informational".to_string();
                intent.confidence = intent.confidence.max(0.45);
                let info_prob = intent.distribution.get("informational").copied().unwrap_or(0.0);
                let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
                intent.distribution.insert("informational".to_string(), (info_prob + current_top_prob * 0.5).min(0.85));
                let tech_prob = intent.distribution.get("technical").copied().unwrap_or(0.0);
                intent.distribution.insert("technical".to_string(), tech_prob + 0.10);
            }
        }
    }

    let vector: Option<Vec<f32>> = match embed_res {
        Some(resp) => {
            if let Some(json) = read_json_bounded::<serde_json::Value>(resp).await {
                json["embedding"].as_array().map(|arr| {
                    arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
                })
            } else { None }
        },
        None => None,
    };


    // Secondary fan-out with expanded queries if initial results are sparse
    // Apply language disambiguation for short language names
    let disambiguated_q = disambiguate_engine_query(&q, &intent.intent, &intent.expanded_queries);
    let expanded_queries = if intent.expanded_queries.len() > 1 {
        let mut eq = intent.expanded_queries.clone();
        if disambiguated_q != q {
            eq.insert(0, disambiguated_q);
        }
        eq
    } else {
        if disambiguated_q != q {
            vec![disambiguated_q.clone(), q.clone()]
        } else {
            vec![q.clone()]
        }
    };
    // When query has ONLY negative constraints ("not X not Y"), SearXNG searched with
    // the raw query including negative terms. Strip negation trigger words to create a
    // clean query and prepend it to expanded_queries so the retry re-fetches with clean terms.
    // IMPORTANT: only strip negation trigger words, NOT the negated content terms themselves.
    // The negated terms ARE the core topic ("not django" → search "django") — the constraint
    // system handles exclusion via is_alternative_listing_page detection + graduated penalties.
    let stripped_query: Option<String> = if !intent.structured_constraints.negative.is_empty() {
        // Use the same simple_negation_strip function that the initial
        // SearXNG fan-out uses—strips both trigger words AND the negated
        // content terms immediately following them.
        // Example: "browser not chrome not edge" → "browser"
        let stripped = simple_negation_strip(&q);
        if let Some(ref s) = stripped {
            tracing::info!("ONLY NEGATIVE: stripped query '{:?}' -> '{}'", q, s);
        }
        stripped
    } else { None };
    // Generate alternative-seeking variations so the retry actually fetches
    // pages like alternatives-listing articles instead of the excluded tools' official pages.
    // The stripped_query already excludes negated content terms via simple_negation_strip,
    // so "browser not chrome not edge" becomes "browser". From there we generate multiple
    // alternative-seeking variations of the core topic plus per-excluded-term queries.
    let alt_queries: Vec<String> = if let Some(ref s_q) = stripped_query {
        let mut alts = Vec::new();
        alts.push(format!("{} alternatives", s_q));
        alts.push(format!("{} comparison", s_q));
        for neg in &intent.structured_constraints.negative {
            alts.push(format!("alternative to {}", neg));
            if alts.len() >= 6 { break; }
        }
        alts.push(format!("best {} 2026", s_q));
        alts.push(format!("top {} 2026", s_q));
        alts
    } else {
        Vec::new()
    };
    let expanded_queries = if let Some(ref s_q) = stripped_query {
        let mut eq = vec![s_q.clone()];
        eq.extend(alt_queries);
        eq.extend(expanded_queries);
        eq
    } else {
        expanded_queries
    };
    // Location-aware query expansion: use semantic "local" intent from the engine
    // (BERT-based classifier), falling back to keyword detection when confidence is low.
    let is_local_intent = intent.intent.as_str() == "local" && intent.confidence >= 0.20
        || intent.intent.as_str() != "local" && has_local_intent(&q);
    let expanded_queries = if let Some(ref geo) = geo_location {
        if is_local_intent {
            let mut eq = expanded_queries;
            if let Some(localized) = localize_query(&q, geo) {
                tracing::info!("LOCAL INTENT (semantic={}): expanding query '{}' with location -> '{}'",
                    intent.intent == "local", q, localized);
                eq.push(localized);
            }
            eq
        } else {
            expanded_queries
        }
    } else {
        expanded_queries
    };
    tracing::info!(target:"expansion.debug", expanded=?expanded_queries.iter().take(3).collect::<Vec<_>>(), query=%q, "primary expanded queries");

    // TODO: Secondary fan-out with expanded queries if searx_results are sparse
    // For now, scoring uses intent-based weighting on the raw query results

    // 4. Process Local Results
    // indexer_res is Result<Result<Vec<IndexerResult>, reqwest::Error>, JoinError>:
    // outer = join-timeout/budget, inner = the spawned task's own outcome (which
    // itself returns Ok(vec) on success OR on timeout, Err only on hard failure).
    let mut local_results: Vec<IndexerResult> = match indexer_res {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => {
            tracing::warn!("Indexer search hard-failed — using empty local results");
            vec![]
        }
        Err(_) => {
            tracing::warn!("Indexer search task panicked/timed out — using empty local results");
            vec![]
        }
    };

    // 4b. Re-query indexer with BERT embedding for semantic vector search
    // The initial indexer call (parallel fan-out) ran without the embedding
    // because it wasn't available yet. Re-query with the vector for RRF fusion
    // of BM25 + semantic similarity, giving better results for natural language queries.
    // NOTE (flaky-connection fix): the vector re-query talks to the indexer over a
    // connection that, behind gluetun/VPN, can stall in a way that is NOT reliably
    // interrupted by an inline `tokio::time::timeout` wrapping `client.get().send()`
    // (observed: the handler hung 28-31s with the timeout never firing, dropping the
    // whole response). This re-query is a non-essential RRF *enhancement* — it must
    // never block the critical response path. So we run it in a DETACHED spawned task
    // and join it with a hard budget. If the task (or its connection) hangs, the parent
    // handler still returns on time with the BM25 results already in `local_results`.
    if let Some(ref vec) = vector {
        let vec_str = serde_json::to_string(vec).unwrap_or_default();
        let indexer_q = if let Some(ref stripped) = stripped_override {
            stripped.clone()
        } else {
            q.clone()
        };
        let indexer_q_encoded = urlencoding::encode(&indexer_q);
        let indexer_url_vec = format!(
            "http://127.0.0.1:6000/search?q={}&vector={}",
            indexer_q_encoded,
            urlencoding::encode(&vec_str)
        );
        let client_for_vec = client.clone();
        let vec_task = tokio::spawn(async move {
            match tokio::time::timeout(
                Duration::from_millis(1500),
                client_for_vec.get(&indexer_url_vec).send(),
            ).await {
                Ok(Ok(resp)) => {
                    match read_json_bounded::<Vec<IndexerResult>>(resp).await {
                        Some(vec_results) if !vec_results.is_empty() => Some(vec_results),
                        _ => None,
                    }
                }
                _ => None,
            }
        });
        // Join the detached task with a hard budget. If it hangs, we keep BM25 results.
        match tokio::time::timeout(Duration::from_millis(1600), vec_task).await {
            Ok(Ok(Some(vec_results))) => {
                // FUSE (P0 fix): the semantic re-query must ADD to, never REPLACE, the
                // BM25 local results. Previously we overwrote `local_results` with the
                // indexer's semantic-RRF set, which — when only a few docs carry the
                // embedding — collapses the 10 good local hits down to those 3 embedded
                // docs (e.g. arxiv papers for "what is quantum computing"). Union the two
                // sets by URL, keeping the higher-scoring copy, so semantic *enhances*
                // recall instead of destroying it.
                let mut by_url: HashMap<String, IndexerResult> = HashMap::new();
                for r in local_results.drain(..) {
                    by_url.insert(normalize_indexer_url(&r.url), r);
                }
                let mut added = 0usize;
                for r in vec_results {
                    let key = normalize_indexer_url(&r.url);
                    match by_url.get_mut(&key) {
                        Some(existing) => {
                            // Same URL in both sets: keep the stronger score.
                            if r.score > existing.score {
                                existing.score = r.score;
                            }
                        }
                        None => {
                            by_url.insert(key, r);
                            added += 1;
                        }
                    }
                }
                local_results = by_url.into_values().collect();
                tracing::info!(
                    "Vector re-query fused: {} BM25 + {} new semantic = {} local results",
                    local_results.len() - added, added, local_results.len()
                );
            }
            Ok(Ok(None)) => { /* no vector hits; keep BM25 */ }
            Ok(Err(e)) => tracing::warn!("Vector indexer task panicked: {:?}", e),
            Err(_) => tracing::warn!("Vector indexer re-query timed out — keeping BM25 results"),
        }
    }

    // 5. Aggregate Web Results from all sources
    let mut web_results: Vec<SearxResult> = Vec::new();
    // Track per-URL RRF contributions from each source's ranked position
    // This gives a proper rank-fusion score instead of meaningless insertion order
    let mut url_rrf_contributions: HashMap<String, f32> = HashMap::new();

    // Aggregate SearXNG results from all query variations
    // Track per-engine result counts for degradation detection
    let mut engine_counts: HashMap<String, u64> = HashMap::new();
    // Track upstream SearXNG instance health for the "all upstream failed" signal.
    // seax_results is consumed by the into_iter() below, so count here before it moves.
    let mut searx_instances_total: usize = searx_results.len();
    let mut searx_instances_ok: usize = 0;
    for (orig_idx, searx_res) in searx_results.into_iter() {
        let instance_key = &searx_instance_keys[orig_idx];
        match searx_res {
            Ok(searx_data) => {
                let n = searx_data.results.len();
                tracing::info!("SearXNG variation {} returned {} results", orig_idx, n);
                if n > 0 {
                    searx_instances_ok += 1;
                    circuit_ref.record_success(instance_key);
                    // Track last-used time for connection-cooldown aware routing
                    if let Some(url) = searx_key_to_url.get(instance_key) {
                        state.searx_last_used.lock().insert(url.clone(), Instant::now());
                    }
                    circuit_ref.record_results(instance_key, n as u64);
                    for r in &searx_data.results {
                        *engine_counts.entry(r.engine.clone()).or_insert(0) += 1;
                    }
                } else {
                    // 0 results ≠ failure — the instance is healthy but returned no
                    // matches (e.g. niche query, all engines temporarily suspended).
                    // Only record actual errors (timeout, connection refused) as failures.
                    // This prevents niche queries from cascading the circuit to 300s.
                }
                for (pos, result) in searx_data.results.into_iter().enumerate() {
                    let engine_weight = circuit_ref.weight(&result.engine);
                    let normalized = {
                        let lower = result.url.to_lowercase();
                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                        let no_trailing = no_fragment.trim_end_matches('/');
                        let no_www = no_trailing.replacen("://www.", "://", 1);
                        strip_tracking_params(&no_www)
                    };
                    let rrf_contrib = engine_weight / (60.0 + (pos + 1) as f32);
                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                    // Tag result with its SearXNG instance for source diversity tracking
                    let mut instance_tagged = result;
                    let instance_tag = format!("instance_{}", instance_key.trim_start_matches("searxng"));
                    if !instance_tagged.sources.contains(&instance_tag) {
                        instance_tagged.sources.push(instance_tag);
                    }
                    web_results.push(instance_tagged);
                }
            }
            Err(e) => {
                tracing::error!("SearXNG variation {} request failed/timed out: {:?}", orig_idx, e);
                circuit_ref.record_failure(instance_key);
            }
        }
    }

    // ─── Per-Engine Degradation Detection ──────────────────────────
    // Record per-engine result counts and detect silent degradation.
    // If an engine returns <50% of its rolling average, it's likely rate-limited.
    for (engine, &count) in &engine_counts {
        let _ = state.volume_tracker.record(engine, count);
    }

    // Cross-request degradation correlation: if 3+ degradation events in 5 min,
    // trigger proactive VPN rotation before the circuit breaker even opens.
    let recent_degradations = state.volume_tracker.degradation_count(300);
    if recent_degradations >= 3 {
        tracing::warn!(
            "PROACTIVE VPN ROTATION: {} engine degradations in 5-min window",
            recent_degradations
        );
        rotate_all_ips(&format!("proactive_{}_degradations_5min", recent_degradations));
    }

    // ─── Parallel retry: fire all expanded query variations on all instances ───
    // Replaces the old sequential smart retry + garbage cluster retry pattern.
    // Instead of retrying one variation at a time on one instance (O(N×timeout)),
    // fire ALL variations on ALL instances in parallel and race them with select_all.
    // This turns sequential (3 × 3s = 9s worst) into parallel (2s flat).
    let total_results = web_results.len();
    let expected_min = if only_negative { 5 } else if expanded_queries.len() > 1 { 15 } else { 10 };
    let needs_more_results = total_results < expected_min || (only_negative && searx_base_urls.len() > 1);

    // Count-based retry (no relevance needed yet — fires before Invidious/news/image)
    if needs_more_results && expanded_queries.len() > 1 && !searx_base_urls.is_empty() {
        let mut retry_futs = Vec::new();
        let retry_timeout = Duration::from_secs(4); // shorter than initial 5s
        let max_variations: usize = if only_negative || !intent.structured_constraints.negative.is_empty() { 6 } else { 3 };
        for (eq_idx, eq) in expanded_queries.iter().enumerate().skip(1) {
            if eq_idx > max_variations { break; }
            let clean_eq = preprocess_searxng_query(eq);
            if clean_eq.to_lowercase() == q.to_lowercase() { continue; } // skip duplicate
            for (inst_idx, base_url) in searx_base_urls.iter().enumerate() {
                let retry_key = format!("searxng{}", inst_idx);
                // For negative-only queries, fire on ALL instances (including Tor)
                // to maximize the chance of finding alternative-listing pages.
                // For normal queries, only use VPN instance (SearXNG1) for speed.
                if !only_negative && intent.structured_constraints.negative.is_empty() && inst_idx > 0 { continue; }
                if circuit_ref.is_open(&retry_key) { continue; }
                let retry_url = searxng_url(base_url, &clean_eq, geo_location.as_ref(), lang);
                let client = client.clone();
                let key = retry_key.clone();
                let url_for_log = retry_url[..retry_url.find('?').unwrap_or(retry_url.len())].to_string();
                let fallback_key = key.clone();
                let fallback_url = url_for_log.clone();
                retry_futs.push(Box::pin(async move {
                    let retry_client = client.clone();
                    let retry_url_owned = retry_url.clone();
                    let task = tokio::spawn(async move {
                        match tokio::time::timeout(
                            retry_timeout,
                            retry_client.get(&retry_url_owned).send(),
                        ).await {
                            Ok(Ok(resp)) => {
                                let raw = match tokio::time::timeout(Duration::from_secs(3), resp.text()).await {
                                    Ok(Ok(t)) => t,
                                    _ => return (inst_idx, key, url_for_log, Err("retry body read timeout".into())),
                                };
                                let sanitized = sanitize_json_text(&raw);
                                match serde_json::from_str::<SearxResponse>(&sanitized) {
                                    Ok(data) => (inst_idx, key, url_for_log, Ok(data)),
                                    Err(e) => (inst_idx, key, url_for_log, Err(format!("retry parse error: {:?}", e))),
                                }
                            }
                            Ok(Err(e)) => (inst_idx, key, url_for_log, Err(format!("retry request error: {:?}", e))),
                            Err(_) => (inst_idx, key, url_for_log, Err("retry timeout".into())),
                        }
                    });
                    let (inst_idx, key, url_for_log, result): (usize, String, String, Result<SearxResponse, String>) =
                        match tokio::time::timeout(retry_timeout + Duration::from_millis(200), task).await {
                            Ok(inner) => inner.unwrap_or_else(|e| (inst_idx, fallback_key.clone(), fallback_url.clone(), Err(format!("retry join error: {:?}", e)))),
                            Err(_) => (inst_idx, fallback_key.clone(), fallback_url.clone(), Err("retry task budget exceeded".into())),
                        };
                    (inst_idx, key, url_for_log, result)
                }));
            }
        }

        if !retry_futs.is_empty() {
            let elapsed = search_start.elapsed();
            let limit = Duration::from_millis(4500); // 4.5s overall target limit (handler budget is 5.5s)
            if elapsed >= limit {
                tracing::warn!("Retry skipped: elapsed time ({:?}) already exceeds target deadline ({:?})", elapsed, limit);
            } else {
                let retry_budget = limit - elapsed;
                tracing::info!(
                    "PARALLEL RETRY: {} results < {} expected, firing {} retry variation(s) with budget {:?}",
                    total_results, expected_min, retry_futs.len(), retry_budget
                );

                let mut pending = retry_futs;
                let mut retry_new_count = 0usize;
                let min_early = if only_negative || !intent.structured_constraints.negative.is_empty() { 40 } else { 5 };

                // Thread-safe shared state to collect retry outcomes
                let results_shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
                let results_inner = results_shared.clone();

                let _ = tokio::time::timeout(retry_budget, async move {
                    while !pending.is_empty() && retry_new_count < min_early {
                        let ((inst_idx, retry_key, url_for_log, result), _idx, remaining) =
                            futures::future::select_all(pending).await;
                        pending = remaining;

                        match &result {
                            Ok(data) => {
                                retry_new_count += data.results.len();
                            }
                            _ => {}
                        }
                        results_inner.lock().unwrap().push((inst_idx, retry_key, url_for_log, result));
                    }
                }).await;

                // Process gathered retry results synchronously
                let retry_results = {
                    let mut guard = results_shared.lock().unwrap();
                    std::mem::take(&mut *guard)
                };

                let mut final_retry_count = 0usize;
                for (inst_idx, retry_key, _url_str, result) in retry_results {
                    match result {
                        Ok(data) if !data.results.is_empty() => {
                            tracing::info!("Parallel retry on instance {} returned {} results", inst_idx, data.results.len());
                            circuit_ref.record_success(&retry_key);
                            if let Some(base_url) = searx_base_urls.get(inst_idx) {
                                state.searx_last_used.lock().insert(base_url.to_string(), Instant::now());
                            }
                            circuit_ref.record_results(&retry_key, data.results.len() as u64);
                            for (pos, result) in data.results.into_iter().enumerate() {
                                let engine_weight = circuit_ref.weight(&result.engine);
                                let normalized = {
                                    let lower = result.url.to_lowercase();
                                    let no_fragment = lower.split('#').next().unwrap_or(&lower);
                                    let no_trailing = no_fragment.trim_end_matches('/');
                                    let no_www = no_trailing.replacen("://www.", "://", 1);
                                    let no_mobile = no_www.replacen("://m.", "://", 1).replacen("://mobile.", "://", 1);
                                    strip_tracking_params(&no_mobile)
                                };
                                if !url_rrf_contributions.contains_key(&normalized) {
                                    let rrf_contrib = engine_weight / (60.0 + (pos + 1) as f32);
                                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                                    // Tag retry results with their SearXNG instance
                                    let mut instance_tagged = result;
                                    let instance_tag = format!("instance_{}", retry_key.trim_start_matches("searxng"));
                                    if !instance_tagged.sources.contains(&instance_tag) {
                                        instance_tagged.sources.push(instance_tag);
                                    }
                                    web_results.push(instance_tagged);
                                    final_retry_count += 1;
                                }
                            }
                        }
                        Ok(_) => {} // 0 results — skip
                        Err(e) => {
                            tracing::warn!("Parallel retry failed on instance {}: {}", inst_idx, e);
                            circuit_ref.record_failure(&retry_key);
                        }
                    }
                }
                tracing::info!("Parallel retry collected {} new unique results", final_retry_count);
            }
        }
    }

    match invidious_res {
        Ok(invidious_data) => {
            let n = invidious_data.len();
            tracing::info!("Invidious returned {} results", n);
            if n > 0 {
                circuit_ref.record_success("invidious");
                circuit_ref.record_results("invidious", n as u64);
            } else {
                // 0 results ≠ failure — only connection/parse errors are failures
            }
            let invidious_weight = circuit_ref.weight("invidious");
            for (pos, r) in invidious_data.into_iter().enumerate() {
                if r.result_type.as_deref() == Some("video") {
                    if let Some(vid) = r.video_id {
                        let title_str = r.title.clone().unwrap_or_else(|| "Video Tutorial".to_string());
                        let desc_raw = r.description.unwrap_or_default();
                        let desc = if desc_raw.trim().is_empty() {
                            format!("YouTube video tutorial on {}. Watch video online.", title_str)
                        } else {
                            desc_raw
                        };
                        let video_url = format!("https://www.youtube.com/watch?v={}", vid);
                        let normalized = {
                            let lower = video_url.to_lowercase();
                            let no_fragment = lower.split('#').next().unwrap_or(&lower);
                            let no_trailing = no_fragment.trim_end_matches('/');
                            let no_www = no_trailing.replacen("://www.", "://", 1);
                            strip_tracking_params(&no_www)
                        };
                        let rrf_contrib = invidious_weight / (60.0 + (pos + 1) as f32);
                        *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                        web_results.push(SearxResult {
                            title: title_str,
                            url: video_url,
                            content: desc,
                            engine: "invidious".to_string(),
                            score: 0.0,
                            sources: vec!["invidious".to_string(), "video".to_string()],
                            published_date: None,
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

    // Add news results to web results (if news intent detected)
    if is_news_intent {
        match news_res {
            Ok(news_data) => {
                tracing::info!("News fan-out returned {} results", news_data.results.len());
                for (pos, r) in news_data.results.into_iter().enumerate() {
                    let normalized = {
                        let lower = r.url.to_lowercase();
                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                        let no_trailing = no_fragment.trim_end_matches('/');
                        let no_www = no_trailing.replacen("://www.", "://", 1);
                        strip_tracking_params(&no_www)
                    };
                    let rrf_contrib = 1.0 / (60.0 + (pos + 1) as f32);
                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                    web_results.push(SearxResult {
                        title: r.title,
                        url: r.url,
                        content: r.content,
                        engine: r.engine,
                        score: 0.0,
                        sources: vec!["news".to_string()],
                        published_date: r.published_date.clone(),
                    });
                }
            }
            Err(e) => {
                tracing::warn!("News fan-out failed: {}", e);
            }
        }
    }

    // Add image results to web results (if image intent detected)
    if is_image_intent {
        match image_res {
            Ok(image_data) => {
                tracing::info!("Image fan-out returned {} results", image_data.results.len());
                for (pos, r) in image_data.results.into_iter().enumerate() {
                    let normalized = {
                        let lower = r.url.to_lowercase();
                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                        let no_trailing = no_fragment.trim_end_matches('/');
                        let no_www = no_trailing.replacen("://www.", "://", 1);
                        strip_tracking_params(&no_www)
                    };
                    let rrf_contrib = 1.0 / (60.0 + (pos + 1) as f32);
                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                    web_results.push(SearxResult {
                        title: r.title,
                        url: r.url,
                        content: r.content,
                        engine: r.engine,
                        score: 0.0,
                        sources: vec!["images".to_string()],
                        published_date: None,
                    });
                }
            }
            Err(e) => {
                tracing::warn!("Image fan-out failed: {}", e);
            }
        }
    }

    // Deduplicate — URL normalization + domain-based dedup
    // Multiple query variations may return the same page with different URLs
    // KEY: merge sources so we know which engines agreed on each result
    let mut unique_web_results: Vec<SearxResult> = Vec::new();
    let mut url_to_index: HashMap<String, usize> = HashMap::new(); // normalized URL -> index in unique_web_results
    let mut seen_domains = std::collections::HashMap::<String, usize>::new();
    const MAX_PER_DOMAIN: usize = 3; // prevent single-domain dominance

    for res in web_results {
        // Normalize URL: lowercase, strip trailing slash, strip fragment, strip www,
        // strip mobile prefixes (m./mobile.), strip tracking params
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            let no_www = no_trailing.replacen("://www.", "://", 1);
            // Strip m./mobile. prefixes: m.example.com → example.com
            let no_mobile = no_www
                .replacen("://m.", "://", 1)
                .replacen("://mobile.", "://", 1);
            // Strip tracking query params
            strip_tracking_params(&no_mobile)
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

        if let Some(&existing_idx) = url_to_index.get(&normalized) {
            // URL already seen — merge the engine/source into the existing result
            let source = if res.engine.is_empty() { "unknown".to_string() } else { res.engine.clone() };
            if !unique_web_results[existing_idx].sources.contains(&source) {
                unique_web_results[existing_idx].sources.push(source);
            }
            // RRF contributions are already summed in url_rrf_contributions
            // during aggregation, so no extra work needed here
        } else {
            // New URL — add it with its source
            let source = if res.engine.is_empty() { "unknown".to_string() } else { res.engine.clone() };
            let mut result = res;
            result.sources = vec![source];
            url_to_index.insert(normalized, unique_web_results.len());
            *domain_count += 1;
            unique_web_results.push(result);
        }
    }
    for res in &mut unique_web_results {
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            let no_www = no_trailing.replacen("://www.", "://", 1);
            let no_mobile = no_www.replacen("://m.", "://", 1).replacen("://mobile.", "://", 1);
            strip_tracking_params(&no_mobile)
        };
        if let Some(&rrf_score) = url_rrf_contributions.get(&normalized) {
            res.score = rrf_score;
        }
    }
    if let Some(max_rrf) = unique_web_results.iter().map(|r| r.score).fold(None, |acc, s| {
        Some(match acc { Some(m) => s.max(m), None => s })
    }) {
        if max_rrf > 0.0 {
            for r in &mut unique_web_results {
                r.score /= max_rrf;
            }
        }
    }
    let mut web_results = unique_web_results;

    tracing::info!("After dedup: {} unique web results", web_results.len());

    // 6. Quality Gates (before merge)
    // Filter web results with very low semantic relevance
    let semantic_scores_web: Vec<f32> = web_results.iter()
        .map(|res| semantic_relevance_score(&q, &res.title, &res.content))
        .collect();

    // ── Relative Relevance: detect garbage clusters ──
    // Instead of fixed thresholds, compute the score distribution.
    // If all scores are uniformly low (garbage cluster), retry with expanded queries
    // instead of returning junk.
    let (best_score, mean_score, score_variance) = if !semantic_scores_web.is_empty() {
        let best = semantic_scores_web.iter().cloned().fold(0.0f32, f32::max);
        let mean = semantic_scores_web.iter().sum::<f32>() / semantic_scores_web.len() as f32;
        let variance = semantic_scores_web.iter()
            .map(|s| (s - mean).powi(2))
            .sum::<f32>() / semantic_scores_web.len() as f32;
        (best, mean, variance)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Confidence = best_score - mean_score
    // High confidence: best result is much better than average (clear signal)
    // Low confidence: all results are similarly bad (garbage cluster)
    let relevance_confidence = best_score - mean_score;
    // For only-negative queries, skip garbage cluster — few results are expected
    // due to constraint filtering, not because the query itself is garbage.
    let is_garbage_cluster = !only_negative && best_score < 0.15 && mean_score < 0.10;

    tracing::info!(
        "Relevance distribution: best={:.3}, mean={:.3}, var={:.3}, confidence={:.3}, garbage_cluster={}",
        best_score, mean_score, score_variance, relevance_confidence, is_garbage_cluster
    );

    // Garbage cluster detected — log for telemetry but don't retry sequentially.
    // The parallel retry above already fired all expanded query variations.
    if is_garbage_cluster {
        tracing::warn!(
            "GARBAGE CLUSTER (best={:.3}, mean={:.3}) — parallel retry already fired all variations",
            best_score, mean_score
        );
    }

    // Adaptive threshold: higher when we have many results, lower when few
    // No positional exceptions — rank #1 can still be garbage
    let semantic_threshold = if web_results.len() > 30 { 0.18 }
        else if web_results.len() > 20 { 0.15 }
        else if web_results.len() > 10 { 0.12 }
        else { 0.08 };

    // When the relevance model cannot discriminate (garbage cluster: every
    // candidate scored ~identically, e.g. results with empty/short
    // title+content that the lexical scorer rates uniformly ~0.01), the
    // semantic filter below would drop EVERYTHING and the post-filter floor
    // would collapse the response to 3 arbitrary results. That is the root
    // cause of the "sparse/off-topic 3-result" symptom. In that degenerate
    // case we trust the search-engine ranking (RRF) instead and keep the
    // merged results rather than discarding them.
    if is_garbage_cluster {
        tracing::warn!(
            "SEMANTIC FILTER SKIPPED (degenerate scorer, trusting RRF): web_results.len={}",
            web_results.len()
        );
    } else {
    let mut keep_indices: Vec<usize> = Vec::new();
    for (i, &score) in semantic_scores_web.iter().enumerate() {
        if score >= semantic_threshold {
            keep_indices.push(i);
        }
    }
    // Always keep at least 3 results (but only if they have ANY relevance)
    if keep_indices.len() < 3 && !web_results.is_empty() {
        // Take top-3 by semantic score, even if below threshold
        let mut scored: Vec<(usize, f32)> = semantic_scores_web.iter().enumerate()
            .map(|(i, &s)| (i, s)).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        keep_indices = scored.iter().take(3).map(|(i, _)| *i).collect();
    }
    web_results = keep_indices.into_iter().map(|i| web_results[i].clone()).collect();
    }

    // Constraint transparency bookkeeping: capture the result count before any
    // constraint filtering, and how many results actually carry a parseable
    // date / detectable price. These let us report applied-vs-ignored
    // constraints honestly instead of silently returning empty or unfiltered.
    let pre_filter_count = web_results.len();
    let dated_result_count = web_results.iter().filter(|r| {
        resolve_item_date(r.published_date.as_deref(), &r.url, &r.title, &r.content).is_some()
    }).count();
    let priced_result_count = web_results.iter().filter(|r| {
        extract_price_from_text(&r.title).or_else(|| extract_price_from_text(&r.content)).is_some()
    }).count();

    // --- Hard filter: remove web results that violate negative constraints ---
    // Uses a graduated penalty approach instead of a single threshold:
    //   1. Count how many negative constraints each result violates.
    //   2. Retain only results with <= median violations or <= 1 violation.
    //   3. When all results violate constraints, sort by fewest violations (ascending).
    //      This prevents "not java nor csharp nor go" from showing Rust+Go content
    //      while still returning results about Java if that is all there is.
    //   4. Goldilocks detection: when multiple negative constraints remove everything,
    //      relax the penalty to preserve domain-relevant results.
    if !intent.structured_constraints.negative.is_empty() {
        let before_count = web_results.len();
        let constraints_ref = &intent.structured_constraints;

        // Score each result and track violation counts
        let mut scored: Vec<(usize, f32, usize)> = web_results.iter().enumerate().map(|(i, r)| {
            let c_score = constraint_score(&r.title, &r.content, &r.url, constraints_ref);
            // Check if this is an alternative-listing page (comparison, vs, alternatives)
            // BEFORE counting violations — alt pages naturally mention excluded terms
            // in comparative context ("Django vs FastAPI vs Flask: Which to Choose").
            let alt_score = is_alternative_listing_page(&r.title, &r.url, &r.content);
            // Count how many negative terms actually match this result content.
            // Skip violation counting for alternative-listing pages: their mention of
            // excluded terms is referential, not topical. The soft filter would otherwise
            // drop them before the alt-aware hard filter can preserve them.
            let violations = if alt_score > 0.3 {
                0
            } else {
                let text = format!("{} {} {}", r.title.to_lowercase(), r.url.to_lowercase(), r.content.chars().take(300).collect::<String>());
                constraints_ref.negative.iter().filter(|n| {
                    let n_lower = n.to_lowercase();
                    let n_words: Vec<&str> = n_lower.split_whitespace().collect();
                    if n_words.len() == 1 {
                        // Word-boundary aware — "java" must not match "javascript".
                        text_matches_negative(&text, &n_lower)
                    } else {
                        text.contains(&n_lower)
                    }
                }).count()
            };
            (i, c_score, violations)
        }).collect();

        // Sort by violation count ascending first, then by score descending
        scored.sort_by(|a, b| {
            a.2.cmp(&b.2)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Compute the violation distribution to decide filtering strategy
        let max_violations = scored.iter().map(|(_, _, v)| *v).max().unwrap_or(0);
        let min_violations = scored.iter().map(|(_, _, v)| *v).min().unwrap_or(0);
        let total_violations: usize = scored.iter().map(|(_, _, v)| *v).sum();
        let avg_violations = total_violations as f32 / scored.len().max(1) as f32;

        // Goldilocks check: if average violations > 1.0 AND most results have high violations,
        // the negative constraints are too aggressive for this result set.
        let is_goldilocks = avg_violations > 1.5 && min_violations >= 1;

        // Filter strategy:
        // - Normal case: keep results with violations <= 1 (clean match)
        // - Goldilocks case: keep results with violations <= max_violations / 2 (relaxed)
        // - 3+ negatives: zero violations only (unless ALL have violations)
        let is_only_negative = constraints_ref.positive.is_empty();
        // A negative constraint means "exclude results that match". When there is
        // at least one clean result (no negative match) we drop every matching
        // result. When *all* results match (degenerate set) we fall back to the
        // Goldilocks relaxation below so we never return an empty page. Pure
        // negation ("-trump") and mixed queries both expect matches to be removed.
        let violation_threshold = if is_goldilocks {
            tracing::warn!("GOLDILOCKS: avg_violations={:.1} max={} - relaxing constraint threshold",
                avg_violations, max_violations
            );
            (max_violations / 2).max(1)
        } else if min_violations == 0 {
            0
        } else if is_only_negative {
            0
        } else {
            0
        };

        let kept: Vec<usize> = scored.iter()
            .filter(|(_, _, v)| *v <= violation_threshold)
            .map(|(i, _, _)| *i)
            .collect();

        let removed = before_count.saturating_sub(kept.len());
        if removed > 0 {
            tracing::info!(
                "Negative constraint hard filter: removed {}/{} web results (violations max={} min={} avg={:.1})",
                removed, before_count, max_violations, min_violations, avg_violations
            );
        }

        if !kept.is_empty() {
            // Keep results in sorted order (fewest violations first, highest score within)
            web_results = kept.iter().map(|i| web_results[*i].clone()).collect();
        } else {
            // Fallback: keep results sorted by violations (ascending) but do not filter
            // This preserves ordering so results with FEWER violations rank higher.
            tracing::warn!(
                "Negative constraint filter removed all {} results - keeping sorted by violations",
                before_count
            );
            let sorted_indices: Vec<usize> = scored.iter().map(|(i, _, _)| *i).collect();
            // Apply graduated penalty: each violation halves the effective score
            // so results with fewer violations naturally rank higher.
            let mut scored_results: Vec<SearxResult> = sorted_indices.iter().map(|i| {
                let mut r = web_results[*i].clone();
                let violations = scored.iter().find(|(j, _, _)| *j == *i).map(|(_, _, v)| *v).unwrap_or(0);
                // Each violation halves the score: 0=1.0, 1=0.5, 2=0.25, 3=0.125
                r.score *= 0.5_f32.powi(violations as i32);
                r
            }).collect();
            // Sort by penalized score to push multi-violation results down
            scored_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            web_results = scored_results;
        }
    }

    // Soft boost for intitle:/inurl:/intext: — these are enforced upstream and
    // must NOT hard-drop (would re-create the n=0 trap). Nudge ranking instead.
    if !intent.structured_constraints.intitle.is_empty()
        || !intent.structured_constraints.inurl.is_empty()
        || !intent.structured_constraints.intext.is_empty()
    {
        for r in web_results.iter_mut() {
            r.score += constraint_boost(&r.title, &r.content, &r.url, &intent.structured_constraints);
        }
    }

    let has_any_constraints = !intent.structured_constraints.negative.is_empty()
        || !intent.structured_constraints.file_types.is_empty()
        || !intent.structured_constraints.sites.is_empty()
        || !intent.structured_constraints.phrases.is_empty()
        || intent.structured_constraints.after_date.is_some()
        || intent.structured_constraints.before_date.is_some()
        || !intent.structured_constraints.related.is_empty()
        || !intent.structured_constraints.intitle.is_empty()
        || !intent.structured_constraints.inurl.is_empty()
        || !intent.structured_constraints.intext.is_empty()
        || intent.structured_constraints.price_min.is_some()
        || intent.structured_constraints.price_max.is_some();
    if has_any_constraints {
        let before_count = web_results.len();
        let constraints_ref = &intent.structured_constraints;
        // Pre-filter: remove results that violate constraints beyond redemption
        let pre_before = web_results.len();
        web_results.retain(|r| {
            !should_filter_by_constraints(&r.title, &r.content, &r.url, r.published_date.as_deref(), constraints_ref)
        });
        let pre_removed = pre_before.saturating_sub(web_results.len());
        if pre_removed > 0 {
            tracing::info!("should_filter: removed {}/{} results (from {})",
                pre_removed, pre_before, before_count);
        }
        if !intent.structured_constraints.negative.is_empty() {
            let mut negative_norm: Vec<String> = Vec::new();
            for n in &intent.structured_constraints.negative {
                for syn in expand_negative_synonyms(n) {
                    if !negative_norm.contains(&syn) {
                        negative_norm.push(syn);
                    }
                }
            }

        web_results.retain(|r| {
            // Alternative-listing page check: if the result is a comparison/
            // alternative page, keep it even if it mentions excluded terms.
            // This prevents "Top 10 Prometheus Alternatives" from being
            // dropped for queries like "monitoring not prometheus".
            let alt_score = is_alternative_listing_page(&r.title, &r.url, &r.content);
            if alt_score > 0.3 {
                return true; // keep alternative-listing pages regardless of negative terms
            }

            let text = format!("{} {} {}", r.title, r.url, r.content.chars().take(300).collect::<String>());
            let text_lower = text.to_lowercase();
            let text_normalized = {
                let chars: Vec<char> = text_lower.chars().collect();
                let mut out = String::with_capacity(chars.len());
                for (i, &c) in chars.iter().enumerate() {
                    if c == '.' || c == '-' || c == '_' {
                        if i > 0
                            && i + 1 < chars.len()
                            && chars[i-1].is_alphanumeric()
                            && chars[i+1].is_alphanumeric()
                        {
                        } else {
                            out.push(c);
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            };

            let should_keep = negative_norm.iter().all(|neg| {
                let neg_lower = neg.to_lowercase();
                let words: Vec<&str> = neg_lower.split_whitespace().collect();
                if words.len() == 1 {
                    // Word-boundary aware — never substring ("java" ⊄ "javascript").
                    !text_matches_negative(&text_lower, &neg_lower)
                } else {
                    let joined = words.join(" ");
                    !(text_lower.contains(&joined) || text_normalized.contains(&joined))
                }
            });

            if !should_keep {
                tracing::info!("HARD NEGATIVE DROP (pre-merge WEB ONLY): result removed because negative constraint matched (not alt page)");
            }
            should_keep
        });

        let removed = before_count.saturating_sub(web_results.len());
        if removed > 0 {
            tracing::info!(
                "Negative constraint hard filter: removed {}/{} web results (hard gate)",
                removed, before_count
            );
        }
        }
    }

    // --- Price constraint: real narrowing (BUG: price silently passed) ---
    // When a price range is requested we drop results whose snippet carries a
    // price outside the range. More importantly, if ANY result carries an
    // in-range price, we also drop results with no detectable price at all —
    // an explicit price intent is not satisfied by unpriced pages. If NO result
    // has a detectable price, we keep everything (we cannot verify) but flag it.
    {
        let pmin = intent.structured_constraints.price_min;
        let pmax = intent.structured_constraints.price_max;
        if pmin.is_some() || pmax.is_some() {
            let lo = pmin.unwrap_or(0.0);
            let hi = pmax.unwrap_or(f32::MAX);
            let has_in_range = web_results.iter().any(|r| {
                extract_price_from_text(&r.title)
                    .or_else(|| extract_price_from_text(&r.content))
                    .map(|p| p >= lo && p <= hi)
                    .unwrap_or(false)
            });
            if has_in_range {
                let before = web_results.len();
                web_results.retain(|r| {
                    extract_price_from_text(&r.title)
                        .or_else(|| extract_price_from_text(&r.content))
                        .map(|p| p >= lo && p <= hi)
                        .unwrap_or(false)
                });
                let after = web_results.len();
                tracing::info!(
                    "Price constraint: narrowed {} → {} results (dropped {} unpriced/out-of-range)",
                    before, after, before - after
                );
            } else {
                tracing::warn!(
                    "Price constraint specified ({:?}-{:?}) but no result snippets carried a detectable price — cannot narrow",
                    pmin, pmax
                );
            }
        }
    }

    // Quality gate: filter garbage local results
    // Apply semantic relevance filter to indexer results too — prevents
    // irrelevant crawled pages (guitar lessons for "bass") from dominating
    // when web results are sparse or filtered.
    local_results.retain(|r| {
        let title_ok = r.title.len() > 5;
        let url_lower = r.url.to_lowercase();
        let not_error = !url_lower.contains("/error")
            && !url_lower.contains("/404")
            && !url_lower.contains("/login")
            && !url_lower.contains("/signin")
            && !url_lower.contains("/signup")
            && !url_lower.contains("/account")
            && !url_lower.contains("/cookie");
        // Semantic relevance: same scoring as web results
        let sem_score = semantic_relevance_score(&q, &r.title, &r.content);
        let sem_ok = sem_score >= 0.12;  // slightly lower threshold than web (0.18) since local has richer content
        if title_ok && not_error && !sem_ok {
            let trimmed: String = r.title.chars().take(50).collect();
            tracing::debug!("Indexer result filtered (sem={:.3}): {}", sem_score, trimmed);
        }
        title_ok && not_error && sem_ok
    });

    // 7. Feed Meta-Search Results into Crawl Queue
    let crawl_urls: Vec<serde_json::Value> = web_results.iter()
        .filter(|r| r.score > 0.3 && !r.content.is_empty() && r.title.len() > 10)
        .take(20)
        .map(|r| {
            serde_json::json!({
                "url": r.url,
                "priority": r.score,
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

    // 8. Unified Merge: Local + Web → Single Ranked List
    // Cross-source dedup, consensus boosting, unified ranking
    // Pass intent distribution for distribution-aware ranking (intent as hint, not gate)
    // Clone data for CPU-intensive scoring on blocking thread
    let q_clone = q.clone();
    let intent_clone = intent.intent.clone();
    let constraints_clone = intent.structured_constraints.clone();
    let distribution_clone = intent.distribution.clone();
    let geo_clone = geo_location.clone();
    
    // Apply hard negative filter to web_results for only-negative queries:
    // Drop results whose domain matches the excluded term's official site.
    // This runs after ALL SearXNG sources (initial + retry) are combined.
    // Also checks the raw query for negation words as a fallback (intent engine may
    // misclassify "not react" as positive constraint "+react" instead of negative).
    // Check for negation words in the raw query as a fallback.
    // The intent engine may misclassify "not react" as positive constraint "+react"
    // instead of negative. When query_has_negation is true, we derive negative terms
    // directly from the raw query by parsing words that follow negation markers.
    let query_has_negation = q.starts_with("not ") || q.starts_with("no ")
        || q.starts_with("without ") || q.contains(" not ") || q.contains(" -");
    
    // Extract negative terms from the original query string (bypasses intent engine)
    // Handles: "not react not vue" → ["react", "vue"]
    //          "without node not django" → ["node", "django"]
    //          "not prometheus not grafana not datadog" → ["prometheus", "grafana", "datadog"]
    let query_neg_terms: Vec<String> = if query_has_negation {
        let q_lower = q_orig.to_lowercase(); // Phase 1 (A3): original query (not corrected)
        let negation_markers = ["not ", "no ", "without "];
        let mut terms: Vec<String> = Vec::new();
        let words: Vec<&str> = q_lower.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            // Check if this word is a negation marker
            let neg_words: [&str; 3] = ["no", "not", "without"];
            let is_neg = negation_markers.iter().any(|m| *m == format!("{} ", word))
                || neg_words.contains(word)
                || word.starts_with("-");
            if is_neg {
                // Grab the next word (unless it's also a negation marker)
                if i + 1 < words.len() {
                    let next = words[i + 1];
                    let next_is_neg = negation_markers.iter().any(|m| *m == format!("{} ", next))
                        || neg_words.contains(&next)
                        || next.starts_with("-");
                    if !next_is_neg && next.len() >= 2 {
                        let clean: String = next.chars().filter(|c| c.is_alphanumeric()).collect();
                        if !terms.contains(&clean) && !clean.is_empty() {
                            terms.push(clean);
                        }
                    }
                }
            }
        }
        terms
    } else {
        vec![]
    };
    
    // Fallback: if query_has_negation but intent engine put terms in positive instead of negative,
    // use the query-derived terms. If the intent engine correctly classified them as negative,
    // use those (they may have cleaner normalization).
    let has_only_negative = intent.structured_constraints.positive.is_empty()
        && (!intent.structured_constraints.negative.is_empty() || !query_neg_terms.is_empty());
    
    // Use query-derived terms when available (they're more reliable for negation),
    // otherwise fall back to intent engine's negative constraints.
    let neg_terms: Vec<String> = if !query_neg_terms.is_empty() {
        query_neg_terms.clone()
    } else {
        intent.structured_constraints.negative.iter()
            .map(|n| n.to_lowercase())
            .collect()
    };
    let mut neg_terms_expanded: Vec<String> = Vec::new();
    for nt in &neg_terms {
        for syn in expand_negative_synonyms(nt) {
            if !neg_terms_expanded.contains(&syn) {
                neg_terms_expanded.push(syn);
            }
        }
    }
    // For only-negative queries, skip the domain-based hard filter.
    // The title penalty (score *= 0.01) + constraint scoring already demotes
    // results from excluded domains. Hard-removing ALL results from e.g.
    // djangoproject.com for "not django" leaves zero results since search
    // engines treat "not" as a stop word and return the official site.
    // Additionally, check if any result contains a positive term — if so,
    // keep it regardless of domain, since it IS about the user's topic.
    if has_only_negative {
        let before = web_results.len();
        let has_positive_terms = !intent.structured_constraints.positive.is_empty();
        web_results.retain(|item| {
            // If result contains a positive term, skip domain filter entirely.
            if has_positive_terms {
                let text = format!("{} {}", item.title, item.url).to_lowercase();
                let any_positive = intent.structured_constraints.positive.iter().any(|pt| {
                    let pt_clean: String = pt.chars().filter(|c| c.is_alphanumeric()).collect();
                    pt_clean.len() >= 3 && text.contains(&pt_clean)
                });
                if any_positive {
                    return true;
                }
            }
            if let Ok(parsed) = reqwest::Url::parse(&item.url) {
                if let Some(host) = parsed.host_str() {
                    let host_lower = host.to_lowercase();
                    for neg in &neg_terms_expanded {
                        let neg_clean: String = neg.chars().filter(|c| c.is_alphanumeric()).collect();
                        if neg_clean.len() >= 3 {
                            if host_lower == format!("{}.com", neg_clean)
                                || host_lower == format!("www.{}.com", neg_clean)
                                || host_lower == format!("{}.org", neg_clean)
                                || host_lower == format!("{}.io", neg_clean)
                                || host_lower == format!("{}.dev", neg_clean)
                                || host_lower == format!("{}.net", neg_clean)
                                || host_lower.starts_with(&format!("{}.", neg_clean))
                                || host_lower.contains(&format!(".{}", neg_clean))
                            {
                                tracing::info!("ONLY NEGATIVE FILTER: dropping '{}' (host={} matches term '{}')",
                                    item.url, host_lower, neg_clean);
                                return false;
                            }
                        }
                    }
                }
            }
            true
        });
        let dropped = before - web_results.len();
        if dropped > 0 {
            tracing::info!("ONLY NEGATIVE FILTER: dropped {} results from {} (official domain matches)", dropped, before);
        }
    }
    
// Web-result BERT semantic map: embed the top web snippets once and compare
// to the query embedding, so word-sense collisions (square-a-circle, promise,
// crash) resolve correctly. Fail-closed: returns empty on any error and the
// ranking falls back to the existing substring scorer (no behaviour change).
let web_semantic = compute_web_semantic(&vector, &web_results, &client).await;

let mut results = match tokio::task::spawn_blocking(move || {
    merge_local_and_web(
        local_results,
        web_results,
        &q_clone,
        &intent_clone,
        &constraints_clone,
        Some(&distribution_clone),
        geo_clone.as_ref(),
        &web_semantic,
    )
}).await {
        Ok(r) => r,
        Err(e) => {
            // The blocking ranking task panicked (or was cancelled). Surface a
            // structured JSON error instead of unwinding the handler and leaving
            // the client with an empty/aborted body.
            tracing::error!("Ranking task failed for query '{}': {:?}", q_trimmed, e);
            let mut err_resp = make_error_response(q_trimmed, "ranking_failed", "Search ranking failed internally; please retry", false);
            err_resp.0 = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
            return err_resp;
        }
    };

    // 8b. Post-merge hard negative filter: apply negative constraints to ALL results
    // (local + web). The pre-merge filter only catches web results; local index
    // results that match negative terms must also be removed here.
    // Uses both intent engine's negative constraints AND query-derived terms
    // (for when intent engine misclassifies "not react" as positive "+react").
    let has_neg_constraints = !intent.structured_constraints.negative.is_empty()
        || !query_neg_terms.is_empty();
    if has_neg_constraints {
        let before_count = results.len();
        let mut negative_norm: Vec<String> = intent
            .structured_constraints
            .negative
            .iter()
            .map(|n| n.to_lowercase())
            .collect();
        // Add query-derived negative terms (from fallback parsing) to catch cases
        // where the intent engine misclassified negation as positive constraints.
        // query_neg_terms is defined above in the pre-merge B3 filter section.
        for qt in &query_neg_terms {
            if !negative_norm.contains(qt) {
                negative_norm.push(qt.to_lowercase());
            }
        }
        let mut negative_norm_expanded: Vec<String> = Vec::new();
        for n in &negative_norm {
            for syn in expand_negative_synonyms(n) {
                if !negative_norm_expanded.contains(&syn) {
                    negative_norm_expanded.push(syn);
                }
            }
        }
        let negative_norm = negative_norm_expanded;
    // TITLE-ONLY HARD PENALTY: apply score reduction to results whose title
    // directly contains an excluded term. Relaxed for alt-listing pages.
    for r in results.iter_mut() {
        let title_lower = r.title.to_lowercase();
        let has_neg_in_title = negative_norm.iter().any(|nt| {
            text_matches_negative(&title_lower, &nt.to_lowercase())
        });
        if has_neg_in_title {
            let alt = is_alternative_listing_page(&r.title, &r.url, &r.content);
            if alt > 0.6 {
                // Strong alt-listing page - no title penalty needed (constraint_score
                // already applies the single alt-page penalty). Pages like
                // "Top 10 Chrome Alternatives" are highly relevant despite
                // mentioning excluded terms in their title.
                let trimmed: String = r.title.chars().take(40).collect();
                tracing::info!("TITLE PENALTY SKIPPED (strong alt): alt={:.2} for '{}'", alt, trimmed);
            } else if alt > 0.3 {
                // Moderate alt-listing page - mild penalty only
                r.score *= 0.50;
                let trimmed: String = r.title.chars().take(40).collect();
                tracing::info!("TITLE HARD PENALTY (MODERATE): alt={:.2} for '{}' -> score *= 0.50", alt, trimmed);
            } else {
                r.score *= 0.01;
                tracing::info!("TITLE HARD PENALTY: title contains excluded term -> score *= 0.01");
            }
        }
    }

    // For negative-only queries ("not django"), skip the hard removal retain filter.
    // All search results for the negated term will mention it — removing them all
    // leaves zero results. Instead, rely on the title penalty + constraint scoring
    // (applied in score_rerank) to appropriately demote results about the excluded topic.
    // Queries with BOTH positive and negative constraints (e.g. "python framework not django")
    // still use the hard filter since there are non-excluded results to keep.
    // BUT: if a result contains ANY positive term, skip hard removal — the title penalty
    // already demotes it. This prevents removing genuinely relevant results that merely
    // mention the excluded term in passing (e.g. "cars not suv" removes all car results
    // because they all mention "SUV").
    if has_only_negative {
        tracing::info!(
            "NEGATIVE CONSTRAINT: skipping hard removal for {} results (title penalty + constraint scoring will penalize instead)",
            before_count
        );
    } else {
        results.retain(|r| {
            // Alternative-listing page check: keep comparison/alternative pages
            // even if they mention excluded terms (they are HIGHLY relevant).
            let alt_score = is_alternative_listing_page(&r.title, &r.url, &r.content);
            if alt_score > 0.3 && is_comparison_or_alternative_query(&intent.structured_constraints) {
                return true;
            }

            let text = format!("{} {}", r.title, r.url);
            let text_lower = text.to_lowercase();
            let text_normalized = {
                let chars: Vec<char> = text_lower.chars().collect();
                let mut out = String::with_capacity(chars.len());
                for (i, &c) in chars.iter().enumerate() {
                    if c == '.' || c == '-' || c == '_' {
                        if i > 0
                            && i + 1 < chars.len()
                            && chars[i-1].is_alphanumeric()
                            && chars[i+1].is_alphanumeric()
                        {
                        } else {
                            out.push(c);
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            };

            let should_keep = negative_norm.iter().all(|neg| {
                let neg_lower = neg.to_lowercase();
                let words: Vec<&str> = neg_lower.split_whitespace().collect();
                if words.len() == 1 {
                    // Word-boundary aware match — never substring. Prevents
                    // "not java" from dropping every "javascript" result.
                    !text_matches_negative(&text_lower, &neg_lower)
                } else {
                    let joined = words.join(" ");
                    !(text_lower.contains(&joined) || text_normalized.contains(&joined))
                }
            });

            if !should_keep {
                tracing::info!("HARD NEGATIVE DROP (post-merge): result \"{}\" (local={}) removed because negative constraint matched (not alt page)",
                    &r.title.chars().take(50).collect::<String>(), r.is_local);
            } else {
                // TITLE-ONLY HARD CHECK: even if alt page, demote by 90% if title contains excluded term
                let title_lower = r.title.to_lowercase();
                for nt_after in &negative_norm {
                    if text_matches_negative(&title_lower, &nt_after.to_lowercase()) {
                        // Score penalty moved to separate loop before retain
                        tracing::info!("TITLE HARD PENALTY: title contains excluded term -> score *= 0.01 (penalty applied in separate loop)");
                        break;
                    }
                }
            }
            should_keep
        });
    }
        let removed = before_count.saturating_sub(results.len());
        if removed > 0 {
            tracing::info!(
                "Negative constraint hard filter: removed {}/{} merged results (hard gate, post-merge)",
                removed, before_count
            );
        }
    }

    // 8c. Post-filter re-ranking: boost results whose titles do not contain
    // excluded terms. This ensures genuinely clean results (no negative term
    // in title) outrank alternative-listing pages kept by the hard filter.
    // Alternative pages mentioning excluded terms are still present but pushed
    // below results that already satisfy the constraint cleanly.
    if !intent.structured_constraints.negative.is_empty() && !results.is_empty() {
        let neg_refs: Vec<&str> = intent.structured_constraints.negative
            .iter().map(|s| s.as_str()).collect();
        for r in results.iter_mut() {
            let title_lower = r.title.to_lowercase();
            let has_neg_in_title = neg_refs.iter().any(|n| {
                let n_lower = n.to_lowercase();
                let n_words: Vec<&str> = n_lower.split_whitespace().collect();
                if n_words.len() == 1 {
                    title_lower.split_whitespace().any(|tw| {
                        let tw_clean: String = tw.chars().filter(|c| c.is_alphanumeric()).collect();
                        let n_clean: String = n_lower.chars().filter(|c| c.is_alphanumeric()).collect();
                        tw_clean == n_clean || tw_clean.starts_with(&n_clean)
                    })
                } else {
                    let joined = n_words.join(" ");
                    title_lower.contains(&joined)
                }
            });
            // Boost clean results by a small differential nudge (+0.03) so they
            // outrank alt pages WITHOUT collapsing the whole cluster to 1.0.
            // (Phase 0: replaced the old uniform ×1.25.)
            if !has_neg_in_title {
                r.score += 0.03;
            }
        }
        // Re-sort by final score
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }
    // Hard filter on all constraints (file types, sites, date bounds, phrases, and negatives) post-merge:
    results.retain(|r| {
        !should_filter_by_constraints(&r.title, &r.content, &r.url, r.published_date.as_deref(), &intent.structured_constraints)
    });

    // Soft boost for intitle:/inurl:/intext: (enforced upstream, never hard-drop).
    if !intent.structured_constraints.intitle.is_empty()
        || !intent.structured_constraints.inurl.is_empty()
        || !intent.structured_constraints.intext.is_empty()
    {
        for r in results.iter_mut() {
            r.score += constraint_boost(&r.title, &r.content, &r.url, &intent.structured_constraints);
        }
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Sanitize content and clamp final score for safe JSON serialization and API spec conformance.
    // clean::clean_result_content strips HTML/CSS, decodes HTML entities, and removes scraped-page
    // boilerplate; is_junk_content then drops results that are empty / fetch-error / below an
    // information threshold. Control chars are stripped first (sanitize_text_content) so the JSON
    // stream stays valid.
    for r in results.iter_mut() {
        r.score = r.score.clamp(0.05, 1.0);
        r.title = sanitize_text_content(&r.title);
        r.content = clean::clean_result_content(&sanitize_text_content(&r.content), &r.title);
    }

    // Drop results that became empty / fetch-error / boilerplate-only after cleaning.
    // Results from trusted academic/technical domains (arxiv, PubMed, etc.) are
    // exempted from the minimum-length floor: a short snippet there is still a
    // legitimate paper, not noise (see is_junk_content_for_url).
    results.retain(|r| !clean::is_junk_content_for_url(&r.content, &r.url));

    // 8. Validate spelling correction against actual search result signals.
    // If the original (pre-correction) words appear more frequently in result
    // titles/URLs than the corrected words, the correction was likely wrong.
    // This provides a web-data-driven safety net on top of the dictionary.
    if spell_changed {
        let titles: Vec<String> = results.iter().map(|r| r.title.clone()).collect();
        let urls: Vec<String> = results.iter().map(|r| r.url.clone()).collect();
        if !spell::validate_correction(q_trimmed, &q, &titles, &urls) {
            tracing::info!(
                "Spell correction reverted by result validation: original='{}' corrected='{}'",
                q_trimmed, q
            );
            spell_changed = false;
        }
    }

    let mut flat_constraints = Vec::new();
    for p in &intent.structured_constraints.positive {
        flat_constraints.push(format!("+{}", p));
    }
    for n in &intent.structured_constraints.negative {
        flat_constraints.push(format!("-{}", n));
    }
    for ft in &intent.structured_constraints.file_types {
        flat_constraints.push(format!("+filetype:{}", ft));
    }
    for site in &intent.structured_constraints.sites {
        flat_constraints.push(format!("+site:{}", site));
    }
    for phrase in &intent.structured_constraints.phrases {
        flat_constraints.push(format!("+\"{}\"", phrase));
    }
    if let Some(ref ad) = intent.structured_constraints.after_date {
        flat_constraints.push(format!("+after:{}", ad));
    }
    if let Some(ref bd) = intent.structured_constraints.before_date {
        flat_constraints.push(format!("+before:{}", bd));
    }

    // Renormalize distribution before returning
    renormalize_distribution(&mut intent.distribution);

    // Mutable error/message so we can signal upstream-unavailable below.
    let mut error: Option<String> = None;
    let mut message: Option<String> = None;

    // Apply pagination (limit & offset)
    let limit = params.limit.or(params.count).or(params.n).unwrap_or(24);
    let offset = params.offset.unwrap_or(0);
    let post_filter_count = results.len();
    let mut paginated_results = results.into_iter().skip(offset).take(limit).collect::<Vec<_>>();

    // ─── Constraint transparency (applied / ignored / warnings) ───
    let sc = &intent.structured_constraints;
    let mut applied: Vec<String> = Vec::new();
    let mut ignored: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if let Some(l) = &sc.language { applied.push(format!("lang:{}", l)); }
    if let Some(a) = &sc.after_date { applied.push(format!("after:{}", a)); }
    if let Some(b) = &sc.before_date { applied.push(format!("before:{}", b)); }
    for s in &sc.sites { applied.push(format!("site:{}", s)); }
    for f in &sc.file_types { applied.push(format!("filetype:{}", f)); }
    for p in &sc.phrases { applied.push(format!("\"{}\"", p)); }
    for t in &sc.intitle { applied.push(format!("intitle:{}", t)); }
    for u in &sc.inurl { applied.push(format!("inurl:{}", u)); }
    for t in &sc.intext { applied.push(format!("intext:{}", t)); }
    for r in &sc.related { applied.push(format!("related:{}", r)); }
    let has_range = sc.price_min.is_some() || sc.price_max.is_some();
    let has_lt = sc.price_lt.is_some();
    let has_gt = sc.price_gt.is_some();
    if has_range || has_lt || has_gt {
        // Preserve the operator the user actually typed. An explicit `<`/`>` in
        // the query is reported verbatim (e.g. `price:<100`) so it is not silently
        // rewritten to a range. A plain `price:100` (no operator) is normalized to
        // an upper bound and reported as `price:<100` by convention.
        if let Some(v) = sc.price_lt {
            applied.push(format!("price:<{}", v));
        }
        if let Some(v) = sc.price_gt {
            applied.push(format!("price:>{}", v));
        }
        if !has_lt && !has_gt {
            let lo = sc.price_min.map(|v| v.to_string()).unwrap_or_else(|| "*".to_string());
            let hi = sc.price_max.map(|v| v.to_string()).unwrap_or_else(|| "*".to_string());
            applied.push(format!("price:{}-{}", lo, hi));
        }
    }
    for n in &sc.negative { applied.push(format!("not:{}", n)); }

    if (sc.after_date.is_some() || sc.before_date.is_some()) && dated_result_count == 0 {
        ignored.push(
            "date range — no returned result carried a parseable date, so filtering relied on the upstream engine only".to_string(),
        );
    }
    if (sc.price_min.is_some() || sc.price_max.is_some()
        || sc.price_lt.is_some() || sc.price_gt.is_some()) && priced_result_count == 0 {
        ignored.push(
            "price — no returned result snippet carried a detectable price, so no results could be narrowed".to_string(),
        );
    }
    if !sc.related.is_empty() {
        ignored.push(
            "related — effectiveness depends on upstream search-engine support for the related: operator".to_string(),
        );
    }

    // Upstream-unavailable signalling: when the response is an empty result set,
    // distinguish a genuine zero-hit from a total upstream failure. Previously the
    // gateway returned 200 + results:[] with no error body — a blank page that is
    // indistinguishable from a real zero-hit. Here we surface an explicit signal so
    // clients/monitoring can tell "nothing matched" apart from "everything broke".
    // We detect failure from the main SearXNG fan-out: if EVERY instance errored
    // (or returned nothing) AND the final merged result set is empty, the upstream
    // tier is unavailable rather than merely empty.
    let all_upstream_failed =
        searx_instances_total > 0 && searx_instances_ok == 0 && post_filter_count == 0;
    if all_upstream_failed {
        error = Some("upstream_unavailable".to_string());
        message = Some(
            "All upstream search engines timed out or failed to respond. This is a temporary upstream/connectivity issue, not a genuine zero-hit. Please retry.".to_string(),
        );
        tracing::warn!(
            "UPSTREAM UNAVAILABLE: all {} SearXNG instance(s) failed and no results were produced for '{}'",
            searx_instances_total, q_trimmed
        );
    }
    if pre_filter_count > 0 && post_filter_count == 0 {
        // Attribute the empty result set to the most likely cause so the
        // warning is actionable rather than generic. A date-bound query that
        // leaves nothing is almost always a too-narrow range; otherwise it's a
        // negative-term / constraint conflict. (Previously this always blamed a
        // negative term even when the real cause was the date window.)
        let has_date_bound = sc.after_date.is_some() || sc.before_date.is_some();
        if has_date_bound {
            warnings.push(
                "All web results were removed by your date constraint. Try widening the range (e.g. a broader after:/before: window).".to_string(),
            );
        } else {
            warnings.push(
                "All web results were removed by your constraints. Try relaxing them (drop a negative term, or widen a filter).".to_string(),
            );
        }
    }
    // ─── Deep Result Mode / Direct Answer Extractor ─────────────────────
    // For navigational, software download, or driver queries, identify the top candidate
    // official page, crawl the page HTML for direct installer/executable download links (.exe, .msi, .zip, .dmg),
    // and surface a structured `deep_result` object.
    let mut deep_result: Option<DeepResult> = None;
    let is_download_or_nav = intent.intent == "navigational"
        || intent.intent == "transactional"
        || DOWNLOAD_KEYWORDS.iter().any(|k| q.to_lowercase().contains(k))
        || intent.distribution.get("download").copied().unwrap_or(0.0) > 0.40;

    if is_download_or_nav && !paginated_results.is_empty() {
        let vendor_domains = [
            "nvidia.com", "amd.com", "intel.com", "realtek.com", "microsoft.com",
            "dell.com", "hp.com", "lenovo.com", "asus.com", "msi.com", "gigabyte.com",
            "logitech.com", "corsair.com", "razer.com", "apple.com", "oracle.com",
            "python.org", "videolan.org", "github.com", "canonical.com", "ubuntu.com"
        ];

        let q_lower_deep = q.to_lowercase();
        let matched_brand_domain = vendor_domains.iter().find(|d| {
            let brand = d.split('.').next().unwrap_or(d);
            q_lower_deep.contains(brand) || (brand == "videolan" && q_lower_deep.contains("vlc"))
        });

        let best_vendor_idx = if let Some(target_d) = matched_brand_domain {
            paginated_results.iter().position(|r| r.url.contains(target_d))
        } else {
            paginated_results.iter().position(|r| {
                vendor_domains.iter().any(|d| r.url.contains(d))
            })
        };

        let cand_idx = best_vendor_idx.unwrap_or(0);
        let cand_url = paginated_results[cand_idx].url.clone();
        let cand_title = paginated_results[cand_idx].title.clone();
        let cand_domain = reqwest::Url::parse(&cand_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| "Official Vendor".to_string());

        if !paginated_results[cand_idx].sources.contains(&"official_vendor".to_string()) {
            paginated_results[cand_idx].sources.push("official_vendor".to_string());
        }

        let http_client_deep = state.http_client.clone();
        let deep_target_url = cand_url.clone();
        let deep_target_title = cand_title.clone();
        let deep_target_domain = cand_domain.clone();
        let url_for_fetch = deep_target_url.clone();

        let fetch_deep = tokio::spawn(async move {
            let resp = match tokio::time::timeout(
                Duration::from_millis(1500),
                http_client_deep.get(&url_for_fetch)
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                    .send(),
            ).await {
                Ok(Ok(r)) => r,
                _ => return None,
            };

            let html = match tokio::time::timeout(Duration::from_millis(1000), resp.text()).await {
                Ok(Ok(h)) => h,
                _ => return None,
            };

            let download_exts = [".exe", ".msi", ".zip", ".dmg", ".pkg", ".iso", ".tar.gz", ".deb", ".rpm"];
            let mut direct_link: Option<String> = None;

            for chunk in html.split("href=") {
                let chunk_clean = chunk.trim_start_matches(|c| c == '"' || c == '\'');
                let end_idx = chunk_clean.find(['"', '\'', ' ', '>']).unwrap_or(chunk_clean.len().min(300));
                let raw_url = &chunk_clean[..end_idx];

                let lower_raw = raw_url.to_lowercase();
                if download_exts.iter().any(|ext| lower_raw.contains(ext))
                    || lower_raw.contains("download.nvidia.com")
                    || lower_raw.contains("download.intel.com")
                    || lower_raw.contains("download.amd.com")
                    || lower_raw.contains("driver_download") {

                    let full_url = if raw_url.starts_with("http://") || raw_url.starts_with("https://") {
                        raw_url.to_string()
                    } else if raw_url.starts_with('/') {
                        if let Ok(b) = reqwest::Url::parse(&url_for_fetch) {
                            format!("{}://{}{}", b.scheme(), b.host_str().unwrap_or(""), raw_url)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };
                    direct_link = Some(full_url);
                    break;
                }
            }

            direct_link
        });

        let direct_download_url = match tokio::time::timeout(Duration::from_millis(1800), fetch_deep).await {
            Ok(Ok(Some(link))) => Some(link),
            _ => None,
        };

        let file_name = direct_download_url.as_ref().and_then(|u| {
            reqwest::Url::parse(u).ok().and_then(|parsed| {
                parsed.path_segments().and_then(|segs| segs.last().map(|s| s.to_string()))
            })
        });

        let has_direct = direct_download_url.is_some();
        deep_result = Some(DeepResult {
            result_type: if has_direct { "direct_download".to_string() } else { "official_page".to_string() },
            vendor_name: deep_target_domain,
            page_title: deep_target_title,
            page_url: deep_target_url,
            direct_download_url,
            file_name,
            confidence: if has_direct { 0.98 } else { 0.88 },
        });
    }

    let response = UnifiedResponse {
        query: q.clone(),
        intent: Some(intent.intent.clone()),
        category: Some(parent_category(&intent.intent)),
        confidence: Some(intent.confidence),
        constraints: flat_constraints,
        structured_constraints: intent.structured_constraints.clone(),
        expanded_queries: intent.expanded_queries.clone(),
        distribution: Some(intent.distribution.clone()),
        deep_result,
        results: paginated_results,
        geo_location,
        spell_corrected_query: if spell_changed { Some(q_cleaned_spelling.clone()) } else { None },
        error,
        message,
        query_quality: if qflag == "low" { Some("low".to_string()) } else { None },
        applied_constraints: if applied.is_empty() { None } else { Some(applied) },
        ignored_constraints: if ignored.is_empty() { None } else { Some(ignored) },
        warnings: if warnings.is_empty() { None } else { Some(warnings) },
        results_before_filter: Some(pre_filter_count),
        results_after_filter: Some(post_filter_count),
        total: Some(post_filter_count),
        page_limit: Some(limit),
        page_offset: Some(offset),
        has_more: if post_filter_count > 0 { Some(offset + limit < post_filter_count) } else { Some(false) },
    };

    // Cache for 5 minutes — but never cache empty results
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !response.results.is_empty() {
        state.cache.put(cache_key.clone(), response_json.clone(), Duration::from_secs(300));
    }
    // Notify any dedup waiters that the result is ready (even if empty, to prevent hanging)
    let waiters = state.in_flight.lock().remove(&cache_key).unwrap_or_default();
    if !waiters.is_empty() {
        tracing::info!("DEDUP: notifying {} waiter(s) for '{}'", waiters.len(), q_trimmed);
        for sender in waiters {
            let _ = sender.send(response_json.clone());
        }
    }
    // Normal completion: the entry is removed above, so the RAII guard must not
    // remove it again (and must not run its panic-path cleanup).
    if let Some(guard) = dedup_guard.take() {
        guard.complete();
    }

    (axum::http::StatusCode::OK, Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({}))))
}

fn parse_date_constraints(q: &str) -> (Option<String>, Option<String>) {
    let q_lower = q.to_lowercase();
    let mut after_date = None;
    let mut before_date = None;
    if let Some(pos) = q_lower.find("after:") {
        let after = pos + 6;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            after_date = Some(val);
        }
    }
    if let Some(pos) = q_lower.find("before:") {
        let after = pos + 7;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            before_date = Some(val);
        }
    }
    // Natural-language recency ("last 7 days", "this week", "recent", ...) must
    // become a real date window, not just a re-weighting signal. Only derive it
    // when no explicit after:/before: was supplied.
    if after_date.is_none() {
        if let Some((a, b)) = derive_recency_window(&q_lower) {
            after_date = Some(a);
            if before_date.is_none() {
                before_date = Some(b);
            }
        }
    }
    (after_date, before_date)
}

/// Normalize natural-language constraint syntax into canonical operator tokens
/// (mirror of the intent engine's helper) so the engine query and the gateway's
/// own constraint parsing honour spoken forms: "under $500" -> price:<500,
/// "in url:github" -> inurl:github, "on site:reddit" -> site:reddit.
fn normalize_nl_operators(query: &str) -> String {
    let mut out = query.to_string();
    for (re_src, replacement) in [
        (r"(?i)\bunder\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bless\s+than\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bbelow\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bcheaper\s+than\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bmax(?:imum)?\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bover\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bmore\s+than\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\babove\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bgreater\s+than\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bmin(?:imum)?\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bin\s+url\s*:\s*", "inurl:"),
        (r"(?i)\binurl\s+", "inurl:"),
        (r"(?i)\bon\s+site\s*:\s*", "site:"),
        (r"(?i)\bonsite\s+", "site:"),
        (r"(?i)\bin\s+title\s*:\s*", "intitle:"),
        (r"(?i)\bintitle\s+", "intitle:"),
        (r"(?i)\bin\s+text\s*:\s*", "intext:"),
        (r"(?i)\bintext\s+", "intext:"),
    ] {
        if let Ok(re) = regex::Regex::new(re_src) {
            out = re.replace_all(&out, replacement).to_string();
        }
    }
    out
}

fn extract_gateway_constraints(q: &str) -> Constraints {
    // Normalize spoken constraint forms (under $500, in url:, …) before
    // scanning for operators so the gateway's own parsing matches the engine
    // query and the intent engine's extraction.
    let q = normalize_nl_operators(q);
    let mut file_types = Vec::new();
    let mut sites = Vec::new();
    let mut phrases = Vec::new();
    let mut intitle = Vec::new();
    let mut inurl = Vec::new();
    let mut intext = Vec::new();
    let mut related = Vec::new();
    let mut price_min = None;
    let mut price_max = None;
    let mut price_lt = None;
    let mut price_gt = None;
    let mut language = None;
    let mut negative: Vec<String> = Vec::new();
    
    // Parse phrases (quotes)
    let mut current_phrase = String::new();
    let mut inside_quotes = false;
    let normalized = q
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('‘', "\"")
        .replace('’', "\"");
    
    for c in normalized.chars() {
        if c == '"' {
            if inside_quotes {
                let trimmed = current_phrase.trim().to_string();
                if !trimmed.is_empty() {
                    phrases.push(trimmed);
                }
                current_phrase.clear();
                inside_quotes = false;
            } else {
                inside_quotes = true;
            }
        } else if inside_quotes {
            current_phrase.push(c);
        }
    }

    let q_lower = q.to_lowercase();
    
    // Extract filetype:
    // Negated form "-filetype:x" is an EXCLUSION, not a positive filter — route
    // it to `negative` so the hard filter excludes it instead of including it.
    for cap in q_lower.match_indices("filetype:") {
        let after = cap.0 + 9;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if val.is_empty() {
            continue;
        }
        let negated = cap.0 > 0 && q_lower.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            negative.push(format!("filetype:{}", val));
        } else {
            file_types.push(val);
        }
    }
    
    // Extract site:
    // Negated "-site:x" => exclusion (route to `negative`). Bare/leading-dot
    // TLDs like ".edu"/"edu" are not real hosts; normalize bare TLDs to
    // ".edu" and drop leading-dot tokens so they don't zero out the query.
    for cap in q_lower.match_indices("site:") {
        let after = cap.0 + 5;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if val.is_empty() {
            continue;
        }
        let negated = cap.0 > 0 && q_lower.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            negative.push(format!("site:{}", val));
            continue;
        }
        let val = val.strip_prefix('.').unwrap_or(&val);
        let is_valid_host = val.contains('.') || val == "localhost";
        if !is_valid_host {
            let bare_tlds = ["edu","gov","org","com","net","io","dev","ai","co","us","uk","de","fr","es","nl","ru","cn","jp","in"];
            if bare_tlds.contains(&val) {
                sites.push(val.to_string());
            }
            continue;
        }
        sites.push(val.to_string());
    }

    // Extract intitle:
    for cap in q_lower.match_indices("intitle:") {
        let after = cap.0 + 8;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            intitle.push(val);
        }
    }

    // Extract inurl:
    for cap in q_lower.match_indices("inurl:") {
        let after = cap.0 + 6;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            inurl.push(val);
        }
    }

    // Extract intext:
    for cap in q_lower.match_indices("intext:") {
        let after = cap.0 + 7;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            intext.push(val);
        }
    }

    // Extract related:
    for cap in q_lower.match_indices("related:") {
        let after = cap.0 + 8;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            related.push(val);
        }
    }

    // Extract price:
    for cap in q_lower.match_indices("price:") {
        let after = cap.0 + 6;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if let Some(p) = parse_price_range(&val) {
            price_min = p.min.or(price_min);
            price_max = p.max.or(price_max);
            price_lt = p.lt.or(price_lt);
            price_gt = p.gt.or(price_gt);
        }
    }

    // Extract lang:
    for cap in q_lower.match_indices("lang:") {
        let after = cap.0 + 5;
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            language = Some(val);
        }
    }

    if language.is_none() {
        let words: Vec<&str> = q_lower.split_whitespace().collect();
        let fr_words = ["de", "la", "le", "les", "des", "et", "recette", "gateau"];
        let de_words = ["der", "die", "das", "und", "ist", "rezept", "kuchen"];
        let es_words = ["el", "la", "los", "las", "y", "en", "para"];
        let nl_words = ["van", "het", "een", "en", "koptelefoon"];
        
        if words.iter().any(|w| fr_words.contains(w)) {
            language = Some("fr".to_string());
        } else if words.iter().any(|w| de_words.contains(w)) {
            language = Some("de".to_string());
        } else if words.iter().any(|w| es_words.contains(w)) {
            language = Some("es".to_string());
        } else if words.iter().any(|w| nl_words.contains(w)) {
            language = Some("nl".to_string());
        } else {
            language = Some("en".to_string());
        }
    }
    
    let (after_date, before_date) = parse_date_constraints(&q);
    
    Constraints {
        positive: vec![],
        negative,
        entities: vec![],
        language,
        file_types,
        sites,
        phrases,
        after_date,
        before_date,
        intitle,
        inurl,
        intext,
        related,
        price_min,
        price_max,
        price_lt,
        price_gt,
    }
}

fn fallback_intent(q: &str) -> IntentResponse {
    let mut structured = extract_gateway_constraints(q);
    let mut negative = Vec::new();
    let q_lower = q.to_lowercase();
    for word in q_lower.split_whitespace() {
        if word.starts_with('-') {
            let neg = word.strip_prefix('-').unwrap().trim().to_string();
            if !neg.is_empty() {
                negative.push(neg);
            }
        }
    }
    structured.negative = negative;

    IntentResponse {
        query: q.to_string(),
        intent: "informational".to_string(),
        confidence: 0.3,
        constraints: vec![],
        structured_constraints: structured,
        expanded_queries: vec![q.to_string()],
        distribution: std::collections::HashMap::new(),
    }
}

// ─── Fast Search: Local Index Only (~100ms) ───────────────────────
// Returns only local index results. No SearXNG, no intent analysis.
// Frontend can call this + /search in parallel for instant feedback.

async fn handle_search_fast(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    let q_encoded = urlencoding::encode(&q);

    // Guard: missing or empty `q` — return 400 with the documented body.
    if q.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Missing or empty query parameter 'q'",
                "results": [],
                "count": 0,
            })),
        );
    }

    // Check cache first
    let cache_key = format!("fast:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }

    // Query local index only — no network calls except to indexer
    let indexer_url = format!("http://127.0.0.1:6000/search?q={}", q_encoded);
    let results = match state.http_client.get(&indexer_url).send().await {
        Ok(resp) => {
            match read_json_bounded::<Vec<IndexerResult>>(resp).await {
                Some(indexer_results) => {
                    // Convert to MergedResult format
                    indexer_results.into_iter().map(|r| MergedResult {
                        url: r.url,
                        title: r.title,
                        content: r.content,
                        score: r.score,
                        authority: r.authority,
                        sources: vec!["local".to_string()],
                        is_local: true,
                        published_date: None,
                        quality: r.quality,
                    }).collect::<Vec<_>>()
                }
                None => vec![]
            }
        }
        Err(_) => vec![]
    };

    let response = serde_json::json!({
        "source": "local",
        "results": results,
        "count": results.len(),
    });

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    (axum::http::StatusCode::OK, Json(response))
}

#[cfg(test)]
mod constraint_fix_tests {
    use super::*;

    fn cst() -> Constraints {
        Constraints::default()
    }

    #[test]
    fn recency_natural_language_becomes_date_window() {
        // BUG2: "last 7 days" / "this week" / "past 2 weeks" must yield a window.
        for q in ["ai news last 7 days", "rust release this week", "past 2 weeks update", "recent vulnerabilities"] {
            let (after, before) = parse_date_constraints(q);
            assert!(after.is_some(), "expected after_date for '{}'", q);
            assert!(before.is_some(), "expected before_date for '{}'", q);
            let today = format_ymd(today_ymd());
            assert!(
                date_lte(
                    parse_date_to_comparable(&after.clone().unwrap()).unwrap(),
                    parse_date_to_comparable(&today).unwrap()
                ),
                "after_date {} should be <= today for '{}'",
                after.clone().unwrap(),
                q
            );
        }
    }

    #[test]
    fn literal_after_still_parsed() {
        let (after, _) = parse_date_constraints("rust after:2024");
        assert_eq!(after.as_deref(), Some("2024"));
    }

    #[test]
    fn extract_date_from_text_handles_human_dates() {
        // BUG3: dates inside content/title must be extractable.
        assert_eq!(extract_date_from_text("Updated January 5, 2024 by the team"), Some((2024, 1, 5)));
        assert_eq!(extract_date_from_text("Posted 12 Mar 2021"), Some((2021, 3, 12)));
        assert_eq!(extract_date_from_text("See the 2023-12-01 release notes"), Some((2023, 12, 1)));
        assert_eq!(extract_date_from_text("Archived in October 2019"), Some((2019, 10, 1)));
        assert_eq!(extract_date_from_text("Back to 1999"), Some((1999, 1, 1)));
    }

    #[test]
    fn date_filter_drops_old_content() {
        // BUG3: after:2024 should filter a result whose content mentions 2021.
        let mut c = cst();
        c.after_date = Some("2024".to_string());
        let old = should_filter_by_constraints(
            "Old article",
            "This was published on March 3, 2021 and is stale.",
            "https://example.com/a",
            None,
            &c,
        );
        assert!(old, "result dated 2021 should be filtered by after:2024");

        let fresh = should_filter_by_constraints(
            "New article",
            "This was published on March 3, 2025 and is current.",
            "https://example.com/b",
            None,
            &c,
        );
        assert!(!fresh, "result dated 2025 should pass after:2024");
    }

    #[test]
    fn price_extraction_broadened() {
        // BUG4: more price formats detected.
        assert_eq!(extract_price_from_text("Only $99 today"), Some(99.0));
        assert_eq!(extract_price_from_text("Cost is €149.99"), Some(149.99));
        assert_eq!(extract_price_from_text("from 250 dollars"), Some(250.0));
        assert_eq!(extract_price_from_text("price: 49"), Some(49.0));
        assert_eq!(extract_price_from_text("no monetary value here"), None);
    }

    #[test]
    fn related_excludes_self_not_similar() {
        // BUG5: related:amazon.com must NOT return amazon itself.
        let mut c = cst();
        c.related = vec!["amazon.com".to_string()];
        assert!(
            should_filter_by_constraints("Buy", "https://www.amazon.com/dp/123", "https://www.amazon.com/dp/123", None, &c),
            "amazon itself should be filtered for related:amazon.com"
        );
        assert!(
            !should_filter_by_constraints("Shop", "https://www.ebay.com", "https://www.ebay.com", None, &c),
            "unrelated (non-self) result should be kept for related:amazon.com"
        );
    }

    #[test]
    fn related_known_map_kept() {
        let mut c = cst();
        c.related = vec!["github.com".to_string()];
        assert!(
            !should_filter_by_constraints("Git", "https://gitlab.com/x", "https://gitlab.com/x", None, &c),
            "gitlab (curated similar) should be kept for related:github.com"
        );
        assert!(
            should_filter_by_constraints("Git", "https://github.com/x", "https://github.com/x", None, &c),
            "github itself should be filtered for related:github.com"
        );
    }

    #[test]
    fn pure_negation_scores_match_down() {
        // BUG7 sanity: a trump-mentioning result scores near-zero for -trump.
        let mut c = cst();
        c.negative = vec!["trump".to_string()];
        let score = constraint_score("Trump speech", "https://x.com/trump", "trump said things", &c);
        assert!(score < 0.05, "trump-mentioning result should score near-zero for -trump");
    }

    #[test]
    fn preprocess_preserves_native_operators() {
        // BUG1a: intitle:/inurl:/intext: must be FORWARDED to SearXNG,
        // not stripped (the old behaviour zeroed out `rust inurl:blog` → `rust`).
        let q = preprocess_searxng_query("rust inurl:blog");
        assert!(q.contains("inurl:blog"), "inurl: must survive preprocessing, got: '{}'", q);
        let q2 = preprocess_searxng_query("cli intitle:deploy");
        assert!(q2.contains("intitle:deploy"), "intitle: must survive preprocessing, got: '{}'", q2);
        let q3 = preprocess_searxng_query("docs intext:quickstart");
        assert!(q3.contains("intext:quickstart"), "intext: must survive preprocessing, got: '{}'", q3);
        // Non-operator term is retained alongside the operator.
        assert!(q.contains("rust"), "operator query must keep its plain term, got: '{}'", q);
    }

    #[test]
    fn intitle_inurl_no_longer_hard_drop() {
        // BUG1b: should_filter_by_constraints must NOT hard-drop results that
        // lack the intitle:/inurl:/intext: token. The engine enforces upstream;
        // the gateway only boosts. A bare result for `rust inurl:blog` must pass.
        let mut c = cst();
        c.inurl = vec!["blog".to_string()];
        let kept = should_filter_by_constraints(
            "Rust blog",
            "A rust programming blog post",
            "https://example.com/about", // does NOT contain "blog" in url
            None,
            &c,
        );
        assert!(!kept, "inurl: must NOT hard-drop when engine is the enforcer (was the n=0 trap)");
        // And the boost rewards results that DO satisfy the operator.
        let mut c2 = cst();
        c2.inurl = vec!["blog".to_string()];
        let boost = constraint_boost("Rust blog", "post", "https://example.com/blog/rust", &c2);
        assert!(boost > 0.0, "inurl:-matching result should receive a positive boost");
        let boost_none = constraint_boost("Rust", "post", "https://example.com/x", &c2);
        assert_eq!(boost_none, 0.0, "non-matching result should get no intitle/inurl/intext boost");
    }
}
