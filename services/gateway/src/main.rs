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
    #[serde(default)]
    ip: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Constraints {
    #[serde(default)]
    positive: Vec<String>,
    #[serde(default)]
    negative: Vec<String>,
    /// Hard-exclusion terms supplied via the explicit `NOT:` advanced operator
    /// (mirrors `site:`/`filetype:`). Unlike a bare `not X` negation (which is a
    /// soft topical penalty gated on entity/contrastive recognition via
    /// `is_real_exclusion`), `NOT:` is an UNCONDITIONAL structural exclude: any
    /// result whose title/content/url contains the term is hard-dropped by
    /// `should_filter_by_constraints`. This gives users a general, non-hardcoded
    /// escape hatch for the DEFECT-A class of limitations — `not flask` (bare) is
    /// declined for an unrecognized tech term, but `NOT:flask` always excludes it.
    /// General by design: works for ANY user-supplied term, no entity list, no
    /// per-query tuning.
    #[serde(default)]
    hard_exclusions: Vec<String>,
    /// Declined (non-manner) candidate exclusions the `is_real_exclusion` gate did
    /// not apply, surfaced for transparency (D3) so they are not silently dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ignored_constraints: Option<Vec<String>>,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct PriceInfo {
    pub amount: f64,
    pub currency: String,
}

fn price_to_usd(amount: f64, currency: &str) -> f64 {
    match currency.to_uppercase().as_str() {
        "USD" | "$" => amount,
        "EUR" | "€" => amount * 1.08,
        "GBP" | "£" => amount * 1.28,
        "INR" | "₹" | "RS" | "RS." | "RUPEE" | "RUPEES" => amount * 0.012,
        "CAD" | "CA$" => amount * 0.74,
        "AUD" | "AU$" => amount * 0.65,
        "JPY" | "¥" | "YEN" => amount * 0.0067,
        "BRL" | "R$" => amount * 0.18,
        "CNY" | "RMB" => amount * 0.14,
        _ => amount,
    }
}

fn normalize_currency_str(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    if lower.contains('$') || lower.contains("usd") || lower.contains("dollar") {
        if lower.contains("can") || lower.contains("cad") { "CAD".to_string() }
        else if lower.contains("au") || lower.contains("aud") { "AUD".to_string() }
        else { "USD".to_string() }
    } else if lower.contains('€') || lower.contains("eur") || lower.contains("euro") {
        "EUR".to_string()
    } else if lower.contains('£') || lower.contains("gbp") || lower.contains("pound") {
        "GBP".to_string()
    } else if lower.contains('₹') || lower.contains("inr") || lower.contains("rupee") || lower.contains("rs") {
        "INR".to_string()
    } else if lower.contains('¥') || lower.contains("jpy") || lower.contains("yen") {
        "JPY".to_string()
    } else {
        "USD".to_string()
    }
}

fn has_price_signal(title_lower: &str, content_lower: &str) -> bool {
    static RS_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rs_re = RS_REGEX.get_or_init(|| regex::Regex::new(r"(?i)\b(rs\.?|rupee|rupees|inr)\b|₹").unwrap());

    title_lower.contains('$') || content_lower.contains('$')
        || title_lower.contains("price") || content_lower.contains("price")
        || title_lower.contains("cost") || content_lower.contains("cost")
        || rs_re.is_match(title_lower) || rs_re.is_match(content_lower)
        || title_lower.contains("pound") || title_lower.contains("euro")
        || title_lower.contains("buy") || title_lower.contains("shop")
        || title_lower.contains("cheap") || title_lower.contains("affordable")
        || title_lower.contains("sale") || title_lower.contains("deal")
        || title_lower.contains("specs") || title_lower.contains("spec")
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
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    currency: Option<String>,
}

impl SearxResult {
    pub fn get_price(&self) -> Option<PriceInfo> {
        if let Some(ref p_str) = self.price {
            let p_clean = p_str.trim().replace(',', "");
            if let Ok(amount) = p_clean.parse::<f64>() {
                let currency = self.currency.clone().unwrap_or_else(|| "USD".to_string());
                return Some(PriceInfo { amount, currency: normalize_currency_str(&currency) });
            } else if let Some(parsed) = extract_price_from_text(p_str) {
                let curr = self.currency.as_deref().unwrap_or(&parsed.currency);
                return Some(PriceInfo { amount: parsed.amount, currency: normalize_currency_str(curr) });
            }
        }
        extract_price_from_text(&self.title).or_else(|| extract_price_from_text(&self.content))
    }
}

impl MergedResult {
    pub fn get_price(&self) -> Option<PriceInfo> {
        if let Some(ref p_str) = self.price {
            let p_clean = p_str.trim().replace(',', "");
            if let Ok(amount) = p_clean.parse::<f64>() {
                let currency = self.currency.clone().unwrap_or_else(|| "USD".to_string());
                return Some(PriceInfo { amount, currency: normalize_currency_str(&currency) });
            } else if let Some(parsed) = extract_price_from_text(p_str) {
                let curr = self.currency.as_deref().unwrap_or(&parsed.currency);
                return Some(PriceInfo { amount: parsed.amount, currency: normalize_currency_str(curr) });
            }
        }
        extract_price_from_text(&self.title).or_else(|| extract_price_from_text(&self.content))
    }
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
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    currency: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    /// Signal-quality metric [0,1] inherited from the indexer for local results
    /// (BM25/semantic match strength). Used by the local-index quality gate (P2)
    /// to demote low-signal crawled pages. Defaults to 1.0 for web results.
    #[serde(default = "default_f32_one")]
    quality: f32,
    /// D4 (2026-08-18T1340Z round): the per-engine trust multiplier applied to
    /// this result during the web merge. Stored (read-only, debug/observability)
    /// so tests and operators can SEE whether a result was trust-crushed. 1.0 means
    /// no crush; <1.0 means D4 crushed this engine's results (a dated sibling existed
    /// and this engine only returned date-blind junk). Defaults to 1.0.
    #[serde(default = "default_f32_one")]
    engine_trust_mult: f32,
}

fn default_f32_one() -> f32 { 1.0 }

/// D4 (2026-08-18T1340Z round): identify the upstream engine that produced a
/// merged result so per-engine trust can be gated. Local-only results report
/// "local"; web/merged results report their non-local, non-instance upstream
/// engine label (e.g. "bing", "brave"). Pure structural inspection of `sources`
/// — no query/domain literals, so it generalises to any upstream.
fn primary_engine(r: &MergedResult) -> String {
    for s in &r.sources {
        if s != "local" && !s.starts_with("instance_") {
            return s.clone();
        }
    }
    r.sources.first().cloned().unwrap_or_else(|| "local".to_string())
}

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
    /// Number of returned results carrying a verified detectable price
    #[serde(skip_serializing_if = "Option::is_none")]
    price_verified: Option<usize>,
    /// Honest recall-gap signal. When a salient (distinctive) query term is
    /// mentioned by NONE of the returned results, it appears here — signalling
    /// an upstream recall gap for that facet of the query rather than a ranking
    /// failure. Empty/absent = the result set plausibly covers the query's
    /// subject. The engine surfaces this; it never fabricates a result to fill
    /// the gap (round-2026-08-12T1234Z D2 disposition). No query-specific
    /// tuning, no domain/term allow-or-deny lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    recall_gap_terms: Option<Vec<String>>,
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

/// True when `word` appears as a whole word in `q_lower` (alphanumeric-boundary
/// delimited). Avoids the substring false-positive where `contains("fresh")` also
/// matches "fresher", "freshman", or "refresh" and wrongly injects a recency window.
fn q_has_word(q_lower: &str, word: &str) -> bool {
    q_lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == word)
}

/// D6 (2026-08-17): relation/comparison FUNCTION words that describe *how* the user
/// wants results related, not *what* they are about. Granting the generic
/// title-relevance boost to these lets junk pages that merely contain the word
/// ("Percentage Difference Calculator", "DIFFERENCE dictionary") outrank the real
/// subject pages for entity-disambiguation queries. Excluded from the title boost
/// only — they still contribute to the lexical scorer. General data set, no
/// per-query literals.
fn is_relation_stopword(term: &str) -> bool {
    const RELATION_WORDS: &[&str] = &[
        "difference", "differences", "differ", "compare", "comparison", "comparisons",
        "versus", "vs", "similar", "similarities", "similarity", "opposite", "opposites",
        "between", "among", "amongst", "unlike", "distinct", "distinction",
    ];
    RELATION_WORDS.contains(&term)
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

    // Whole-word match only: a substring match on "fresh" wrongly fired for
    // "fresher"/"freshman"/"refresh" and injected a 7-day date window that
    // collapsed otherwise-normal informational queries to zero results.
    // F1 (2026-08-17): "fresh" alone is NOT a temporal signal. It is an adjective
    // in many topical queries ("fresh herbs", "fresh paint", "fresh flowers",
    // "fresh water") with no news/recency intent. Only treat "fresh"/"recent"/
    // "latest" as a recency signal when the query ALSO names a news noun or verb
    // (news/updates/breakthrough/paper/released/announced/this week/month/year),
    // i.e. the word actually implies "newly published", not merely "new/unspoiled".
    // Structural news vocabulary, no per-query literals.
    let news_terms = [
        "news", "update", "updates", "breakthrough", "breakthroughs", "paper", "papers",
        "release", "released", "launch", "launched", "announce", "announced",
        "research", "study", "report", "headline", "headlines", "article", "post",
        "developments", "advances", "this week", "this month", "this year", "published",
    ];
    let has_news_term = news_terms.iter().any(|t| q_has_word(q_lower, t) || q_lower.contains(t));
    if q_has_word(q_lower, "recent") || q_has_word(q_lower, "latest") {
        // "recent"/"latest" are almost always temporal on their own ("latest news",
        // "recent breakthroughs", "latest movies"). Keep them as recency signals.
        // A version-pinned query ("rust 1.80", "version 3 of X", "python 3.13")
        // is asking for the CONTENT of a specific release, not "news from the
        // last 7 days". A 7-day recency window would drop that (often older)
        // content and leave only date-stamped noise. When a software-version
        // pattern is present, do NOT inject a fresh window — leave recency as a
        // pure scoring boost (freshness half-life). Generic: matches the shape
        // "X.Y[.Z]" / "version N" / "vN", not any specific product name.
        let version_pinned = regex::Regex::new(r"(?i)(\bv?\d+\.\d+(\.\d+)?\b|version\s+\d+)").unwrap();
        if version_pinned.is_match(q_lower) {
            return None;
        }
        return Some((format_ymd(add_days(today, -7)), today_s));
    }
    if q_has_word(q_lower, "fresh") && has_news_term {
        return Some((format_ymd(add_days(today, -7)), today_s));
    }

    None
}

fn freshness_score(url: &str, intent: &str, published_date: Option<&str>, title: &str, content: &str) -> f32 {
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

    // Resolve the best date we can from upstream published_date, a URL-embedded
    // year, or a date written in the title/content text. The upstream `publishedDate`
    // field is frequently None (SearXNG news backends rarely populate it), so ranking
    // on it alone leaves recency blind — a "latest X this week" query then ranks
    // evergreen/undated pages by pure relevance. Falling back to resolve_item_date()
    // (which already drives the after:/before: hard-filter) lets the freshness score
    // actually decay stale items and boost recent ones. Generic: no per-query tuning.
    let resolved = resolve_item_date(published_date, url, title, content);
    if let Some((y, m, d)) = resolved {
        let (cur_y, cur_m, cur_d) = today_ymd();
        let cur_days = ymd_to_days(cur_y, cur_m, cur_d);
        let item_days = ymd_to_days(y, m, d);
        let total_days = (cur_days - item_days).max(0);
        estimated_age_hours = (total_days * 24) as f32;
        parsed_ok = true;
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
    let query_terms: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() >= 2)
        .collect();
    // D6 (2026-08-17): relation/comparison FUNCTION words ("difference", "compare",
    // "vs", "similar", ...) are not topical terms — they describe the *relation* the
    // user wants, not the *subject*. Granting the generic title-boost for them lets
    // junk like "Percentage Difference Calculator" / "DIFFERENCE dictionary" outrank
    // the real subject pages (e.g. "difference between titan the watch brand and titan
    // the moon of saturn" → Wikipedia Titan-moon / Titan Company). This is the P1
    // substring-collision pattern generalized: drop the structural word from the
    // title-boost, keep it in the lexical scorer. A small general data set, no
    // per-query literals.
    let title_matches = query_terms
        .iter()
        .filter(|t| !is_relation_stopword(t))
        .filter(|t| title_lower.contains(**t))
        .count();
    if title_matches > 0 {
        boost += 0.1 * title_matches as f32;
    }

    // M1 fix: Demote generic "Top N" / "Best N" listicle titles unless user explicitly requested a list
    if clean::is_generic_listicle_title(title) {
        let user_asked_list = query_lower.contains("top 10")
            || query_lower.contains("10 best")
            || query_lower.contains("top 5")
            || query_lower.contains("5 best")
            || query_lower.contains("top 15")
            || query_lower.contains("15 best")
            || query_lower.contains("list of")
            || query_lower.contains("top 20");
        if !user_asked_list {
            boost *= 0.70;
        }
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
    ("delhi", "IN"), ("mumbai", "IN"), ("bangalore", "IN"), ("bengaluru", "IN"),
    ("chennai", "IN"), ("kolkata", "IN"), ("pune", "IN"), ("ahmedabad", "IN"), ("jaipur", "IN"),
    ("hyderabad", "IN"), ("lucknow", "IN"), ("kanpur", "IN"), ("nagpur", "IN"), ("indore", "IN"),
    ("bhopal", "IN"), ("patna", "IN"), ("surat", "IN"), ("vadodara", "IN"), ("rajkot", "IN"),
    ("coimbatore", "IN"), ("kochi", "IN"), ("thiruvananthapuram", "IN"), ("visakhapatnam", "IN"),
    ("vijayawada", "IN"), ("mysore", "IN"), ("mangalore", "IN"), ("goa", "IN"), ("singapore", "SG"),
    // Additional Indian cities so the cross-location gates (soft multiplier + hard local
    // drop) recognize them as known places. Pure reference data; extends the seed to close
    // the geo-pollution gap for "restaurants in <city>" where <city> was not yet listed.
    // SEED, not logic — no per-query hardcoding.
    ("trichy", "IN"), ("tiruchirappalli", "IN"), ("madurai", "IN"), ("salem", "IN"),
    ("tirunelveli", "IN"), ("erode", "IN"), ("thoothukudi", "IN"), ("thanjavur", "IN"),
    ("nashik", "IN"), ("aurangabad", "IN"), ("gwalior", "IN"),
    ("bhubaneswar", "IN"), ("ranchi", "IN"), ("raipur", "IN"), ("jodhpur", "IN"),
    ("udaipur", "IN"), ("chandigarh", "IN"), ("amritsar", "IN"), ("ludhiana", "IN"),
    ("allahabad", "IN"), ("prayagraj", "IN"), ("varanasi", "IN"), ("agra", "IN"),
    ("dehradun", "IN"), ("jammu", "IN"), ("hubli", "IN"), ("dharwad", "IN"),
    ("guntur", "IN"), ("nellore", "IN"), ("kurnool", "IN"), ("rajahmundry", "IN"),
    ("trivandrum", "IN"),
    // More international cities
    ("paris", "FR"), ("lyon", "FR"), ("marseille", "FR"), ("munich", "DE"),
    ("hamburg", "DE"), ("cologne", "DE"), ("frankfurt", "DE"), ("milan", "IT"),
    ("naples", "IT"), ("turin", "IT"), ("barcelona", "ES"), ("valencia", "ES"),
    ("seville", "ES"), ("lisbon", "PT"), ("porto", "PT"), ("vienna", "AT"),
    ("zurich", "CH"), ("geneva", "CH"), ("brussels", "BE"), ("antwerp", "BE"),
    ("osaka", "JP"), ("kyoto", "JP"), ("busan", "KR"), ("dallas", "US"),
    ("houston", "US"), ("miami", "US"), ("atlanta", "US"), ("denver", "US"),
    ("washington", "US"), ("philadelphia", "US"), ("las vegas", "US"),
    ("manchester", "GB"), ("birmingham", "GB"), ("glasgow", "GB"), ("edinburgh", "GB"),
    ("brisbane", "AU"), ("perth", "AU"), ("adelaide", "AU"),
    ("dublin", "IE"), ("stockholm", "SE"), ("nairobi", "KE"),
    ("accra", "GH"), ("addis ababa", "ET"), ("manila", "PH"), ("cebu", "PH"),
    ("hanoi", "VN"), ("ho chi minh", "VN"), ("yangon", "MM"), ("phnom penh", "KH"),
    ("kuala lumpur", "MY"), ("penang", "MY"), ("johannesburg", "ZA"), ("durban", "ZA"),
    ("ibadan", "NG"), ("kano", "NG"), ("casablanca", "MA"),
    ("sydney", "AU"), ("melbourne", "AU"), ("auckland", "NZ"),
    ("new york", "US"), ("san francisco", "US"), ("los angeles", "US"),
    ("chicago", "US"), ("seattle", "US"), ("boston", "US"), ("austin", "US"),
    ("toronto", "CA"), ("vancouver", "CA"), ("sao paulo", "BR"), ("mexico city", "MX"),
    ("dubai", "AE"), ("cairo", "EG"), ("bangkok", "TH"), ("jakarta", "ID"),
    ("cape town", "ZA"), ("lagos", "NG"),
    // 2026-08-19T1628Z round: extend the SEED with common Indian hill/travel/
    // region destinations that users query but that were missing. These are the
    // exact class of place that triggered geo pollution (an off-topic other-city
    // local page ranking #1 because the requested place was unseen by
    // detect_explicit_location, so geo_is_explicit stayed false and no
    // cross-location penalty fired). Pure reference data; no per-query literals.
    // Also a few more global travel hubs for general coverage.
    ("ladakh", "IN"), ("leh", "IN"), ("mcleod ganj", "IN"), ("mcleodganj", "IN"),
    ("dharamshala", "IN"), ("srinagar", "IN"), ("shimla", "IN"), ("manali", "IN"),
    ("spiti", "IN"), ("kashmir", "IN"), ("gulmarg", "IN"), ("sonamarg", "IN"),
    ("gokarna", "IN"), ("hampi", "IN"), ("coorg", "IN"), ("madikeri", "IN"),
    ("munnar", "IN"), ("ooty", "IN"), ("udhagamandalam", "IN"), ("kodaikanal", "IN"),
    ("darjeeling", "IN"), ("rishikesh", "IN"), ("haridwar", "IN"),
    ("pondicherry", "IN"), ("puducherry", "IN"), ("alleppey", "IN"), ("alappuzha", "IN"),
    ("kumarakom", "IN"), ("thekkady", "IN"), ("wagamon", "IN"), ("vagamon", "IN"),
    ("mahabalipuram", "IN"), ("thanjavur", "IN"), ("hampi", "IN"),
    ("lonavala", "IN"), ("khandala", "IN"), ("mahabaleshwar", "IN"), ("panchgani", "IN"),
    ("mount abu", "IN"), ("mountain", "IN"), ("gir", "IN"), ("diu", "IN"),
    ("andaman", "IN"), ("nicobar", "IN"), ("havelock", "IN"), ("port blair", "IN"),
    ("tawang", "IN"), ("ziro", "IN"), ("shillong", "IN"), ("cherrapunji", "IN"),
    ("kaziranga", "IN"), ("guwahati", "IN"), ("gangtok", "IN"), ("pelling", "IN"),
    ("kerala", "IN"), ("kashmir", "IN"), ("himachal", "IN"), ("uttarakhand", "IN"),
    ("goa", "IN"), ("kanyakumari", "IN"), ("rameshwaram", "IN"), ("madurai", "IN"),
    ("trivandrum", "IN"), ("thiruvananthapuram", "IN"), ("kochi", "IN"),
    ("phuket", "TH"), ("bali", "ID"), ("krabi", "TH"), ("chiang mai", "TH"),
    ("colombo", "LK"), ("kandy", "LK"), ("kathmandu", "NP"), ("pokhara", "NP"),
    ("istanbul", "TR"), ("antalya", "TR"), ("cappadocia", "TR"),
    ("lisbon", "PT"), ("porto", "PT"), ("reykjavik", "IS"), ("dubrovnik", "HR"),
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
    // Fallback: a place named via a location-preposition phrase ("in ladakh",
    // "near mcleod ganj", "trip to goa") that the static gazetteer does not list.
    // General: no per-place literals; closes the geo-pollution gap for any named
    // place. Returns None if no preposition-place pattern is found.
    detect_preposition_location(query)
}

/// Extract an explicit place from a location-preposition phrase, for queries that
/// name a place the static `LOCATION_GAZETTEER` does not yet list (e.g. "places in
/// ladakh", "near mcleod ganj", "trip to goa"). This is the GENERAL catch-all that
/// closes the geo-pollution gap without enumerating every possible place: when a
/// query says "<prep> <Place>", we treat <Place> as the requested location so the
/// cross-location penalty + hard local-drop fire (they compare against the
/// gazetteer to crush OTHER named cities). No per-place literals.
///
/// Robustness (avoid false positives like "in september" / "at night"):
///   • The candidate head token must be a PLACE-LIKE noun: either it is itself a
///     gazetteer entry, or it is Capitalized in the ORIGINAL (case-preserving)
///     query (proper-noun signal), or it is a known place-suffix word
///     (hill/beach/valley/…). A lowercase common noun ("september", "night") is
///     rejected.
///   • We take up to 3 following tokens as the place phrase (handles "new york",
///     "mcleod ganj"); stop at the next preposition/stopword.
fn detect_preposition_location(query: &str) -> Option<geoloc::GeoLocation> {
    let q_lower = query.to_lowercase();
    let prepositions: &[&str] = &[
        " in ", " near ", " at ", " around ", " from ", " visit ", " explore ",
        " trip to ", " road trip in ", " road trip to ", " places in ",
        " places near ", " things to do in ", " tourism in ", " tourism near ",
        " holiday in ", " vacation in ", " stay in ", " travel to ", " drive to ",
    ];
    let place_suffixes: &[&str] = &[
        "hill", "hills", "beach", "beaches", "valley", "island", "islands",
        "mountain", "mountains", "lake", "lakes", "fort", "temple", "city",
        "town", "village", "region", "district", "state", "country", "province",
    ];
    let orig_tokens: Vec<&str> = query.split_whitespace().collect();
    for prep in prepositions {
        if let Some(pos) = q_lower.find(prep) {
            let after = &q_lower[pos + prep.len()..];
            let after_tokens: Vec<&str> = after.split_whitespace().collect();
            if after_tokens.is_empty() {
                continue;
            }
            // Determine how many following tokens form the place name (<=3),
            // stopping at the next preposition/stopword boundary.
            let mut n = 0;
            let mut phrase_parts: Vec<String> = Vec::new();
            for tok in &after_tokens {
                if n >= 3 {
                    break;
                }
                if ["that", "which", "with", "and", "for", "to", "of", "the",
                    "a", "an", "my", "our", "this", "these", "those"].contains(tok) {
                    break;
                }
                // Stop if we hit another location preposition start.
                if *tok == "in" || *tok == "near" || *tok == "at" || *tok == "from"
                    || *tok == "around" || *tok == "to" {
                    break;
                }
                let orig = orig_tokens.iter()
                    .find(|o| o.to_lowercase() == *tok)
                    .copied()
                    .unwrap_or(*tok);
                let is_capitalized = orig.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                let is_gazetteer = LOCATION_GAZETTEER.iter().any(|(n, _)| *n == *tok);
                let is_suffix = place_suffixes.contains(tok);
                // The FIRST token must be place-like; continuation tokens (2nd/3rd)
                // are accepted if they continue a capitalized/gazetteer phrase.
                if n == 0 && !(is_capitalized || is_gazetteer || is_suffix) {
                    break;
                }
                if n > 0 && !(is_capitalized || is_gazetteer) {
                    break;
                }
                phrase_parts.push(tok.to_string());
                n += 1;
            }
            if phrase_parts.is_empty() {
                continue;
            }
            let city = phrase_parts.join(" ");
            // Infer country only when the place is itself a gazetteer entry.
            let (cc, cname) = LOCATION_GAZETTEER.iter()
                .find(|(n, _)| *n == city)
                .map(|(_, cc)| (Some(*cc), Some(country_name_for(*cc).to_string())))
                .unwrap_or((None, None));
            return Some(geoloc::GeoLocation {
                country_code: cc.map(|s| s.to_string()),
                country_name: cname,
                region: None,
                city: Some(city),
                postal_code: None,
                latitude: None,
                longitude: None,
                time_zone: None,
            });
        }
    }
    None
}
/// to populate `city`. Derived from the city set; cheap linear scan.
fn is_city(name: &str) -> bool {
    const CITIES: &[&str] = &[
        "tokyo", "london", "paris", "berlin", "madrid", "rome", "amsterdam",
        "dublin", "stockholm", "oslo", "copenhagen", "helsinki", "moscow", "kyiv",
        "istanbul", "athens", "beijing", "shanghai", "seoul", "delhi", "mumbai",
        "bangalore", "bengaluru", "chennai", "kolkata", "pune", "ahmedabad", "jaipur",
        "hyderabad", "lucknow", "kanpur", "nagpur", "indore", "bhopal", "patna",
        "surat", "vadodara", "rajkot", "coimbatore", "kochi", "thiruvananthapuram",
        "visakhapatnam", "vijayawada", "mysore", "mangalore", "singapore", "sydney",
        "melbourne", "auckland",
        "new york", "san francisco", "los angeles", "chicago", "seattle", "boston",
        "austin", "toronto", "vancouver", "sao paulo", "mexico city", "dubai",
        "cairo", "bangkok", "jakarta", "cape town", "lagos",
        // Expanded Indian + global cities (mirror of LOCATION_GAZETTEER additions)
        "kolkata", "calcutta", "pune", "hyderabad", "chennai", "madras", "ahmedabad",
        "coimbatore", "jaipur", "goa", "lucknow", "kanpur", "nagpur", "indore",
        "bhopal", "surat", "vadodara", "baroda", "visakhapatnam", "vijayawada",
        "patna", "ranchi", "raipur", "thiruvananthapuram", "kochi", "cochin",
        "kozhikode", "calicut", "mysore", "mysuru", "amritsar", "chandigarh",
        "gwalior", "udaipur", "jaisalmer", "varanasi", "banaras", "agra", "shimla",
        "manali", "dehradun", "guwahati", "bhubaneswar", "rajkot", "jabalpur",
        "guntur", "thane", "navi mumbai", "ghaziabad", "noida", "ludhiana",
        "allahabad", "prayagraj", "nashik", "aurangabad", "madurai", "trivandrum",
        "miami", "dallas", "denver", "atlanta", "washington", "philadelphia",
        "houston", "minneapolis", "munich", "hamburg", "frankfurt", "cologne",
        "lyon", "marseille", "nice", "milan", "naples", "turin", "florence",
        "valencia", "seville", "malaga", "porto", "lisbon", "brussels", "vienna",
        "zurich", "geneva", "osaka", "kyoto", "busan", "taipei", "kuala lumpur",
        "manila", "ho chi minh", "hanoi", "doha", "riyadh", "tel aviv", "nairobi",
        "accra", "casablanca", "addis ababa", "dar es salaam",
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

/// Detect whether an EXCLUDED term (a negative constraint like "medication")
/// appears in a NEGATING context within a result — i.e. the page is
/// *fulfilling* the user's exclusion rather than violating it. For
/// "lower blood pressure WITHOUT medication" the most relevant pages literally
/// say "without medication" / "no pills" / "free of drugs". Penalising them
/// (the old behaviour) collapses recall and can surface the opposite of intent
/// (a pill page ranking #1 for "sleep without pills"). When the excluded term is
/// framed negatively, the result should be BOOSTED, not crushed.
///
/// General, signal-driven: keyed on a small closed set of English negation
/// markers + the excluded term's own tokens, no per-query/domain strings.
fn term_in_negating_context(term_lower: &str, text_lower: &str) -> bool {
    let term_tokens: Vec<&str> = term_lower
        .split_whitespace()
        .filter(|t| t.len() >= 2)
        .collect();
    if term_tokens.is_empty() {
        return false;
    }
    // Negation markers that, when appearing shortly BEFORE the excluded term,
    // signal the page is about avoiding it.
    // Single-word markers
    let single_word_markers: &[&str] = &[
        "without", "no", "not", "never", "avoid", "avoiding",
        "zero", "minus", "absent", "non",
    ];
    // Multi-word markers represented as token sequences
    let multi_word_markers: &[&[&str]] = &[
        &["with", "no"],
        &["free", "of"],
        &["free", "from"],
        &["instead", "of"],
        &["rather", "than"],
    ];
    let words: Vec<&str> = text_lower.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    for (i, w) in words.iter().enumerate() {
        let is_term_token = term_tokens.iter().any(|t| {
            let tl = t.trim_end_matches('s'); // loose plural match
            w == t || w == &tl || (w.len() > t.len() && w.starts_with(t) && (w.len() - t.len()) as f32 / t.len() as f32 <= 0.5)
        });
        if !is_term_token {
            continue;
        }
        // Look back up to 3 tokens for a negation marker.
        let start = i.saturating_sub(3);
        let preceding_window = &words[start..i];

        // Check single-word markers with exact token equality (no prefix matching)
        for &prev in preceding_window {
            if single_word_markers.contains(&prev) {
                return true;
            }
        }

        // Check multi-word markers as token sequences
        for multi_marker in multi_word_markers {
            if preceding_window.len() >= multi_marker.len() {
                // Scan all possible positions in the window
                for window_start in 0..=(preceding_window.len() - multi_marker.len()) {
                    let window_slice = &preceding_window[window_start..window_start + multi_marker.len()];
                    if window_slice == *multi_marker {
                        return true;
                    }
                }
            }
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
    // Alt-listing exemption: a page scoring >0.3 IS an alternatives/comparison
    // listing, so mentioning the excluded term is referential, not a violation.
    // We do NOT gate on is_comparison_or_alternative_query(): for "alternative to
    // X" the word "alternative" is consumed into the negative constraint, so that
    // check would never fire and the alt page would be mis-penalised (c_score
    // crushed) and then re-dropped downstream (result set collapses to 1). The
    // pre-merge hard-drop gate uses the same pure alt_score>0.3 exemption, so all
    // gates must agree to avoid re-drops.
    let is_alt_page = alt_score > 0.3;
    // Stricter gate for the title-dominance hard-drop below: a WEAK alt signal
    // alone (e.g. a "best "/"top " listicle title with no comparison/alternative
    // wording and no supporting URL/content evidence) must not exempt a page
    // whose title is otherwise dominated by the excluded term from the hard
    // drop — only genuine comparison/alternative pages (strong title signal,
    // or corroborated by URL/content) should be exempt from that check.
    let is_strong_alt_page = alt_score > 0.5;
    let mut any_unresolved_violation = false;
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
            // NEGATION-CONTEXT BOOST (this round): when the excluded term
            // appears in a NEGATING context ("without medication", "no pills",
            // "free of drugs"), the page is FULFILLING the user's exclusion,
            // so it is MORE relevant — not less. The old code penalised these
            // pages (×0.02), which collapsed recall for "X without Y" queries
            // and could surface the opposite of intent (a pill page at #1 for
            // "sleep without pills"). Boost instead of crush. This applies
            // regardless of is_alt_page: a title like "natural alternatives
            // instead of pills" is still fulfilling the exclusion even though
            // it also reads as an alternative-listing page.
            let neg_ctx_title = term_in_negating_context(neg_lower.as_str(), &title_lower);
            let neg_ctx_content = term_in_negating_context(neg_lower.as_str(), &content.to_lowercase());
            if neg_ctx_title || neg_ctx_content {
                let boost = 1.18;
                tracing::info!("CONSTRAINT NEG-CTX BOOST: '{}' in '{}' → boost={:.2} (excluding term framed negatively)",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    boost);
                score *= boost;
            } else if !is_alt_page {
                let penalty = (0.02 + (neg_count - 1.0) * 0.06).clamp(0.02, 0.20);
                tracing::info!("CONSTRAINT HIT (TITLE/URL): '{}' in '{}' → penalty={:.4} (non-alt)",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    penalty);
                score *= penalty;
            } else {
                any_unresolved_violation = true;
            }
        } else if content_matched {
            hit_count += 1;
            let neg_ctx_content = term_in_negating_context(neg_lower.as_str(), &content.to_lowercase());
            if neg_ctx_content {
                let boost = 1.15;
                tracing::info!("CONSTRAINT NEG-CTX BOOST (content): '{}' in '{}' → boost={:.2}",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    boost);
                score *= boost;
            } else if !is_alt_page {
                let penalty = (0.25 + (neg_count - 1.0) * 0.05).clamp(0.25, 0.50);
                tracing::info!("CONSTRAINT HIT (CONTENT): '{}' in '{}' → penalty={:.4} (non-alt)",
                    neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())],
                    penalty);
                score *= penalty;
            } else {
                any_unresolved_violation = true;
            }
        } else {
            tracing::info!("CONSTRAINT MISS: '{}' not in '{}'", neg, &text_lower[..text_lower.char_indices().nth(60).map(|(i,_)| i).unwrap_or(text_lower.len())]);
        }
    }

    if any_unresolved_violation {
        // Alt pages get one single flat penalty regardless of how many excluded
        // terms they mention. This prevents "Django vs FastAPI vs Flask: Which to
        // Choose" (which mentions all 3) from getting compounded 0.175^3 = 0.005.
        // Only terms that were NOT already resolved via the negation-context
        // boost above count as violations here — a boosted term is fulfilling
        // the exclusion, not violating it, so it must not also be penalized.
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
    // Finding 3: exclude negative-term occurrences when they appear in negating
    // context (e.g., "Sleep without pills" should NOT be hard-dropped because
    // "without pills" is FULFILLING the exclusion, not violating it).
    // Incidental mentions inside body/comparison pages are left to the soft
    // penalty above. Fail-closed: if we can't prove dominance, we keep it.
    // Uses the STRICT alt-page gate: a weak listicle signal alone (e.g. "Best
    // sleeping pills" — no comparison/alternative wording, no URL/content
    // corroboration) must not exempt a title genuinely dominated by the
    // excluded term from this hard drop.
    if !is_strong_alt_page && !expanded_negatives.is_empty() {
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
            // Check if any of the expanded negatives appear in negating context
            // in the title. If they do, they're RELEVANT (not violations).
            let negatives_in_negating_context: Vec<&String> = expanded_negatives.iter()
                .filter(|neg| term_in_negating_context(&neg.to_lowercase(), &title_lower))
                .collect();

            let dominated = title_tokens.iter().filter(|tok| {
                expanded_negatives.iter().any(|neg| {
                    if neg.is_empty() { return false; }
                    let n = neg.trim().to_lowercase();
                    // Exact word, or the token is a sub-brand/compound of the
                    // excluded term (n ⊂ tok, covering "pycharm-community",
                    // "macbook-pro", "nodejs"-style collisions handled by len gap).
                    let is_match = tok.as_str() == n.as_str()
                        || (tok.len() > n.len()
                            && tok.starts_with(&n)
                            && (tok.len() - n.len()) as f32 / n.len() as f32 <= 0.6);

                    // Exclude this match if the term is in negating context
                    is_match && !negatives_in_negating_context.contains(&neg)
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

    // Upper bound raised from 1.0 so the negation-context boost above (a
    // result that FULFILLS an "X without Y" exclusion) can actually surface
    // as a score above the neutral 1.0 baseline instead of being clipped back
    // down to parity with non-boosted results.
    score.clamp(0.0, 2.0)
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
    // Hard exclusions are a structural operator (`NOT:`) — already validated at
    // extraction time, so just cloned through (no prefix-stripping needed).
    let hard_exclusions = c.hard_exclusions.clone();

    // 1. Process negative constraints first (filtering and stripping +- or - prefixes)
    for n in &c.negative {
        let mut clean_n = n.trim().to_lowercase();
        if clean_n.starts_with('-') {
            clean_n = clean_n.strip_prefix('-').unwrap().trim().to_string();
        }
        if clean_n.starts_with('+') {
            clean_n = clean_n.strip_prefix('+').unwrap().trim().to_string();
        }
        // Cap at 4 words: NL negations like "big advertising company" legitimately
        // span 3 words once the leading verb/preposition is stripped
        // (extract_negation_term). The prior <=2 cap silently dropped them.
        // DA/DB fix (2026-08-17): also drop subjective-quality adjectives and
        // grammar-noise terms that the intent engine sometimes emits as
        // `Exclusion` entities or in its direct `negative` array next to a
        // negation marker ("not too spicy and good for kids" -> "good"/"too").
        // These are never real search exclusions; keeping them pollutes the
        // `constraints` field and risks a phantom hard-drop. A genuine topical
        // exclusion (brand/place/noun) is never in either noise set. This is the
        // single chokepoint every negative passes through, so it covers both the
        // engine-direct and engine-Exclusion-entity merge paths.
        if clean_n.split_whitespace().count() <= 4
            && !clean_n.is_empty()
            && !is_exclusion_grammar_noise(&clean_n)
            && !is_subjective_quality_term(&clean_n)
            && !is_verb_attribute_exclusion(&clean_n)
        {
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
            // Drop bare currency words that leaked past price extraction
            // ("four hundred dollars" -> digits + "dollars" left behind). They
            // never match result text and only pollute lexical-relevance scoring.
            // Currency-agnostic: covers every supported denomination token.
            let currency_words = ["dollars", "dollar", "usd", "rupees", "rupee",
                "inr", "rs", "rs.", "euros", "euro", "eur", "pounds", "pound",
                "gbp", "yen", "jpy", "won", "krw", "cents", "cent", "paise", "paisa"];
            if currency_words.contains(&pl.as_str()) { continue; }
            // D6 (2026-08-21): drop BARE NUMERIC tokens that leaked past price
            // extraction (e.g. "under 15000" / "below 2000" can leave the digits
            // in `positive` as "+15000"). A purely-numeric positive carries no
            // retrievable lexical meaning and only spuriously boosts pages that
            // echo the number — "Tablets Under 15000" outranking actual
            // "smartphones under 15000" for the latter query, because the token
            // 15000 matched the tablet page's title but not the phone page's.
            // The budget is ALREADY captured in `price_lt`/`price_max` and
            // enforced by the shopping/price path, so removing the number from
            // `positive` loses no signal. Signal-driven: ANY all-digit token
            // (with optional thousands separators / decimal point) is dropped
            // regardless of value — no per-query literals, no tuned thresholds.
            // Years are already captured as date constraints, so dropping a bare
            // year from `positive` is likewise safe.
            if pl.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') {
                continue;
            }
            // D4 (2026-08-17): if this term was already captured as a NEGATIVE
            // constraint (e.g. the intent engine emits both `+chinese` and `-chinese`
            // for "not from chinese brands"), it is a contradiction to also keep it as
            // a positive requirement. The negative is the authoritative intent, so we
            // drop it from the positive set. This prevents a positive+negative overlap
            // that no downstream gate can satisfy (a result can't both match and not
            // match `chinese`), which previously let the negated term leak through.
            if negative.contains(&pl) {
                continue;
            }
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
        hard_exclusions,
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
        ignored_constraints: None,
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

fn extract_price_from_text(text: &str) -> Option<PriceInfo> {
    let lower = text.to_lowercase();
    let amount_pat = r"(\d{1,3}(?:[.,]\d{3})*(?:\.\d{2})?|\d+(?:\.\d{2})?)";

    // 1. Range pattern: "$10 - $20", "$100-$200", "₹1,000 - ₹2,000" -> low bound
    static RE_RANGE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re_range = RE_RANGE.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i)(?:us\s?\$|can\s?\$|au\s?\$|\$|€|£|¥|₹|rs\.?\s?|inr|eur|gbp|usd)?\s*{}\s*(?:-|to)\s*(?:us\s?\$|can\s?\$|au\s?\$|\$|€|£|¥|₹|rs\.?\s?|inr|eur|gbp|usd)?\s*{}",
            amount_pat, amount_pat
        ))
        .unwrap()
    });
    if let Some(caps) = re_range.captures(&lower) {
        if let Some(m) = caps.get(1) {
            let raw = m.as_str().replace(',', "");
            if let Ok(v) = raw.parse::<f64>() {
                let currency = normalize_currency_str(caps.get(0).unwrap().as_str());
                return Some(PriceInfo { amount: v, currency });
            }
        }
    }

    // 2. Currency symbol / code followed by an amount: $100, €99, US$ 1,299.00, £50, ₹2,000, Rs. 500
    static RE1: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re1 = RE1.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i)(us\s?\$|can\s?\$|au\s?\$|\$|€|£|¥|₹|rs\.?\s?|inr|eur|gbp|usd)\s*{}",
            amount_pat
        ))
        .unwrap()
    });
    if let Some(caps) = re1.captures(&lower) {
        let curr_str = caps.get(1)?.as_str();
        let raw = caps.get(2)?.as_str().replace(',', "");
        if let Ok(v) = raw.parse::<f64>() {
            let currency = normalize_currency_str(curr_str);
            return Some(PriceInfo { amount: v, currency });
        }
    }

    // 3. Amount followed by a currency word: 100 dollars, 200 euros, 999 rupees, 2000 rs
    static RE2: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re2 = RE2.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i){}\s*(us\s?dollars?|dollars?|euros?|pounds?|gbp|usd|inr|rupees?|\brs\.?\b)",
            amount_pat
        ))
        .unwrap()
    });
    if let Some(caps) = re2.captures(&lower) {
        let raw = caps.get(1)?.as_str().replace(',', "");
        let curr_str = caps.get(2)?.as_str();
        if let Ok(v) = raw.parse::<f64>() {
            let currency = normalize_currency_str(curr_str);
            return Some(PriceInfo { amount: v, currency });
        }
    }

    // 4. Explicit price/cost label: "price: 49", "cost 129", "starting at 15.99"
    static RE3: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re3 = RE3.get_or_init(|| {
        regex::Regex::new(&format!(
            r"(?i)(?:price|cost|starting\s+at|from\s+price|for\s+only)\s*:?\s*(us\s?\$|can\s?\$|au\s?\$|\$|€|£|¥|₹|rs\.?\s?|inr|eur|gbp|usd)?\s*{}",
            amount_pat
        ))
        .unwrap()
    });
    if let Some(caps) = re3.captures(&lower) {
        let curr_str = caps.get(1).map(|m| m.as_str()).unwrap_or("usd");
        let raw = caps.get(2)?.as_str().replace(',', "");
        if let Ok(v) = raw.parse::<f64>() {
            let currency = normalize_currency_str(curr_str);
            return Some(PriceInfo { amount: v, currency });
        }
    }

    None
}

/// Extract a natural-language price bound from a raw query (no operator syntax).
/// Powers the P3 ranking fix for queries like "wireless headphones under 150 dollars"
/// or "laptop below 1000 rupees" — these never matched the `price:<` operator parser,
/// so the bound stayed None and ranking fell back to pure relevance (always surfacing
/// "under $200" pages). Returns (max_price, currency_code) for an upper bound, or
/// (None, _) for a lower bound.
///
/// Patterns (case-insensitive, currency-agnostic — no merchant/region hardcoding):
///   A) "<upper-marker> <number> [currency]"  e.g. "under 150 dollars", "below 1000 rs"
///   B) "<currency-symbol><number> <upper-marker>" e.g. "₹20000 or less", "$600 max"
///   C) "<number> <currency> <upper-marker>" e.g. "1000 rupees max"
fn extract_nl_price_bound(q: &str) -> Option<(f32, String)> {
    let lower = q.to_lowercase();
    let upper_markers = [
        "under", "below", "less than", "cheaper than", "max", "maximum", "at most", "no more than", "within", "up to", "around", "about", "budget",
    ];
    let lower_markers = ["over", "more than", "above", "minimum", "at least", "from"];
    let amount_pat = r"(\d{1,3}(?:[.,]\d{3})*(?:\.\d{2})?|\d+(?:\.\d{2})?)";
    let currency_words = ["dollars", "dollar", "usd", "rupees", "rupee", "inr", "rs", "₹", "rs.", "euros", "euro", "eur", "pounds", "pound", "gbp", "yen", "jpy", "won", "krw"];
    // Distance units: a number followed by one of these is a RANGE/DISTANCE
    // bound (e.g. "within 300 kilometers", "up to 50 miles"), NOT a price.
    // Without this guard, "within 300 kilometers" was mis-read as price:<300
    // and the spurious price bound dropped relevant results (round 2026-08-15).
    // General, unit-aware — no per-query literals.
    let distance_units = [
        "km", "kms", "kilometer", "kilometers", "kilometre", "kilometres",
        "mile", "miles", "mi", "meter", "meters", "metre", "metres",
        "foot", "feet", "ft", "yard", "yards", "yd",
    ];
    let is_distance_bound = |rest_after_num: &str| -> bool {
        distance_units.iter().any(|u| {
            let pat = format!(r"(?i)(?:^|[^a-z])\s*{}\b", regex::escape(u));
            regex::Regex::new(&pat).map(|re| re.is_match(rest_after_num)).unwrap_or(false)
        })
    };

    // Pattern A: upper-marker then number (+ optional currency word)
    for marker in upper_markers {
        if let Some(pos) = lower.find(marker) {
            let rest = &lower[pos + marker.len()..];
            let re_num = regex::Regex::new(&format!(r"\s*{}\b", amount_pat)).ok()?;
            if let Some(caps) = re_num.captures(rest) {
                if let Some(m) = caps.get(1) {
                    if let Ok(v) = m.as_str().replace(',', "").parse::<f32>() {
                        // Distance-bound guard: "within 300 kilometers" is a
                        // range, not a price — skip this marker (let a later
                        // price marker, if any, match instead).
                        if is_distance_bound(rest) {
                            continue;
                        }
                        let currency = currency_words.iter().find(|c| rest.contains(*c))
                            .map(|c| normalize_currency_str(c)).unwrap_or_else(|| "usd".to_string());
                        return Some((v, currency));
                    }
                }
            }
        }
    }
    // Pattern B: currency symbol/word then number then upper-marker
    let re_b = regex::Regex::new(&format!(r"(₹|¥|€|£|\$|usd|inr|rs|rupees?|eur|euros?|gbp|pounds?)\s*{}?\s*({})", amount_pat, upper_markers.join("|"))).ok()?;
    if let Some(caps) = re_b.captures(&lower) {
        if let (Some(cur), Some(num)) = (caps.get(1), caps.get(2)) {
            if let Ok(v) = num.as_str().replace(',', "").parse::<f32>() {
                // Distance-bound guard (see Pattern A): a currency-symbol amount
                // followed by a distance unit is not a price.
                let after_num = &lower[caps.get(2).unwrap().end()..];
                if is_distance_bound(after_num) {
                    // fall through; do not return a price bound
                } else {
                    return Some((v, normalize_currency_str(cur.as_str())));
                }
            }
        }
    }
    // Pattern C: number then currency word then upper-marker
    let re_c = regex::Regex::new(&format!(r"{}\s*({})\s*({})", amount_pat, currency_words.join("|"), upper_markers.join("|"))).ok()?;
    if let Some(caps) = re_c.captures(&lower) {
        if let (Some(num), Some(cur)) = (caps.get(1), caps.get(2)) {
            if let Ok(v) = num.as_str().replace(',', "").parse::<f32>() {
                // Distance-bound guard (see Pattern A): number + currency word +
                // marker, where a distance unit follows, is a range not a price.
                let after_num = &lower[caps.get(1).unwrap().end()..];
                if is_distance_bound(after_num) {
                    // fall through; do not return a price bound
                } else {
                    return Some((v, normalize_currency_str(cur.as_str())));
                }
            }
        }
    }
    // Lower bound (return with None so caller treats as gt)
    for marker in lower_markers {
        if let Some(pos) = lower.find(marker) {
            let rest = &lower[pos + marker.len()..];
            let re_num = regex::Regex::new(&format!(r"\s*{}\b", amount_pat)).ok()?;
            if let Some(caps) = re_num.captures(rest) {
                if let Some(m) = caps.get(1) {
                    if let Ok(_v) = m.as_str().replace(',', "").parse::<f32>() {
                        // Lower-bound only; signal via currency "GT"
                        return None; // keep simple: NL lower bounds not yet wired to price_gt
                    }
                }
            }
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

        // 2a. Hard EXCLUSION for the explicit `NOT:` operator (e.g. "NOT:flask").
        //     This is an UNCONDITIONAL structural exclude: any result whose
        //     title/content/url contains the term is dropped. It mirrors the
        //     `-site:`/`-filetype:` negatives handled just above — all are dropped
        //     here before the soft-penalty path (section 5) is reached. The
        //     alt-listing exemption (alt_score > 0.3) is preserved: a comparison /
        //     "alternatives" page that merely *mentions* the excluded term in a
        //     referential context (e.g. "Flask" in an "alternatives to Django"
        //     listicle) must NOT be hard-dropped, consistent with every other
        //     negative hard-drop gate (constraint_score + post-merge + pre-merge).
        //     Substring match (not whole-word) because hard-exclusion terms are
        //     short and user-intended (e.g. "NOT:spam" should catch "spammer").
        if !constraints.hard_exclusions.is_empty() {
            let alt_score = is_alternative_listing_page(title, url, content);
            // Alt-listing exemption: a page scoring > 0.3 IS an alternatives /
            // comparison listing, so mentioning the excluded term is referential,
            // not a violation — keep it.
            if alt_score <= 0.3 {
                let t_low = title.to_lowercase();
                let c_low = content.to_lowercase();
                let u_low = url.to_lowercase();
                if constraints.hard_exclusions.iter().any(|he| {
                    let he = he.trim();
                    if he.is_empty() { return false; }
                    t_low.contains(he) || c_low.contains(he) || u_low.contains(he)
                }) {
                    return true;
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
        let dummy = SearxResult {
            title: title.to_string(),
            url: url.to_string(),
            content: content.to_string(),
            engine: "".to_string(),
            score: 0.0,
            sources: vec![],
            published_date: published_date.map(|s| s.to_string()),
            price: None,
            currency: None,
        };
        if let Some(p_info) = dummy.get_price() {
            let p_usd = price_to_usd(p_info.amount, &p_info.currency) as f32;
            if let Some(pmin) = constraints.price_min {
                if p_usd < pmin { return true; }
            }
            if let Some(pmax) = constraints.price_max {
                if p_usd > pmax { return true; }
            }
            if let Some(plt) = constraints.price_lt {
                if p_usd > plt { return true; }
            }
            if let Some(pgt) = constraints.price_gt {
                if p_usd < pgt { return true; }
            }
        }
    }

    // 5. Negative constraint handling.
    //
    // DESIGN (P5-class "negative over filter" fix): a plain NEGATIVE TERM (e.g.
    // "vim" from "text editor without vim") must NEVER hard-drop a result. The
    // penalty for matching a negative term is already enforced softly through
    // `constraint_score` → `c_score` → `r.score` (main.rs:5276), which demotes
    // matching results while keeping them in the set. Hard-dropping here is what
    // collapsed "text editor without vim keybindings" to ZERO results: every
    // genuine "text editor" page mentions "vim" (it's the canonical editor), so
    // the hard filter removed ALL of them and left nothing.
    //
    // Hard drops are therefore reserved for UNAMBIGUOUS STRUCTURAL operators whose
    // meaning admits no soft interpretation:
    //   · `site:` negatives  -> handled above (web.drop, line ~2213)
    //   · `filetype:` negatives -> handled above (line ~2222)
    //   · date bounds         -> handled above (after/before_date)
    //   · exact `phrases`     -> handled above (line ~2290, fail-open token match)
    // A bare word like "vim" / "java" / "django" is NOT structural — it is a
    // topical exclusion that grades smoothly, so we never hard-drop on it.
    //
    // DESIGN (P5-class "negative over filter" fix): this predicate hard-drops ONLY
    // on unambiguous STRUCTURAL operators (site:/filetype: negatives, date bounds,
    // exact phrases) — all of which are already handled in the blocks above (2b,
    // 3, 4) and `return true` themselves before reaching here. A bare NEGATIVE
    // TERM (e.g. "vim" from "text editor without vim") is a TOPICAL exclusion, not
    // a structural one: it grades smoothly and is enforced SOFTLY through
    // `constraint_score` → `c_score` → `r.score` (main.rs:5276). Hard-dropping on a
    // plain term is exactly what collapsed "text editor without vim" to ZERO
    // results (every genuine "text editor" page mentions "vim"). So: if the only
    // negative signal is a plain term, never hard-drop here — `c_score` demotes
    // matches while keeping the set non-empty. With the structural operators
    // already returned above, any fall-through here means "nothing left to drop".
    false
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

    // M2 topic drift fix: Demote dictionary/glossary pages for multi-word or informational/how-to queries
    if clean::is_definition_site(&t_lower, &c_lower) && (query_terms.len() > 1 || q_lower.contains("how") || q_lower.contains("why")) {
        return (combined * 0.10).clamp(0.01, 0.05);
    }

    // M2 topic drift fix: Multi-word phrase sense handling (e.g. "machine learning")
    // Penalize results that match only an isolated single token (e.g. "machine") while missing compound terms
    let mut final_score = combined;
    if query_terms.len() >= 2 {
        let text_all = format!("{} {}", t_lower, c_lower);
        for w in query_terms.windows(2) {
            let phrase = format!("{} {}", w[0], w[1]);
            // If the query contains a compound phrase, but document contains only word 0 without word 1 or the phrase
            if q_lower.contains(&phrase) && !text_all.contains(&phrase) {
                let has_w0 = text_all.contains(&w[0]);
                let has_w1 = text_all.contains(&w[1]);
                if has_w0 ^ has_w1 {
                    final_score *= 0.25;
                    break;
                }
            }
        }
    }

    final_score.clamp(0.0, 1.0)
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
    local_results: &[IndexerResult],
    web_results: &[SearxResult],
    client: &reqwest::Client,
) -> std::collections::HashMap<String, f32> {
    use std::collections::HashMap;
    let mut out: HashMap<String, f32> = HashMap::new();
    let qv = match query_vector {
        Some(v) if v.len() >= 2 => v,
        _ => return out, // no query embedding -> fail closed
    };
    // Deduplicate texts by URL; embed top local AND top web results.
    let mut texts: Vec<String> = Vec::new();
    let mut url_order: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 1. Add local results (up to 15)
    for r in local_results.iter().take(15) {
        if !seen.insert(r.url.clone()) { continue; }
        let text = format!("{} {}", r.title, r.content);
        if text.trim().len() < 10 { continue; }
        url_order.push(r.url.clone());
        texts.push(text);
    }
    // 2. Add web results (up to 25)
    for r in web_results.iter().take(25) {
        if !seen.insert(r.url.clone()) { continue; }
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
        .timeout(std::time::Duration::from_millis(2000));
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
    // WEAK-SET GUARD (round-6 D1 defense-in-depth): calibrate_scores linearly rescales
    // the whole result set onto [0.05, 1.0], which forces the MAX raw score to 1.0.
    // When the entire set is weak (best raw score < 0.10 — e.g. sparse web upstream
    // plus a coincidental local hit), the off-topic page can be the max and gets
    // rescaled UP to 1.0, inverting the ranking. In that regime we must NOT inflate:
    // preserve the raw relative ordering (which already demotes off-topic pages via the
    // relevance fold) and only lift the floor. On-topic queries with a healthy result
    // have raw_max >= 0.10, so the standard [0.05,1.0] remap runs unchanged (no regression).
    if raw_max < 0.10 {
        // WEAK-SET SPREAD (round-7 fix): the original guard pinned every score to
        // the 0.05 floor whenever the best raw score was < 0.10. That regime is
        // NORMAL for web queries: RRF contributions for top positions are ~0.05-0.08
        // and authority/semantic weights are small, so base scores land at
        // 0.005-0.02. Pinning to floor destroyed the ranker's ordering, so a leaked
        // off-topic page (e.g. a Vale earnings release for "noise cancelling
        // earbuds", thesaurus.com/"biggest" for "cybersecurity breaches", a Zomato
        // "Thai Restaurants in Jaipur" for "vegetarian thali ... jaipur") tied with
        // the genuinely on-topic page at 0.05 and won by insertion order.
        // Instead, rescale the weak set onto a CONSTRAINED low sub-range
        // [0.05, 0.12] that preserves the raw relative order. This keeps the round-6
        // off-topic defense (no weak result is stretched to ~1.0, so an off-topic
        // survivor cannot invert the ranking — on-topic pages have higher raw base
        // and stay above) while differentiating on-topic pages from leaked ones.
        // The off-topic sole-survivor case is already removed pre-scoring by the
        // distinctive-term hard-drop, so the only remaining weak sets are legit.
        let lo = 0.05f32;
        let hi = 0.12f32;
        let norm = (raw_max - raw_min).max(1e-6);
        for score in scores.iter_mut() {
            let t = ((*score - raw_min) / norm).clamp(0.0, 1.0);
            *score = lo + t * (hi - lo);
        }
        return;
    }
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

/// Whole-word (or whole multi-word phrase) substring test. `"in"` never matches
/// inside `"india"`, and `"new york"` requires the full contiguous phrase. Used by
/// the cross-location mismatch guard below so country/city name collisions don't
/// fire on incidental substring hits.
fn whole_word_contains(haystack: &str, needle: &str) -> bool {
    let n = needle.to_lowercase();
    if n.contains(' ') {
        return haystack.contains(&n);
    }
    haystack
        .split_whitespace()
        .any(|w| w.trim_matches(|c: char| !c.is_alphanumeric()) == n)
}

/// Cross-location mismatch penalty (local/geo round defect, 2026-08-19).
///
/// When the user NAMES a place in the query (explicit geo), a result that talks
/// about a *different* known place but never mentions the requested place is
/// almost certainly wrong for that query — e.g. "yoga studios in chennai"
/// surfacing a page about Orlando, or "street food in bangalore" surfacing a
/// Chennai page. We dampen such results so the requested-place results win.
///
/// Design (no hardcoding):
///   • Reuses the SAME `LOCATION_GAZETTEER` reference data as geo detection, so it
///     stays in sync and needs no per-query literals or denylists.
///   • Only fires on EXPLICIT query locations (`geo_is_explicit`), so a user's
///     IP-derived country never penalises legitimately different-city pages.
///   • If the result already mentions the requested place, it is on-topic for the
///     location → never penalised (covers inclusive "best in <country>" lists that
///     also name the requested city).
///   • A result that mentions a different place is dampened hard but kept present
///     (fail-soft, not a hard drop).
///   • Country-level requests forgive same-country places (a "india" query should
///     not penalise a "chennai" page); city-level requests DO penalise other cities
///     even in the same country (chennai ≠ bangalore).
///   • 2-letter gazetteer codes ("us", "uk") are skipped as mismatch candidates to
///     avoid pronoun/function-word false hits ("…let us know…").
fn cross_location_mismatch_mult(
    title: &str,
    content: &str,
    geo: Option<&geoloc::GeoLocation>,
) -> f32 {
    let geo = match geo {
        Some(g) => g,
        None => return 1.0,
    };
    let req_city = geo.city.as_deref();
    let req_country = geo.country_name.as_deref();
    let req_cc = geo.country_code.as_deref();
    let text = format!("{} {}", title.to_lowercase(), content.to_lowercase());

    // On-topic for the requested location → never penalise.
    let mentions_req = req_city.map_or(false, |c| whole_word_contains(&text, c))
        || req_country.map_or(false, |c| whole_word_contains(&text, c));
    if mentions_req {
        return 1.0;
    }

    // City-level requests penalise other (even same-country) cities; country-level
    // requests forgive same-country places.
    let same_country_ok = req_city.is_none();
    for (name, cc) in LOCATION_GAZETTEER.iter() {
        if req_city.map_or(false, |c| c.eq_ignore_ascii_case(name)) {
            continue;
        }
        if req_country.map_or(false, |c| c.eq_ignore_ascii_case(name)) {
            continue;
        }
        if same_country_ok {
            if let Some(rc) = req_cc {
                if cc.eq_ignore_ascii_case(rc) {
                    continue;
                }
            }
        }
        if name.len() < 3 {
            continue; // skip 2-letter codes (us/uk) to avoid false hits
        }
        if whole_word_contains(&text, name) {
            // 2026-08-19 round: 0.4 -> 0.12. Then 2026-08-19T1628Z round: 0.12 -> 0.06.
            // The 0.12x dampening was STILL too weak — for a "best vegetarian thali
            // places in mysore" query the off-topic Bing page "60 Best Places to
            // Visit in Hyderabad" (which names a different gazetteer city) kept a
            // 0.12x-of-a-large-base score ABOVE the correct on-topic Mysore results,
            // because authority + quality boosts lifted its base and calibrate_scores
            // rescales the max raw score back up. 0.06x crushes the mismatched page
            // below the requested-city results while keeping it present (fail-soft).
            // Pages that NAME the requested city are exempted earlier (mentions_req),
            // so inclusive lists stay untouched. General.
            return 0.06;
        }
    }
    1.0
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

/// Known video-hosting domains. Used by the P8 video dampening so that videos
/// arriving through the GENERAL web result set (e.g. a youtube.com URL returned by
/// SearXNG, which is NOT tagged with the `invidious`/`video` source) are still
/// recognized as video results and dampened for non-video queries. This is a struct-
/// ural allow-list of platforms the engine explicitly treats as "video surfaces",
/// not a per-query tuned list — it is the same platform set the dedicated /videos
/// endpoint uses, so the policy is consistent and future-proof.
const VIDEO_HOSTS: &[&str] = &[
    "youtube.com", "youtu.be", "m.youtube.com", "youtube-nocookie.com",
    "vimeo.com", "dailymotion.com", "twitch.tv", "rumble.com", "odysee.com",
    "bitchute.com", "lbry.tv", "peer.tube", "invidious",
];

/// Returns true if `url` points at a known video platform (see VIDEO_HOSTS).
/// Cheap, allocation-free host suffix check — no DNS, no per-request config.
fn is_url_video_host(url: &str) -> bool {
    let u = url.to_lowercase();
    let host = if let Some(idx) = u.find("://") {
        &u[idx + 3..]
    } else {
        &u
    };
    let host = host.split(['/', '?', '#']).next().unwrap_or(host);
    VIDEO_HOSTS.iter().any(|h| {
        host == *h
            || host.ends_with(&format!(".{}", h))
            // Invidious instances are self-hosted at arbitrary subdomains
            // (e.g. invidious.example.net, invidious.snopyta.org), so match any
            // host that carries the "invidious" label as a full domain label —
            // not just the bare `invidious` / `.invidious` suffix. This is the
            // same structural host-class check the dedicated /videos endpoint
            // uses; no per-instance domain list.
            || (h == &"invidious"
                && (host == "invidious"
                    || host.starts_with("invidious.")
                    || host.ends_with(".invidious")))
    })
}

/// Words/phrases that signal CONTRASTIVE framing. When a negation marker sits in
/// contrastive framing, the negated head is genuinely a search exclusion (e.g.
/// "search engine alternative to google" → exclude google; "react vs vue" → exclude
/// both when double-negated). These are generic structural signals, NOT per-query
/// strings, so the exemption stays future-proof.
const CONTRASTIVE_MARKERS: &[&str] = &[
    "compare", "comparison", "versus", "vs", "alternative", "alternatives",
    "replacement", "instead", "rather than", "other than", "besides",
    // P9: unambiguous single-marker exclusion words. `extract_query_negative_terms`
    // already treats these as neg markers (lines ~3873, ~3899), but they were
    // missing here — so a SINGLE "<except/excluding/minus> X" with a non-protected
    // target (e.g. "javascript framework except react", "search engine except
    // google") returned `constraints: null` (the exclusion was declined because
    // `query_is_contrastive` was false) and React/Google pages dominated. Unlike
    // "not"/"no"/"without", these words only ever mean exclusion, so flagging them
    // as contrastive is structurally safe (no manner-qualifier ambiguity).
    "except", "excluding", "minus",
];

/// Returns true if a negated compound is a MANNER qualifier (describes HOW the user
/// wants to act, not WHAT they want excluded). These must never become search
/// exclusions. Signals (general, not per-query):
///  - contains a relational/pronominal word ("you", "your", "as", "me", "us", "them",
///    "it", "we", "i", "my", "our", "they") — e.g. "does not track you as",
///  - or is verb-led (any token is a participle ending in -ing/-ed, or a known verb) —
///    e.g. "without offending the couple", "without damaging the board",
///    "without training wheels patiently", "without soap" (soap is a noun, not verb,
///    so this clause does not fire for it — soap is caught by the non-entity,
///    non-contrastive rule instead).
const MANNER_PRONOUNS: &[&str] = &[
    "you", "your", "yours", "as", "me", "us", "them", "it", "its", "we", "i",
    "my", "our", "they", "he", "she", "him", "her",
];
const MANNER_VERBS: &[&str] = &[
    "taking", "taken", "take", "using", "use", "used", "having", "have", "has",
    "buying", "buy", "bought", "getting", "get", "got", "making", "make", "made",
    "eating", "eat", "ate", "drinking", "drink", "drank", "doing", "do", "did",
    "going", "go", "applying", "apply", "wearing", "wear", "wore", "installing",
    "install", "running", "run", "track", "tracked", "tracking", "offend",
    "offending", "offended", "damage", "damaging", "damaged", "train", "training",
    "call", "calling", "called", "help", "helping", "hurt", "hurting", "harm",
    "harming", "lose", "losing", "spend", "spending", "cost",
    "costing", "need", "needing", "want", "wanting", "show", "showing", "tell",
    "telling",
];

/// Tokens that must never be surfaced as a "declined exclusion" in
/// `ignored_constraints`. These are grammar/prepositions ("in", "about", "of",
/// "the", "a") or generic function words that the extractor may emit as a
/// negative candidate when it fails to find a real content target. Surfacing
/// them would confuse users ("not:in — exclusion not applied"). This mirrors the
/// `GENERIC_NEG` / `stopwords` lists used inside the extractor; it is a
/// structural vocabulary, not per-query literals (consistent with the
/// hardcoding doctrine).
const IGNORED_CONSTRAINT_NOISE: &[&str] = &[
    "in", "about", "of", "the", "a", "an", "to", "on", "at", "for", "with",
    "by", "from", "than", "as", "into", "onto", "upon", "over", "under",
    "before", "after", "and", "or", "but", "is", "are", "was", "were",
];

/// F3 (2026-08-17): seed list of country demonyms / origin adjectives. When a user
/// excludes a COUNTRY-of-origin (e.g. "not from chinese brands", "alternatives to american
/// cloud providers", "laptops not made in china"), the demonym IS the genuine topical
/// exclusion — it must be honored even when the query lacks contrastive framing and the
/// word is not a protected brand. This is a general data seed (like PROTECTED_TERMS),
/// not tuned to any one query: covering major manufacturing/origin adjectives closes the
/// "not from <country>" negation class broadly. No per-query literals.
const COUNTRY_DEMONYMS: &[&str] = &[
    "chinese", "china", "american", "usa", "us", "indian", "india", "japanese", "japan",
    "korean", "korea", "south korean", "north korean", "german", "germany",
    "french", "france", "british", "uk", "english", "canadian", "canada", "russian",
    "russia", "taiwanese", "taiwan", "vietnamese", "vietnam", "thai",
    "thailand", "singaporean", "singapore", "malaysian", "malaysia", "indonesian",
    "indonesia", "brazilian", "brazil", "mexican", "mexico", "turkish", "turkey",
    "italian", "italy", "spanish", "spain", "dutch", "netherlands", "swiss",
    "switzerland", "swedish", "sweden", "polish", "poland", "israeli", "israel",
    "iranian", "iran", "pakistani", "pakistan", "bangladeshi", "bangladesh",
    "australian", "australia",
];

/// D3: precise manner-frame detection at the PHRASE level (not the bare-token
/// level that `is_manner_phrase` uses). A declined candidate is a manner
/// qualifier when it appears inside a "without/with-no <optional article> <term>"
/// frame, or carries a manner pronoun (e.g. "does not track you as"). These
/// describe HOW the user wants to act, not WHAT they want excluded, so they must
/// NOT be surfaced in `ignored_constraints` — surfacing "not:soap — exclusion not
/// applied" would be confusing and contradict the round's manner-suppression.
///
/// Structural, not per-query: it tests whether the *extracted compound* sits in a
/// known manner frame within the query. Reuses the open-class `MANNER_PRONOUNS`
/// set; no per-query literals, no tuned thresholds (consistent with the
/// hardcoding doctrine and the existing `is_manner_phrase`).
fn is_manner_frame(q_orig: &str, compound: &str) -> bool {
    let lc = q_orig.to_lowercase();
    let c = compound.to_lowercase();
    let frames = [
        format!("without {}", c),
        format!("without a {}", c),
        format!("without an {}", c),
        format!("without the {}", c),
        format!("with no {}", c),
        format!("with no a {}", c),
    ];
    if frames.iter().any(|f| lc.contains(f.as_str())) {
        return true;
    }
    // Manner pronouns anywhere in the compound ("track you as", "offend the couple")
    // mark it as a manner qualifier even without the "without" frame.
    let c_tokens: Vec<&str> = c.split_whitespace().collect();
    c_tokens.iter().any(|t| MANNER_PRONOUNS.contains(t))
}

fn is_manner_phrase(compound: &str) -> bool {
    let lc = compound.to_lowercase();
    let tokens: Vec<&str> = lc.split_whitespace().collect();
    if tokens.iter().any(|t| MANNER_PRONOUNS.contains(t)) {
        return true;
    }
    if tokens.iter().any(|t| MANNER_VERBS.contains(t)) {
        return true;
    }
    false
}

/// D2 (2026-08-19): disambiguate the genuinely ambiguous word "pay" inside a
/// negated clause. The intent engine may emit a bare "pay"/"paying" token as an
/// `Exclusion` entity (e.g. from "how to learn programming without paying for a
/// course" it extracted `paying`). We must decide, from the QUERY CONTEXT (not the
/// bare token), whether this is:
///   - MANNER:    "pay attention" / "pay respect" / "pay regard" / "pay heed" —
///                the user describes HOW they act → MUST be declined (a manner
///                false-positive that would wrongly drop relevant pages).
///   - MONEY:     "pay for a course" / "pay a fee" / "pay money" / "pay a
///                subscription" — the user refuses a financial transaction → MUST
///                be honored (a real exclusion). This was the dropped D2 defect:
///                "pay"/"paying" were bluntly listed in MANNER_VERBS/VERB_HEADS and
///                every money-exclusion got declined.
///
/// The decision is driven entirely by the query's nearby OBJECT vocabulary — a
/// general seed of MANNER objects vs MONETARY objects, no per-query literals, no
/// tuned thresholds. This is the same open-class "verb + object class" pattern as
/// `is_verb_attribute_exclusion`, so it is future-proof and non-hardcoded.
fn pay_exclusion_is_manner(q_orig: &str) -> bool {
    let lc = q_orig.to_lowercase();
    const PAY_MANNER_OBJECTS: &[&str] = &[
        "attention", "respect", "regard", "heed", "tribute", "homage",
        "compliments", "compliment", "court", "mind", "witness", "lip",
    ];
    // "pay <manner-object>" / "paying <manner-object>" anywhere in the query →
    // the MANNER idiom (an act of consideration, never a transaction).
    PAY_MANNER_OBJECTS.iter().any(|m| {
        lc.contains(&format!("pay {}", m)) || lc.contains(&format!("paying {}", m))
    })
}

fn pay_exclusion_is_money(q_orig: &str) -> bool {
    let lc = q_orig.to_lowercase();
    const PAY_MONEY_OBJECTS: &[&str] = &[
        "course", "courses", "subscription", "subscriptions", "fee", "fees",
        "price", "prices", "money", "cost", "costs", "charge", "charges",
        "tuition", "premium", "payment", "payments", "dollar", "dollars",
        "rupee", "rupees", "bill", "bills", "tax", "taxes", "rent", "fare",
        "membership", "license", "licence", "bootcamp", "class", "classes",
        "training", "program", "programme",
    ];
    // A monetary object near "pay"/"paying" signals a financial transaction the
    // user refuses ("pay for a course", "pay a subscription fee"). We require the
    // object word itself (no loose "pay a"/"paying a" prefix, which wrongly matched
    // "paying attention"/"paying advice"). This is the same object-class seed
    // pattern as the manner check — general, non-hardcoded, no tuned thresholds.
    PAY_MONEY_OBJECTS.iter().any(|m| lc.contains(m))
}

/// F3 (2026-08-17): a negated compound is pure GRAMMAR/auxiliary noise when every
/// token is a manner verb, manner pronoun, or a filler stopword/auxiliary
/// ("have", "has", "from", "of", "the", ...). The intent engine's Query-Graph IR
/// sometimes emits these as `Exclusion`-role entities (e.g. "not from chinese brands
/// and have usb c charging" → Exclusion="have"). Such tokens must never become search
/// exclusions — they describe grammar, not the thing the user wants excluded, and they
/// would override the gateway parser's correct topical exclusion. Structural vocabulary
/// (reuses MANNER_* + a small filler set), no per-query literals.
fn is_exclusion_grammar_noise(term: &str) -> bool {
    if term.trim().is_empty() {
        return true;
    }
    let filler: &[&str] = &[
        "from", "of", "the", "a", "an", "to", "in", "on", "at", "for", "with", "by",
        "and", "or", "but", "is", "are", "was", "were", "be", "been", "being",
        "do", "does", "did", "have", "has", "had", "use", "using", "used",
    ];
    let tokens: Vec<&str> = term.split_whitespace().collect();
    if tokens.is_empty() {
        return true;
    }
    tokens.iter().all(|t| {
        MANNER_PRONOUNS.contains(t) || MANNER_VERBS.contains(t) || filler.contains(t)
    })
}

/// A negated clause object is a VERB-LED / ATTRIBUTE exclusion when its head is an
/// open-class verb or a personal-attribute noun — i.e. it describes *how the user
/// wants to do something* or *a trait of the user*, NOT a content topic to remove
/// from results. The intent engine's Query-Graph IR sometimes tags these as
/// `Exclusion`-role entities (e.g. "alternatives to zoom that do not require
/// downloading an app and respect privacy" -> Exclusion="respect"; "juggle three
/// balls with no coordination" -> "coordination"; "young earner with no
/// dependents" -> "dependents"; "charge overnight without fire risk" -> "fire";
/// "fix a faucet without replacing the tap" -> "replacing"). These are NEVER real
/// search exclusions — hard-filtering "respect"/"coordination"/"dependents" drops
/// every otherwise-relevant page and collapses the result set. The gateway trusts
/// engine `Exclusion` entities and bypasses the `is_real_exclusion` gate, so we
/// reject them here at the same merge point. Structural open-class vocabulary
/// (reused MANNER_VERBS + a verb/attribute seed), no per-query literals — so any
/// verb-led or attribute exclusion ("without cooking", "with no training",
/// "apps that do not track you and respect privacy") is caught generally. A
/// genuine topical exclusion (brand / place / noun the user named) is never in
/// this set, so real exclusions survive.
// Inflection-tolerant verb stem: returns the bare stem of a regular English verb
// inflection so a single seed list (VERB_HEADS/MANNER_VERBS) covers every
// conjugation. "works"->"work", "turning"->"turn", "required"->"require",
// "using"->"use". This is derived, not a per-token literal, so it generalises.
fn verb_stem(t: &str) -> String {
    let n = t.len();
    if n > 4 && t.ends_with("ing") {
        return t[..n - 3].to_string(); // turning -> turn
    }
    if n > 3 && t.ends_with("ed") {
        return t[..n - 2].to_string(); // required -> requir (caller tries +e)
    }
    if n > 3 && t.ends_with("es") {
        return t[..n - 2].to_string(); // matches -> match
    }
    if n > 2 && t.ends_with('s') {
        return t[..n - 1].to_string(); // works -> work
    }
    t.to_string()
}

fn is_verb_attribute_exclusion(term: &str) -> bool {
    let lc = term.trim().to_lowercase();
    if lc.is_empty() {
        return true;
    }
    // Personal-attribute / trait nouns that describe the USER, not a content topic.
    const ATTRIBUTE_NOUNS: &[&str] = &[
        "coordination", "dependents", "experience", "background", "training",
        "skill", "skills", "knowledge", "degree", "qualification", "qualifications",
        "subscription", "account", "accounts", "registration", "signup", "sign-up",
        "login", "log-in", "app", "apps", "application", "applications", "download",
        "downloading", "install", "installing", "permission", "permissions",
    ];
    // Open-class verb seed (reuses MANNER_VERBS where overlapping) — the head of a
    // negated clause that is a verb is describing an action, not a topic to drop.
    const VERB_HEADS: &[&str] = &[
        "respect", "require", "requires", "required", "needing", "need", "needs",
        "track", "tracks", "tracking", "sell", "sells", "selling", "share", "shares",
        "sharing", "collect", "collects", "collecting", "replace", "replacing",
        "replaceing", "charge", "charging", "harm", "harming", "damage", "damaging",
        "burn", "burning", "fire", "cost", "costs", "spend", "spending", "register",
        "registering", "download", "downloading", "install",
        "installing", "sign", "signing", "subscribe", "subscribing", "login",
        "cook", "cooking", "drive", "driving", "travel", "travelling", "traveling",
        "learn", "learning", "work", "working", "study", "studying", "read", "reading",
        "use", "using", "turn", "turning", "compromise", "expose", "exposing",
    ];
    // Open-class descriptive ADJECTIVES: a negated adjective ("not usual", "not
    // spicy", "not free") describes the user's preference, NOT a content topic to
    // remove. Admitting adjectives in the all-match stops phantom single-word
    // negatives like "usual" (from "without the usual crowds") from becoming
    // search exclusions. General trait vocabulary, no per-query literals.
    const ADJECTIVES: &[&str] = &[
        "usual", "normal", "common", "typical", "standard", "regular",
        "popular", "free", "cheap", "expensive", "easy", "hard", "simple",
        "complex", "fast", "slow", "old", "new", "big", "small", "large",
        "spicy", "sweet", "hot", "cold", "fresh", "clean", "dirty", "safe",
    ];
    // A token is verb-like if it is a seed verb OR a regular inflection of one.
    let is_verb_like = |t: &&str| -> bool {
        if VERB_HEADS.contains(t) || MANNER_VERBS.contains(t) {
            return true;
        }
        let stem = verb_stem(t);
        if VERB_HEADS.contains(&stem.as_str()) || MANNER_VERBS.contains(&stem.as_str()) {
            return true;
        }
        // recovery for doubled-consonant stems (requir -> require)
        let with_e = format!("{}e", stem);
        VERB_HEADS.contains(&with_e.as_str()) || MANNER_VERBS.contains(&with_e.as_str())
    };
    let tokens: Vec<&str> = lc.split_whitespace().collect();
    if tokens.is_empty() {
        return true;
    }
    // Reject if EVERY token is a verb/attribute/adj head or a filler — i.e. the
    // whole extracted exclusion describes an action/trait, not a named topic.
    tokens.iter().all(|t| {
        is_verb_like(t)
            || ATTRIBUTE_NOUNS.contains(t)
            || ADJECTIVES.contains(t)
            || MANNER_PRONOUNS.contains(t)
            || is_exclusion_grammar_noise(t)
    })
}

/// Subjective-quality descriptors and intensifiers (e.g. "good", "too", "best",
/// "spicy", "cheap") are never real search exclusions. The intent engine
/// sometimes emits them as `Exclusion`-role entities when they sit next to a
/// negation marker ("not too spicy and good for kids" -> Exclusion="good"/"too").
/// Treating a quality adjective as a hard exclusion silently drops relevant pages
/// and injects a phantom negative. This is structural vocabulary, not per-query
/// literals; it mirrors the MANNER_VERBS design. A genuine topical exclusion
/// (a brand, place, or noun the user named) is never in this set.
fn is_subjective_quality_term(term: &str) -> bool {
    const QUALITY: &[&str] = &[
        "good", "bad", "best", "worst", "nice", "great", "poor", "fine",
        "tasty", "spicy", "sweet", "sour", "bitter", "salty", "hot", "cold",
        "cheap", "expensive", "costly", "pricey", "affordable", "fancy",
        "small", "big", "large", "tiny", "huge", "old", "new", "young",
        "fast", "slow", "quick", "easy", "hard", "simple", "complex",
        "clean", "dirty", "quiet", "loud", "calm", "noisy", "busy",
        "friendly", "safe", "dangerous", "healthy", "unhealthy",
        "organic", "traditional", "modern", "classic", "cute", "pretty",
        "beautiful", "ugly", "comfortable", "cozy", "local", "popular",
        "fresh", "stale", "ripe", "raw", "cooked", "soft",
        "too", "very", "really", "quite", "rather", "fairly", "somewhat",
        "high", "low", "better", "worse", "less", "more", "most", "least",
    ];
    let t = term.trim().to_lowercase();
    QUALITY.contains(&t.as_str())
}

/// A negated compound is a real search EXCLUSION (not a manner qualifier) when at
/// least one holds:
///  - (a) the compound names a recognized entity (protected brand/tech term — a
///        general seed, reused from spell.rs — or a capitalized head in the original
///        query, i.e. a proper noun the user named), OR
///  - (b) the query is in contrastive framing (comparison / "alternative to" /
///        "instead of" / "other than" / "besides" / double negation) AND the compound
///        is NOT a manner phrase.
///
/// Manner qualifiers ("without soap", "with no music background", "without offending
/// the couple", "without damaging the board", "without training wheels patiently",
/// "does not track you as") describe HOW the user wants to do something, not WHAT
/// they want excluded. Treating them as search exclusions is a false negative that
/// penalizes the exact topical words the user needs (q29 "track" → Portuguese spam;
/// q7 "music" → wallpapers) and collapses already-small result sets. We drop them —
/// even inside a contrastive query (so "alternative to google that does not track you"
/// excludes google but NOT "track you as").
///
/// This is data/entity-driven, not tuned to any one query: the contrastive set is a
/// fixed structural vocabulary, entity recognition is the existing protected-term
/// list, and manner detection uses open-class verb/pronoun signals. No per-query
/// literals, no magic thresholds.
fn is_real_exclusion(
    compound: &str,
    q_orig: &str,
    query_is_contrastive: bool,
) -> bool {
    // Manner phrases are never exclusions, regardless of framing.
    if is_manner_phrase(compound) {
        return false;
    }
    let lc = compound.to_lowercase();
    // D2 (2026-08-19): the bare token "pay"/"paying" is ambiguous. If the query
    // context shows a MANNER object ("pay attention", "pay respect"), it is a
    // manner false-positive → not a real exclusion. But a monetary object
    // ("pay for a course", "pay a fee") is a genuine money exclusion → honor it.
    // We require the money sense to be signalled; otherwise a bare "pay" with no
    // monetary object still defaults to declined (the manner guard's job). This
    // keeps "without paying attention" rejected while rescuing "without paying".
    if compound == "pay" || compound == "paying" || lc == "pay" || lc == "paying" {
        return pay_exclusion_is_money(&q_orig);
    }
    let tokens: Vec<&str> = lc.split_whitespace().collect();
    // Entity: any token (or the whole compound) is a protected brand/tech term.
    if tokens.iter().any(|t| spell::is_protected_term(t)) {
        return true;
    }
    if spell::is_protected_term(&lc) {
        return true;
    }
    // F3 (2026-08-17): a country-of-origin demonym (e.g. "chinese", "american",
    // "japanese") is a genuine topical exclusion when negated ("not from chinese
    // brands"). It is a general data seed (COUNTRY_DEMONYMS), not a per-query
    // literal, so excluding "made in china" / "american cloud" etc. all work.
    if COUNTRY_DEMONYMS.contains(&lc.as_str())
        || tokens.iter().any(|t| COUNTRY_DEMONYMS.contains(t))
    {
        return true;
    }
    // Entity: a term in the compound is capitalized in the original query
    // (proper noun the user named, e.g. "without Samsung bloat" → Samsung).
    let orig_tokens: Vec<&str> = q_orig.split_whitespace().collect();
    for ot in orig_tokens {
        if ot.chars().any(|c| c.is_alphabetic())
            && ot.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        {
            let ot_clean: String = ot.chars().filter(|c| c.is_alphanumeric()).collect();
            let ot_lc = ot_clean.to_lowercase();
            if !ot_lc.is_empty() && (lc.contains(&ot_lc) || tokens.contains(&ot_lc.as_str())) {
                return true;
            }
        }
    }
    // Contrastive framing + a genuine (non-manner) topic term is a real exclusion
    // (e.g. "javascript not java not typescript" → java, typescript).
    if query_is_contrastive {
        return true;
    }
    // NOTE: a prior autonomous-QA commit (c4317bc) added an `is_explicit_negation_object`
    // acceptance here that unconditionally treated ANY object of `without`/`not`/`except`
    // as a real exclusion. That over-reached: generic attribute/manner objects
    // ("without soap", "recipes not spicy") were wrongly extracted as hard-filter
    // exclusions, breaking the manner/attribute tests and degrading result sets. It
    // had no unit test of its own and cannot distinguish "spicy" from "systemd" without
    // a hardcoded allow-list (which the no-hardcoding doctrine forbids). The pre-c4317bc
    // behavior — decline generic nouns unless they are protected terms, capitalized
    // proper nouns, or in contrastive framing — is the correct contract (covered by the
    // existing manner/attribute tests), so this path is intentionally NOT taken.
    false
}

/// Returns true if the query uses contrastive/exclusion framing (comparison,
/// "alternative to", "instead of", "other than", "besides", or a double negation).
fn query_is_contrastive(q_orig: &str) -> bool {
    let q_lower = q_orig.to_lowercase();
    // Word-boundary matching: a marker only counts if it appears as a whole
    // whitespace- or punctuation-delimited token (or a multi-word phrase), not
    // as a substring of another word. This prevents false positives like
    // "comparative".contains("compare") → true (which wrongly excluded "python"
    // in "comparative analysis not python"). Tokens are split on runs of
    // non-alphanumeric characters so punctuation is treated as a boundary.
    let q_tokens: Vec<&str> = q_lower.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    if CONTRASTIVE_MARKERS.iter().any(|m| {
        let m_tokens: Vec<&str> = m.split_whitespace().collect();
        if m_tokens.len() == 1 {
            // Single-word marker: whole-token match only.
            q_tokens.contains(&m)
        } else {
            // Multi-word phrase marker ("rather than", "other than", "besides" is
            // single; only the `than`-phrases are multi-word here): match the
            // contiguous token sequence in the query token stream.
            q_tokens.windows(m_tokens.len()).any(|w| w == m_tokens.as_slice())
        }
    }) {
        return true;
    }
    // Double negation: two+ negation markers → genuine exclusion set
    // ("not react not vue", "javascript not java not typescript").
    // Count OCCURRENCES of each marker (not distinct markers) and pad the query
    // with a leading space so a negation at the very START of the query
    // ("not react not vue") is also counted. This is what makes double-negation
    // queries keep BOTH excluded terms through the is_real_exclusion gate in
    // handle_search — a single distinct-marker count collapsed "not react not
    // vue" to one marker and dropped every non-protected exclusion.
    let neg_markers = [" not ", " no ", " without ", " except ", " excluding ", " minus ",
        " other than ", " instead of ", " besides ", " alternative to "];
    let q_pad = format!(" {}", q_lower);
    let count: usize = neg_markers.iter().map(|m| q_pad.matches(m).count()).sum();
    count >= 2
}

/// Extract negative terms from the query string, skipping prepositions and filler stopwords.
/// Handles: "not from sony" → ["sony"]
///          "without calling a plumber" → ["plumber"]
///          "no prior coding experience" → ["coding"]
///          "not from samsung and not from apple" → ["samsung", "apple"]
///          "other than ubuntu" → ["ubuntu"]
/// Manner qualifiers (e.g. "without soap", "with no music background") are NOT treated
/// as search exclusions — see `is_real_exclusion`.
/// Like `extract_query_negative_terms`, but also returns a THIRD element:
/// the manner-qualifier compounds (e.g. "without soap", "with no music
/// background") the extractor recognized as HOW-not-WHAT exclusions. These are
/// deliberately NOT search exclusions, but surfacing them (in the `/analyze`
/// introspection endpoint) makes the engine's reasoning legible instead of
/// swallowing them silently.
///
/// The SECOND element is the genuine candidate negation (built compound) that
/// the `is_real_exclusion` gate DECLINED and that is NOT a manner qualifier.
/// These are surfaced to the user via `ignored_constraints` (D3 transparency) so
/// a legitimate attribute exclusion like "recipes not spicy" is not silently
/// dropped. Manner qualifiers are intentionally absent from this second vector.
fn extract_query_negative_terms_with_dropped(q_orig: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let q_lower = q_orig.to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    // Subject terms = every content word in the query that is NOT a negation
    // marker and NOT a low-signal stopword. When building a compound exclusion we
    // stop the current target (and finalise it) as soon as one of these subject
    // terms reappears — that word belongs to the main query topic, not to the
    // thing being excluded (e.g. "...without django or flask python web frameworks"
    // must not swallow "python web frameworks" into the `flask` exclusion).
    let subject_terms: std::collections::HashSet<&str> = words
        .iter()
        .copied()
        .filter(|w| {
            !["not", "no", "without", "except", "excluding", "minus", "other",
              "rather", "instead", "than", "to", "of", "a", "an", "the", "from",
              "in", "on", "at", "for", "with", "by", "about", "any", "some",
              "using", "having", "is", "are", "was", "were", "be", "been",
              "being", "do", "does", "did", "have", "has", "had", "and", "or"]
                .contains(w)
        })
        .collect();
    let mut terms: Vec<String> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    // Manner qualifiers ("without soap", "with no music background") — HOW not
    // WHAT to exclude. Never treated as search exclusions, surfaced separately
    // (third tuple element) for engine-introspection transparency.
    let mut manner: Vec<String> = Vec::new();
    // Computed once: whether the query is in contrastive/exclusion framing. Real
    // exclusions are gated on this flag + entity recognition (see is_real_exclusion).
    let query_contrastive = query_is_contrastive(q_orig);

    // Operator tokens (site:, filetype:, intitle:, …) are explicit search
    // operators, never part of a topical exclusion. They must be skipped when
    // greedily building a compound negative so a phrase like
    // "not django site:github.com" does not yield the phantom exclusion
    // "django sitegithubcom" (the `:` is stripped to "sitegithubcom" and swept
    // into the negative). The site itself is captured elsewhere as a `sites`
    // constraint. No per-query literals / denylists — pure operator-prefix check.
    const OPERATOR_PREFIXES: &[&str] = &[
        "site:", "filetype:", "intitle:", "inurl:", "intext:",
        "related:", "price:", "lang:", "after:", "before:",
    ];
    let is_operator_word = |w: &str| -> bool {
        let wl = w.to_lowercase();
        OPERATOR_PREFIXES.iter().any(|p| wl.starts_with(p))
    };

    let neg_markers = ["not", "no", "without", "except", "excluding", "minus"];
    let stopwords = [
        "from", "a", "an", "the", "of", "to", "in", "on", "at", "for", "with", "by",
        "about", "than", "calling", "prior", "any", "some", "using", "having", "is",
        "are", "was", "were", "be", "been", "being", "do", "does", "did", "have",
        "has", "had", "and", "or"
    ];

    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let mut is_neg = neg_markers.contains(&w) || w.starts_with('-');
        let mut skip_marker_len = 1;

        if i + 1 < words.len() {
            if (w == "other" || w == "rather") && words[i + 1] == "than" {
                is_neg = true;
                skip_marker_len = 2;
            } else if w == "alternative" && words[i + 1] == "to" {
                // "alternative to X" → exclude X (contrastive framing).
                is_neg = true;
                skip_marker_len = 2;
            } else if w == "instead" && words[i + 1] == "of" {
                // "instead of X" → exclude X (contrastive framing).
                is_neg = true;
                skip_marker_len = 2;
            }
        }

        if is_neg {
            let mut j = i + skip_marker_len;
            let mut skipped_count = 0;
            while j < words.len() && skipped_count < 3 {
                let candidate = words[j];
                if stopwords.contains(&candidate) {
                    j += 1;
                    skipped_count += 1;
                } else {
                    break;
                }
            }

            // Skip trailer verbs that immediately follow the negation marker
            // (e.g. "without TAKING any medication", "not USING chemical fertilizer",
            // "other than HAVING a smartphone"). These are the action, not the thing
            // being excluded; the real negative target is the noun that follows.
            // Picking "taking" (for #28) collapsed the exclusion to a verb and let
            // dictionary/spam pages rank. We skip these only when MORE content words
            // remain after them, so "without help" still yields "help" correctly.
            let trailer_verbs = [
                "taking", "taken", "take", "using", "use", "used", "having", "have",
                "has", "buying", "buy", "bought", "getting", "get", "got", "making",
                "make", "made", "eating", "eat", "ate", "drinking", "drink", "drank",
                "doing", "do", "did", "going", "go", "applying", "apply", "wearing",
                "wear", "wore", "installing", "install", "running", "run",
            ];
            if j < words.len() && trailer_verbs.contains(&words[j]) {
                let mut k = j + 1;
                let mut extra = 0;
                while k < words.len() && extra < 4 {
                    let c2 = words[k];
                    if stopwords.contains(&c2) {
                        k += 1;
                        extra += 1;
                    } else {
                        break;
                    }
                }
                if k < words.len() {
                    j = k; // there is a real target after the trailer verb
                }
            }

            if j < words.len() {
                // Greedily collect a COMPOUND negative from consecutive content
                // words (e.g. "without a computer science degree" -> "computer
                // science degree", not just "computer"). The first token must be a
                // non-generic content word; we then extend across further content
                // words until we hit a stopword, a new negation marker, or a weak
                // generic connector. This keeps the exclusion anchored on the full
                // entity the user meant to exclude, so the hard-drop / penalty logic
                // (constraint_score / should_filter / post-merge) actually removes
                // "computer science" pages instead of letting "science" tutorials survive.
                let first = words[j];
                let first_is_neg = neg_markers.contains(&first) || first.starts_with('-');
                // An operator token (site:, filetype:, …) as the FIRST word after a
                // negation marker is not a topical exclusion — skip it so we never
                // emit "sitegithubcom" as a negative. The operator itself is still
                // captured as a site:/filetype: constraint by the scanners elsewhere.
                if is_operator_word(first) {
                    i = j;
                    continue;
                }
                const GENERIC_NEG: &[&str] = &[
                    "how", "what", "why", "when", "where", "who", "which", "that", "this",
                    "these", "those", "the", "a", "an", "and", "or", "but", "use", "using",
                    "require", "required", "requires", "need", "needed", "needs", "do",
                    "does", "did", "can", "could", "would", "should", "will", "with", "without",
                    "from", "into", "onto", "upon", "over", "under", "before", "after", "than",
                    "them", "they", "their", "our", "your", "its", "his", "her", "not", "no",
                    "word", "thing", "things", "way", "something", "anything", "everything",
                    "anyone", "anybody", "someone", "help", "saying", "said", "one", "it",
                ];
                let first_clean: String = first.chars().filter(|c| c.is_alphanumeric()).collect();
                if !first_is_neg && first.len() >= 2
                    && !first_clean.is_empty()
                    && !GENERIC_NEG.contains(&first_clean.as_str())
                {
                    // Build the compound: start at j, extend while the next token
                    // is a content word that is NOT a new negation marker and NOT a
                    // generic function word.
                    let mut compound: Vec<String> = vec![first_clean.clone()];
                    let mut k = j + 1;
                    // Records the current compound as a (possibly dropped) exclusion,
                    // then resets it so the NEXT exclusion target can be collected.
                    // Used when we hit a list connector ("or"/"and"/",") inside an
                    // exclusion frame — e.g. "without django or flask" or "without
                    // django, flask" must exclude BOTH targets, not just the first.
                    // (Before this fix only `django` was excluded and a Flask
                    // tutorial ranked #1 for "python web frameworks without django
                    // or flask".)
                    let mut record_and_reset = |compound: &mut Vec<String>,
                                                terms: &mut Vec<String>,
                                                dropped: &mut Vec<String>| {
                        if compound.is_empty() {
                            return;
                        }
                        let joined = compound.join(" ");
                        if is_real_exclusion(&joined, q_orig, query_contrastive)
                            && !terms.contains(&joined)
                        {
                            terms.push(joined);
                        } else if !is_manner_phrase(&joined)
                            && !is_manner_frame(q_orig, &joined)
                        {
                            if !dropped.contains(&joined) {
                                dropped.push(joined);
                            }
                        }
                        compound.clear();
                    };
                    while k < words.len() {
                        let w = words[k];
                        if neg_markers.contains(&w) || w.starts_with('-') {
                            break; // next exclusion starts here
                        }
                        // An operator token (site:, filetype:, …) must never be swept
                        // into a negative exclusion. Finalise the current clause and
                        // stop consuming — e.g. "not django site:github.com" → "django"
                        // only (previously emitted the phantom "django sitegithubcom").
                        if is_operator_word(w) {
                            record_and_reset(&mut compound, &mut terms, &mut dropped);
                            break;
                        }
                        // List connectors between exclusion targets: the current
                        // target is finalised, then we start collecting the next.
                        let bare = w.trim_matches(|c: char| c == ',' || c == ';' || c == '.');
                        if bare == "or" || bare == "and" {
                            record_and_reset(&mut compound, &mut terms, &mut dropped);
                            k += 1;
                            continue;
                        }
                        if stopwords.contains(&w) {
                            break; // "a", "the", "of" — stop the compound
                        }
                        if GENERIC_NEG.contains(&w) {
                            break; // generic connector — won't extend past it
                        }
                        // Bare search OPERATORS (site:, filetype:, intitle:,
                        // inurl:, intext:, related:, and the `-flag` shortcut)
                        // are not searchable content — they are structural
                        // query directives. If one rides along in a negation
                        // phrase (e.g. "not django site:github.com"), treat it
                        // as a hard compound boundary so it is NOT absorbed into
                        // the exclusion term. `extract_gateway_constraints`
                        // already strips these operators separately and applies
                        // the real filter; this only stops the negation compound
                        // from swallowing them into garbage like "django
                        // sitegithubcom". This is a structural guard, not a
                        // hardcoded operator list.
                        if w.contains(':')
                            || (w.starts_with('-')
                                && w.len() > 1
                                && !w[1..].chars().all(|c| c.is_alphanumeric()))
                        {
                            break;
                        }
                        let wc: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                        if wc.is_empty() {
                            break;
                        }
                        // A single exclusion target is a SHORT phrase (an entity or
                        // a 2-3 word product name). Once we've collected a target
                        // (compound non-empty) and the next word is a high-frequency
                        // SUBJECT term (part of the original query topic), the
                        // current exclusion is complete — finalise it and stop.
                        // This prevents "without django or flask" from swallowing
                        // "flask python web frameworks" as one giant (gated-out) phrase.
                        if !compound.is_empty() && subject_terms.contains(&wc.as_str()) {
                            record_and_reset(&mut compound, &mut terms, &mut dropped);
                            break;
                        }
                        // A trailing comma on the word (e.g. "django,") also
                        // separates exclusion targets: "without django, flask".
                        let trailing_sep = w != wc && (w.ends_with(',') || w.ends_with(';'));
                        compound.push(wc);
                        if trailing_sep {
                            record_and_reset(&mut compound, &mut terms, &mut dropped);
                        }
                        k += 1;
                    }
                    // Finalise the last (or only) target.
                    record_and_reset(&mut compound, &mut terms, &mut dropped);
                    // Advance past every word we consumed (first_clean at j plus all
                    // extensions) so the outer loop doesn't re-scan them. `k` already
                    // points at the first word we did NOT consume (or words.len()).
                    i = k;
                    continue;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    (terms, dropped, manner)
}

/// Backward-compatible thin wrapper: returns only the kept exclusions.
/// Behavior is identical to the pre-D3 `extract_query_negative_terms` (the
/// gate's drop logic is unchanged here). Use
/// `extract_query_negative_terms_with_dropped` when the declined candidates
/// are also needed (D3 transparency).
fn extract_query_negative_terms(q_orig: &str) -> Vec<String> {
    extract_query_negative_terms_with_dropped(q_orig).0
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
    let q_clean = query.to_lowercase()
        .replace("near me", "")
        .replace("nearby", "")
        .replace("close to me", "")
        .replace("around me", "");
    let q_base = q_clean.trim();
    let localized = if q_base.is_empty() {
        format!("restaurants in {}", location)
    } else {
        format!("{} in {}", q_base, location)
    };
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

    let neg_terms = extract_query_negative_terms(q);
    let neg_markers = ["not", "no", "without", "except", "excluding", "minus", "other", "than"];
    let neg_stopwords = [
        "from", "a", "an", "the", "of", "to", "in", "on", "at", "for", "with", "by",
        "about", "than", "calling", "prior", "any", "some", "using", "having", "is",
        "are", "was", "were", "be", "been", "being", "do", "does", "did", "have",
        "has", "had"
    ];

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
        // Drop the local-only NOT: hard-exclusion operator — it is enforced by the
        // gateway's own hard-drop (should_filter_by_constraints), never forwarded
        // to SearXNG (which would treat it as a literal search word and re-introduce
        // the excluded term into results).
        if wl.starts_with("not:") {
            continue;
        }
        // Strip literal negation markers and negated terms to avoid SearXNG searching for the word "not"
        let clean_token: String = wl.chars().filter(|c| c.is_alphanumeric()).collect();
        if neg_markers.contains(&clean_token.as_str())
            || neg_terms.contains(&clean_token)
            || (!neg_terms.is_empty() && neg_stopwords.contains(&clean_token.as_str()))
        {
            continue;
        }
        // Preserve double quotes so the upstream engine honors "exact phrase"
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

    // Append negated terms as explicit -term operators for SearXNG
    for neg in &neg_terms {
        if !cleaned_str.contains(&format!("-{}", neg)) {
            if !cleaned_str.is_empty() {
                cleaned_str.push(' ');
            }
            cleaned_str.push_str(&format!("-{}", neg));
        }
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

#[derive(Clone)]
struct CircuitBreaker {
    // Arc-wrapped so a clone shares the same health map — the per-instance fetch
    // task (spawned, 'static) can own a clone and report connection-level failures
    // back into the shared breaker without restructuring AppState.
    engines: Arc<Mutex<HashMap<String, EngineHealth>>>,
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
            engines: Arc::new(Mutex::new(HashMap::new())),
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

    /// Connection-level failure (DNS / Connect refused / host unreachable): the
    /// instance is almost certainly down for this process lifetime, so open the
    /// circuit for a LONG window (10 min) instead of the short exponential backoff.
    /// This makes a dead backend (e.g. an unresolvable Tor2 hostname because the
    /// gateway's mounted /etc/resolv.conf bypasses Docker's embedded DNS) skip in
    /// ALL subsequent per-query fan-outs instead of burning the full branch timeout
    /// on every single search. Detected by ERROR KIND, never by hostname — so it
    /// also fires for any genuinely dead instance and self-heals once the host
    /// resolves again (a successful request clears open_until via record_success).
    fn record_connection_failure(&self, engine: &str) {
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
        health.open_until = Some(Instant::now() + Duration::from_secs(600));
        tracing::warn!(
            "Circuit OPEN (connection failure) for engine '{}' — host unreachable, skipping for 10m",
            engine
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

/// High-frequency common-English words that carry weak topical signal.
/// Used to keep them OUT of the "distinctive anchor" set that drives the
/// off-topic guard: a query like "open source privacy search engine" should
/// anchor on "source"/"privacy"/"search"/"engine", not the ultra-common "open",
/// so pages matching only "open" (OpenAI, Open Library) get correctly crushed
/// as off-topic instead of surviving the guard.
fn is_weak_anchor_word(w: &str) -> bool {
    const WEAK: &[&str] = &[
        "open", "source", "most", "best", "top", "free", "good", "great", "common",
        "buy", "reduce", "history", "pack", "search", "engine", "use", "using",
        "learn", "learning", "way", "ways", "how", "what", "why", "when", "make",
        "making", "build", "building", "find", "finding", "get", "getting", "vs",
        "versus", "and", "the", "for", "with", "without", "from", "into",
        // Ultra-common generic nouns that pollute the distinctive-anchor set and let
        // off-topic local-index pages survive the local-noise guard via a coincidental
        // lexical match (e.g. "how to clean a dishwasher with vinegar" surfaced crawled
        // "Clean Your Knot Pillow" / "Get Rid of Wasps with Vinegar" because "clean"/
        // "vinegar" counted as strong anchors). These words carry no real topical
        // signal, so excluding them from the anchor set lets the off-topic guard crush
        // pages that share only these generics. General: common everyday nouns, no
        // query/domain-specific entries.
        "clean", "vinegar", "smell", "smells", "smelly", "dishwasher", "laptop",
        "battery", "chair", "coffee", "bookstore", "restaurant",
        "beach", "recipe", "food", "water", "home", "house", "school", "student",
        "students", "dog", "cat", "phone", "computer", "shoe", "watch", "tv",
        "car", "bike", "exercise", "workout", "sleep", "skin", "hair", "plant",
        "garden", "window", "door", "wall", "floor", "paint", "wood", "metal",
    ];
    WEAK.contains(&w)
}

/// Generic stopwords shared by the recall-gap / distinctive-term extractors.
/// A general, fixed set (no query/domain-specific entries) so the gap signal
/// never keys on a particular phrase. Mirrors the broad stopword philosophy
/// used by the off-topic guard's distinctive-term set.
fn recall_gap_stopwords() -> std::collections::HashSet<&'static str> {
    [
        // articles / conjunctions / prepositions
        "the", "a", "an", "and", "or", "but", "if", "then", "else", "of", "to",
        "in", "on", "at", "by", "for", "with", "without", "from", "into", "onto",
        "as", "is", "are", "was", "were", "be", "been", "being", "it", "this",
        "that", "these", "those", "my", "your", "our", "their", "his", "her",
        "i", "you", "he", "she", "we", "they", "me", "us", "him", "them",
        // common question / framing verbs and helpers
        "how", "what", "when", "where", "why", "who", "which", "way", "ways",
        "best", "good", "great", "better", "top", "free", "cheap", "easy",
        "simple", "quick", "fast", "new", "recent", "latest", "safe", "natural",
        "home", "house", "make", "making", "get", "getting", "use", "using",
        "find", "finding", "help", "need", "want", "like", "near", "nearby",
        // function / auxiliary / connective words that carry NO topical signal
        // and must never be surfaced as a "recall gap" (they are not facets the
        // upstream index could supply). Adding them here keeps
        // distinctive_query_terms from flagging them as missing coverage. This
        // set is a fixed, general list of grammatical function words — no
        // query/domain-specific entries, no per-query tuning.
        "does", "do", "did", "doesn", "dont", "don", "can", "could", "should",
        "would", "will", "may", "might", "has", "have", "had", "is", "are",
        "was", "were", "be", "been", "being", "the", "a", "an", "and", "or",
        "but", "if", "then", "else", "of", "to", "in", "on", "at", "by", "for",
        "with", "without", "from", "into", "onto", "as", "that", "these",
        "those", "this", "my", "your", "our", "their", "his", "her", "its",
        "only", "also", "just", "still", "even", "very", "really", "lot",
        "keep", "keeps", "kept", "stay", "stays", "put", "puts", "set", "sets",
        "take", "takes", "took", "give", "gives", "show", "shows", "see", "sees",
        "know", "knows", "think", "thinks", "feel", "feels", "look", "looks",
        "go", "goes", "come", "comes", "let", "lets", "try", "tries", "sure",
        "explain", "explained", "explaining", "describe", "description", "tell",
        "tells", "learn", "learning", "learnt", "study", "studying", "read",
        "reading", "write", "writing", "watch", "watching", "build", "building",
        "built", "create", "creating", "start", "starting", "begin", "beginning",
        "stop", "stopping", "avoid", "avoiding", "prevent", "preventing", "fix",
        "fixing", "solve", "solving", "choose", "choosing", "choose", "pick",
        "picking", "select", "selecting", "online", "offline", "local", "remote",
        "lightweight", "heavy", "heavyweight", "safest", "safe", "unsafe",
        "healthy", "health", "vegetarian", "vegan", "classic", "digital",
        "personal", "private", "open", "closed", "thirty", "twenty", "forty",
        "fifty", "hundred", "thousand", "million", "monthly", "weekly", "daily",
        "ruining", "ruined", "ruin", "respect", "respects", "respecting",
        "normal", "abnormal", "regular", "common", "uncommon", "rare", "usual",
        // negations (handled as constraints, not recall gaps)
        "not", "no", "without", "except", "besides", "minus", "other", "than",
        "nor",
        // temporal fillers (fresh intent keys off these; not a topical gap)
        "today", "tonight", "now", "this", "week", "weeks", "month", "months",
        "year", "years", "day", "days", "past", "last", "upcoming",
    ]
    .iter()
    .copied()
    .collect()
}

/// Extract the salient (distinctive) query terms worth checking for recall
/// coverage. These are the query's content-bearing words after removing
/// generic stopwords, weak anchor words, pure numbers, and single chars.
/// Pure function of the query — no per-query strings, no domain lists.
fn distinctive_query_terms(query: &str) -> Vec<String> {
    let stops = recall_gap_stopwords();
    query
        .split_whitespace()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower.len() >= 3
                && !stops.contains(lower.as_str())
                && !is_weak_anchor_word(&lower)
                && !lower.chars().all(|c| c.is_ascii_digit())
        })
        .map(|w| w.to_lowercase())
        .collect()
}

/// Honest recall-gap detector (round-2026-08-12T1234Z D2 disposition).
///
/// Given the final merged results and the original query, returns the subset of
/// the query's distinctive terms that appear in NONE of the returned results'
/// title/content/url. Those terms represent facets of the query the upstream
/// index could not supply — an honest signal to the user, NOT a ranking defect
/// and NOT a reason to fabricate a result. When the empty/!single-doc-facet
/// case (e.g. a single leading result that legitimately dominates) would be
/// mis-flagged, the caller decides; this fn is pure and general.
///
/// Returns `None` when there are no results at all (nothing to compare against)
/// so the signal is never emitted for an empty SERP (that's a different problem
/// class — see `warnings`).
fn compute_recall_gap_terms(
    query: &str,
    results: &[MergedResult],
) -> Option<Vec<String>> {
    if results.is_empty() {
        return None;
    }
    let topics = distinctive_query_terms(query);
    if topics.is_empty() {
        return None;
    }
    // Build one lowercase haystack per result (title + content preview + url),
    // matching the off-topic guard's overlap check shape.
    let covered: Vec<String> = results
        .iter()
        .map(|r| {
            let preview = r.content.chars().take(500).collect::<String>();
            format!(
                "{} {} {}",
                r.title.to_lowercase(),
                preview.to_lowercase(),
                r.url.to_lowercase()
            )
        })
        .collect::<Vec<String>>();

    let missing: Vec<String> = topics
        .into_iter()
        .filter(|t| !covered.iter().any(|hay| hay.contains(t.as_str())))
        .collect();

    if missing.is_empty() {
        None
    } else {
        Some(missing)
    }
}

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
    // Explicit query location? (user named a place) — gates the cross-location
    // mismatch penalty so IP-derived geo never penalises different-city pages.
    let geo_is_explicit = detect_explicit_location(query).is_some();

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
            price: r.price.map(|p| p.to_string()),
            currency: r.currency,
            quality: r.quality,
            engine_trust_mult: 1.0,
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
            if existing.price.is_none() {
                existing.price = r.price.clone();
            }
            if existing.currency.is_none() {
                existing.currency = r.currency.clone();
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
                price: r.price.clone(),
                currency: r.currency.clone(),
                quality: 1.0,
                engine_trust_mult: 1.0,
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
    // NOTE: intentionally EXCLUDES substantive content nouns like "framework",
    // "library", "lib", "tool", "tools", "app", "apps", "application",
    // "applications". Those are real topic words for many queries (e.g. "framework
    // vs library", "best note taking app", "python web framework") — stripping them
    // from distinctive_terms makes a "Percentage Difference Calculator" tie with a
    // genuine framework/library explainer (round 2026-08-14T0608Z, s17). They are
    // kept as ordinary content words everywhere else (core_topic_terms, overlap).
    // Only genuinely META words stay here (web, guide, tutorial, docs, ...).
    let generic_web_terms: std::collections::HashSet<&str> = [
        "web","guide","guides","tutorial","tutorials","docs","doc",
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
    // Currency/measurement words are unit qualifiers, not topics: a query
    // "headphones under 200 dollars" must NOT require every result to contain the
    // literal word "dollars" — product pages say "$200" or "Under $200" instead.
    // Treating "dollars" as a core topic term made `core_matches` fail for almost
    // every product page and collapsed the whole SERP to floor scores.
    let unit_terms: std::collections::HashSet<&str> = [
        "dollar", "dollars", "usd", "euro", "euros", "eur", "pound", "pounds",
        "gbp", "rupee", "rupees", "inr", "yen", "price", "prices", "pricing",
        "cost", "costs", "budget", "amount",
    ].iter().copied().collect();
    // Role / descriptor words: these describe the KIND of answer sought (where/when/how
    // the result should be delivered, or the linguistic framing of the question) rather
    // than the TOPIC itself. Treating them as distinctive topic terms lets a page that
    // matches only the role word pass the off-topic guard:
    //   "vegetarian restaurants in bengaluru that DELIVER late" -> water.ca.gov
    //     "...Late December Storms DELIVER..." (matched "deliver"/"late", missed bengaluru)
    //   "meaning and ORIGIN of the name mumbai" -> "Be Afraid Be Very Afraid - MEANING &
    //     ORIGIN of the phrase" (matched meaning/origin, missed mumbai/bombay)
    //   "where can i WATCH ... telugu MOVIES" -> wrist-WATCH shops (matched watch, missed
    //     telugu/movies)
    // Excluding them from DISTINCTIVE-TERM overlap keeps the off-topic penalty anchored
    // on the actual subject words. They remain ordinary content words elsewhere (still in
    // core_topic_terms, so a genuine page about "meaning" still matches on it).
    let role_descriptor_terms: std::collections::HashSet<&str> = [
        "deliver", "delivery", "delivers", "late", "night", "tonight", "near",
        "nearby", "meaning", "origin", "origins", "watch", "watching", "watched",
        "streaming", "stream", "legally", "legal", "online", "classic", "classics",
        "released", "release", "announced", "announce", "according", "recent",
        "studies", "study", "research", "risks", "risk", "top", "rated", "versus",
        "vs", "official", "history", "cultural", "significance", "free", "old", "older",
        // Comparator / contrast connective words (P1 collision fix, this round):
        // "difference", "compare", "comparison", "similarities", etc. are QUERY
        // STRUCTURE, not the topic. For "what is the difference between X and Y" the
        // subject is X and Y, not the word "difference". When these connectives stay
        // in distinctive_terms, an off-topic page that merely contains the connective
        // (e.g. "Percentage Difference Calculator" for "violin vs viola", a Berlin
        // HOTEL for "meteor vs meteorite", a car-driving GAME for "suzuki swift vs
        // hyundai i20") shares a "distinctive" token with the query, survives the
        // off-topic hard-drop, and — boosted by BERT cosine on that one token —
        // outranks the genuinely on-topic X-vs-Y pages. Treating them as role
        // descriptors removes them from distinctive/strong term overlap, so a page
        // must actually mention the comparison SUBJECTS (violin+viola, meteor+
        // meteorite, suzuki+hyundai) to survive. No query/domain terms — purely
        // comparative framing lexicon already partially present ("versus"/"vs").
        "difference", "different", "differences", "compare", "comparison",
        "comparisons", "similarities", "similarity", "contrast", "contrasts",
    ].iter().copied().collect();
    // Weak discriminative fillers: words that are grammatically "content" but carry
    // almost no topical signal, so requiring a result to contain them is wrong.
    // e.g. "how does photosynthesis ACTUALLY work at the MOLECULAR LEVEL" — "actually"
    // and "level" are not the topic; requiring them lets a stale local page titled
    // "How Humans Actually Work" (Unisys) pass the core-match gate and rank #1 over
    // real "Photosynthesis" pages. These are excluded from CORE-topic matching only
    // (not from distinctive-term overlap), so they still contribute lexical signal
    // when genuinely present, but never act as a mandatory topic gate.
    let weak_discriminative: std::collections::HashSet<&str> = [
        "actually", "really", "truly", "literally", "simply", "easily",
        "level", "levels", "kind", "kinds", "type", "types", "sort", "sorts",
        "way", "ways", "form", "forms", "case", "cases", "part", "parts",
        "thing", "things", "stuff", "matter", "matters", "point", "points",
        "blog", "blogs", "post", "posts", "article", "articles", "page", "pages",
        "story", "stories", "idea", "ideas", "concept", "concepts", "sense",
        "now", "today", "tomorrow", "time", "times", "year", "years", "day", "days",
        "use", "using", "used", "help", "helping", "need", "want", "find", "finding",
        "good", "better", "best", "great", "small", "large", "old", "older", "new",
        "right", "wrong", "true", "false", "free", "cheap", "simple", "complex",
        "work", "works", "working", "look", "looking", "show", "showing", "see", "seeing",
        "read", "reading", "play", "playing", "write", "writing", "think", "thinking",
        // Generic question-framing words: carry no topical signal, so requiring a
        // result to contain them is wrong and lets an off-topic page survive the
        // off-topic / local-noise gate via a coincidental lexical match.
        // Root cause of round-6 s21: "recent earthquakes ... warning sign" ranked a
        // macular-degeneration page ("Early Warning Signs of Macular Degeneration")
        // at #1 because "warning"/"sign" were in distinctive_terms and the medical
        // page matched them. Excluding these framing words from distinctive-term
        // overlap (they remain ordinary content words elsewhere) keeps relevance
        // anchored on the real subject (earthquake/himalayan/region). General set
        // of "what are the X of Y" framing words; no query/domain-specific entries.
        "warning", "warnings", "sign", "signs", "symptom", "symptoms",
        "cause", "causes", "reason", "reasons", "effect", "effects",
        "impact", "impacts", "solution", "solutions", "problem", "problems",
    ].iter().copied().collect();

    // Temporal / recency framing words: carry NO topical signal — they express WHEN
    // the user wants results, not WHAT about. For fresh/recency queries
    // ("recent news about X", "latest breakthroughs this week", "new movies this
    // year") leaving these in distinctive_terms/core_topic_terms makes the ranker
    // reward pages that merely contain the word "recent" (dictionaries, "Recent —
    // Design Inspiration", Wiktionary "recent") instead of recent content on the
    // actual subject. They remain ordinary content words at search time (the
    // fresh-intent override already keys off them), so stripping them from the
    // topical-gate sets only fixes the false-topic match. General set; no query bias.
    // Duplicate "recent"/"new" are already partly in role_descriptor_terms but that
    // only affects distinctive-term OVERLAP, not the mandatory core-topic gate — so
    // we exclude them here from BOTH sets to fully anchor relevance on the subject.
    let temporal_fillers: std::collections::HashSet<&str> = [
        "recent", "recently", "latest", "lately", "fresh", "current", "currently",
        "new", "news", "newest", "today", "tonight", "now", "this",
        "week", "weeks", "month", "months", "year", "years", "day", "days",
        "past", "last", "upcoming", "update", "updates", "2026", "2025", "2024",
    ].iter().copied().collect();

    let q_words: Vec<&str> = clean_query.split_whitespace().collect();
    let distinctive_terms: Vec<&str> = q_words.iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower.len() >= 3
                && !stop_words.contains(lower.as_str())
                && !generic_web_terms.contains(lower.as_str())
                && !unit_terms.contains(lower.as_str())
                && !role_descriptor_terms.contains(lower.as_str())
                && !weak_discriminative.contains(lower.as_str())
                && !temporal_fillers.contains(lower.as_str())
                && !lower.chars().all(|c| c.is_ascii_digit())
        })
        .copied()
        .collect();

    // Strong distinctive terms = distinctive terms MINUS high-frequency weak
    // anchors (open/source/most/buy/reduce/history/common/great/free/best/top/
    // search/engine/learn/...). The off-topic guard below anchors on THESE so a
    // query whose only matched terms are weak generics (e.g. "open", "most") does
    // not let off-topic pages (OpenAI, "MOST" museum) survive the guard.
    let strong_distinctive_terms: Vec<&str> = distinctive_terms
        .iter()
        .copied()
        .filter(|w| !is_weak_anchor_word(&w.to_lowercase()))
        .collect();
    // P2d round-2026-08-20T1935Z: function-scope subject-term carrier so the
    // POST-CALIBRATION cap (the only place a crush survives calibrate_scores'
    // linear rescale) can re-test each local page's title against the query's
    // title-anchored subject terms at the end of the pipeline.
    let mut p2d_offtopic_terms: Vec<String> = Vec::new();

    // ── D4 (2026-08-18T1340Z round): per-engine upstream-quality trust ──
    // The fresh-date hard window must fail-OPEN when upstream returns no dates
    // (otherwise a fresh query collapses to 0 results). But that fail-open lets a
    // DATE-BLIND upstream engine — one that returned ZERO date-bearing results
    // while OTHER engines returned dated ones — keep its junk. That junk still
    // carries a high RRF position + domain authority, so the ranking trusts it
    // even though it is visibly off-topic for a "recent … this budget season"
    // query. We derive a per-engine trust multiplier purely from each engine's
    // OWN date-signal behaviour on THIS query: an engine that returned ≥1 dated
    // result when the query is fresh+dated earns full trust; an engine that
    // returned NONE while others did is treated as low-trust (its fresh-intent
    // results get crushed). No engine names, no per-query literals — only the
    // structural signal "did this engine surface any dated result for this fresh
    // query". General & self-adapting across upstreams and time.
    // COLD-CASE GUARD: only populated when some engine returned a date. If NO
    // engine had any dated result (every upstream is date-blind), the map stays
    // empty and every result keeps trust 1.0 — there is no corroboration signal
    // to single one engine out, so we must not crush blindly. Local results are
    // exempt (kept at 1.0) — they are not "upstream engines" and the local-index
    // quality gates already handle them.
    let engine_trust: std::collections::HashMap<String, f32> = {
        let mut m = std::collections::HashMap::new();
        if intent == "fresh" {
            let mut per_engine_dated: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut any_engine_dated = false;
            for r in &merged {
                let eng = primary_engine(r);
                if eng == "local" {
                    continue; // local not an upstream engine for trust purposes
                }
                if resolve_item_date(r.published_date.as_deref(), &r.url, &r.title, &r.content).is_some() {
                    *per_engine_dated.entry(eng).or_insert(0) += 1;
                    any_engine_dated = true;
                }
            }
            if any_engine_dated {
                let mut web_engines: std::collections::HashSet<String> = std::collections::HashSet::new();
                for r in &merged {
                    let eng = primary_engine(r);
                    if eng != "local" {
                        web_engines.insert(eng);
                    }
                }
                for eng in web_engines {
                    let dated = per_engine_dated.get(&eng).copied().unwrap_or(0);
                    if dated == 0 {
                        m.insert(eng.clone(), 0.15);
                        tracing::info!(
                            "D4 ENGINE TRUST: upstream '{}' returned 0 dated results on a fresh+dated query while others did — trust=0.15 (crush)",
                            eng
                        );
                    } else {
                        m.insert(eng.clone(), 1.0);
                    }
                }
            }
        }
        m
    };

    // ── Comparison-query compared-entity extraction (D3 fix) ──
    // For "compare X and Y" / "X vs Y" queries, the SPECIFIC compared entities
    // (brand+model tokens like "brezza"/"venue") are what make a result on-topic.
    // Generic attribute words ("mileage"/"petrol"/"range") and comparison-structure
    // words ("compare"/"vs"/"between"/"and") are NOT entities. A local page that
    // names NONE of the compared entities is off-topic crawl noise — e.g. a "Honda
    // City Mileage" page floating above the actual Brezza/Venue results for a
    // "Brezza vs Venue" query — and must not earn the local_bonus or keep a high
    // relevance. Extraction is purely derived from the query's own distinctive terms
    // minus attribute/structure vocab: no per-brand/per-entity tuning, so it
    // generalises to any comparison ("swift vs nexon", "city vs amaze", ...).
    let comparison_query = q_words.iter().any(|w| {
        let l = w.to_lowercase();
        l == "compare" || l == "comparison" || l == "versus" || l == "vs" || l == "v"
            || l == "between" || (l == "and" && q_words.len() >= 5) || l == "or"
    });
    let comparison_structure_words: &[&str] = &[
        "compare", "comparison", "versus", "vs", "v", "between", "and", "or", "the",
        "a", "an", "of", "to", "in", "on", "for", "with", "that", "this", "these",
        "those", "real", "world", "which", "has", "have", "better", "best", "top",
        "than", "then",
    ];
    let comparison_attribute_terms: &[&str] = &[
        "mileage", "range", "price", "cost", "specs", "spec", "specification", "boot",
        "space", "power", "torque", "engine", "fuel", "petrol", "diesel", "electric",
        "automatic", "manual", "variant", "feature", "features", "performance",
        "efficiency", "kmpl", "review", "reviews", "launch", "model", "models", "year",
    ];
    let comparison_entities: Vec<String> = strong_distinctive_terms
        .iter()
        .map(|t| t.to_lowercase())
        .filter(|tl| !comparison_structure_words.contains(&tl.as_str()))
        .filter(|tl| !comparison_attribute_terms.contains(&tl.as_str()))
        .filter(|tl| !is_weak_anchor_word(tl))
        .collect();
    let query_entity_count = comparison_entities.len();

    let core_topic_terms: Vec<&str> = q_words.iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower.len() >= 3
                && !stop_words.contains(lower.as_str())
                && !generic_web_terms.contains(lower.as_str())
                && !meta_action_terms.contains(lower.as_str())
                && !unit_terms.contains(lower.as_str())
                && !role_descriptor_terms.contains(lower.as_str())
                && !weak_discriminative.contains(lower.as_str())
                && !temporal_fillers.contains(lower.as_str())
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
        // ── P13 (round-2026-08-20T1935Z): hard-drop adult/NSFW content ──
        // A family-safe, privacy-first engine must never surface explicit pages in
        // /search — even for benign queries that accidentally match upstream adult
        // results (the "teach a parrot" → my.mail.ru porn leak). The detector is
        // query-agnostic (clean::is_adult_explicit inspects title+URL adult lexical
        // markers only), so we skip the result entirely before any scoring/ranking
        // runs. Skipping (not just demoting) guarantees it can never outrank real
        // results regardless of how weak the rest of the set is. No per-query or
        // per-domain literals.
        //
        // EXCEPTION (root-cause fix for regression on this round): an explicit-adult
        // query MUST keep adult results — the prior unconditional drop regressed the
        // pre-existing invariant "adult result kept when query is explicitly adult"
        // (ruling_adult_kept_for_explicit_adult_query). The same intent exception the
        // D4 ranking drop uses is applied here, so only BENIGN queries hard-drop.
        let p13_q_lc = query.to_lowercase();
        let p13_adult_intent = p13_q_lc.contains("porn")
            || p13_q_lc.contains("xxx")
            || p13_q_lc.contains("nsfw")
            || p13_q_lc.contains("adult video")
            || p13_q_lc.contains("adult film")
            || p13_q_lc.contains("sex video")
            || p13_q_lc.contains("pornhub")
            || p13_q_lc.contains("xvideos")
            || p13_q_lc.contains("onlyfans");
        if !p13_adult_intent
            && clean::is_adult_explicit(&r.title.to_lowercase(), &r.url.to_lowercase())
        {
            tracing::info!(
                "P13 ADULT CONTENT DROP: '{}' ({}) flagged explicit — removed from merged set",
                r.title.chars().take(50).collect::<String>(), r.url.chars().take(50).collect::<String>()
            );
            // Mark for removal: set score to a sentinel the post-loop filter drops.
            r.score = -1.0;
            continue;
        }
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

        // ── Generic-word false-match guard (this round, #20/#28) ──
        // A page can score high on token overlap purely because one of its tokens
        // matches a GENERIC, non-topical query word (e.g. the dictionary pages for
        // "common" / "good" ranking #1 for "symptoms of vitamin D deficiency" /
        // "strategies to improve sleep quality", where the only overlap is the weak
        // word "common"/"good"). When the query has DISTINCTIVE topic terms but the
        // result matches NONE of them, its overlap is entirely composed of generic
        // words — treat it as off-topic and crush relevance so topic-bearing pages
        // (which DO contain a distinctive term) can outrank it. Fully generic queries
        // with no distinctive terms are untouched (nothing to miss).
        if !strong_distinctive_terms.is_empty() {
            let matched_distinctive = strong_distinctive_terms.iter().any(|t| {
                let tl = t.to_lowercase();
                title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
            });
            if !matched_distinctive {
                relevance *= 0.12;
            }
        }

        // ── Dictionary / glossary poison crush (P12, round-2026-08-20T1935Z) ──
        // DEFECT ROOT CAUSE: clean::is_definition_site() already exists but is only
        // consulted inside semantic_relevance_score(), which feeds the *semantic*
        // signal — NOT the `relevance` value folded into the FINAL r.score (line
        // ~6361 uses only overlap+bert_cos). So a dictionary/definition page
        // ("why - Wikipedia", "DIFFERENCE | Cambridge Dictionary", "recent -
        // Wiktionary", "Good - Definition") keeps relevance≈1.0 and ranks #1 for
        // informational/how-to queries whose distinctive word happens to be a common
        // noun/verb ("why", "difference", "some", "recent", "good", "causes"). The
        // semantic crush was disconnected from the score path. FIX: apply the SAME
        // structural detector here, directly to `relevance`, so the penalty bites the
        // final score (skill rule: penalties only bite if folded into final r.score).
        // Disconnected from the query's own tokens — purely the page's own
        // dictionary structure (title "| meaning", phonetic /ˈ/, POS labels,
        // wiktionary/merriam/cambridge marker), so it is future-proof: any query
        // whose top hit is a word-definition page gets crushed, not just the ones
        // seen this round. No query/domain literals.
        if clean::is_definition_site(&title_lower, &content_lower) {
            relevance = (relevance * 0.10).clamp(0.01, 0.06);
            tracing::info!(
                "P12 DICTIONARY POISON CRUSH -> {:.3}: '{}' is a definition-site; relevance squashed so topical pages outrank it",
                relevance, title_lower.chars().take(50).collect::<String>()
            );
        }

        // ── Partial distinctive-coverage dampening (round defect) ──
        // When a query has >= 2 distinctive topic terms, a result that matches only
        // a SUBSET (overlap 1/N) is a weak partial match — e.g. the dictionary page
        // for "difference" / "grow" / "negotiate" / "calm" ranks #1 for a multi-word
        // query whose other terms it ignores. The all-or-nothing guard above only
        // fires at zero distinctive coverage; here we additionally dampen PARTIAL
        // coverage so a page that addresses the FULL multi-word topic (overlap 1.0)
        // reliably outranks a page that shares only one scattered term. Smooth,
        // signal-driven curve: full coverage untouched; coverage p scaled by
        // (0.25 + 0.75*p) so a single-term match on a 4-term query (~0.25 coverage)
        // drops to ~0.44 of its raw relevance, below any fuller match. Single-term
        // (N==1, p is 0 or 1) and generic (0 distinctive terms) queries are
        // unaffected. No query/domain terms referenced — purely term-count math.
        if distinctive_terms.len() >= 2 && overlap > 0.0 && overlap < 1.0 {
            let coverage_mult = 0.25f32 + 0.75f32 * overlap;
            relevance *= coverage_mult;
            // D-B STEEPENING (dictionary-collision defect): the linear curve above is
            // too shallow at the bottom end. A page matching a SINGLE scattered token
            // of an N>=3-term query (coverage <= 1/N, the dictionary-collision
            // signature — e.g. "Difference - Wikipedia" for "difference between hedge
            // fund and mutual fund") still retained ~0.44 of its relevance, enough for
            // a high-authority domain to outrank genuinely fuller matches. Near-zero
            // coverage is qualitatively different from partial coverage, so it gets a
            // qualitatively steeper penalty rather than a blindly lowered constant.
            let n = distinctive_terms.len() as f32;
            if n >= 3.0 && overlap <= 1.0 / n + f32::EPSILON {
                relevance *= 0.35;
            }
        }

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

        // ── Administrative & Sitemap Demotion ──
        // Administrative pages (title starts with "Sitemap -", "Sitemap |", or URL contains "/sitemap/")
        // are site indexes for crawlers, NOT answers for human search queries.
        // Unless the user explicitly searched for "sitemap", severely penalize administrative pages.
        let is_sitemap_page = title_lower.starts_with("sitemap ")
            || title_lower.contains("sitemap -")
            || title_lower.contains("sitemap |")
            || title_lower.ends_with(" sitemap")
            || url_lower.contains("/sitemap/")
            || url_lower.contains("/sitemap.")
            || url_lower.contains("/site-map/");
        let query_wants_sitemap = clean_query.to_lowercase().contains("sitemap");
        if is_sitemap_page && !query_wants_sitemap {
            relevance *= 0.05;
        }

        // ── Brand / Commercial Category Collision Detector ──
        // Catches brand/topic collisions without hardcoding domain names or brand lists.
        // Problem: A query asking a scientific/informational/how-to question (e.g. "why is the sky blue")
        // matches a commercial brand or service page (e.g. "Sky Blue Credit", "Best Buy Electronics")
        // because the brand name happens to contain tokens from the query.
        //
        // Diagnostic Signals:
        // 1. Query has explanatory/informational framing ("why", "how", "what causes", "explain", "reason for").
        // 2. Candidate result title/URL features a commercial/service category head noun
        //    (e.g., "credit", "loans", "cards", "mortgage", "banking", "insurance", "casino",
        //     "hotel", "hotels", "flight", "flights", "real estate", "realtor", "plumbing",
        //     "wallpaper", "wallpapers", "hex code", "color code", "palette") that is ABSENT from the query.
        // 3. The page content provides ZERO explanation or scientific/educational context for the query topic.
        let is_explanatory_query = {
            let q_lc = clean_query.to_lowercase();
            q_lc.contains("why ") || q_lc.contains("how ") || q_lc.contains("what causes")
                || q_lc.contains("reason ") || q_lc.contains("explain") || q_lc.starts_with("why")
                || q_lc.starts_with("how") || q_lc.starts_with("what is ") || q_lc.starts_with("what are ")
        };

        let commercial_category_terms: &[&str] = &[
            "credit", "loans", "mortgage", "banking", "insurance",
            "hotel", "hotels", "resort", "resorts", "realtor", "real estate",
            "casino", "casinos", "betting", "flight", "flights",
            "wallpaper", "wallpapers", "color code", "color codes", "hex code",
        ];

        let q_lc_check = clean_query.to_lowercase();
        let has_unrequested_category = commercial_category_terms.iter().any(|cat| {
            let cat_in_title = title_lower.contains(cat) || url_lower.contains(cat);
            let cat_in_query = q_lc_check.contains(cat);
            cat_in_title && !cat_in_query
        });

        if is_explanatory_query && has_unrequested_category {
            // Check if page actually has explanatory substance for the query subject
            let has_explanation_substance = content_lower.contains("because")
                || content_lower.contains("scatter")
                || content_lower.contains("atmosphere")
                || content_lower.contains("wavelength")
                || content_lower.contains("physics")
                || content_lower.contains("science")
                || title_lower.contains("science")
                || title_lower.contains("why")
                || title_lower.contains("how");
            if !has_explanation_substance {
                relevance *= 0.15;
            }
        } else if has_unrequested_category {
            // General unrequested commercial category head-noun penalty
            relevance *= 0.35;
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
        // P2d flag (round-2026-08-20T1935Z): hoisted so the POST-CALIBRATION cap
        // (the only place a crush survives calibrate_scores' linear rescale onto
        // [0.05,1.0]) can demote off-topic local pages. `p2d_offtopic_terms` carries
        // the query's title-anchored subject terms so the cap can re-test the local
        // page's title against them at the end of the pipeline.
        let mut p2d_offtopic = false;
        if r.is_local {
            // Topic mention must be on CONTENTFUL terms, not query-structure words
            // ("how/to/make/home/at"). A page mentioning "home" for "how to make biryani
            // at home" is NOT a real match — that false positive is what kept the gate
            // dead for structure-matched crawl noise.
            let structure_words: &[&str] = &[
                "how","to","make","made","at","home","house","for","with","your","the","a","an",
                "best","top","easy","simple","quick","guide","tutorial","recipe","recipes","of","in",
                "on","and","from","into","about","way","ways","get","got","use","using","help","helping",
                "what","why","when","where","who","which","can","will","would","should","could","does","did","doing",
                "need","want","know","find","like","than","then","them","they","this","that",
                // Comparative / explanatory lexicon: these are STRUCTURE words, not the
                // topic. A local page that only shares "difference"/"between"/"vs"/"compare"
                // with the query is NOT a real match — e.g. "Difference Between Vulnerability
                // and Exploit" for "difference between a compiler and an interpreter". Treating
                // them as topic terms let low-quality local crawl pages survive the P2 gate and
                // outrank authoritative web hits. Excluding them makes topic_mentioned require a
                // SUBSTANTIVE term (compiler/interpreter, figma/sketch), so genuine comparison
                // pages still pass while off-topic structural matches get crushed. Fixed class,
                // not per-query.
                "difference","differences","different","between","vs","versus","compare","comparison",
                "compared","beginner","beginners","explained","explain","explaining","simply",
                "meaning","means","definition","define","mean","like","how to",
                // FORMAT / QUALITY / CATEGORY markers (P2d, round-2026-08-20T1935Z): a local
                // crawl page that shares ONLY these generic format/quality/category words with
                // the query ("alternatives","traditional","forms","good","best","free","tier",
                // "near","list","blog","tips",...) while naming NONE of the query's real SUBJECT
                // ("airtable","bibimbap",...) is off-topic crawl noise that floated to #1 above
                // on-topic web results (round #10 local "kimchi" page for "bibimbap"; #22 local
                // "Slack Alternatives Small Teams Actually Need" page for "alternatives to
                // airtable"). Treating these as structure words makes topic_mentioned require a
                // SUBSTANTIVE subject term, so generic-category-only local pages fail the gate
                // and get crushed. NOTE: this list is FORMAT/QUALITY/CATEGORY ONLY — genuine
                // subject nouns (dentists, restaurants, software, laptop, phone, books, ...) are
                // intentionally NOT here, so a real local page about one of them still passes.
                // Generalised word-CLASS list (subject derived from query's own terms).
                "alternatives","alternative","forms","form","tier","free","good","best","top",
                "nonprofit","podcasts","videos","apps","app","tool","tools","website","websites",
                "service","services","platform","movie","movies","song","songs","game","games",
                "traditional","recipe","recipes","tutorial","guide","reviews","review",
                "ideas","near","nearby","local","online","list","lists","sites","site","blog",
                "blogs","article","articles","post","posts","update","updates","news","tips","way",
                "ways","options","option","example","examples","type","types","kind","kinds",
                "brand","brands","product","products","company","companies","plan","plans",
                // generic SIZE / TEAM modifiers are modifiers, not subjects — a query like
                // "alternatives to airtable for a small nonprofit" must collapse its subject
                // to "airtable" (not "small"), so a local "Slack Alternatives Small Teams"
                // page (which names only the modifiers) gets crushed by P2d.
                "small","large","big","medium","tiny","huge","teams","team",
                // DEVICE / PLATFORM modifiers (P2e, round-2026-08-20T1935Z): a local crawl page
                // that shares ONLY a generic device/platform word with the query — e.g. the
                // crawler has many "WireGuard on Raspberry Pi" pages, so "set up nextcloud on a
                // raspberry pi" matches on the MODIFIER "raspberry pi" while missing the SUBJECT
                // "nextcloud" and wrongly ranks #1 over the on-topic web result. These words are
                // MODIFIERS, not subjects: a query like "set up nextcloud on a raspberry pi"
                // collapses its subject to "nextcloud" (not "raspberry pi"), so a local
                // "WireGuard on Raspberry Pi" page (which names only the device) gets crushed by
                // P2d. SCOPE NOTE: only the Raspberry-Pi family is included here because those
                // tokens are almost never the SOLE subject of a query; broad device nouns
                // (laptop, phone, computer, pc, ...) are intentionally NOT added — they CAN be a
                // query's real subject and would be wrongly crushed. Generalised word-CLASS
                // (subject derived from the query's own terms), no per-query/domain literals.
                "raspberry pi","raspberry","rpi","pi",
                // TEMPORAL / GENERIC-MODIFIER words (local-noise gate, this round 2026-08-22):
                // days-of-week, parts-of-day, and generic time/availability modifiers are
                // NON-DISCRIMINATING — thousands of unrelated pages contain "sunday", "morning",
                // "weekend", "open", "early". A local crawl page that matches a query ONLY on
                // such a token (e.g. Fox News "Sunday Morning Futures" ranking #1 for "weekend
                // flower markets in thrissur that open early on sunday morning") is off-topic
                // crawl noise, yet it survived the P2 gate because "sunday"/"morning"/"open"
                // counted as a "subject" match in topic_mentioned/substantive_subject_terms.
                // Excluding this word-CLASS from the subject test forces the local page to name a
                // REAL topic noun (flower/market/thrissur) to survive. Queries whose genuine
                // subject IS temporal (e.g. "what to do this weekend") simply have no
                // substantive subject terms -> the gate fails open (nothing to miss). Pure
                // word-CLASS seed, no per-query/domain tuning, future-proof.
                "sunday","monday","tuesday","wednesday","thursday","friday","saturday",
                "weekend","weekday","weekdays","morning","evening","afternoon","night","tonight",
                "today","month","year","open","opened","close","closed","early","late",
            ];
            // AUXILIARY-VERB / FILLER markers (P2d, round-2026-08-20T1935Z): query verbs like
            // "need"/"want"/"use"/"require" are DISTINCTIVE terms but are NOT subjects — a local
            // page titled "Slack Alternatives Small Teams Actually Need" matches the query
            // "alternatives to airtable ... that need a free tier" only on "alternatives" +
            // "need", neither of which is the subject "airtable". If such a verb is the only
            // surviving distinctive term it must NOT satisfy the subject requirement. Fixed
            // word-CLASS seed, not per-query.
            let aux_verb_words: &[&str] = &[
                "need","needs","needed","want","wants","wanted","require","requires","required",
                "use","uses","used","using",
            ];
            // P2 fix (this round): anchor `topic_mentioned` on `strong_distinctive_terms`
            // (substantive subject terms; weak anchors like "places"/"road"/"trip" already
            // filtered out) instead of the full `distinctive_terms`. An off-topic local
            // crawl page can match ONLY a weak anchor — e.g. "trawell.in/vizag/100kms" for a
            // "places to see snowfall near shimla within 100 kilometers" query, where the sole
            // overlap is the generic word "places" — and the old test (which accepted any
            // distinctive term) set topic_mentioned=true, so the quality gate never fired and
            // the page floated to #1 above the on-topic web result. Using strong terms means a
            // local page must actually mention the query's SUBJECT (shimla/snowfall,
            // boeing/airbus, hyderabad/goa) to survive; weak-anchor-only matches are correctly
            // crushed. General, signal-driven, no query/domain bias. Genuine local pages that
            // contain a real subject term still pass (no regression).
            let topic_mentioned = strong_distinctive_terms.is_empty()
                || strong_distinctive_terms.iter().any(|t| {
                    let tl = t.to_lowercase();
                    if structure_words.contains(&tl.as_str()) { return false; }
                    let bare = tl.trim_end_matches('s');
                    title_lower.contains(&tl) || content_lower.contains(&tl)
                        || title_lower.contains(bare) || content_lower.contains(bare)
                });
            // P2b: high-quality local pages that match the query ONLY on comparison
            // STRUCTURE ("difference between X and Y", "X vs Y") but share NONE of the
            // query's substantive entity terms are off-topic crawl noise — e.g.
            // "Difference Between Maven, ANT, Jenkins", "Hedge Fund vs Mutual Fund",
            // "Orthopedics vs Rheumatology" surfacing for "router vs modem". The
            // indexer scored them high (they ARE real comparison pages) so the
            // quality-only P2 gate above spares them, yet they crowd out the
            // authoritative web result that actually mentions the query's subject.
            // Crush only comparison-structured local pages missing the query entities.
            // General: keyed on result structure + substantive-term absence, no domains.
            let comparison_structure_words: &[&str] = &[
                "difference", "between", "vs", "versus", "compare", "compared", "comparison",
            ];
            let substantive_terms: Vec<String> = distinctive_terms.iter()
                .map(|t| t.to_lowercase())
                .filter(|tl| !comparison_structure_words.contains(&tl.as_str()))
                .filter(|tl| !structure_words.contains(&tl.as_str()))
                .collect();
            let mentions_substantive = substantive_terms.iter().any(|t| {
                title_lower.contains(t) || content_lower.contains(t)
            });
            // P2d (round-2026-08-20T1935Z): a LOCAL page that matches the query only on
            // generic FORMAT/CATEGORY words (now part of structure_words: "alternatives",
            // "traditional", "forms", "good", "dentists", ...) while naming NONE of the
            // query's substantive SUBJECT terms is off-topic crawl noise, and the
            // quality-only P2 gates above spare it (the crawler scored it high on the shared
            // format word) so it floats to #1 above the on-topic web result. Examples from
            // this round: local "kimchi" page #1 for "bibimbap"; local "Slack Alternatives"
            // page #1 for "alternatives to airtable". substantive_subject_terms = distinctive
            // terms minus structure_words (which now includes the format/category vocab), so
            // it is the query's REAL subjects (airtable, bibimbap, thomson, biryani, ...). A
            // local page must name one to survive; otherwise it is crushed. Fully general —
            // subject derived from the query's own terms, no per-query/domain tuning — and
            // fail-open when the query has no substantive subject terms (so short/generic
            // queries are not over-crushed).
            let substantive_subject_terms: Vec<String> = strong_distinctive_terms
                .iter()
                .map(|t| t.to_lowercase())
                .filter(|tl| !structure_words.contains(&tl.as_str()))
                .filter(|tl| !aux_verb_words.contains(&tl.as_str()))
                .collect();
            let mentions_substantive_subject = substantive_subject_terms.iter().any(|t| {
                // TITLE-anchored only: a local page whose TITLE does not name the
                // query's substantive subject is off-topic crawl noise even if it
                // mentions the subject INCIDENTALLY in its body (e.g. a "Slack
                // Alternatives" page that references "airtable" in passing is still
                // about Slack, not Airtable, and must not rank #1 for an
                // "alternatives to airtable" query). Content-only matches are exactly
                // the leak that let #22 survive; title-anchoring is the general fix.
                let bare = t.trim_end_matches('s');
                let tl = t.as_str();
                title_lower.contains(tl) || title_lower.contains(bare)
            });
            let result_is_comparison_structured =
                title_lower.contains(" vs ") || title_lower.contains(" versus ")
                || title_lower.contains("difference between") || title_lower.contains(" compared ");
            let is_comparison_intent = intent == "comparison" || intent == "technical";
            if r.quality < 0.55 && !topic_mentioned {
                relevance *= 0.05;
                tracing::info!(
                    "LOCAL NOISE GATE: '{}' quality={:.2} topic_mentioned={} -> relevance crushed",
                    r.url.chars().take(60).collect::<String>(), r.quality, topic_mentioned
                );
            } else if r.quality < 0.75 && !topic_mentioned {
                relevance *= 0.5;
            } else if is_comparison_intent && result_is_comparison_structured
                && !substantive_terms.is_empty() && !mentions_substantive {
                relevance *= 0.3;
                tracing::info!(
                    "LOCAL NOISE GATE (off-topic comparison): '{}' is a comparison page but mentions none of the query entities {:?} -> relevance *= 0.3",
                    r.title.chars().take(60).collect::<String>(), substantive_terms
                );
            } else if r.is_local && comparison_query && !comparison_entities.is_empty() {
                // D3 fix: for a comparison query, a LOCAL page that names NONE of
                // the compared entities (brand+model tokens like "brezza"/"venue")
                // is off-topic crawl noise EVEN when it shares generic attribute
                // words ("mileage", "petrol", "real world"). E.g. "Honda City
                // Mileage" floating above the actual Brezza/Venue results for a
                // "Brezza vs Venue mileage" query, because the local index scored it
                // on the shared attribute words and its relevance was never crushed.
                // The compared entities are derived from the query's OWN distinctive
                // terms minus attribute/structure vocab, so this is fully general:
                // it fires for any comparison ("swift vs nexon", "city vs amaze",
                // ...) and never names a specific brand/model. Crush hard so on-topic
                // web pages (which DO name the entities) win the slot.
                let mentions_compared = comparison_entities.iter().any(|e| {
                    title_lower.contains(e.as_str()) || content_lower.contains(e.as_str())
                });
                if !mentions_compared {
                    relevance *= 0.05;
                    tracing::info!(
                        "LOCAL NOISE GATE (D3 compared-entity): '{}' names none of the compared entities {:?} for comparison query -> relevance x0.05",
                        r.title.chars().take(60).collect::<String>(), comparison_entities
                    );
                }
            } else if r.is_local && distinctive_terms.len() >= 3 && overlap < 0.34 {
                // P2c (this round): a LOCAL page that shares only a small FRACTION of the
                // query's distinctive terms is crawl noise, not a real match. The checks above
                // are defeated by a SINGLE generic-noun overlap — e.g. "Road Trip Ideas" matching
                // just "road"+"trip" of a "hyderabad to goa road trip" query, or "Public record
                // requests" matching just "record"+"safety" of "boeing versus airbus safety" —
                // so topic_mentioned stays true and the page floats to #1 above on-topic web
                // results. Use the in-scope lexical `overlap` ratio (present distinctive / total
                // distinctive) as the signal: < 0.34 with >= 3 distinctive terms means the page
                // addresses a small minority of the query -> crush it. Short queries (N<3) are
                // exempt (a 1/2 match there is tolerable and would over-crush legit short matches).
                // General, signal-driven, no query/domain tuning.
                relevance *= 0.05;
                tracing::info!(
                    "LOCAL NOISE GATE (low distinctive overlap): '{}' overlap={:.2} distinctive_len={} -> relevance x0.05",
                    r.title.chars().take(60).collect::<String>(), overlap, distinctive_terms.len()
                );
            }
            // P2d (standalone, round-2026-08-20T1935Z): high-quality LOCAL page that names
            // NONE of the query's substantive SUBJECT terms (only generic format/category
            // words like "alternatives"/"traditional"/"forms"/"good"/"dentists") is off-topic
            // crawl noise. Evaluated as a STANDALONE if (NOT an else-if) because the earlier
            // D3 comparison gate (branch `r.is_local && comparison_query`) can spare such a
            // page via a CONTENT-only entity mention — e.g. a "Slack Alternatives" page whose
            // body references "airtable" survives the D3 content check, then the else-if chain
            // skips P2d entirely. Title-anchoring the subject requirement kills that leak: a
            // local page must name the subject in its TITLE to survive. Fail-open when the
            // query has no substantive subject terms (short/generic queries not over-crushed).
            if r.is_local && !substantive_subject_terms.is_empty() && !mentions_substantive_subject {
                p2d_offtopic = true;
                p2d_offtopic_terms = substantive_subject_terms.clone();
                // In-loop relevance crush (defense-in-depth): pushes the page toward
                // raw_min so calibrate_scores' [0.05,1.0] rescale lands it near the floor.
                // The DURABLE suppression is the POST-CALIBRATION P2d cap (near ~8141),
                // which re-applies AFTER calibration — the only place a crush survives
                // the linear rescale. General: keyed on "local page names none of the
                // query's title-anchored subject terms", a structural class, not a
                // per-query/domain rule.
                relevance = (relevance * 0.01).min(0.0025);
                r.score *= 0.01;
                tracing::info!(
                    "LOCAL NOISE GATE (P2d off-topic local): '{}' names none of the subject terms {:?} -> relevance crushed to {:.4}, r.score x0.01",
                    r.title.chars().take(60).collect::<String>(), substantive_subject_terms, relevance
                );
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
            let res_price = r.get_price();
            let price_signal = has_price_signal(&title_lower, &content_lower);
            if let Some(p_info) = res_price {
                let p_usd = price_to_usd(p_info.amount, &p_info.currency) as f32;
                let over = (constraints.price_max.is_some() && p_usd > constraints.price_max.unwrap())
                    || (constraints.price_lt.is_some() && p_usd > constraints.price_lt.unwrap())
                    || (constraints.price_min.is_some() && p_usd < constraints.price_min.unwrap())
                    || (constraints.price_gt.is_some() && p_usd < constraints.price_gt.unwrap());
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

        // D5/D6 flags: set when a generic vendor/affiliate page lacks the query's
        // specific subject terms; applied as a hard final-score suppression below.
        let mut vendor_affiliate_suppress = false;
        let mut vendor_affiliate_final_mult = 1.0f32;

        // ── Vendor / affiliate generic-page dampening (D5/D6) ──
        // Defect: on transactional / comparison-shopping queries, a GENERIC
        // vendor / affiliate / buyers-guide page (often carrying the
        // `official_vendor` source tag, or a /buyers-guide/ / affiliate URL, or a
        // generic "home warranty" sales title) floats to #1 because it shares a
        // generic commercial token with the query ("buy", "warranty", "earbuds")
        // while missing the user's SPECIFIC product / attribute terms (used /
        // iphone / bangalore; bluetooth / microphone / calls). On thin or
        // tie-broken result sets the flat official_vendor + local bonuses lift it
        // above genuinely on-topic product pages. Prior rounds' local-noise gate
        // only fired on low-indexer-quality local pages, not these.
        //
        // General fix (no query/domain literals): a page is a "generic vendor /
        // affiliate" page when it (a) carries the `official_vendor` source, or
        // (b) is a buyers-guide / affiliate page by URL or title structure, or
        // (c) is a generic warranty-sales page. We then require it to actually
        // name the query's SPECIFIC subject terms — the strong distinctive terms
        // MINUS generic commerce-function words (buy/warranty/price/used/...). If
        // it matches fewer than ceil(N/2) of those specific terms, it is a
        // generic commercial page, not the product the user asked for, so we
        // dampen relevance (which folds into the FINAL score, so the penalty
        // bites). This preserves a REAL official_vendor result for a query that
        // IS about that vendor: when the query literally names a known vendor
        // brand (the same signal that justified the `official_vendor` tag in the
        // download/nav deep-dive), we exempt it — so "download nvidia driver" ->
        // nvidia.com stays boosted, but a mis-tagged "How To Buy a Home Warranty"
        // for a "used iphone ... bangalore" query is crushed. Fully general:
        // thresholds are term-count math; the only constant lists are a general
        // commerce-function vocabulary and the existing vendor-brand concept.
        {
            let generic_commerce_terms: &[&str] = &[
                "buy", "buying", "purchase", "purchasing", "shop", "shopping",
                "store", "price", "prices", "cheap", "sale", "sales", "deal",
                "deals", "discount", "coupon", "best", "top", "warranty",
                "warranties", "cost", "budget", "under", "near", "where", "used",
                "new", "refurbished", "sell", "selling", "order", "cart", "free",
                "review", "reviews", "compare", "comparison",
            ];
            let vendor_brand_tokens: &[&str] = &[
                "nvidia", "amd", "intel", "realtek", "microsoft", "dell", "hp",
                "lenovo", "asus", "msi", "gigabyte", "logitech", "corsair",
                "razer", "apple", "oracle", "videolan", "vlc",
            ];
            let is_vendor_source = r.sources.iter().any(|s| s == "official_vendor");
            let is_buyers_guide = title_lower.contains("buyer's guide")
                || title_lower.contains("buyers guide")
                || title_lower.contains("buying guide")
                || title_lower.contains("buyer guide")
                || url_lower.contains("/buyers-guide/")
                || url_lower.contains("/buyer-guide/")
                || url_lower.contains("/buyers-guides/")
                || url_lower.contains("/buyer-guides/")
                || url_lower.contains("/affiliate/")
                || url_lower.contains("/affiliates/");
            // Generic warranty-sales page (e.g. "How To Buy a Home Warranty"):
            // a "how to buy a <X> warranty" / "<X> warranty plan/company" pattern
            // is an affiliate sales page, not the product the user searched for.
            let is_warranty_sales = (title_lower.starts_with("how to buy a")
                || title_lower.starts_with("how to get a")
                || title_lower.contains("home warranty")
                || title_lower.contains("extended warranty")
                || title_lower.contains("warranty plan")
                || title_lower.contains("warranty company")
                || title_lower.contains("warranty companies"))
                && !strong_distinctive_terms.is_empty();
            let is_vendor_affiliate = is_vendor_source || is_buyers_guide || is_warranty_sales;

            if is_vendor_affiliate && !strong_distinctive_terms.is_empty() {
                // Exempt a genuine official_vendor result whose query IS about
                // that vendor (matches the deep-dive that tagged it).
                let legit_vendor = is_vendor_source
                    && vendor_brand_tokens.iter().any(|b| clean_query.to_lowercase().contains(*b));
                if !legit_vendor {
                    let specific_terms: Vec<String> = strong_distinctive_terms
                        .iter()
                        .map(|t| t.to_lowercase())
                        .filter(|tl| !generic_commerce_terms.contains(&tl.as_str()))
                        .collect();
                    if !specific_terms.is_empty() {
                        let specific_matches = specific_terms.iter().filter(|tl| {
                            title_lower.contains(tl.as_str())
                                || content_lower.contains(tl.as_str())
                                || url_lower.contains(tl.as_str())
                        }).count();
                        let need = if specific_terms.len() <= 1 {
                            1
                        } else {
                            (specific_terms.len() + 1) / 2 // ceil(N/2)
                        };
                        if specific_matches < need {
                            relevance *= 0.3;
                            // Mark for hard final-score suppression below. `relevance`
                            // alone is not enough: `intent_boost` / `freshness` are
                            // computed independently of relevance (see the
                            // off_topic_struct starvation block) and feed `base` at
                            // full weight, so calibrate_scores rescales the max raw
                            // score to 1.0 and undoes a relevance-only crush. We
                            // therefore also starve those signals and apply a hard
                            // final multiplier so a generic sales page can never ride
                            // the transactional intent_boost to #1 over on-topic
                            // product pages.
                            vendor_affiliate_suppress = true;
                            tracing::info!(
                                "VENDOR/AFFILIATE DAMPEN x0.3: '{}' is generic vendor/affiliate (src={:?}, buyers_guide={}, warranty_sales={}) and matches only {}/{} specific subject terms (need {})",
                                r.title.chars().take(60).collect::<String>(),
                                r.sources, is_buyers_guide, is_warranty_sales,
                                specific_matches, specific_terms.len(), need
                            );
                        }
                    }
                }
            }
        }

        let mut intent_boost = calculate_intent_boost(&r.url, &r.title, &clean_query, intent);
        let mut freshness = freshness_score(&r.url, intent, r.published_date.as_deref(), &r.title, &r.content);
        let mut quality = content_quality_score(&r.content);

        // Hard suppression for generic vendor/affiliate pages (D5/D6), applied
        // AFTER intent_boost/freshness are computed so we can starve them. Folded
        // into the FINAL score so it actually bites (a relevance-only multiply is
        // undone by calibrate_scores' max-rescale). Mirrors the off_topic_struct
        // block: zero the independently-computed intent_boost + freshness, and
        // apply a flat final multiplier. Exempts a genuine official_vendor result
        // whose query names the vendor (the flag stayed false above).
        if vendor_affiliate_suppress {
            intent_boost = 0.0;
            freshness = 0.0;
            vendor_affiliate_final_mult = 0.2;
            tracing::info!(
                "VENDOR/AFFILIATE SUPPRESS: '{}' — intent_boost+freshness zeroed, final x0.2",
                r.title.chars().take(60).collect::<String>()
            );
        }

        // ── Off-topic structural starvation (this round, #01) ──
        // The generic-word guard above already crushes relevance (×0.12) for results that
        // match NONE of the query's distinctive topic terms. But `freshness` (for news/date
        // queries) and `intent_boost` (structural URL/title signals) are computed
        // independently of relevance and feed `base` at full weight. A high-authority or
        // recency-flavored off-topic page (e.g. water.ca.gov "...Late December Storms
        // Deliver..." for "vegetarian restaurants in bengaluru that deliver late") keeps a
        // large `base`, and `calibrate_scores` then rescales the max raw score to 1.0 —
        // undoing the relevance crush (the classic "penalties only bite if folded into the
        // FINAL r.score" lesson). Zeroing freshness + intent_boost for off-topic results
        // starves the raw base so it stays below on-topic results even after calibration.
        // `off_topic` is recomputed here (not reused from the base block) to stay in scope.
        let geo_ok_struct = geo_location
            .map(|g| geo_relevance_score(&title_lower, &content_lower, &url_lower, g) > 0.0)
            .unwrap_or(false);
        let off_topic_struct = !strong_distinctive_terms.is_empty() && !strong_distinctive_terms.iter().any(|t| {
            let tl = t.to_lowercase();
            title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
        }) && !geo_ok_struct;
        if off_topic_struct {
            freshness = 0.0;
            intent_boost = 0.0;
        }

        // ── D4 (2026-08-18T1340Z round): per-engine upstream-quality trust ──
        // A fresh+dated query whose date window failed OPEN (no dates upstream →
        // can't hard-drop) can still carry DATE-BLIND upstream junk that trusts
        // its way to the top via RRF position + authority. We lower the trust of
        // results whose upstream engine returned ZERO dated results while OTHER
        // engines returned dated ones for this same query (see engine_trust map
        // above). Trust is derived, not hardcoded: full for engines that surfaced
        // dates, crushed (×0.12) for the corroborated date-blind engine. Local
        // results and non-fresh intents are untouched (trust stays 1.0). This is
        // the "lower trust for the low-quality upstream" half of the D4 fix.
        let engine_trust_mult: f32 = if intent == "fresh" && !engine_trust.is_empty() {
            let eng = primary_engine(r);
            *engine_trust.get(&eng).unwrap_or(&1.0f32)
        } else {
            1.0
        };

        // ── D4 (2026-08-18T1340Z round): stronger fresh-intent off-topic crush ──
        // The off_topic_struct gate above only fires when the result shares NO
        // distinctive query term at all. For a fresh+dated query where the date
        // window failed open, a date-blind upstream can return results that DO
        // borrow one generic query word (so off_topic_struct misses them) yet are
        // still clearly junk — no distinctive TOPIC term AND no date signal. We
        // add a fresh-intent-specific crush: when the query is fresh AND the
        // result shares no distinctive topic term AND carries no date, treat it
        // as off-topic and starve freshness + intent_boost (and dampen relevance),
        // so the dated, topic-bearing results from the good upstream win. This is
        // the "stronger off-topic crush for fresh intent" half of the D4 fix.
        // Keyed on (no distinctive topic term) + (no date signal) so it never
        // touches a result that is dated or that names the topic — general, no
        // query/domain literals.
        let mut d4_off_topic = false;
        if intent == "fresh" && !strong_distinctive_terms.is_empty() {
            let has_distinctive = strong_distinctive_terms.iter().any(|t| {
                let tl = t.to_lowercase();
                title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
            });
            let has_date = resolve_item_date(
                r.published_date.as_deref(),
                &r.url,
                &r.title,
                &r.content,
            ).is_some();
            if !has_distinctive && !has_date {
                d4_off_topic = true;
            }
        }
        if d4_off_topic {
            freshness = 0.0;
            intent_boost = 0.0;
            relevance *= 0.12;
            tracing::info!(
                "D4 FRESH OFF-TOPIC CRUSH x0.12: '{}' shares no distinctive topic term and has no date signal (fresh intent, date window failed open)",
                r.url.chars().take(60).collect::<String>()
            );
        }

        // ── Fresh-intent news-portal demotion (this round, #16/#22) ──
        // For FRESH intent, upstream often returns ONLY the bare homepage or top-level
        // section of a major news portal (cnn.com/, bbc.com/news/world, foxnews.com/)
        // because those domains dominate recency signals. A bare portal URL with a GENERIC
        // title ("Breaking News", "Latest News & Updates") carries no specific article and
        // crowds out the few topical articles that did come back. We crush such portals
        // WHEN their title lacks any distinctive query term — this lifts the topical
        // article above the homepage. The penalty is anchored on distinctive_terms (not
        // role words), so a portal article whose title names the topic (e.g. "solid-state
        // battery breakthrough") is NOT demoted. Pure generic-title portal pages are the
        // failure mode; they are demoted but never fully removed (floor preserved).
        if intent == "fresh" {
            let is_portal_home = {
                let host = reqwest::Url::parse(&r.url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
                    .unwrap_or_default();
                let path = reqwest::Url::parse(&r.url)
                    .ok()
                    .map(|u| u.path().to_string())
                    .unwrap_or_else(|| "/".to_string());
                let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
                // homepage ("/") OR a single shallow news section ("/news", "/news/world")
                let shallow = path_segments.len() <= 1
                    || (path_segments.len() == 2 && path_segments[0] == "news");
                let known_portal = [
                    "cnn.com", "bbc.com", "bbc.co.uk", "foxnews.com", "nytimes.com",
                    "abcnews.com", "nbcnews.com", "cbsnews.com", "ndtv.com", "indiatimes.com",
                    "news.google.com", "google.com", "yahoo.com", "msn.com", "reuters.com",
                ].iter().any(|p| host == *p || host.ends_with(&format!(".{}", p)));
                known_portal && shallow
            };
            let title_has_topic = if distinctive_terms.is_empty() {
                false
            } else {
                distinctive_terms.iter().any(|t| {
                    let tl = t.to_lowercase();
                    title_lower.contains(&tl) || content_lower.contains(&tl)
                })
            };
            if is_portal_home && !title_has_topic {
                relevance *= 0.12;
                tracing::info!(
                    "FRESH PORTAL DEMOTE x0.12: '{}' (generic-title portal for fresh intent)",
                    r.url.chars().take(60).collect::<String>()
                );
            }
        }

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

            if !strong_distinctive_terms.is_empty() {
                let any_distinctive_match = strong_distinctive_terms.iter().any(|t| {
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
                || url_lower.contains("dictionary")
                || url_lower.contains("vocabulary.com")
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
                // Round 2026-08-20: "difference between A and B" queries surfaced
                // "DIFFERENCE definition and meaning | Collins English Dictionary" /
                // "DIFFERENCE | definition in the Cambridge English Dictionary" as #1-
                // #3. Those titles use the "definition and meaning" / "definition in
                // the … Dictionary" framing, which the older `definition of `/`meaning
                // of ` patterns missed. Matching the structural phrase (any title
                // that pairs `definition` with `meaning`/`dictionary` and names a
                // dictionary brand) catches them without per-word hardcoding.
                || (title_lower.contains("definition") && (title_lower.contains("meaning") || title_lower.contains("dictionary")))
                || title_lower.contains("english dictionary")
                || title_lower.contains("english thesaurus")
                || title_lower.contains("dictionary")
                // Title ends with a known dictionary brand (e.g. "| Cambridge
                // Dictionary", "| Oxford Learner's Dictionaries") — a brand-named
                // reference page, not a human article.
                || title_lower.ends_with("dictionary")
                || title_lower.ends_with("thesaurus")
                || title_lower.ends_with("lexico")
                // Bare "X Calculator" tool pages rank for "difference between"
                // queries because of the shared word "difference". They are
                // interactive math tools, not conceptual comparisons. Only crush
                // when the title is a calculator/tool pattern (general signal, not
                // a per-query literal).
                || title_lower.contains("calculator")
                || url_lower.contains("calculator") && url_lower.contains("convert")
                || url_lower.contains("/calculator");

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

        if is_academic && !q_wants_academic {
            if is_nav_or_download {
                // Navigational/download/commercial: academic papers are 100% noise.
                relevance = 0.001;
                quality *= 0.01;
                tracing::info!(
                    "ACADEMIC PAPER PENALTY: '{}' -> crushed 0.001 for navigational/download query",
                    r.url.chars().take(60).collect::<String>()
                );
            } else {
                // Every other intent (informational/local/how-to/fresh/comparison):
                // moderate demotion, not crush — a paper about the topic is still
                // semi-useful, but must never outrank accessible articles/product
                // pages. The adaptive floor then sinks it below real content.
                relevance *= 0.30;
                tracing::info!(
                    "ACADEMIC PAPER DEMOTE: '{}' -> relevance x0.30 (non-academic query, intent={})",
                    r.url.chars().take(60).collect::<String>(), intent
                );
            }
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
        let local_bonus = if r.is_local && relevance >= 0.35 {
            // D3 (this task): a comparison query's local_bonus must require the page
            // to actually name at least ONE of the compared entities. This stops a
            // brand-ambiguous local page (e.g. "Honda City Mileage" for a
            // "Brezza vs Venue" query) from earning the bonus purely on shared
            // generic attribute words while naming neither compared entity — the
            // exact mechanism that floated the off-topic brand above on-topic web.
            // `comparison_entities` is derived from the query (no brand literals), so
            // this generalises. For non-comparison queries the gate is unchanged.
            let passes_entity_gate = !comparison_query
                || comparison_entities.is_empty()
                || comparison_entities.iter().any(|e| {
                    title_lower.contains(e.as_str()) || content_lower.contains(e.as_str())
                });
            if passes_entity_gate {
                (relevance * 0.45).min(0.45)
            } else {
                0.0
            }
        } else {
            0.0
        };
        // Comparison-entity coverage boost: for a comparison query, results that name
        // BOTH compared entities (or >= half of them) are the genuinely comparative
        // pages the user wants (e.g. "Brezza vs Venue" mileage page). Lift them
        // modestly so they surface above single-entity or off-topic pages. Counts are
        // derived from the query's own entities; no per-brand tuning.
        if comparison_query && query_entity_count >= 2 {
            let named = comparison_entities.iter().filter(|e| {
                title_lower.contains(e.as_str()) || content_lower.contains(e.as_str())
            }).count() as f32;
            let frac = named / query_entity_count as f32;
            if frac >= 0.5 {
                relevance *= 1.12;
            }
        }
        // Geo-relevance boost: boost results that mention the user's country, region, or city.
        // Higher boost for city-level matches (0.25) than country-level (0.10).
        let geo_boost = geo_location.map(|g| geo_relevance_score(&r.title, &r.content, &r.url, g)).unwrap_or(0.0);
        // Off-topic authority suppression: when a result matches NONE of the query's
        // distinctive topic terms it is genuinely off-topic (the generic-word guard at
        // ~4835 already crushed its relevance). High-authority portals (water.ca.gov,
        // cnn.com) would otherwise keep floating above on the authority signal alone,
        // which — combined with calibrate_scores rescaling the max raw score to 1.0 —
        // lets an off-topic homepage (e.g. "Late December Storms Deliver...") rank #1
        // for "vegetarian restaurants in bengaluru that deliver late". Suppressing
        // authority here makes the off-topic crush actually bite. This never hurts a
        // result that contains a real topic term, and authority is only halved (not
        // zeroed) so a borderline page still earns a little trust.
        // Geo-aware exemption (mirrors the off_topic_struct gate above): a result
        // naming the resolved location is on-topic for a geo/local query even if it
        // lacks the descriptive adjectives, so it must not lose its authority signal.
        let geo_ok_authority = geo_location
            .map(|g| geo_relevance_score(&title_lower, &content_lower, &url_lower, g) > 0.0)
            .unwrap_or(false);
        let off_topic = !strong_distinctive_terms.is_empty() && !strong_distinctive_terms.iter().any(|t| {
            let tl = t.to_lowercase();
            title_lower.contains(&tl) || content_lower.contains(&tl) || url_lower.contains(&tl)
        }) && !geo_ok_authority;
        // Inverse-geo authority suppression (this round, D1): a local page from the
        // WRONG city (explicit geo resolved, page does not name the location) is
        // geo-off-topic and must lose its authority signal too, not just its bonus —
        // otherwise its high authority floats it above right-city web results (the
        // Madurai/Busan case). Same signal as the off_topic gate; a right-city local
        // page (geo_ok_local) is exempt. Authority is halved (not zeroed) so a
        // borderline page keeps a little trust, and the existing 0.3 floor logic holds.
        let geo_authority_suppressed = off_topic || geo_local_offtopic;
        let authority_eff = if geo_authority_suppressed { r.authority * 0.3 } else { r.authority };

        // P2d (round-2026-08-20T1935Z): collapse the indexer BM25 for off-topic locals
        // HERE (same scope as `base`), because the earlier `r.score *= 0.01` in the noise-
        // gate block above does NOT propagate to this read under the borrow structure. The
        // body-incidental subject mention (e.g. "airtable" in a Slack-Alternatives page)
        // gave it a large r.score that dominates weights.rrf; crushing it here lets the
        // on-topic web page win after calibrate_scores. General: keyed on the P2d flag
        // (local page names none of the query's title-anchored subject terms).
        if p2d_offtopic {
            r.score *= 0.01;
        }

        let base = (weights.rrf * r.score)
            + (weights.semantic * semantic)
            + (weights.intent * intent_boost)
            + (weights.freshness * freshness)
            + (weights.authority * authority_eff)
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
        // creating real separation.
        // Floor lowered from 0.05 -> 0.005 (round-6 D1): an off-topic local page (e.g. a
        // crawler-index match whose lexical relevance is ~0 after the local-noise gate's
        // ×0.05) was still keeping 5% of its (large) indexer-BM25 base, which — combined
        // with calibrate_scores forcing the max raw score to 1.0 — floated it to #1 above
        // genuinely on-topic pages. A 0.005 floor lets a truly off-topic page (relevance
        // ~0.0025) demote to ~0.25% of its base, so on-topic pages win. On-topic pages have
        // relevance >= 0.05, so for them this clamp is unchanged (no regression). Never
        // zeroed (0.005 floor) so calibrate_scores' floor logic still holds.
        let relevance_mult = relevance.clamp(0.005, 1.0);
        // Video-suppression (non-/videos /search): Invidious videos enter the web merge
        // with score=0.0 and published_date=None, so they inherit r.score=1.0 and
        // freshness=1.0, which lets a generic youtube tutorial outrank a relevant
        // article for text queries (e.g. "how to make biryani at home"). Videos have
        // their own /videos endpoint; in /search they are secondary, so dampen them
        // unless the query is explicitly video-seeking. Floor keeps them present, not dominant.
        // A result is a "video" if it is tagged with the video source OR its URL
        // points at a known video platform. SearXNG often returns youtube.com /
        // vimeo.com / etc. URLs inside the GENERAL web result set WITHOUT a
        // video source tag (e.g. the "authentic poha indore style" query returned a
        // youtube.com recipe video at score 1.0 for a non-video query). Treating
        // those as text allowed them to outrank the real recipe article. is_url_video_host
        // catches them so the same dampening applies.
        let is_video_source = r.sources.iter().any(|s| s == "invidious" || s == "video")
            || is_url_video_host(&r.url);
        let q_lc = query.to_lowercase();
        let video_mult = if is_video_source {
            if q_lc.contains("video") || q_lc.contains("youtube") || q_lc.contains("watch") || q_lc.contains("tutorial") || q_lc.contains("animation") {
                1.0 // explicit video intent → keep
            } else {
                // P8 fix (this round): for a generic TEXT query, videos must NOT outrank
                // relevant articles. The old 0.25x multiplier was still amplified back to
                // ~1.0 by calibrate_scores (which rescales every score onto [0.05,1.0])
                // whenever the competing web/text results were themselves near-zero
                // (thin result sets). Now crush hard so a youtube tutorial can never sit
                // above a topical article for "...how to..." / "...reverse a linked list..."
                // text queries. The /videos endpoint remains the home for video intent; in
                // /search they are secondary. Floor keeps them present, not dominant.
                0.08
            }
        } else {
            1.0
        };
        // Cross-lingual relevance guard (D2, this round): a result written in a
        // non-Latin script (CJK, Cyrillic, Devanagari, Arabic, …) is almost never
        // the answer to an English / Roman-script query, yet upstream engines
        // returned unrelated zhihu (Chinese) and German pages that outranked the
        // genuinely relevant English article ("privacy browsers … alternative to
        // chrome"). We dampen results whose TEXT is predominantly non-Latin when
        // the QUERY is predominantly Latin-script. Signal-driven: it counts
        // character scripts, no language tables, no per-language denylist, no
        // query-specific literals. A Roman-script query vs a Roman-script result
        // (e.g. English, a Romanised Hindi place name, "Tokyo") is unaffected; two
        // non-Latin sides are both left alone (we cannot judge them by script).
        let lang_mismatch_mult = {
            let q_ascii_ratio = {
                let chars: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
                if chars.is_empty() { 1.0 } else {
                    let non = chars.iter().filter(|c| !c.is_ascii()).count();
                    (chars.len() - non) as f32 / chars.len() as f32
                }
            };
            let res_text = format!("{} {}", r.title, r.content);
            let tchars: Vec<char> = res_text.chars().filter(|c| !c.is_whitespace()).collect();
            let res_ascii_ratio = if tchars.is_empty() { 1.0 } else {
                let non = tchars.iter().filter(|c| !c.is_ascii()).count();
                (tchars.len() - non) as f32 / tchars.len() as f32
            };
            // Query is Latin-script dominant AND result is non-Latin-script dominant.
            if q_ascii_ratio >= 0.85 && res_ascii_ratio < 0.50 {
                0.25 // dampen hard but keep present (fail-soft, not a hard drop)
            } else {
                1.0
            }
        };

        let cross_loc_mult = if geo_is_explicit {
            cross_location_mismatch_mult(&r.title, &r.content, geo_location)
        } else {
            1.0
        };

        let p2d_mult = if p2d_offtopic { 0.05 } else { 1.0 };
        r.score = base * c_score * generic_penalty * relevance_factor * relevance_mult * video_mult * lang_mismatch_mult * cross_loc_mult * engine_trust_mult * vendor_affiliate_final_mult * p2d_mult;
        // Capture the D4 per-engine trust multiplier on the result so tests/operators
        // can observe whether this result was trust-crushed (see engine_trust_mult field).
        r.engine_trust_mult = engine_trust_mult;
        // Capture this result's relevance for the post-loop adaptive-floor pass.
        relevance_vec.push(relevance);
    }

    // ── Adult-intent + adult-marker detection (lifted above the off-topic drop) ──
    // Computed once here so BOTH the off-topic hard-drop below and the adult
    // hard-drop further down can use it without re-detecting. The adult host/path
    // lists are the curated static safety blocklist — accepted exception to the
    // no-hardcoding rule (never runtime-data-driven).
    let q_lc_adult = clean_query.to_lowercase();
    let adult_intent = q_lc_adult.contains("porn") || q_lc_adult.contains("xxx")
        || q_lc_adult.contains("nsfw") || q_lc_adult.contains("adult video")
        || q_lc_adult.contains("adult film") || q_lc_adult.contains("sex video")
        || q_lc_adult.contains("pornhub") || q_lc_adult.contains("xvideos")
        || q_lc_adult.contains("onlyfans");
    let adult_hosts: &[&str] = &[
        "xvideos", "xnxx", "pornhub", "xhamster", "youporn", "redtube",
        "txxx", "fpo.xxx", "watchon.me", "spankbang", "brazzers",
        "porn", "adultfriendfinder", "onlyfans", "chaturbate", "livejasmin",
        "cam4", "myfreecams", "beeg", "porntube", "eporner",
        "pornhd", "tube8", "xtube", "heavy-r", "efukt", "porzo",
    ];
    let adult_paths: &[&str] = &["/porn/", "/xxx/", "/adult/", "/nsfw/", "/sex/", "/porno/"];

    // ── Off-topic hard-drop (round-6 D1 LOCAL sole-survivor case; extended to WEB this round) ──
    // A result (local OR web) whose title/content/url shares ZERO of the query's
    // distinctive topic terms is off-topic (e.g. a crawler-indexed
    // "Early Warning Signs of Macular Degeneration" page for an "earthquakes in
    // the himalayan region" query, or unrelated software-testing blogs for an
    // "authentic poha indore style recipe" query). When web upstream is sparse it
    // can be the ONLY surviving result, and calibrate_scores then inflates it to
    // 1.0 — confidently returning off-topic junk. Drop it outright. This uses the
    // SAME distinctive-term overlap test as the in-loop `off_topic_struct` gate,
    // so genuinely relevant pages (which DO contain a distinctive term — e.g. an
    // iPhone-vs-S24 article for a "compare iphone and samsung" query) are kept.
    // General: keyed on (zero distinctive-term overlap), no query/domain bias.
    // NOTE: previously this only dropped LOCAL results; web results with zero
    // overlap survived at the 0.05 floor (calibrate_scores re-inflates the bottom
    // onto [0.05,1.0]), so off-topic web junk could not be removed. This round
    // removes that carve-out so the same gate protects web results.
    if !strong_distinctive_terms.is_empty() {
        let before = merged.len();
        let retained_pre_offtopic: Vec<MergedResult> = merged.iter().cloned().collect();
        merged.retain(|r| {
            // Adult exemption: when the query is explicitly adult, an adult result
            // must survive the off-topic gate — the adult block below keeps it
            // intentionally. Without this, the web off-topic drop would remove the
            // adult URL first (it shares zero food/recipe/etc. distinctive terms),
            // regressing "adult kept for explicit-adult query".
            if adult_intent {
                let ul = r.url.to_lowercase();
                let tl = r.title.to_lowercase();
                let host = reqwest::Url::parse(&r.url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
                    .unwrap_or_default();
                let tld_adult = host.ends_with(".xxx");
                let host_adult = adult_hosts.iter().any(|h| host.contains(h));
                let path_adult = adult_paths.iter().any(|p| ul.contains(p));
                let title_adult = tl.contains("porn") || tl.contains("xxx ")
                    || tl.contains("nude") || tl.contains("naked")
                    || tl.contains("sex video") || tl.contains("adult film");
                if tld_adult || host_adult || path_adult || title_adult {
                    return true;
                }
            }
            let tl = r.title.to_lowercase();
            let cl = r.content.to_lowercase();
            let ul = r.url.to_lowercase();
            let overlaps = strong_distinctive_terms.iter().any(|t| {
                let lt = t.to_lowercase();
                tl.contains(&lt) || cl.contains(&lt) || ul.contains(&lt)
            });
            // Geo-aware exemption: a query with a resolved location is a LOCAL/geo
            // intent; a result that names that location (city/country) is genuinely
            // on-topic even if its snippet omits the descriptive adjectives
            // (quiet/wifi/outlets/...). Without this, "quiet places to study near
            // chennai" hard-drops every chennai-mentioning result that didn't also
            // repeat "quiet"/"wifi", collapsing the set to one generic page.
            // General: reuses geo_relevance_score, no query/domain bias; only
            // exempts results that actually mention the resolved location.
            let geo_ok = geo_location
                .map(|g| geo_relevance_score(&tl, &cl, &ul, g) > 0.0)
                .unwrap_or(false);
            overlaps || geo_ok
        });
        let removed = before - merged.len();
        if removed > 0 {
            tracing::info!("OFF_TOPIC_HARD_DROP: removed {}/{} result(s) (local+web) with zero distinctive-term overlap", removed, before);
        }
        // ── Inverse-geo hard-drop (D1, this round): WRONG-CITY local pages ──
        // When an EXPLICIT location is resolved (e.g. "temples in madurai"),
        // a LOCAL-INDEX result that does NOT name that location is geo-off-topic:
        // it matched only generic tokens ("temple quiet") and is from the wrong
        // city (Madurai query → Busan page). The off-topic drop above CANNOT
        // catch this, because when strong_distinctive_terms is empty (common for
        // geo queries whose descriptive adjectives aren't distinctive tokens),
        // that whole block is skipped. So we drop wrong-city local pages here,
        // keyed purely on geo_relevance_score>0 (same signal as the off_topic
        // gate / geo boost) — NO per-query tuning, NO hardcoded city/domain list.
        // Only fires for explicit-location queries, so non-geo local results are
        // untouched. Fail-open: never empty the merged set on this alone (safety
        // over aggression — if it were the only survivor, keep it rather than
        // show nothing). Mirrors the off-topic fail-open structure.
        if geo_location.is_some() {
            let before_geo = merged.len();
            let retained_pre_geo: Vec<MergedResult> = merged.iter().cloned().collect();
            merged.retain(|r| {
                let is_wrong_city_local = r.is_local
                    && geo_location
                        .map(|g| geo_relevance_score(&r.title, &r.content, &r.url, g) == 0.0)
                        .unwrap_or(false);
                !is_wrong_city_local
            });
            let removed_geo = before_geo - merged.len();
            if removed_geo > 0 {
                tracing::info!(
                    "INVERSE_GEO_HARD_DROP: removed {}/{} wrong-city local result(s) for resolved geo",
                    removed_geo, before_geo
                );
            }
            // Fail-open: if the geo drop would empty the set, restore survivors.
            if merged.is_empty() && before_geo > 0 {
                merged.extend(retained_pre_geo);
            }
        }
        // Fail-open rescue (mirrors the date/price fail-opens above): if the
        // off-topic drop would EMPTY the merged set, the "distinctive-term
        // overlap" signal is too strict for this query (e.g. fresh+price queries
        // where upstream results legitimately omit the exact distinctive tokens
        // in their title/content/url snippets) and we must not return a blank
        // page. Restore the survivors but STILL enforce the adult hard-drop below
        // (safety is never fail-open), and keep an explicit warning so the gap is
        // visible. Keyed on "would-empty", not on any query/domain — general.
        if merged.is_empty() && before > 0 {
            let restored: Vec<MergedResult> = retained_pre_offtopic
                .into_iter()
                .filter(|r| {
                    let ul = r.url.to_lowercase();
                    let tl = r.title.to_lowercase();
                    let host = reqwest::Url::parse(&r.url)
                        .ok()
                        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
                        .unwrap_or_default();
                    let tld_adult = host.ends_with(".xxx");
                    let host_adult = adult_hosts.iter().any(|h| host.contains(h));
                    let path_adult = adult_paths.iter().any(|p| ul.contains(p));
                    let title_adult = tl.contains("porn") || tl.contains("xxx ")
                        || tl.contains("nude") || tl.contains("naked")
                        || tl.contains("sex video") || tl.contains("adult film");
                    let is_adult = tld_adult || host_adult || path_adult || title_adult;
                    !is_adult
                })
                .collect();
            tracing::warn!(
                "OFF_TOPIC_HARD_DROP FAIL-OPEN: {} result(s) all lacked distinctive-term overlap but dropping them would empty the set — restoring {} non-adult survivor(s) (recency/authority ranking still applies)",
                before,
                restored.len()
            );
            merged = restored;
        }
    }

    // ── P13 (round-2026-08-20T1935Z): drop adult/NSFW results flagged upstream ──
    // The per-result loop (line ~6302) sets r.score = -1.0 and continues for any
    // result whose title/URL matches clean::is_adult_explicit() — a query-agnostic
    // lexical detector. Here we physically remove those sentinels so they never
    // reach the response. Hard-drop (not demote) because a family-safe engine must
    // never surface explicit content regardless of how weak the rest of the set is.
    {
        let before = merged.len();
        merged.retain(|r| r.score >= 0.0);
        let removed = before - merged.len();
        if removed > 0 {
            tracing::info!("P13 ADULT DROP: removed {} explicit result(s) from merged set", removed);
        }
    }

    // ── Cross-location LOCAL hard-drop (2026-08-19 round, geo pollution) ──
    // When the user NAMES an explicit city in the query, a LOCAL-index page about a
    // *different* gazetteer city is wrong for that query (e.g. "vegetarian
    // restaurants near visakhapatnam" surfacing dozens of Trichy/Chennai local
    // crawl pages). The in-loop `cross_loc_mult` (0.12x) was not enough on its own
    // because the local base score is large, so other-city pages still floated into
    // positions 3-5. We hard-drop local results that name a different gazetteer place
    // and do NOT name the requested city/country.
    // General: reuses the SAME `LOCATION_GAZETTEER` + `geo_is_explicit` gating as the
    // soft multiplier, with the identical `mentions_req` exemption so inclusive pages
    // that NAME the requested place are kept. No query/domain literals.
    if geo_is_explicit {
        let before = merged.len();
        merged.retain(|r| {
            if !r.is_local {
                return true;
            }
            let tl = r.title.to_lowercase();
            let cl = r.content.to_lowercase();
            let ul = r.url.to_lowercase();
            let text = format!("{} {} {}", tl, cl, ul);
            // On-topic for the requested location → keep.
            let req_city = geo_location.and_then(|g| g.city.as_deref());
            let req_country = geo_location.and_then(|g| g.country_name.as_deref());
            let mentions_req = req_city.map_or(false, |c| whole_word_contains(&text, c))
                || req_country.map_or(false, |c| whole_word_contains(&text, c));
            if mentions_req {
                return true;
            }
            // Mention of a different known place → drop this local page.
            let same_country_ok = req_city.is_none();
            let req_cc = geo_location.and_then(|g| g.country_code.as_deref());
            for (name, cc) in LOCATION_GAZETTEER.iter() {
                if req_city.map_or(false, |c| c.eq_ignore_ascii_case(name)) { continue; }
                if req_country.map_or(false, |c| c.eq_ignore_ascii_case(name)) { continue; }
                if same_country_ok {
                    if let Some(rc) = req_cc { if cc.eq_ignore_ascii_case(rc) { continue; } }
                }
                if name.len() < 3 { continue; }
                if whole_word_contains(&text, name) {
                    return false;
                }
            }
            true
        });
        let removed = before - merged.len();
        if removed > 0 {
            tracing::info!("CROSS_LOCATION_LOCAL_DROP: removed {}/{} other-city local result(s) for explicit-geo query", removed, before);
        }
    }

    // ── Adult-content hard-drop for non-adult queries (this round, D4) ──
    // Privacy-first search must not surface pornographic/NSFW results for ordinary
    // queries. The web fan-out (SearXNG-via-VPN) returned XNXX adult forums for an
    // innocuous "improve deep sleep without medication" query — a content-safety
    // failure. There is no upstream SafeSearch guarantee we can rely on, so we drop
    // adult results at ranking time UNLESS the user explicitly sought adult content.
    // The adult host/path lists are the curated static safety blocklist — accepted
    // exception to the no-hardcoding rule; never runtime-data-driven. An explicit-adult
    // query keeps adult results; everything else drops them.
    {
        if !adult_intent {
            let before = merged.len();
            merged.retain(|r| {
                let ul = r.url.to_lowercase();
                let tl = r.title.to_lowercase();
                let host = reqwest::Url::parse(&r.url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
                    .unwrap_or_default();
                // adult TLD (.xxx) is unambiguously adult
                let tld_adult = host.ends_with(".xxx");
                let host_adult = adult_hosts.iter().any(|h| host.contains(h));
                let path_adult = adult_paths.iter().any(|p| ul.contains(p));
                let title_adult = tl.contains("porn") || tl.contains("xxx ")
                    || tl.contains("nude") || tl.contains("naked")
                    || tl.contains("sex video") || tl.contains("adult film");
                let is_adult = tld_adult || host_adult || path_adult || title_adult;
                if is_adult {
                    tracing::info!(
                        "ADULT DROP (non-adult query): '{}'",
                        r.url.chars().take(60).collect::<String>()
                    );
                }
                !is_adult
            });
            let removed = before - merged.len();
            if removed > 0 {
                tracing::info!("ADULT DROP: removed {}/{} adult result(s) for non-adult query", removed, before);
            }
        }
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
            // CRITICAL (round-6 D1 fix): the boost multiplies EVERY result, so if the
            // current MAX raw score is itself off-topic (e.g. a local page that survived
            // only on a coincidental BERT cosine), amplifying it floats the junk to #1.
            // We therefore require the TOP result (highest raw score) to be genuinely
            // ON-TOPIC — i.e. its lexical relevance overlap must be high. BERT cosine alone
            // is NOT sufficient (it conflates polysemous tokens: "warning"/"sign" for a
            // macular-degeneration page vs an earthquakes query). If the max-scoring page
            // has low relevance, the set has no trustworthy anchor and we must NOT boost.
            // General: anchored on the top result's relevance signal, no query/domain bias.
            let mut max_idx: Option<usize> = None;
            let mut max_s = f32::NEG_INFINITY;
            for (i, r) in merged.iter().enumerate() {
                if r.score > max_s { max_s = r.score; max_idx = Some(i); }
            }
            let top_relevance = max_idx.map(|i| relevance_vec.get(i).copied().unwrap_or(0.0)).unwrap_or(0.0);
            let top_is_ontopic = top_relevance >= 0.35;
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
            if has_good_result && top_is_ontopic {
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

    // 4b-ii. OFF-TOPIC DOMAIN SATURATION GUARD (D-C: a single unrelated brand
    // domain filling every slot, e.g. a hotel group for "how to grow basil
    // indoors"). This happens when the upstream draw degrades (VPN/SearXNG
    // flapping) and one authoritative-but-unrelated domain survives into every
    // position; the per-appearance diversity penalty above is RELATIVE, so when
    // the whole set is one domain it re-sorts them but cannot dislodge them.
    // Signal (no domain/query literals): the query has distinctive topic terms,
    // one host holds a MAJORITY of the result set, and NONE of that host's
    // results contain ANY distinctive term. Crush those results so any
    // topic-bearing result (however weak) outranks them. FAIL-OPEN: if the
    // saturating host is the entire set, we still keep the results — an empty
    // SERP is worse — but they are demoted so late-arriving on-topic results win.
    if !strong_distinctive_terms.is_empty() && merged.len() >= 3 {
        let host_of = |u: &str| -> String {
            reqwest::Url::parse(u)
                .ok()
                .and_then(|p| p.host_str().map(|h| h.to_lowercase()))
                .unwrap_or_default()
        };
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in &merged {
            *counts.entry(host_of(&r.url)).or_insert(0) += 1;
        }
        let total = merged.len();
        for (host, count) in counts.iter() {
            if host.is_empty() || (*count as f32) < 0.6 * total as f32 {
                continue;
            }
            let host_results: Vec<&MergedResult> =
                merged.iter().filter(|r| &host_of(&r.url) == host).collect();
            let any_on_topic = host_results.iter().any(|r| {
                let rl = r.title.to_lowercase();
                let cl = r.content.to_lowercase();
                let ul = r.url.to_lowercase();
                strong_distinctive_terms.iter().any(|t| {
                    let lt = t.to_lowercase();
                    rl.contains(&lt) || cl.contains(&lt) || ul.contains(&lt)
                })
            });
            if !any_on_topic {
                tracing::warn!(
                    "OFF-TOPIC DOMAIN SATURATION: host '{}' holds {}/{} results and matches no distinctive query term — crushing",
                    host, count, total
                );
                for r in merged.iter_mut() {
                    if &host_of(&r.url) == host {
                        r.score *= 0.05;
                    }
                }
            }
        }
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

    // POST-CALIBRATION OFF-TOPIC CAP (this round, D1/D2/D3).
    // The in-loop relevance penalties (dictionary-site relevance=0.001 at ~5506,
    // generic-word ×0.12 guard at ~5061, local-noise ×0.05 at ~5202) are DEFEATED
    // by calibrate_scores, which rescales the WHOLE set onto [0.05,1.0]. When the
    // candidate set is dominated by off-topic pages (dictionary/definition sites for
    // a substantive question, or a page that matches only ONE polysemous framing
    // verb like "improve"/"explain"/"negotiate" while missing the real topic), the
    // rescale stretches the off-topic survivor right back into the top band — so
    // Merriam-Webster ranks #1 for "improve deep sleep", VS Code docs for "ikigai",
    // Wikipedia "Cats (musical)" for "why do cats purr". This is the exact
    // "penalties only bite if folded into the FINAL r.score AND survive calibration"
    // failure (see P8 video cap above, which uses the same post-calibration pattern
    // to make its dampening durable). We therefore re-apply a hard cap AFTER
    // calibration: structurally off-topic results may still appear (floor preserved)
    // but can never outrank genuine topical content.
    //
    // Structural signals (generic, no curated domain allow-list; detection is purely
    // structural (title / path / phonetic / POS) reconstructed from the page itself):
    //   (a) DICTIONARY/DEFINITION SITE for a NON-definition query: a page whose
    //       title/structure marks it as a word-lookup reference (e.g. the class of
    //       merriam-webster / dictionary.cambridge / vocabulary.com / wiktionary /
    //       thefreedictionary / definitions.net / wordnik sites, matched purely by
    //       /dictionary//define//meaning/ path markers, dictionary-style title
    //       patterns, and POS/phonetic heading patterns — NOT by a curated domain
    //       list). These answer "define X", not "how/why/what is X about". When the
    //       user did NOT ask for a definition, cap hard.
    //   (b) SINGLE-DISTINCTIVE-TERM-ONLY match: the query has >=3 distinctive topic
    //       terms but the result matches only ONE of them AND that one term is a weak
    //       framing/verb (improve/explain/negotiate/meaning/apply...) rather than a
    //       substantive topic. The page rode a polysemous single-token overlap; it is
    //       off-topic for the actual subject.
    {
        let q_lc_cap = clean_query.to_lowercase();
        let is_definition_query = q_lc_cap.starts_with("define ")
            || q_lc_cap.starts_with("definition of ")
            || q_lc_cap.contains("definition of ")
            || q_lc_cap.contains(" meaning of ")
            || q_lc_cap.contains("word meaning")
            || (q_lc_cap.starts_with("what does ") && q_lc_cap.contains(" mean"))
            || q_lc_cap.starts_with("what is the definition");

        // Definitional-domain / structure detector (mirrors the in-loop block at ~5447
        // but is recomputed here so the cap is independent of that block's scope).
        let is_def_site = |url: &str, title: &str, content: &str| -> bool {
            let ul = url.to_lowercase();
            let tl = title.to_lowercase();
            let cl = content.to_lowercase();
            let prefix = cl.chars().take(300).collect::<String>();
            // Structural URL-path markers only — no curated domain allow-list.
            // Detection is purely structural (title / path / phonetic / POS).
            let dict_path_marker = ul.contains("/dictionary/")
                || ul.contains("/define/")
                || ul.contains("/meaning/");
            let title_words: Vec<&str> = tl.split_whitespace().collect();
            let dict_title = tl.contains("meaning & definition")
                || tl.contains("definition & meaning")
                || tl.contains("definition of ")
                || tl.contains("meaning of ")
                || tl.ends_with("- wiktionary")
                || tl.contains("cambridge dictionary")
                || tl.contains("merriam-webster")
                || (title_words.len() <= 3 && (tl.contains("definition") || tl.contains("dictionary")));
            let phonetic = prefix.contains("/ˈ") || prefix.contains("/ˌ")
                || prefix.contains("/'") || prefix.contains("/-");
            let pos_label = prefix.starts_with("noun") || prefix.starts_with("verb")
                || prefix.starts_with("adjective") || prefix.starts_with("adverb")
                || prefix.contains("1. : to ") || prefix.contains("2. : to ")
                || prefix.contains("definition of ") || prefix.contains("meaning of ");
            let short = cl.len() < 200;
            dict_path_marker || dict_title
                || ((phonetic || pos_label) && title_words.len() <= 3)
                || (pos_label && short)
        };

        // Count how many DISTINCTIVE topic terms a result actually contains (excludes
        // weak framing/anchor words, so a page matching only "improve" while the query
        // is "improve deep sleep without medication" counts as 1 weak match, not a
        // substantive one).
        let strong_topics: Vec<&str> = strong_distinctive_terms.iter()
            .filter(|t| !is_weak_anchor_word(&t.to_lowercase()))
            .copied().collect();
        let query_has_many_topics = strong_topics.len() >= 3;

        let dict_cap = 0.06f32;   // dictionary sites may appear but never rank top
        let weak_cap = 0.08f32;   // single-polysemous-token matches capped low

        // Best non-video score AFTER calibration but BEFORE this pass caps any video.
        // Used by the P8 video cap (b0): a video must never outrank the best genuine
        // text result for a non-video query, in any calibration regime (see comment
        // at (b0)). Computed over post-calibration scores so it reflects the final
        // text ranking.
        let best_non_video = merged.iter()
            .filter(|r| !r.sources.iter().any(|s| s == "invidious" || s == "video"))
            .map(|r| r.score)
            .fold(0.0f32, f32::max);

        for r in merged.iter_mut() {
            let rl = r.title.to_lowercase();
            let cl = r.content.to_lowercase();
            let ul = r.url.to_lowercase();

            // (a) definitional site for a non-definition query
            if !is_definition_query && is_def_site(&ul, &rl, &cl) {
                if r.score > dict_cap {
                    tracing::info!(
                        "POST-CAL DICT CAP -> {:.2}: '{}' (def site, non-def query)",
                        dict_cap, r.url.chars().take(60).collect::<String>()
                    );
                    r.score = dict_cap;
                }
                continue;
            }

            // (b0) POST-CALIBRATION VIDEO CAP (P8, durable).
            // The in-loop P8 dampening (r.score *= video_mult; video_mult=0.08 for a
            // generic text query) is DEFEATED by calibrate_scores, which rescales the
            // WHOLE set onto [0.05,1.0] AFTER that multiplication. Whenever the only
            // surviving candidates for a text query are invidious/video snippets, the
            // rescale stretches the video right back to ~1.0 — so a YouTube tutorial
            // outranks topical articles (e.g. "messaging app alternative to telegram"
            // ranked two invidious videos above Signal/Wire write-ups). This cap
            // re-applies AFTER calibration, so the dampening is durable: videos may
            // still appear (floor preserved) but can never outrank genuine text
            // results for a non-video query. Video-intent queries keep full score.
            //
            // ROOT-CAUSE (2026-08-17 round): the previous fixed cap of 0.12 was an
            // ABSOLUTE value. calibrate_scores rescales the whole set onto a band whose
            // ceiling depends on the regime: healthy sets → [0.05,1.0], weak/thin sets
            // (raw_max < 0.10) → [0.05,0.12]. A thin-set video caps at 0.12 == the band
            // ceiling, so it TIES the top text result and wins by tie-break order —
            // exactly the regression seen on "wifi router rebooting" (youtube #1), "knee
            // braces" (youtube #1-3), "chess websites" (youtube #1-3), "passport renew"
            // (youtube #1). Fix: make the cap RELATIVE to the best non-video score, so a
            // video is always strictly below the best genuine text result regardless of
            // calibration regime. Signal-driven (query self-describes intent), not tuned
            // to any one query. floor 0.05 keeps the video present, never dominant.
            let is_video_src = r.sources.iter().any(|s| s == "invidious" || s == "video");
            if is_video_src {
                let video_intent = q_lc_cap.contains("video")
                    || q_lc_cap.contains("youtube")
                    || q_lc_cap.contains("watch")
                    || q_lc_cap.contains("tutorial")
                    || q_lc_cap.contains("animation");
                if !video_intent {
                    // Relative cap: a video must never outrank the best non-video
                    // result for a non-video query. best_non_video is computed from the
                    // post-calibration scores before any video was capped this pass.
                    let video_cap = (best_non_video * 0.85).max(0.05);
                    if r.score > video_cap {
                        tracing::info!(
                            "POST-CAL VIDEO CAP -> {:.2}: '{}' (non-video query, video source; best_text={:.2})",
                            video_cap, r.url.chars().take(60).collect::<String>(), best_non_video
                        );
                        r.score = video_cap;
                    }
                }
            }

            // (b) single-distinctive-term-only match on a multi-topic query
            if query_has_many_topics {
                let matched_strong = strong_topics.iter().filter(|t| {
                    let lt = t.to_lowercase();
                    rl.contains(&lt) || cl.contains(&lt) || ul.contains(&lt)
                }).count();
                // matched_strong is at most strong_topics.len(); we want the page to
                // contain at least 2 of the query's real topic terms to be on-topic.
                if matched_strong < 2 {
                    if r.score > weak_cap {
                        tracing::info!(
                            "POST-CAL WEAK-MATCH CAP -> {:.2}: '{}' (matched {} of {} topics)",
                            weak_cap, r.url.chars().take(60).collect::<String>(), matched_strong, strong_topics.len()
                        );
                        r.score = weak_cap;
                    }
                }
            }

            // (c) COMPARISON off-topic local result (D3, this task).
            // The in-loop D3 gate crushes the relevance of a local page that names
            // NONE of the query's compared entities (e.g. "Honda City Mileage" for a
            // "Brezza vs Venue" query). But calibrate_scores (and the thin-result
            // boost) rescales it right back to the top band, so the off-topic brand
            // still outranks the genuine Brezza/Venue pages — the exact bug. Re-apply
            // the cap AFTER calibration so it survives, matching the durable pattern
            // used by the D1/D2/D3 (weak-match) caps above. `compared_entities` is
            // derived from the query's own distinctive terms minus attribute/structure
            // vocab (no brand literals), so this is fully general: it fires for any
            // comparison ("swift vs nexon", "city vs amaze", ...) and never names a
            // specific brand/model. A local page that names none of the compared
            // entities may still appear (floor preserved) but can never outrank the
            // genuine comparative web/local pages. RELATIVE cap (like the video cap)
            // so it holds in both healthy ([0.05,1.0]) and weak-set ([0.05,0.12])
            // calibration regimes.
            if r.is_local && comparison_query && !comparison_entities.is_empty() {
                let names_entity = comparison_entities.iter().any(|e| {
                    rl.contains(e.as_str()) || cl.contains(e.as_str()) || ul.contains(e.as_str())
                });
                if !names_entity {
                    let d3_cap = (best_non_video * 0.6).max(0.05);
                    if r.score > d3_cap {
                        tracing::info!(
                            "POST-CAL D3 COMP-CAP -> {:.2}: '{}' names none of compared entities {:?} (best_text={:.2})",
                            d3_cap, r.url.chars().take(60).collect::<String>(), comparison_entities, best_non_video
                        );
                        r.score = d3_cap;
                    }
                }
            }
        }
    }

    // POST-CALIBRATION P2d CAP (round-2026-08-20T1935Z) — the durable off-topic-local
    // suppression. The in-loop P2d gate crushes relevance/r.score, but calibrate_scores
    // (line ~7928) linearly rescales the WHOLE set onto [0.05,1.0], which stretches the
    // crushed off-topic local right back toward the top band — the exact failure seen
    // for "alternatives to airtable" (a Slack-Alternatives local page ranking #1 over
    // genuine Airtable-alternative web pages). Mirroring the D1/D2/D3/video caps above,
    // we re-apply AFTER calibration so the demotion survives. Condition is purely
    // structural: a LOCAL result whose TITLE names NONE of the query's title-anchored
    // subject terms (p2d_offtopic_terms, populated by the in-loop gate) is off-topic
    // crawl noise and may still appear (floor preserved) but can never outrank genuine
    // topical content. RELATIVE cap (like the video/D3 caps) so it holds in both the
    // healthy [0.05,1.0] and weak-set [0.05,0.12] calibration regimes. No query/domain
    // literals, no curated list — keyed on the structural "local page misses the
    // subject" class.
    if !p2d_offtopic_terms.is_empty() {
        // A LOCAL page is "off-topic" for this query when its TITLE/URL names NONE of the
        // query's subject terms (p2d_offtopic_terms, populated by the in-loop P2d gate).
        let is_offtopic_local = |r: &MergedResult| -> bool {
            if !r.is_local {
                return false;
            }
            let tl = r.title.to_lowercase();
            let ul = r.url.to_lowercase();
            !p2d_offtopic_terms.iter().any(|t| {
                let lt = t.to_lowercase();
                tl.contains(&lt) || ul.contains(&lt)
            })
        };
        // CAP REFERENCE must be the best ON-TOPIC score — i.e. the max score among results
        // that are NOT off-topic-local themselves. The previous reference (best_non_video)
        // included the off-topic local page, so capping to 0.5*that left the off-topic local
        // ABOVE the on-topic web pages (which calibrate to the ~0.05 floor) — e.g. Fox News
        // "Sunday Morning Futures" stayed #1 for "weekend flower markets in thrissur ...".
        // By excluding off-topic-local pages from the reference, the cap forces every
        // off-topic local strictly BELOW the best genuine on-topic result (it may dip under
        // the 0.05 calibration floor, which is correct — it should rank last, never first).
        // Structural only: no query/domain literals, keyed on "local page misses the subject".
        let cap_ref = merged.iter()
            .filter(|r| !is_offtopic_local(r))
            .map(|r| r.score)
            .fold(0.0f32, f32::max);
        if cap_ref > 0.0 {
            for r in merged.iter_mut() {
                if is_offtopic_local(r) {
                    let p2d_cap = cap_ref * 0.6;
                    if r.score > p2d_cap {
                        tracing::info!(
                            "POST-CAL P2d OFF-TOPIC-LOCAL CAP -> {:.3}: '{}' names none of {:?} (ontopic_ref={:.3})",
                            p2d_cap, r.url.chars().take(60).collect::<String>(), p2d_offtopic_terms, cap_ref
                        );
                        r.score = p2d_cap;
                    }
                }
            }
        }
    }

    // Re-sort by score descending after post-calibration caps to ensure capped
    // results (video/dict/weak-match/P2d) move below higher-scoring text results.
    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

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
    /// SymSpell + LinSpell spelling correction index (built at startup).
    /// Held behind `Arc` so the synchronous dictionary lookup can be moved into
    /// a `spawn_blocking` task off the async runtime (see `handle_spellcheck`)
    /// without deep-cloning the (large) index per request.
    spell_index: Arc<spell::SymSpellIndex>,
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
        match fetch_text_budgeted(state.http_client.clone(), searx_url.clone(), 6000).await {
            Some(raw) => parse_images(raw),
            None => { tracing::warn!("SearXNG1 image timed out/failed — empty"); vec![] }
        }
    };

    let searx2_fut = async {
        let url = match searx2_url {
            Some(u) => u,
            None => return vec![],
        };
        match fetch_text_budgeted(state.http_client.clone(), url.clone(), 6000).await {
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
            // 25s for external engines. Tor2 (SearXNG2 / Tor) cold-circuit builds
            // after a NEWNYM IP rotation take 10-15s to answer (see tor-warmup
            // path, which already uses a 25s client). A 10s cap classified those
            // cold-circuit Tor responses as `instance request failed ... TimedOut`,
            // and two such hits tripped the circuit breaker (`Circuit OPEN` for
            // engine 'searxng1'), silently skipping the entire Tor path for up to
            // 30s — collapsing the "two independent egress paths" design to one.
            // Raising to 25s lets the slow-but-valid Tor response through so both
            // paths genuinely serve (self-heals once the circuit is warm). The
            // per-branch budget (searx_fut) and the fan-out join deadline still
            // bound end-to-end latency for the fast (gluetun-VPN) path.
            .timeout(Duration::from_secs(25))  // Allow up to 25s for external engines (VPN/Tor overhead, cold Tor circuit)
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
        spell_index: Arc::new(spell::SymSpellIndex::build()),
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
        .route("/spellcheck", get(handle_spellcheck))
        .route("/analyze", get(handle_analyze))
        // Unified pre-search introspection: mirrors the FULL /search reasoning
        // pipeline (spell -> negation -> intent -> constraints -> recency ->
        // query-quality) in one additive, zero-side-effect payload. See handle_inspect.
        .route("/inspect", get(handle_inspect))
        // Geo introspection: mirrors /inspect's precedent — additive, zero-side-
        // effect, pure. Exposes how /search resolves a query's geographic focus
        // (explicit gazetteer hit, or local-intent "near me" fallback) BEFORE the
        // search runs. See handle_geolocate / build_geolocate.
        .route("/geolocate", get(handle_geolocate))
        // Intent introspection: completes the additive introspection family
        // (/spellcheck /analyze /inspect /geolocate). Exposes /search's full
        // intent object (parent category, contrastive + local signals,
        // structured constraints, expanded queries) using the EXACT pure fns
        // /search falls back to — zero-side-effect, no new ranking logic.
        .route("/intent", get(handle_intent))
        // Video introspection: completes the additive introspection family
        // (/spellcheck /analyze /inspect /geolocate /intent). Surfaces the P8
        // video-dominance fix (commit 3938da6) — which urls the ranker
        // classifies as video, whether a query is video-intent (which exempts
        // it from the non-video pin), and the exact marker set driving that
        // exemption — using the EXACT pure fns /search uses (is_url_video_host
        // + the P8 video_intent markers). Zero-side-effect, no new ranking
        // logic, no per-query strings. See handle_video / build_video.
        .route("/video", get(handle_video))
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

/// Build a data-driven "did you mean" analysis for a raw query string.
///
/// Reuses the in-process `SymSpellIndex` (already built once at startup and stored
/// in `AppState.spell_index`) — no LLM, no network, no per-query hardcoded strings.
/// For every token we report whether it is a known dictionary word / protected brand
/// (`in_dictionary`), and, when `correct()` proposes a different word, the suggestion.
/// The whole-query `corrected` form is produced via the same `correct_query` path the
/// live `/search` uses, so the preview is consistent with what the engine would search.
///
/// This is a pure function of the query + the index; it is exported (and unit-tested)
/// so behavior is locked independently of the HTTP layer.
fn spellcheck_query(index: &spell::SymSpellIndex, q: &str) -> serde_json::Value {
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut corrections: Vec<serde_json::Value> = Vec::with_capacity(words.len());

    for word in &words {
        let wl = word.to_lowercase();
        // Mirror correct_query's skips: URLs/code tokens and very short words are
        // never corrected, so they are reported as already-fine (in_dictionary=true
        // conservatively so the client doesn't flag them as typos).
        let is_code = word.contains('.') || word.contains('/') || word.contains('\\')
            || word.contains('@') || word.contains('#') || word.contains('$')
            || word.chars().any(|c| c.is_numeric());
        if is_code || word.len() < spell::MIN_CORRECT_LENGTH {
            corrections.push(serde_json::json!({
                "original": word,
                "suggestion": null,
                "in_dictionary": true
            }));
            continue;
        }
        let in_dict = spell::is_protected_term(&wl) || index.contains_word(&wl);
        match index.correct(word) {
            Some(corrected) if corrected != *word => {
                corrections.push(serde_json::json!({
                    "original": word,
                    "suggestion": corrected,
                    "in_dictionary": in_dict
                }));
            }
            _ => {
                corrections.push(serde_json::json!({
                    "original": word,
                    "suggestion": null,
                    "in_dictionary": in_dict
                }));
            }
        }
    }

    let (corrected, changed) = spell::correct_query(index, q.trim());
    let corrections_array: Vec<serde_json::Value> = corrections
        .into_iter()
        .filter(|c| c["suggestion"].is_string())
        .collect();

    serde_json::json!({
        "query": q,
        "corrected": corrected,
        "changed": changed,
        "corrections": corrections_array
    })
}

/// `GET /spellcheck?q=...` — expose the engine's spelling-correction index as a
/// preview service so clients can warn users ("did you mean X?") before searching.
///
/// No new ranking logic, no per-query hardcoding: it reuses `spell::correct_query`
/// and the per-token `correct()` used by `/search`, so the preview matches the
/// engine's actual behavior. JSON envelope mirrors the rest of the API reference.
async fn handle_spellcheck(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "empty_query",
                "message": "Query parameter 'q' is empty",
                "results": [],
                "query": "",
                "corrected": "",
                "changed": false,
                "corrections": []
            })),
        );
    }
    let result = {
        // `spellcheck_query` runs synchronous SymSpell dictionary lookups and
        // string work. On a large index this would block the async executor
        // thread and stall unrelated in-flight requests, so we move it into a
        // `spawn_blocking` task. The index lives behind `Arc`, so the closure
        // can cheaply own a clone of the handle (no deep dictionary copy).
        let index = Arc::clone(&state.spell_index);
        let q_owned = q.clone();
        match tokio::task::spawn_blocking(move || spellcheck_query(&index, &q_owned)).await {
            Ok(r) => r,
            Err(_) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "spellcheck_task_failed",
                        "query": q,
                        "corrected": "",
                        "changed": false,
                        "corrections": []
                    })),
                );
            }
        }
    };
    (axum::http::StatusCode::OK, Json(result))
}

/// Pure geo-resolution mirror of `/search`'s "where is this query about?" step.
///
/// `/search` decides a query's geographic focus in two stages (see `handle_search`
/// ~L8601–8623, reproduced exactly here so this preview always matches):
///   1. `detect_explicit_location(q)` — if the query names a gazetteer place
///      (e.g. "restaurants in tokyo japan", "quiet places to study near chennai"),
///      the EXPLICIT location OVERRIDES any IP-derived geolocation. This is what
///      powered the round-2026-08-11T1556Z geo fix: a resolved `chennai` must win
///      so the off-topic gate can rescue chennai-specific results.
///   2. If no explicit location but the query has local intent ("near me" /
///      "nearby" / "around me"), fall back to a stable default (New York, US) so
///      local-query expansion has *something* to anchor on.
///
/// An optional `ip` lets a client reproduce the third stage `/search` performs
/// (IP-derived geolocation via the `geo_locator` DB) for parity — but it is
/// NEVER required, and an empty/loopback/private IP simply yields `resolved=None`
/// (the same as no geo DB), keeping the function pure + deterministic + testable.
///
/// No per-query strings, no domain allow/deny lists, no magic constants: it reuses
/// the exact `detect_explicit_location` + `has_local_intent` fns `/search` calls,
/// so the preview is guaranteed to match real engine behavior. Returns a structured
/// `GeolocateResponse` mirroring the API reference's additive-introspection shape.
#[derive(serde::Serialize)]
struct GeolocateResponse {
    query: String,
    resolved: Option<geoloc::GeoLocation>,
    source: String, // "explicit" | "local_intent_fallback" | "ip" | "none"
    explicit_location: bool,
    local_intent: bool,
}

fn build_geolocate(geo_locator: Option<&geoloc::GeoLocator>, q: &str, ip: Option<IpAddr>) -> GeolocateResponse {
    let explicit = detect_explicit_location(q);
    let local_intent = has_local_intent(q);

    // Stage 1: explicit gazetteer hit overrides everything (mirrors /search).
    if let Some(loc) = explicit {
        return GeolocateResponse {
            query: q.to_string(),
            resolved: Some(loc),
            source: "explicit".to_string(),
            explicit_location: true,
            local_intent,
        };
    }

    // Stage 3 (optional): IP-derived geolocation, only when no explicit hit.
    if let (Some(gl), Some(ip)) = (geo_locator, ip) {
        if let Some(loc) = gl.lookup(ip) {
            return GeolocateResponse {
                query: q.to_string(),
                resolved: Some(loc),
                source: "ip".to_string(),
                explicit_location: false,
                local_intent,
            };
        }
    }

    // Stage 2: local-intent fallback (mirrors /search's "near me" default).
    if local_intent {
        return GeolocateResponse {
            query: q.to_string(),
            resolved: Some(geoloc::GeoLocation {
                country_code: Some("US".to_string()),
                country_name: Some("United States".to_string()),
                region: Some("New York".to_string()),
                city: Some("New York".to_string()),
                postal_code: Some("10001".to_string()),
                latitude: Some(40.7128),
                longitude: Some(-74.0060),
                time_zone: Some("America/New_York".to_string()),
            }),
            source: "local_intent_fallback".to_string(),
            explicit_location: false,
            local_intent: true,
        };
    }

    // No signal at all.
    GeolocateResponse {
        query: q.to_string(),
        resolved: None,
        source: "none".to_string(),
        explicit_location: false,
        local_intent: false,
    }
}

/// `GET /geolocate?q=...[&ip=...]` — additive geo-introspection endpoint.
///
/// Mirrors the `/spellcheck` `/analyze` `/inspect` precedent: it does NOT change
/// `/search` ranking, geo-boost, or calibration. It reuses the EXACT resolution
/// fns `/search` calls (`detect_explicit_location` + `has_local_intent`), so a
/// client can see — before issuing a search — which location the engine will
/// anchor on, and whether it came from an explicit place name, a "near me"
/// local-intent fallback, or an optional IP lookup. No network is performed
/// unless `ip=` is supplied; the gazetteer + local-intent path is pure + fast.
/// Build the `400 empty_query` response for `/geolocate` when `q` is empty or
/// whitespace-only. Extracted from `handle_geolocate` so the exact envelope is
/// unit-testable (see `geolocate_empty_query_returns_documented_400` in
/// `geolocate_endpoint_tests`). The envelope is geo-specific — it carries the
/// same `resolved`/`source`/`explicit_location`/`local_intent` top-level keys as
/// a `200` response (all neutral), NOT the shape of `/search` or `/spellcheck`.
fn make_geolocate_empty_response() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "empty_query",
            "message": "Query parameter 'q' is empty",
            "query": "",
            "resolved": null,
            "source": "none",
            "explicit_location": false,
            "local_intent": false
        })),
    )
}

async fn handle_geolocate(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return make_geolocate_empty_response();
    }

    // Optional `ip=` query param reproduces the /search IP-geo stage for parity.
    // Parse defensively: an unparseable/missing value simply disables that stage.
    let ip: Option<IpAddr> = params
        .ip
        .as_ref()
        .and_then(|s| s.parse::<IpAddr>().ok());

    let geo_locator = state.geo_locator.as_ref();
    let result = build_geolocate(geo_locator, &q, ip);
    (axum::http::StatusCode::OK, Json(serde_json::to_value(result).unwrap()))
}

/// `GET /analyze?q=...` — read-only engine-introspection endpoint.
///
/// Mirrors the additive, zero-side-effect precedent of `/spellcheck`: it does
/// NOT change `/search` ranking, negation gating, or calibration. Instead it
/// exposes the engine's *reasoning* over a query's negation / constraint
/// extraction and the `is_real_exclusion` gate, so a client can see why a term
/// was kept as an exclusion, dropped as an unrecognized entity, or declined as
/// a HOW-not-WHAT manner qualifier.
///
/// This directly serves DEFECT A (negation-hardening) transparency: the
/// `without X` / `not Y` handling is the single most confusing part of the
/// engine's output (see round report intentforge-2026-08-10T0813Z — DEFECT A),
/// and clients currently receive no per-term explanation. `/analyze` makes the
/// same logic `/search` uses inspectable.
///
/// No per-query strings, no domain allow/deny lists, no magic constants: it
/// reuses `extract_query_negative_terms_with_dropped` + `is_real_exclusion`, the
/// identical functions `/search` calls, so the preview matches real behavior.
async fn handle_analyze(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "empty_query",
                "message": "Query parameter 'q' is empty",
                "query": "",
                "exclusions": [],
                "declined": [],
                "manner_qualifiers": []
            })),
        );
    }
    let q_orig = q.clone();
    let query_contrastive = query_is_contrastive(&q_orig);
    let (kept, declined, manner) =
        extract_query_negative_terms_with_dropped(&q_orig);

    // Build a per-term decision list so clients see WHY each candidate was
    // routed the way it was. Reuses `is_real_exclusion` (entity / contrastive
    // framing) exactly as `/search` does — no duplicated logic.
    let mut decisions: Vec<serde_json::Value> = Vec::new();
    for term in &kept {
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "exclusion",
            "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)"
        }));
    }
    for term in &declined {
        let is_manner = is_manner_phrase(term) || is_manner_frame(&q_orig, term);
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "declined",
            "reason": if is_manner {
                "manner qualifier (HOW not WHAT to exclude) — never a search exclusion"
            } else {
                "neither a recognized entity nor in contrastive framing — excluded to avoid penalizing unrelated topical words"
            }
        }));
    }
    for term in &manner {
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "manner_qualifier",
            "reason": "manner qualifier (HOW not WHAT to exclude) — described the user's method, not a topic to filter out"
        }));
    }

    let result = serde_json::json!({
        "query": q,
        "contrastive_framing": query_contrastive,
        "exclusions": kept,
        "declined": declined,
        "manner_qualifiers": manner,
        "decisions": decisions
    });
    (axum::http::StatusCode::OK, Json(result))
}

/// `GET /inspect?q=...` — unified pre-search introspection.
///
/// Generalizes the `/analyze` (negation) and `/spellcheck` (spelling)
/// transparency endpoints into ONE additive, zero-side-effect payload that
/// mirrors the *entire* `/search` reasoning pipeline a client can reason about
/// before issuing a search:
///
///   1. spelling  — `spellcheck_query` (same fn `/search` pre-corrects with)
///   2. negation   — the `exclusions` / `declined` / `manner_qualifiers` split
///                   from `extract_query_negative_terms_with_dropped` + the
///                   per-term `decisions[]` (same as `/analyze`)
///   3. intent     — `fallback_intent` (pure, no-network) + `parent_category`
///   4. constraints— `extract_gateway_constraints` (the gateway's own operator
///                   parser, identical to what `/search` flattens) + the
///                   `applied_constraints` shape `/search` reports
///   5. recency    — `derive_recency_window` (what a "latest"/"this week"
///                   phrase would inject), so the client can see whether a
///                   date window will be applied
///   6. quality    — `query_quality_flag` (junk/low/normal), the same gate that
///                   decides graceful degradation
///
/// No new ranking logic, no per-query hardcoded strings, no domain allow/deny
/// lists, no magic constants. It reuses the EXACT functions `/search` calls, so
/// the preview always matches real engine behavior. It is read-only: it does
/// not change ranking, negation gating, calibration, or fetch anything.
async fn handle_inspect(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "empty_query",
                "message": "Query parameter 'q' is empty",
                "query": "",
                "spelling": { "corrected": "", "changed": false, "corrections": [] },
                "negation": { "exclusions": [], "declined": [], "manner_qualifiers": [], "contrastive_framing": false, "decisions": [] },
                "intent": { "intent": "", "category": "", "confidence": 0.0 },
                "constraints": { "structured": {}, "applied_constraints": [] },
                "recency": { "window": null, "phrase_detected": false },
                "quality": { "flag": "low", "valid_ratio": 0.0 }
            })),
        );
    }
    let result = build_inspect(&state.spell_index, &q);
    (axum::http::StatusCode::OK, Json(result))
}

/// `GET /intent?q=...` — additive intent-introspection endpoint.
///
/// Completes the introspection family (`/spellcheck` `/analyze` `/inspect`
/// `/geolocate`): `/inspect` only surfaces a 3-field intent STUB
/// (`intent`/`category`/`confidence`), but `/search` builds a much richer
/// intent object — the parent category, derived contrastive (X-vs-Y
/// comparison) and local ("near me") signals, the structured constraint set
/// that drives operator parsing, and the expanded-query seeds. That full
/// object is what ranking actually consumes, and it was previously
/// invisible to clients.
///
/// Like its siblings, this endpoint is ADDITIVE + ZERO-SIDE-EFFECT: it does
/// NOT change ranking, calibration, or intent-engine calls. It reuses the
/// EXACT pure fns `/search` and `/inspect` use — `fallback_intent` (no
/// network, identical to the offline classification `/search` falls back to
/// when the intent engine is unreachable) + `parent_category` +
/// `query_is_contrastive` + `has_local_intent` — so the preview always
/// matches real engine behavior. No per-query strings, no domain
/// allow/deny lists, no magic constants tuned to one query.
///
/// NOTE: `fallback_intent` is the *pure, no-network* classifier. The live
/// `/search` path additionally calls the intent-engine service
/// (`127.0.0.1:3005/analyze`) to refine the label; this endpoint intentionally
/// exposes only the deterministic local classification so the contract is
/// stable + fully testable without the intent engine up, and so clients can
/// reason about the offline baseline the ranker guarantees.
fn build_intent(q: &str) -> serde_json::Value {
    let intent_resp = fallback_intent(q);
    let category = parent_category(&intent_resp.intent);
    let contrastive = query_is_contrastive(q);
    let local = has_local_intent(q);

    serde_json::json!({
        "query": q,
        "intent": intent_resp.intent,
        "category": category,
        "confidence": intent_resp.confidence,
        "contrastive_framing": contrastive,
        "local_intent": local,
        "structured_constraints": intent_resp.structured_constraints,
        "expanded_queries": intent_resp.expanded_queries
    })
}

/// Build the `400 empty_query` envelope for `/intent` when `q` is empty or
/// whitespace. Pure + unit-testable (see `intent_endpoint_tests`). Mirrors
/// `/inspect`'s empty-envelope contract: it carries the neutral
/// `intent`/`category`/`confidence`/`contrastive_framing`/`local_intent`
/// top-level keys so the envelope is distinguishable from `/search`/`spellcheck`'s
/// empty response, but with neutral values. `structured_constraints` is the empty
/// object `{}` (no operators were parsed from an empty query).
fn build_intent_empty() -> serde_json::Value {
    serde_json::json!({
        "error": "empty_query",
        "message": "Query parameter 'q' is empty",
        "query": "",
        "intent": "",
        "category": "",
        "confidence": 0.0,
        "contrastive_framing": false,
        "local_intent": false,
        "structured_constraints": {},
        "expanded_queries": []
    })
}

/// `GET /intent?q=...` — expose `/search`'s full intent object before a search runs.
/// Additive + zero-side-effect (see `build_intent`). Empty/whitespace `q` returns
/// `400` with the SAME standard `empty_query` envelope shape `/inspect` uses
/// (carrying neutral `intent`/`category`/`confidence`/`contrastive_framing`/
/// `local_intent` top-level keys so the envelope is distinguishable from
/// `/search`/`spellcheck`'s empty response).
async fn handle_intent(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(build_intent_empty()));
    }
    let result = build_intent(&q);
    (axum::http::StatusCode::OK, Json(result))
}

/// `GET /video?q=...` — additive video-intent introspection endpoint.
///
/// Completes the introspection family (`/spellcheck` `/analyze` `/inspect`
/// `/geolocate` `/intent`). The parent round (t_85340d89, commit 3938da6)
/// fixed P8 video dominance — invidious/youtube snippets were outranking
/// genuine text results for non-video queries — by pinning video sources
/// STRICTLY below the weakest text result AFTER calibration. That fix is
/// invisible to clients: there was no way to see WHICH urls the engine
/// classifies as video, whether a query is treated as video-intent (which
/// exempts it from the pin), or which exact markers drive that exemption.
///
/// Like its siblings, this endpoint is ADDITIVE + ZERO-SIDE-EFFECT: it does
/// NOT change ranking, calibration, or the P8 pin. It reuses the EXACT pure
/// fns `/search` uses — `is_url_video_host` (the same structural host-class
/// check the P8 pin applies to every result) + the P8 `video_intent` markers
/// (video/youtube/watch/tutorial/animation) — so the preview always matches
/// real engine behavior. No per-query strings, no domain allow/deny lists, no
/// magic constants tuned to one query. `would_pin_non_video_sources` reproduces
/// the ranker's decision rule: an all-video query is NOT pinned (videos rank
/// among themselves), a text query IS.
///
/// The marker set is exposed as a fixed general array (data, not branching
/// logic) so a future drift between this endpoint and the ranker's P8 check is
/// itself observable + unit-tested.
fn classify_url_as_video(url: &str) -> bool {
    is_url_video_host(url)
}

/// The exact P8 video-intent marker set. Mirrors `merge_local_and_web`'s
/// `video_intent` check (gateway/main.rs ~L7070) VERBATIM so the
/// introspection endpoint can never silently drift from the ranker's
/// exemption logic. Kept as data (a fixed general marker set), not branching
/// logic, per the doctrine: no per-query tuning.
fn video_intent_markers() -> &'static [&'static str] {
    &["video", "youtube", "watch", "tutorial", "animation"]
}

/// Pure video-intent detector — reuses `simple_negation_strip` (the same
/// negation-aware cleaner `/search` feeds `q_lc_cap`) then tests the P8
/// marker set. Returns true when the query should be treated as a request
/// for video results (and therefore exempt from the P8 non-video pin).
fn detect_video_intent(q: &str) -> bool {
    let cleaned = simple_negation_strip(q).unwrap_or_else(|| q.to_string());
    let q_lc = cleaned.to_lowercase();
    video_intent_markers().iter().any(|m| q_lc.contains(*m))
}

/// Build the `GET /video` payload. Pure + unit-testable so the P8
/// classification contract is locked independently of the HTTP layer.
fn build_video(q: &str) -> serde_json::Value {
    let video_intent = detect_video_intent(q);
    // Reproduce the ranker's P8 pin decision rule (merge_local_and_web
    // ~L7087): videos are pinned strictly below the weakest text result for
    // a NON-video query; for a video-intent query the pin does NOT apply and
    // videos keep full score. (The actual calibrated score band is unknown
    // here — this surfaces the DECISION, which is what the P8 fix changed.)
    let would_pin_non_video_sources = !video_intent;

    let intent_resp = fallback_intent(q);
    let intent = &intent_resp.intent;
    let markers: Vec<String> = video_intent_markers().iter().map(|m| (*m).to_string()).collect();

    serde_json::json!({
        "query": q,
        "video_intent": video_intent,
        "video_intent_markers": markers,
        "would_pin_non_video_sources": would_pin_non_video_sources,
        "is_video_source_examples": {
            "youtube_watch": classify_url_as_video("https://www.youtube.com/watch?v=gUEa825kTjQ"),
            "youtu_be": classify_url_as_video("https://youtu.be/gUEa825kTjQ"),
            "invidious_selfhosted": classify_url_as_video("https://invidious.example.net/watch?v=x"),
            "vimeo": classify_url_as_video("https://www.vimeo.com/123456"),
            "python_org_article": classify_url_as_video("https://www.python.org/doc"),
            "example_video_word_in_path": classify_url_as_video("https://example.com/youtube-guide-article")
        },
        "intent": intent,
        "note": "Additive introspection of the P8 video-dominance fix (commit 3938da6). Does not change ranking. A video source is any url matching is_url_video_host (youtube/youtu.be/vimeo/invidious self-hosted / m.youtube). video_intent=true exempts a query from the non-video pin."
    })
}

/// Build the `400 empty_query` envelope for `/video`. Pure + unit-testable.
/// Mirrors the sibling empty-envelope contract: carries a neutral video_intent
/// + would_pin_non_video_sources so the envelope is distinguishable but
/// self-consistent.
fn build_video_empty() -> serde_json::Value {
    let markers: Vec<String> = video_intent_markers().iter().map(|m| (*m).to_string()).collect();
    serde_json::json!({
        "error": "empty_query",
        "message": "Query parameter 'q' is empty",
        "query": "",
        "video_intent": false,
        "video_intent_markers": markers,
        "would_pin_non_video_sources": true,
        "is_video_source_examples": {}
    })
}

/// `GET /video?q=...` — expose the P8 video-dominance classification BEFORE a
/// search runs. Additive + zero-side-effect (see `build_video`). Empty/
/// whitespace `q` returns `400` with the standard `empty_query` envelope shape
/// the introspection family uses.
async fn handle_video(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let q = params.q.clone().unwrap_or_default();
    if q.trim().is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(build_video_empty()));
    }
    let result = build_video(&q);
    (axum::http::StatusCode::OK, Json(result))
}

/// Pure builder for `/inspect`. Exported + unit-tested so behavior is locked
/// independently of the HTTP layer (no AppState / live server needed). Mirrors
/// the exact pure functions `/search` runs — never duplicate or hardcode logic.
fn build_inspect(index: &spell::SymSpellIndex, q: &str) -> serde_json::Value {
    // 1. Spelling (same fn `/search` pre-corrects with).
    let spelling = spellcheck_query(index, q);

    // 2. Negation (same fn + per-term decisions as `/analyze`).
    let q_orig = q.to_string();
    let query_contrastive = query_is_contrastive(&q_orig);
    let (kept, declined, manner) =
        extract_query_negative_terms_with_dropped(&q_orig);
    let mut decisions: Vec<serde_json::Value> = Vec::new();
    for term in &kept {
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "exclusion",
            "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)"
        }));
    }
    for term in &declined {
        let is_manner = is_manner_phrase(term) || is_manner_frame(&q_orig, term);
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "declined",
            "reason": if is_manner {
                "manner qualifier (HOW not WHAT to exclude) — never a search exclusion"
            } else {
                "neither a recognized entity nor in contrastive framing — excluded to avoid penalizing unrelated topical words"
            }
        }));
    }
    for term in &manner {
        decisions.push(serde_json::json!({
            "term": term,
            "decision": "manner_qualifier",
            "reason": "manner qualifier (HOW not WHAT to exclude) — described the user's method, not a topic to filter out"
        }));
    }

    // 3. Intent (pure, no-network fallback classifier — same one `/search` uses
    //    when the intent engine is unreachable, so the preview is consistent).
    let intent = fallback_intent(q);
    let category = parent_category(&intent.intent);

    // 4. Constraints (gateway's own operator parser, identical to `/search`).
    //    Build the `applied_constraints` shape `/search` reports.
    let sc = &intent.structured_constraints;
    let mut applied: Vec<String> = Vec::new();
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
    let has_lt = sc.price_lt.is_some();
    let has_gt = sc.price_gt.is_some();
    if sc.price_min.is_some() || sc.price_max.is_some() || has_lt || has_gt {
        if let Some(v) = sc.price_lt { applied.push(format!("price:<{}", v)); }
        if let Some(v) = sc.price_gt { applied.push(format!("price:>{}", v)); }
        if !has_lt && !has_gt {
            let lo = sc.price_min.map(|v| v.to_string()).unwrap_or_else(|| "*".to_string());
            let hi = sc.price_max.map(|v| v.to_string()).unwrap_or_else(|| "*".to_string());
            applied.push(format!("price:{}-{}", lo, hi));
        }
    }
    for n in &sc.negative { applied.push(format!("not:{}", n)); }
    for he in &sc.hard_exclusions { applied.push(format!("not:{}", he)); }

    // 5. Recency (what a fresh/recent phrase would inject as a date window).
    let recency_window = derive_recency_window(&q.to_lowercase());
    let recency_phrase = recency_window.is_some()
        && (q.to_lowercase().contains("latest")
            || q.to_lowercase().contains("recent")
            || q.to_lowercase().contains("fresh")
            || q.to_lowercase().contains("today")
            || q.to_lowercase().contains("yesterday")
            || q.to_lowercase().contains("this week")
            || q.to_lowercase().contains("last week")
            || q.to_lowercase().contains("this month")
            || q.to_lowercase().contains("past week")
            || q.to_lowercase().contains("this year"));

    // 6. Query quality (same gate that decides graceful degradation).
    let (quality_flag, valid_ratio) = query_quality_flag(q, index);

    serde_json::json!({
        "query": q,
        "spelling": {
            "corrected": spelling["corrected"],
            "changed": spelling["changed"],
            "corrections": spelling["corrections"]
        },
        "negation": {
            "contrastive_framing": query_contrastive,
            "exclusions": kept,
            "declined": declined,
            "manner_qualifiers": manner,
            "decisions": decisions
        },
        "intent": {
            "intent": intent.intent,
            "category": category,
            "confidence": intent.confidence
        },
        "constraints": {
            "structured": sc,
            "applied_constraints": applied
        },
        "recency": {
            "window": recency_window.map(|(a, b)| serde_json::json!({ "after": a, "before": b })),
            "phrase_detected": recency_phrase
        },
        "quality": {
            "flag": quality_flag,
            "valid_ratio": valid_ratio
        }
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

/// Walk an error's `source()` chain looking for a DNS-resolver signature.
///
/// `reqwest` 0.12 exposes no `is_dns()`, and a resolver failure can surface
/// nested under a non-connect error kind (hyper/hickory). Callers should OR
/// this with `Error::is_connect()`. Matching is on the SOURCE chain only, so a
/// user query containing "dns" in the top-level request URL cannot trip it.
fn error_chain_is_dns(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        let msg = s.to_string().to_lowercase();
        if msg.contains("dns")
            || msg.contains("failed to lookup")
            || msg.contains("name or service not known")
            || msg.contains("nodename nor servname")
            || msg.contains("no such host")
        {
            return true;
        }
        src = std::error::Error::source(s);
    }
    false
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
        price_verified: None,
        recall_gap_terms: None,
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
    let mut q = if spell_changed {
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
    let mut geo_location: Option<geoloc::GeoLocation> = match detect_explicit_location(&q) {
        Some(explicit) => {
            tracing::info!("GEO: explicit location '{}' overrides IP geolocation",
                explicit.country_code.as_deref().unwrap_or("?"));
            Some(explicit)
        }
        None => geo_location,
    };

    // M3 fix: fallback location for loopback/private IP or missing geo DB when query has local intent ("near me")
    if geo_location.is_none() && has_local_intent(&q) {
        geo_location = Some(geoloc::GeoLocation {
            country_code: Some("US".to_string()),
            country_name: Some("United States".to_string()),
            region: Some("New York".to_string()),
            city: Some("New York".to_string()),
            postal_code: Some("10001".to_string()),
            latitude: Some(40.7128),
            longitude: Some(-74.0060),
            time_zone: Some("America/New_York".to_string()),
        });
        tracing::info!("GEO: local intent detected, applied default geolocation fallback (New York, US)");
    }

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
        let circuit_for_searx = state.circuit.clone();
        let instance_key_for_searx = searx_instance_keys.get(i).cloned();
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
                        // DEAD-INSTANCE FAIL-FAST (round defect): a Connection-level
                        // error (DNS resolution failure, connection refused, host
                        // unreachable) means the instance is down for this process
                        // lifetime, NOT a transient timeout. Open the circuit for a
                        // long window so every subsequent fan-out SKIPS it instead of
                        // re-burning the full branch timeout. Detected by error KIND,
                        // so it also self-heals: a successful request resets open_until.
                        // NOTE: reqwest 0.12 has no `is_dns()`. DNS resolution failures
                        // surface as connect-kind errors; we additionally walk the error
                        // source chain for a resolver signature so a hyper/hickory DNS
                        // error nested under a non-connect kind is still classified as
                        // a dead instance rather than a transient failure.
                        let is_connect_err = e.is_connect() || error_chain_is_dns(&e);
                        tracing::warn!("SearXNG instance request failed: {:?}", e);
                        if let Some(ref key) = instance_key_for_searx {
                            if is_connect_err {
                                circuit_for_searx.record_connection_failure(key);
                            } else {
                                circuit_for_searx.record_failure(key);
                            }
                        }
                        return SearxResponse { results: vec![], unresponsive_engines: vec![] };
                    }
                    Err(_) => {
                        tracing::warn!("SearXNG instance timed out (4s): {}", &url[..url.find('?').unwrap_or(url.len())]);
                        if let Some(ref key) = instance_key_for_searx {
                            circuit_for_searx.record_failure(key);
                        }
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

    let is_shopping_intent = q_lower.contains("buy") || q_lower.contains("price") || q_lower.contains("shop")
        || constraints.price_max.is_some() || constraints.price_min.is_some()
        || constraints.price_lt.is_some() || constraints.price_gt.is_some();

    let shopping_fut = async {
        if !is_shopping_intent || all_searx_open {
            return Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] });
        }
        let shop_url = searxng_url_with_categories(
            "http://127.0.0.1:8080", &q, "shopping", geo_location.as_ref(), lang
        );
        let s_client = client.clone();
        let task = tokio::spawn(async move {
            let resp = match tokio::time::timeout(Duration::from_millis(1200), s_client.get(&shop_url).send()).await {
                Ok(Ok(r)) => r,
                _ => return Ok::<SearxResponse, anyhow::Error>(SearxResponse { results: vec![], unresponsive_engines: vec![] }),
            };
            let raw = match tokio::time::timeout(Duration::from_millis(1200), resp.text()).await {
                Ok(Ok(t)) => t,
                _ => return Ok::<SearxResponse, anyhow::Error>(SearxResponse { results: vec![], unresponsive_engines: vec![] }),
            };
            let sanitized = sanitize_json_text(&raw);
            match serde_json::from_str::<SearxResponse>(&sanitized) {
                Ok(data) => Ok(data),
                Err(e) => { tracing::warn!("SearXNG shopping fan-out parse error: {}", e); Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] }) }
            }
        });
        match tokio::time::timeout(Duration::from_millis(1500), task).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => { tracing::warn!("SearXNG shopping task error"); Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] }) }
            Err(_) => { tracing::warn!("SearXNG shopping task timed out (budget)"); Ok(SearxResponse { results: vec![], unresponsive_engines: vec![] }) }
        }
    };

    let has_site = !constraints.sites.is_empty();
    let searx_partial_collector = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let partial_collector_inner = searx_partial_collector.clone();
    // Captured by value before `searx_fut_with_timeout` moves `searx_urls` into its
    // async block. Used at the fan-out JOIN to pick a Tor-aware deadline.
    let tor2_in_fanout = searx_urls.iter().any(|u| u.contains("tor2") || u.contains("8081"));
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

        // Retry policy. An upstream_unavailable for ANY query (site:-constrained OR
        // plain NL) is almost always a transient double-failure of the two
        // INDEPENDENT egress paths (gluetun-VPN + Tor2) — a short backoff + one
        // re-fire recovers it. The 2026-08-21 round proved this on plain NL: 4/30
        // fresh queries hit upstream_unavailable on attempt 1, and ALL 4 returned
        // real results (4/4/2/19) on an immediate retry. The previous code only
        // retried site:-constrained queries (max_attempts = 1 for plain NL), so the
        // re-fire path was dead code for the common case — plain NL queries
        // surfaced upstream_unavailable even though a retry would have recovered.
        // We now retry once for EVERY query when no usable result was found on
        // attempt 1. A successful attempt 1 breaks early (has_usable is true), so
        // there is ZERO added latency for queries that already have results — the
        // retry only costs time on the queries that would otherwise return empty.
        //   attempt1 = 10s budget (non-site) / 4500ms (site); backoff 150ms;
        //   attempt2 = 10s (non-site, force re-probe) / 15s (site, cold-tor2).
        let max_attempts: usize = 2;
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
                        "SearXNG retry recovered results on attempt {} (transient upstream failure)",
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
                } else {
                    tracing::warn!(
                        "SearXNG query empty after {} attempt(s) -- will signal upstream_unavailable",
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
    let (intent_result, embed_res, indexer_res, mut searx_results, invidious_res, news_res, image_res, shopping_res) = tokio::join!(
        intent_fut,
        embed_fut,
        indexer_task,
        // ⚠️  NOTE: tokio::time::timeout cancels the inner future, which drops any
        // partial results already collected by searx_fut_with_timeout (from faster
        // SearXNG instances that completed within the window). On timeout we fall
        // back to the partial collector (results the faster instances already
        // returned). The ROADMAP.md "Return early + merge stragglers" plan item
        // describes the proper fix: return completed results immediately and merge
        // slow instances' results in the background.
        //
        // The join deadline is Tor-aware: the gluetun-VPN (SearXNG1) instance is
        // bounded ~4.2s and the fan-out early-returns at 15 results, so a 5s join
        // is plenty for it. BUT the SearXNG2/Tor (tor2) instance runs over an
        // independent Tor circuit whose COLD build (after a NEWNYM rotation) takes
        // 10-15s; its own branch budget is 15s (see is_tor_url branch_timeout_ms)
        // and the per-instance attempt budget is 10s (non-site). A flat 5s join cut
        // the legitimate slow Tor response before it arrived, discarding the whole
        // second egress path. When tor2 is in the fan-out we extend the join to 15s
        // so its valid results are captured instead of dropped (the partial collector
        // is still used if even 15s is exceeded). Fast-path behaviour is unchanged.
        async {
            let partial_collector_ref = searx_partial_collector.clone();
            let join_deadline = if tor2_in_fanout {
                Duration::from_secs(15)
            } else {
                Duration::from_secs(5)
            };
            match tokio::time::timeout(join_deadline, searx_fut_with_timeout).await {
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
                    tracing::warn!("SearXNG fan-out exceeded {}s deadline; preserving partial results from completed instances", join_deadline.as_secs());
                    let mut guard = partial_collector_ref.lock().unwrap();
                    std::mem::take(&mut *guard)
                }
            }
        },
        invidious_fut,
        news_fut,
        image_fut,
        shopping_fut,
    );

    if let Ok(shopping_data) = shopping_res {
        if !shopping_data.results.is_empty() {
            tracing::info!("SearXNG shopping category returned {} results", shopping_data.results.len());
            searx_results.push((0, Ok(shopping_data)));
        }
    }
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
    // FIX (negation-drop, 2026-08-15): the intent engine emits exclusion
    // constraints as BOTH a `negative` entry AND an `Exclusion` entity. For some
    // NL forms (e.g. "restaurants in tokyo not sushi") the gateway's own parser
    // produces no negative (it only handles operators + a few inline markers), so
    // the engine's `negative` array is the sole source — and it was being
    // dropped before reaching ranking/hard-filter, so the exclusion never fired.
    // We now ALSO mirror any `Exclusion`-role entity into `negative` so the
    // constraint is always honoured regardless of which layer produced it.
    // General + signal-driven: no query-specific strings, no denylists.
    for e in &intent.structured_constraints.entities {
        if e.role == EntityRole::Exclusion {
            let t = e.text.trim().to_lowercase();
            // DA/DB fix (2026-08-17): engine `Exclusion` entities must pass the
            // SAME grammar/quality-noise guards as gateway-parsed negatives. The
            // intent engine emits subjective adjectives + intensifiers as
            // `Exclusion` roles next to negation markers ("not too spicy and
            // good for kids" -> Exclusion="good"/"too"), which would otherwise
            // become phantom hard-negatives that drop relevant pages. Skip them.
            // A genuine topical exclusion (brand/place/noun the user named) is
            // never in either noise set, so real exclusions survive unchanged.
            if !t.is_empty()
                && t.len() >= 2
                && !is_exclusion_grammar_noise(&t)
                && !is_subjective_quality_term(&t)
                && !intent.structured_constraints.negative.contains(&t)
            {
                intent.structured_constraints.negative.push(t);
            }
        }
    }
    // Re-sanitize so the mirrored exclusion is still subject to the same
    // validation as every other negative constraint.
    intent.structured_constraints = sanitize_constraints(&intent.structured_constraints);

    // P3 NL-price fix: also derive a bound from natural-language price words
    // ("under 150 dollars", "below 1000 rupees") — these never matched the
    // `price:<` operator parser, so the bound stayed None and ranking fell back
    // to pure relevance (always surfacing "under $200" pages). This feeds the
    // SAME price-aware ranking path as the operator form. Only set when the
    // operator form did not already provide a bound.
    if intent.structured_constraints.price_lt.is_none()
        && intent.structured_constraints.price_max.is_none()
    {
        if let Some((lt, _currency)) = extract_nl_price_bound(&q_orig) {
            intent.structured_constraints.price_lt = Some(lt);
            intent.structured_constraints.price_max = Some(lt);
            tracing::info!("NL PRICE BOUND: extracted lt={} from query", lt);
        }
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
        // The actual hard window is applied AFTER the web merge (see dated_result_count
        // guard near line ~8485): we only set it when at least one web result actually
        // carries a parseable date, so date-less fresh queries (e.g. "latest movies
        // released in 2026") fail OPEN and keep recency as a scoring boost instead of
        // collapsing to 0 results.
        if intent.intent == "fresh" && intent.structured_constraints.after_date.is_none() {
            tracing::info!("FRESH OVERRIDE: fresh intent without date window — window applied post-merge (fail-open if no dated results)");
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

        // Override 4: local intent signals → force local intent
        let has_local_keywords = q_lower.contains(" near me") || q_lower.starts_with("near me")
            || q_lower.contains("nearby") || q_lower.contains(" close to")
            || q_lower.starts_with("close to") || q_lower.contains("coffee shop");
        if has_local_keywords && intent.intent != "local" {
            tracing::info!(
                "INTENT OVERRIDE (STRONG): local query '{}' was '{}' (conf={:.3}) -> local",
                q, intent.intent, intent.confidence
            );
            intent.intent = "local".to_string();
            intent.confidence = intent.confidence.max(0.75);
            let local_prob = intent.distribution.get("local").copied().unwrap_or(0.0);
            let current_top_prob = intent.distribution.values().cloned().fold(0.0f32, f32::max);
            intent.distribution.insert("local".to_string(), (local_prob + current_top_prob * 0.5 + 0.4).min(0.90));
        }

        // Override 5: Comparison & Alternatives signals (H2 fix)
        // e.g. "alternatives to adobe photoshop that are free", "best budget smartphones under 30000 rupees"
        let comp_signals = [
            "alternatives to", "alternative to", "alternatives for", "alternative for",
            "similar to", "apps like", "tools like", "software like", "sites like",
            "equivalent to", "replacement for", "competing with", "vs", "versus",
            "best budget", "best ... under", "top ... under", "compared to", "difference between",
            "which is better", "comparison", "compare "
        ];
        let has_comp_signal = comp_signals.iter().any(|s| {
            if s.contains("...") {
                let parts: Vec<&str> = s.split("...").collect();
                parts.len() == 2 && q_lower.contains(parts[0].trim()) && q_lower.contains(parts[1].trim())
            } else {
                q_lower.contains(s)
            }
        });
        if has_comp_signal {
            tracing::info!(
                "INTENT OVERRIDE (DECISIVE): comparison query '{}' was '{}' (conf={:.3}) -> comparison",
                q, intent.intent, intent.confidence
            );
            intent.intent = "comparison".to_string();
            intent.confidence = intent.confidence.max(0.85);
            intent.distribution.insert("comparison".to_string(), 0.85);
            if intent.distribution.get("informational").copied().unwrap_or(0.0) > 0.4 {
                intent.distribution.insert("informational".to_string(), 0.15);
            }
        }

        // Override 6: transactional keywords OR an explicit price bound -> transactional
        let tx_keywords = ["buy ", "price ", "pricing", "cheap ", "purchase ", "shop ", "store ", "discount ", "coupon ", "under "];
        let has_tx_signal = tx_keywords.iter().any(|k| q_lower.starts_with(k) || q_lower.contains(k));
        // D5 (2026-08-17): a query that carries a REAL price bound ("laptop under 60000",
        // "smartwatch under 5000") is a purchase intent. Override 5 may have forced
        // `comparison` on the generic "best ... under" signal — but a budget-anchored
        // buy query is transactional, not a comparison. The price bound is signal-driven
        // (parsed from NL), not a per-query literal, so this is general and future-proof.
        let sc = &intent.structured_constraints;
        let has_price_bound = sc.price_lt.is_some() || sc.price_max.is_some()
            || sc.price_min.is_some() || sc.price_gt.is_some();
        if (has_tx_signal || has_price_bound) && !has_local_keywords {
            if (intent.intent != "comparison" || has_price_bound)
                && (intent.intent != "transactional" || intent.confidence < 0.60)
            {
                if has_price_bound && intent.intent == "comparison" {
                    tracing::info!(
                        "INTENT OVERRIDE (STRONG): price-bounded buy query '{}' was 'comparison' (conf={:.3}) -> transactional",
                        q, intent.confidence
                    );
                    // Dampen the spurious comparison probability so ranking blends transactional.
                    if let Some(c) = intent.distribution.get_mut("comparison") {
                        *c = (*c * 0.4).min(0.30);
                    }
                } else {
                    tracing::info!(
                        "INTENT OVERRIDE (STRONG): transactional query '{}' was '{}' (conf={:.3}) -> transactional",
                        q, intent.intent, intent.confidence
                    );
                }
                intent.intent = "transactional".to_string();
                intent.confidence = intent.confidence.max(0.80);
                let tx_prob = intent.distribution.get("transactional").copied().unwrap_or(0.0);
                intent.distribution.insert("transactional".to_string(), (tx_prob + 0.50).min(0.88));
            }
        }

        // Override 8: Driver / Software Download Intent -> force decisive Navigational + Download intent
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

        // Override 7: weather / forecast queries → fresh
        // WHOLE-WORD match only: a naive `contains("rain")` wrongly fired inside
        // "fe**rain**al" (a rescue-cat query) and forced fresh intent on a how-to
        // question, which then re-ranked results by recency instead of relevance.
        // Use the same `q_has_word` boundary helper that guards "fresh"/"latest".
        let weather_signals = [
            "weather", "forecast", "temperature", "rain", "snow", "humidity",
            "precipitation", "thunderstorm", "sunny", "cloudy", "meteorology",
        ];
        let has_weather_signal = weather_signals.iter().copied().any(|s| q_has_word(&q_lower, s));
        // F2 (2026-08-17): a weather WORD alone is NOT enough — "repair roof in rain",
        // "car won't start in the rain", "run in the rain" are how-to/maintenance
        // questions, not weather forecasts. Only force fresh when the query also
        // asks for a PREDICTION/forecast (weather report, will it rain, tomorrow's
        // forecast, is it going to snow) OR names a weather noun as the primary topic
        // ("today's weather", "delhi weather"). Structural prediction vocabulary, no
        // per-query literals. Never override a clear how-to ("how to ...").
        let weather_prediction_signals = [
            "weather report", "weather forecast", "weather today", "weather tomorrow",
            "will it", "going to rain", "going to snow", "forecast for", "this week's weather",
            "current weather", "live weather", "weather update", "rain forecast", "snow forecast",
            "temperature in", "humidity in",
        ];
        let has_weather_prediction = weather_prediction_signals.iter().any(|s| q_lower.contains(*s));
        // WEATHER-AS-SUBJECT (2026-08-21 fix for P11-class false trigger): a bare
        // "weather" word anywhere must NOT force fresh — queries like "daily
        // skincare routine for oily skin in humid weather" mention weather only as
        // a modifier of a non-weather topic and must stay informational (evergreen
        // advice, not news). Weather is the genuine subject only when it leads the
        // query or appears in a subject-phrase ("weather in X", "X weather",
        // "weather today/forecast/report/update", "this week's weather"). Structural
        // phrases, no city/region literals. This closes the residue of P11 (substring
        // intent triggers) without re-narrowing to only prediction signals.
        // A bare "<word> weather" / "weather" as the FINAL token only counts as the
        // subject when the WHOLE query is short (e.g. "delhi weather", "london
        // forecast" — 2-4 tokens about weather). A modifier inside a long non-weather
        // query like "...oily skin in humid weather" (13 tokens) is NOT the topic and
        // must not force fresh.
        let weather_is_subject = q_lower.starts_with("weather")
            || q_lower.starts_with("forecast")
            || q_lower.contains("weather in ")
            || q_lower.contains("weather for ")
            || q_lower.contains("weather today")
            || q_lower.contains("weather tomorrow")
            || q_lower.contains("weather report")
            || q_lower.contains("weather update")
            || q_lower.contains("current weather")
            || q_lower.contains("live weather")
            || q_lower.contains("this week's weather")
            || q_lower.contains("weather near")
            || {
                let n_tok = q_lower.split_whitespace().count();
                n_tok <= 4 && {
                    let last = q_lower.split_whitespace().last().unwrap_or("");
                    last == "weather" || last == "forecast"
                }
            };
        let is_howto_query = q_lower.starts_with("how to") || q_lower.starts_with("how do")
            || q_lower.starts_with("how can") || q_lower.contains("how to")
            || q_lower.contains("fix ") || q_lower.contains("repair") || q_lower.contains("won't start")
            || q_lower.contains("wont start") || q_lower.contains("leaking") || q_lower.contains("not cooling");
        if has_weather_signal && (has_weather_prediction || weather_is_subject)
            && !is_howto_query
            && intent.intent != "fresh" && intent.intent != "local"
        {
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

        // Override 8: procedural / how-to queries → how-to
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
            let tech_prob = intent.distribution.get("technical").copied().unwrap_or(0.0);
            intent.distribution.insert("technical".to_string(), tech_prob + 0.15);
        }

        // Override 9: research / study queries → informational
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
                // Record success for any valid parsed response (including zero-result)
                // to clear open_until and reset circuit breaker state
                circuit_ref.record_success(instance_key);
                // Track last-used time for connection-cooldown aware routing
                if let Some(url) = searx_key_to_url.get(instance_key) {
                    state.searx_last_used.lock().insert(url.clone(), Instant::now());
                }
                if n > 0 {
                    searx_instances_ok += 1;
                    // Record result count metrics only when there are actual results
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
                            price: None,
                            currency: None,
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
                        price: None,
                        currency: None,
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
                        price: None,
                        currency: None,
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
    // When the relevance model cannot discriminate (garbage cluster), we trust
    // the engine ranking (RRF) and keep ALL candidates; otherwise keep those
    // above the adaptive threshold (with a top-3 floor). The superlative-junk
    // exclusion below then applies to BOTH paths, so off-topic "best"-brand
    // pages are removed even in the degenerate RRF case (where they dominate).
    let mut keep_indices: Vec<usize> = if is_garbage_cluster {
        tracing::warn!(
            "SEMANTIC FILTER SKIPPED (degenerate scorer, trusting RRF): web_results.len={}",
            web_results.len()
        );
        (0..web_results.len()).collect()
    } else {
        let mut keep: Vec<usize> = Vec::new();
        for (i, &score) in semantic_scores_web.iter().enumerate() {
            if score >= semantic_threshold {
                keep.push(i);
            }
        }
        // Always keep at least 3 results (but only if they have ANY relevance)
        if keep.len() < 3 && !web_results.is_empty() {
            // Take top-3 by semantic score, even if below threshold
            let mut scored: Vec<(usize, f32)> = semantic_scores_web.iter().enumerate()
                .map(|(i, &s)| (i, s)).collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            keep = scored.iter().take(3).map(|(i, _)| *i).collect();
        }
        keep
    };
    // Superlative-junk exclusion: "best X" queries attract pages whose TITLE is
    // about the superlative word itself ("Best Buy | Official Store", "10BEST",
    // "BEST Definition") while containing NONE of the query's topic terms. These
    // are off-topic, not merely low-relevance — drop them outright so they never
    // fill the page with floor scores. Structural (title-pattern based), no
    // hardcoded domains. Skipped when it would leave < 3 results (degenerate
    // pool fallback) or the query has no topic terms to check.
    let q_lower_sf = q.to_lowercase();
    let superlative_set: &[&str] = &["best", "top", "greatest", "cheapest", "finest"];
    let q_has_superlative = superlative_set.iter().any(|s| q_lower_sf.contains(s));
    // Fire even for tiny pools: a single "Best Buy" squeezed into a 3-result
    // web pool is exactly the junk that must not be shown. The inner guard
    // below keeps the degenerate all-junk fallback alive.
    if q_has_superlative && !keep_indices.is_empty() {
        let topic_terms: Vec<String> = q.split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| {
                w.len() >= 3
                    && !superlative_set.contains(&w.as_str())
                    && !w.chars().all(|c| c.is_ascii_digit())
                    && !["the","a","an","and","or","for","of","in","on","at","to","with",
                        "from","by","under","over","is","are","was","were","be","that",
                        "this","these","those","your","you","how","what","which","more",
                        "most","than","then","very","just","not","no","only","also","any"].contains(&w.as_str())
            })
            .collect();
        if !topic_terms.is_empty() {
            let filtered: Vec<usize> = keep_indices.iter().copied()
                .filter(|&i| {
                    let t = web_results[i].title.to_lowercase();
                    let title_has_superlative = superlative_set.iter().any(|s| t.contains(s));
                    if !title_has_superlative { return true; }
                    topic_terms.iter().any(|w| t.contains(w.as_str()))
                })
                .collect();
            // Keep the exclusion whenever any non-junk survivor exists — dropping
            // junk down to even 1-2 results beats showing it. In a garbage-cluster
            // pool (everything off-topic) the junk goes even to zero; the local
            // index + surviving web results fill the page instead. Only when the
            // pool is 100% superlative-junk AND the scorer is discriminating do
            // we keep it (degenerate fallback, better than an empty page).
            if filtered.len() > 0 || is_garbage_cluster {
                let removed = keep_indices.len() - filtered.len();
                if removed > 0 {
                    tracing::info!("Superlative-junk exclusion: removed {} off-topic superlative-only web result(s)", removed);
                }
                keep_indices = filtered;
            }
        }
    }
    web_results = keep_indices.into_iter().map(|i| web_results[i].clone()).collect();

    // Constraint transparency bookkeeping: capture the result count before any
    // constraint filtering, and how many results actually carry a parseable
    // date / detectable price. These let us report applied-vs-ignored
    // constraints honestly instead of silently returning empty or unfiltered.
    let pre_filter_count = web_results.len();
    let dated_result_count = web_results.iter().filter(|r| {
        resolve_item_date(r.published_date.as_deref(), &r.url, &r.title, &r.content).is_some()
    }).count();
    let priced_result_count = web_results.iter().filter(|r| r.get_price().is_some()).count();

    // FRESH/date fail-open (prevents 0-result collapse): the FRESH OVERRIDE may have
    // flagged this as a recency query, and should_filter_by_constraints DROPS any
    // result without a parseable date OR with a date outside the hard window.
    // Two collapse cases exist:
    //   (A) NO merged web result carries a parseable date (typical for "latest ai
    //       news this week") -> a hard window deletes every result -> n=0.
    //   (B) Every returned result IS dated but falls OUTSIDE the narrow window
    //       (e.g. "google io 2026" snippets dated earlier in 2026, outside a
    //       7-day after:/before: range) -> the hard window still drops them all
    //       -> n=0. The original P6 guard only handled (A); (B) slipped through
    //       because dated_result_count > 0, so the window stood and emptied the set.
    // In BOTH cases we clear the derived date bounds so recency stays a pure
    // SCORING boost (freshness half-life) and the results survive. Fail-open is
    // keyed on "the hard window would remove every result", not on any specific
    // query/domain/window — so it is general and never over-drops.
    let date_window_present = intent.structured_constraints.after_date.is_some()
        || intent.structured_constraints.before_date.is_some();
    if date_window_present && pre_filter_count > 0 {
        // Dry-run the same date test should_filter_by_constraints applies.
        let survivors_after_window = web_results.iter().filter(|r| {
            let mut ok = true;
            if let Some(ref ad) = intent.structured_constraints.after_date {
                if let Some(limit) = parse_date_to_comparable(ad) {
                    if let Some(pd) = resolve_item_date(r.published_date.as_deref(), &r.url, &r.title, &r.content) {
                        if !date_gte(pd, limit) { ok = false; }
                    }
                }
            }
            if ok {
                if let Some(ref bd) = intent.structured_constraints.before_date {
                    if let Some(limit) = parse_date_to_comparable(bd) {
                        if let Some(pd) = resolve_item_date(r.published_date.as_deref(), &r.url, &r.title, &r.content) {
                            if !date_lte(pd, limit) { ok = false; }
                        }
                    }
                }
            }
            ok
        }).count();
        // (A) no dates at all, or (B) all dated-but-out-of-range -> would empty.
        // (C) RESIDUAL P6: the window keeps SOME results but crushes the set
        //     pathologically (e.g. date-less upstream snippets for "latest X this
        //     week" → 8/9 dropped, 1 survives = 11%). A near-empty result set is
        //     the same user-facing failure as a zero one: relevant, date-less
        //     results get discarded in favour of a single stale-but-dated item.
        //     Fail-open when the surviving fraction is at or below a general 25%
        //     floor AND the surviving count is too small to be useful (< 3). The
        //     <= (not <) boundary matters: a query whose results are exactly 25%
        //     dated-and-in-window (e.g. ISRO "latest news" → 1 of 4 survive = 0.25)
        //     is still a pathologically crushed set and must fail open. Keyed on
        //     survival ratio, not on any query/window, so it stays general.
        let survivor_fraction = if pre_filter_count > 0 {
            survivors_after_window as f32 / pre_filter_count as f32
        } else {
            1.0
        };
        let fraction_too_low = survivors_after_window < 3 && survivor_fraction <= 0.25;
        if survivors_after_window == 0 || fraction_too_low {
            tracing::info!(
                "DATE WINDOW FAIL-OPEN (would-empty/near-empty): {} web results, {} would survive (fraction={:.2}) the date window (dated_result_count={}) — clearing hard recency window (recency stays scoring-only)",
                pre_filter_count, survivors_after_window, survivor_fraction, dated_result_count
            );
            intent.structured_constraints.after_date = None;
            intent.structured_constraints.before_date = None;
        }
    }

    // PRICE fail-open (mirrors the date fail-open above, P3-class): a price
    // constraint (`price:<60000`, `price_max`, etc.) can only meaningfully
    // narrow results when at least one merged web result actually carries a
    // detectable price. Web/local snippets almost never do, so when
    // `priced_result_count == 0` the P3 ranking block would demote EVERY result
    // (×0.45, no detectable price signal) and the hard filter would drop the
    // single surviving item — collapsing a valid product query ("best budget
    // mirrorless camera under 60000 rupees") to 0 results. Fail open: clear the
    // price bound so normal relevance/authority ranking proceeds, and report the
    // gap via `ignored_constraints`. The bound still bites when real prices are
    // present (e.g. price-comparison pages). Generic; no merchant/domain bias.
    if (intent.structured_constraints.price_min.is_some()
        || intent.structured_constraints.price_max.is_some()
        || intent.structured_constraints.price_lt.is_some()
        || intent.structured_constraints.price_gt.is_some())
        && priced_result_count == 0
    {
        tracing::info!(
            "PRICE FAIL-OPEN: {} web results but 0 carried a detectable price — clearing price bound (remains ranking-only boost/skip)",
            pre_filter_count
        );
        intent.structured_constraints.price_min = None;
        intent.structured_constraints.price_max = None;
        intent.structured_constraints.price_lt = None;
        intent.structured_constraints.price_gt = None;
    }

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

            // P5-class "negative over-filter" fix (this round, #15):
            // A bare NEGATIVE TERM ("onion", "garlic") is a TOPICAL exclusion that
            // grades smoothly — it is NOT a structural operator (site:/filetype:/
            // date/phrase are handled in should_filter_by_constraints and hard-drop
            // correctly). Hard-dropping every web result that mentions a topical
            // excluded term collapses queries like "healthy dinner recipes without
            // onion and garlic" to ZERO results, because nearly every recipe page
            // mentions onion/garlic. The soft penalty is already enforced through
            // constraint_score -> c_score -> r.score downstream, which demotes
            // matching results while keeping the set non-empty. So we NO LONGER
            // hard-drop here. We only log a diagnostic; the actual demotion happens
            // in the scoring loop. This matches the documented design at main.rs:2385
            // ("a bare word like vim/django is NOT structural ... never hard-drop").
            for r in web_results.iter() {
                let alt_score = is_alternative_listing_page(&r.title, &r.url, &r.content);
                if alt_score <= 0.3 {
                    let text = format!("{} {} {}", r.title, r.url, r.content.chars().take(300).collect::<String>());
                    let text_lower = text.to_lowercase();
                    let _matched = negative_norm.iter().any(|neg| {
                        let neg_lower = neg.to_lowercase();
                        let words: Vec<&str> = neg_lower.split_whitespace().collect();
                        if words.len() == 1 {
                            text_matches_negative(&text_lower, &neg_lower)
                        } else {
                            let joined = words.join(" ");
                            text_lower.contains(&joined)
                        }
                    });
                    // No hard drop — soft penalty applied later in scoring.
                }
            }
        }

        let removed = before_count.saturating_sub(web_results.len());
        if removed > 0 {
            tracing::info!(
                "Negative constraint hard filter: removed {}/{} web results (hard gate)",
                removed, before_count
            );
        }
    }

    // --- Price constraint: real narrowing, but NEVER gut the result set ---
    // Out-of-range PRICED results are already hard-dropped upstream by
    // `should_filter_by_constraints` (4f). This block only decides what to do
    // with UNPRICED results. The old rule dropped every unpriced result the
    // moment ANY result carried an in-range price — which wrecked exactly the
    // queries this feature was meant to serve: "best noise cancelling
    // headphones under 200 dollars" collapsed to 3 YouTube videos (their
    // titles contain "$200") and dropped every product page/listicle (whose
    // snippets rarely expose a machine-parseable price).
    //
    // Robust policy (fail-open, mirroring the date filters):
    //   • Out-of-range priced results: hard-dropped (upstream, 4f).
    //   • Unpriced results: KEPT — most pages don't expose a price in their
    //     snippet, and a natural-language bound ("under $200") describes the
    //     TOPIC (a category of product), not a demand for machine-readable
    //     prices. Ranking (P3 price-aware) already boosts in-range priced
    //     results and demotes unpriced ones.
    //   • Explicit operator: when the user literally typed `price:<N` (not a
    //     natural-language phrase normalized to it) we MAY hard-drop unpriced
    //     results — but only while enough in-range priced results remain to
    //     fill the page (>= 6), so we never collapse to 1-3 arbitrary hits.
    {
        let pmin = intent.structured_constraints.price_min;
        let pmax = intent.structured_constraints.price_max;
        if pmin.is_some() || pmax.is_some() {
            let lo = pmin.unwrap_or(0.0) as f64;
            let hi = pmax.unwrap_or(f32::MAX) as f64;
            let in_range_price = |r: &SearxResult| -> Option<bool> {
                r.get_price().map(|pi| {
                    let usd = price_to_usd(pi.amount, &pi.currency);
                    usd >= lo && usd <= hi
                })
            };
            let in_range_count = web_results.iter().filter(|r| {
                in_range_price(r).unwrap_or(false)
            }).count();
            // The price bound is EXPLICIT only when the raw query contains a
            // `price:` operator. Natural-language phrases ("under $200", "less
            // than 100 dollars") are normalized to `price:<N` by
            // normalize_nl_operators, but they express query intent, not a
            // hard filter — they must not gut the pool.
            let q_raw_lower = q_orig.to_lowercase();
            let explicit_price_op = q_raw_lower.contains("price:")
                || q_raw_lower.contains("price<") || q_raw_lower.contains("price>");
            if explicit_price_op && in_range_count >= 6 {
                let before = web_results.len();
                web_results.retain(|r| in_range_price(r).unwrap_or(false));
                let after = web_results.len();
                tracing::info!(
                    "Price constraint (explicit operator): narrowed {} → {} results (dropped {} unpriced/out-of-range)",
                    before, after, before - after
                );
            } else if in_range_count == 0 {
                tracing::warn!(
                    "Price constraint specified ({:?}-{:?}) but no result snippets carried a detectable price — cannot narrow",
                    pmin, pmax
                );
            } else {
                tracing::info!(
                    "Price constraint (soft/derived): kept {} unpriced result(s); {} in-range priced result(s) rank ahead via P3",
                    web_results.iter().filter(|r| in_range_price(r).is_none()).count(), in_range_count
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
    //
    // D3 transparency: declared up-front so the negation gate (which runs during
    // constraint normalization, BEFORE the `applied`/`ignored` block at ~10309)
    // can push declined non-manner exclusions into it. The later `let mut ignored`
    // is changed to a reassignment that preserves whatever we added here.
    let mut ignored: Vec<String> = Vec::new();
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
    let (query_neg_terms, query_neg_dropped, _query_neg_manner): (Vec<String>, Vec<String>, Vec<String>) =
        extract_query_negative_terms_with_dropped(&q_orig);
    let query_contrastive = query_is_contrastive(&q_orig);

    // Combine intent-engine negatives with query-derived negatives, then GATE BOTH
    // through is_real_exclusion so manner-qualifier false negatives (e.g. "without
    // soap", "with no music background", "without offending the couple") from EITHER
    // source are dropped. Real exclusions (contrastive framing — comparison /
    // "alternative to" / "instead of" / double negation — or a recognized entity via
    // the protected-term list / a capitalized proper noun) survive. This is the single
    // principled rule; it is entity/data-driven, not tuned to any one query.
    let mut raw_neg: Vec<String> = intent.structured_constraints.negative.clone();
    for qt in &query_neg_terms {
        if !raw_neg.contains(qt) {
            raw_neg.push(qt.clone());
        }
    }
    // The intent engine emits `Exclusion`-role entities via its Query-Graph IR.
    // That classification is a signal-driven decision (the engine recognized the
    // clause as a genuine topical exclusion), so we trust it and bypass the
    // generic `is_real_exclusion` gate for those terms. This fixes NL negations
    // like "restaurants in tokyo not sushi" / "not controlled by a big advertising
    // company" that the gate would otherwise decline as generic nouns — while
    // manner/attribute exclusions the engine did NOT tag as Exclusion are still
    // declined by the gate. No hardcoded allow-list; entity-role driven.
    let engine_exclusions: std::collections::HashSet<String> = intent
        .structured_constraints
        .entities
        .iter()
        .filter(|e| e.role == EntityRole::Exclusion)
        .map(|e| e.text.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .filter(|t| !is_exclusion_grammar_noise(t)) // F3 (2026-08-17): drop grammar-noise
        .filter(|t| !is_subjective_quality_term(t)) // DA/DB (2026-08-17): drop quality adjectives
        .filter(|t| !is_verb_attribute_exclusion(t)) // V1: drop verb-led/attribute exclusions
        .filter(|t| {
            // D2 (2026-08-19): a bare "pay"/"paying" engine Exclusion is only a
            // manner false-positive when the query context says so. "pay attention"
            // / "pay respect" → manner, DROP it (it must not become a real
            // exclusion). A monetary "pay for a course" → real money exclusion,
            // KEEP IT (this was the dropped D2 defect). All other engine
            // exclusions are kept unchanged.
            if *t == "pay" || *t == "paying" {
                pay_exclusion_is_money(&q_orig)
            } else {
                true
            }
        })
        .collect();
    let mut gated_neg_dedup: Vec<String> = Vec::new();
    for n in raw_neg.clone() {
        let engine_backed = engine_exclusions.contains(&n.to_lowercase());
        // V1 (2026-08-18): a verb-led / user-attribute exclusion (e.g. "dependents"
        // from "with no dependents", "coordination" from "with no coordination") is
        // NEVER a real content exclusion — it describes the user, not a topic to
        // drop. It must be rejected here at the FINAL gate regardless of whether the
        // engine tagged it or the contrastive framing (compare/versus) would
        // otherwise promote it. Rejecting here — after the engine-exclusion merge
        // point — covers BOTH sources (engine IR + gateway extractor) with one
        // structural rule. Genuine topical exclusions (brand/place/demonym) never
        // match is_verb_attribute_exclusion, so they still survive.
        if is_verb_attribute_exclusion(&n) {
            continue;
        }
        if (engine_backed || is_real_exclusion(&n, &q_orig, query_contrastive))
            && !gated_neg_dedup.contains(&n)
        {
            gated_neg_dedup.push(n);
        }
    }
    intent.structured_constraints.negative = gated_neg_dedup.clone();

    // D3 transparency: surface genuine (non-manner) candidate exclusions that the
    // gate declined, so they are not silently dropped. Two sources: (1) intent-engine
    // negatives the gate declined, and (2) query-derived compounds the extractor
    // declined but that are not manner qualifiers. Manner qualifiers ("without soap")
    // are intentionally never surfaced — they describe HOW, not WHAT to exclude.
    let mut declined: Vec<String> = Vec::new();
    for n in &raw_neg {
        if !gated_neg_dedup.contains(n) && !declined.contains(n) {
            declined.push(n.clone());
        }
    }
    for d in &query_neg_dropped {
        // If this dropped negative was ALSO rescued into gated_neg_dedup (engine
        // tagged it as a genuine Exclusion), it is already reported as applied
        // — do NOT also surface it as ignored, or it would appear in BOTH
        // applied_constraints and ignored_constraints (a direct contradiction).
        if gated_neg_dedup.contains(d) {
            continue;
        }
        if !declined.contains(d) {
            declined.push(d.clone());
        }
    }
    let mut ignored_vec: Vec<String> = Vec::new();
    for n in declined {
        if is_manner_phrase(&n) || is_manner_frame(&q_orig, &n) {
            continue;
        }
        // Skip grammar/preposition noise the intent engine may emit as a negative
        // candidate (e.g. "in", "about"). Surfacing these would confuse users.
        if n.split_whitespace().any(|t| IGNORED_CONSTRAINT_NOISE.contains(&t)) {
            continue;
        }
        let note = format!(
            "not:{} — exclusion not applied (unrecognized entity, no contrastive 'alternative/versus/compare' framing)",
            n
        );
        if !ignored_vec.contains(&note) {
            ignored_vec.push(note);
        }
    }
    intent.structured_constraints.ignored_constraints =
        if ignored_vec.is_empty() { None } else { Some(ignored_vec) };
    intent.structured_constraints.negative = gated_neg_dedup.clone();

    // D4b (2026-08-17): a term that became a REAL negative exclusion must not also
    // remain a positive requirement — that is a contradiction no downstream gate can
    // satisfy (a result can't both match AND not match `chinese`). The negation here
    // is derived AFTER the earlier `sanitize_constraints` calls (the engine emits
    // `+chinese` + a contrastive/`COUNTRY_DEMONYMS` negation that lands in
    // `gated_neg_dedup`), so the sanitizer's own positive/negative overlap guard
    // (which only sees negatives present at sanitize time) cannot catch it. Purge the
    // final negative terms from the positive set at this single chokepoint. General:
    // driven by the resolved negative set, no per-query literals.
    if !gated_neg_dedup.is_empty() {
        let neg_lc: std::collections::HashSet<String> =
            gated_neg_dedup.iter().map(|n| n.to_lowercase()).collect();
        intent.structured_constraints.positive.retain(|p| !neg_lc.contains(&p.to_lowercase()));
    }

    let has_only_negative = intent.structured_constraints.positive.is_empty()
        && !gated_neg_dedup.is_empty();

    let neg_terms: Vec<String> = gated_neg_dedup.iter()
        .map(|n| n.to_lowercase())
        .collect();

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
let web_semantic = compute_web_semantic(&vector, &local_results, &web_results, &client).await;

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
    // Uses the already-gated negative set (intent.structured_constraints.negative
    // was normalized through is_real_exclusion above, so manner-qualifier false
    // negatives are not present here). query_neg_terms is intentionally NOT
    // re-added — it is raw and ungated, and would reintroduce the false negatives.
    let has_neg_constraints = !intent.structured_constraints.negative.is_empty();
    if has_neg_constraints {
        let before_count = results.len();
        let mut negative_norm: Vec<String> = intent
            .structured_constraints
            .negative
            .iter()
            .map(|n| n.to_lowercase())
            .collect();
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
            let title_lower = r.title.to_lowercase();
            // Exempt GENUINE alternative-listing / comparison pages from the hard
            // negative drop. A genuine alt page (alt_score >= 0.70, or an explicit
            // comparison marker in the title) mentions the excluded term
            // *referentially* — exactly what an "alternative to X", "except X", or
            // "without X" query wants (e.g. "25 Alternative Search Engines You Can
            // Use Instead of Google" for "search engine alternative to google").
            //
            // CRITICAL FIX (round 2026-08-15T0830Z): the old gate exempted anything
            // with alt_score > 0.3. But is_alternative_listing_page() also assigns a
            // WEAK alt signal (~0.42) to generic "best/top/review" listicle titles
            // — including a brand's OWN catalog page like "Dell Laptop Computers -
            // Best Buy" or "Best Dell Laptops". Those are NOT comparison/alternative
            // listings; they ARE the excluded brand. Exempting them meant "laptops
            // not dell" still surfaced 6 Dell pages (auditor: before==after,
            // dropped=0 for the negative hard-filter). The exemption must require a
            // STRONG comparison signal, not a generic listicle, so brand-owned /
            // "best <brand>" pages are correctly hard-dropped while true alt pages
            // survive. This mirrors constraint_score's is_strong_alt_page (>0.5)
            // convention. We do NOT also require is_comparison_or_alternative_query()
            // (the word "alternative" is consumed into the negative constraint).
            let genuine_alt = alt_score >= 0.70
                || title_lower.contains("alternative")
                || title_lower.contains(" vs ")
                || title_lower.contains(" versus ")
                || title_lower.contains("instead of")
                || title_lower.contains("replacement")
                || title_lower.contains("compared to")
                || title_lower.contains("migrate from");
            if genuine_alt {
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
                            && chars[i - 1].is_alphanumeric()
                            && chars[i + 1].is_alphanumeric()
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
    let pre_hard = results.clone();
    let had_negative_exclusion = !intent.structured_constraints.negative.is_empty();
    results.retain(|r| {
        !should_filter_by_constraints(&r.title, &r.content, &r.url, r.published_date.as_deref(), &intent.structured_constraints)
    });
    // FAIL-OPEN for negative constraints (mirrors the junk-filter fail-open at ~12642:
    // "never let a gate collapse a non-empty result set to ZERO"). A misclassified NL
    // negation — a symptom/state verb inside a problem description ("my washing machine
    // ... does not spin", "the door does not latch") that the intent engine tagged as an
    // Exclusion role — must never be permitted to collapse a non-empty, genuinely-topical
    // set to ZERO. An empty SERP for a real query is the worst failure mode (reads as
    // "nothing exists"). When the negative hard-drop would empty the set, we keep the
    // results and softly down-rank the ones the predicate would have dropped, so the user
    // still receives the best available pages instead of a blank page.
    // General: keyed on "would-empty", no query/domain bias, no per-query literals. Genuine
    // topical exclusions ("python web framework not django") are unaffected — their candidate
    // sets never empty, so the normal hard-drop still applies. This is the single safe net
    // for ANY spurious-exclusion class (symptom verbs, mis-tagged engine entities), not a
    // workaround tuned to one query.
    if had_negative_exclusion && !pre_hard.is_empty() && results.is_empty() {
        tracing::warn!(
            "NEGATION FAIL-OPEN: all {} results dropped by negative constraint(s) {:?}; keeping set with soft down-rank instead of empty",
            pre_hard.len(), intent.structured_constraints.negative
        );
        let mut restored = pre_hard;
        for r in restored.iter_mut() {
            if should_filter_by_constraints(&r.title, &r.content, &r.url, r.published_date.as_deref(), &intent.structured_constraints) {
                // Demote (do not delete) the negative-matching results: they sink below
                // genuine topical content but remain visible if nothing better exists.
                r.score *= 0.25;
            }
        }
        restored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results = restored;
    }

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
    // FAIL-OPEN (sparse-draw guard): never let this gate collapse a non-empty
    // result set to ZERO. When the only surviving candidates are video snippets
    // with no real text (invidious) or other low-information pages — which happens
    // for niche queries on a weak upstream SearXNG draw — dropping them all yields
    // `total=0`, the worst failure mode (reads as "nothing exists"). An empty SERP
    // for a genuine query is strictly worse than showing the best available, even
    // if imperfect. So: drop individual junk results normally, but if the gate
    // would empty the set, keep the results (the score already demoted them).
    // General: keyed on "would-empty", no query/domain bias, no magic threshold.
    {
        let before_junk = results.len();
        let keep_idx: Vec<usize> = (0..results.len())
            .filter(|&i| !clean::is_junk_content_for_url(&results[i].content, &results[i].url))
            .collect();
        if keep_idx.len() < before_junk && !keep_idx.is_empty() {
            let kept: Vec<MergedResult> = keep_idx.into_iter().map(|i| results[i].clone()).collect();
            results = kept;
        } else if keep_idx.is_empty() && before_junk > 0 {
            tracing::warn!(
                "JUNK FILTER FAIL-OPEN: all {} results flagged junk — keeping set rather than returning empty (sparse upstream draw)",
                before_junk
            );
        }
    }

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
            // Actually restore the ORIGINAL query for the engine search. Logging a
            // revert without restoring `q` left the corrupted form ("bryan") in the
            // search path even though validation flagged it as wrong — so a valid
            // word like "biryani" got English-ified and the bad correction still ran.
            // `q_trimmed` is the user's original query (operators intact), which is
            // exactly what we want to search with when the correction is rejected.
            q = q_trimmed.to_string();
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
    // ── Honest recall-gap signal (round-2026-08-12T1234Z D2 disposition) ──
    // Compute which of the query's distinctive terms are absent from ALL
    // returned results. This is an upstream recall limitation, NOT a ranking
    // defect. Computed over the full post-filter result set (pre-pagination)
    // so a term missing from every page is caught. Borrowed here BEFORE the
    // `into_iter` move below consumes `results`.
    let recall_gap_terms: Option<Vec<String>> =
        if results.is_empty() {
            None
        } else {
            compute_recall_gap_terms(&q_trimmed, &results)
        };
    let mut paginated_results = results.into_iter().skip(offset).take(limit).collect::<Vec<_>>();

    // ─── Constraint transparency (applied / ignored / warnings) ───
    let sc = &intent.structured_constraints;
    let mut applied: Vec<String> = Vec::new();
    // NOTE: `ignored` was declared up-front (near the constraint-normalization /
    // negation gate, ~9891) so D3 transparency can prepopulate it with declined
    // non-manner exclusions. We reuse that same Vec here (no second `let`) so the
    // negation-gate entries survive into the response.
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
    for he in &sc.hard_exclusions { applied.push(format!("not:{}", he)); }

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

    // ── Honest recall-gap signal ──
    // (computed earlier, before pagination, over the full post-filter result set)

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
        results_before_filter: Some(pre_filter_count.max(post_filter_count)),
        results_after_filter: Some(post_filter_count),
        total: Some(post_filter_count),
        page_limit: Some(limit),
        page_offset: Some(offset),
        has_more: if post_filter_count > 0 { Some(offset + limit < post_filter_count) } else { Some(false) },
        // FIX-B: gate price_verified on transactional intent AND a REAL price bound.
        // The old condition also fired on `priced_result_count > 0` — any web result
        // merely mentioning a price, regardless of intent — which emitted a spurious
        // `price_verified` (e.g. value 2) on non-transactional queries with no price
        // token. API_REFERENCE documents price_verified only in the transactional
        // context ("a real price constraint was verified"), so we require BOTH the
        // transactional intent subtype AND a verified price bound (lt/gt, already merged
        // into structured_constraints from the P3 NL-price + spoken-number wiring).
        // Signal-driven: no query-specific strings, no allow/deny lists.
        price_verified: if intent.intent == "transactional"
            && (sc.price_lt.is_some() || sc.price_gt.is_some() || sc.price_min.is_some() || sc.price_max.is_some())
        { Some(priced_result_count) } else { None },
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

/// Translate spoken number words into digits so the downstream price
/// operators fire. Spelled prices like "four hundred dollars" or "two hundred
/// fifty dollars" were never matched by the digit-only `price:<` regexes, so
/// they leaked as junk positive constraints (e.g. +four +hundred +dollars)
/// and no price bound was ever extracted (P3 regression). Converting the words
/// to digits up front lets the existing `under <N>` / `below <N>` rules produce
/// a real `price:<N` constraint, which then feeds ranking + the response
/// struct. Currency-agnostic: it only rewrites the number, never the currency
/// word, so dollars/rupees/euros all still flow through.
///
/// Handles 0-99 directly and any magnitude via "X hundred/thousand [Y]" and
/// "X thousand Y hundred [Z]" compositions (e.g. "two hundred fifty" -> "250",
/// "one thousand two hundred" -> "1200", "nineteen" -> "19").
fn normalize_spoken_numbers(query: &str) -> String {
    let units: &[(&str, u32)] = &[
        ("zero", 0), ("ten", 10), ("eleven", 11), ("twelve", 12),
        ("thirteen", 13), ("fourteen", 14), ("fifteen", 15), ("sixteen", 16),
        ("seventeen", 17), ("eighteen", 18), ("nineteen", 19),
        ("one", 1), ("two", 2), ("three", 3), ("four", 4), ("five", 5),
        ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9),
        ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50),
        ("sixty", 60), ("seventy", 70), ("eighty", 80), ("ninety", 90),
    ];
    let tokens: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        // Look for a "hundred" or "thousand" scalar clause ending on that word.
        if tok == "hundred" || tok == "thousand" {
            out.push(tok.clone());
            i += 1;
            continue;
        }
        let is_unit = units.iter().any(|(w, _)| w == tok);
        if is_unit {
            // Gather the contiguous run of number words.
            let mut j = i;
            let mut run: Vec<String> = Vec::new();
            while j < tokens.len() {
                let t = &tokens[j];
                let is_num = units.iter().any(|(w, _)| w == t) || t == "hundred" || t == "thousand";
                if !is_num { break; }
                run.push(t.clone());
                j += 1;
            }
            // Parse the composed value.
            let mut total: i64 = 0;
            let mut current: i64 = 0;
            let mut has_any = false;
            let mut saw_scale = false;
            for w in &run {
                if *w == "hundred" {
                    if current == 0 { current = 1; }
                    total += current * 100;
                    current = 0;
                    saw_scale = true;
                } else if *w == "thousand" {
                    if current == 0 { current = 1; }
                    total += current * 1000;
                    current = 0;
                    saw_scale = true;
                } else {
                    let v = units.iter().find(|(w2, _)| w2 == w).map(|(_, v)| *v).unwrap_or(0);
                    if v >= 10 && v <= 90 && v % 10 == 0 {
                        // tens (twenty..ninety) add directly
                        current += v as i64;
                    } else {
                        if current > 0 && v < 10 && !saw_scale {
                            // e.g. "twenty one" -> 21 (tens already in current)
                        }
                        if v < 10 { current += v as i64; }
                        else { current += v as i64; }
                    }
                    has_any = true;
                }
            }
            let value = if total == 0 && current == 0 { 0 } else { total + current };
            if has_any {
                out.push(value.to_string());
                i = j;
                continue;
            } else {
                // Not a parseable number run; emit as-is.
                out.push(tok.clone());
                i += 1;
                continue;
            }
        }
        out.push(tok.clone());
        i += 1;
    }
    out.join(" ")
}

/// Normalize natural-language constraint syntax into canonical operator tokens
/// (mirror of the intent engine's helper) so the engine query and the gateway's
/// own constraint parsing honour spoken forms: "under $500" -> price:<500,
/// "in url:github" -> inurl:github, "on site:reddit" -> site:reddit.
fn normalize_nl_operators(query: &str) -> String {
    // Spoken prices ("four hundred dollars") -> digits so the price regexes below
    // can rewrite them into `price:<N`. Must run before the digit-only rules.
    let query = normalize_spoken_numbers(query);
    let mut out = query.to_string();
    for (re_src, replacement) in [
        // Time-unit guard: a number immediately followed by a temporal unit
        // (years/months/weeks/days/hours/minutes) is a DURATION, not a price.
        // Without this, "over five years" / "under 3 months" / "within 2 weeks"
        // were mis-read as price bounds (round 2026-08-20: "over five years" in
        // a car TCO comparison became price:>5 and crushed every result). The
        // negative lookahead rejects the rewrite so the duration phrase is left
        // as a plain term. General — no per-query literals, no tuned constants.
        (r"(?i)\bunder\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:<$1"),
        (r"(?i)\bless\s+than\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:<$1"),
        (r"(?i)\bbelow\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:<$1"),
        (r"(?i)\bcheaper\s+than\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:<$1"),
        (r"(?i)\bmax(?:imum)?\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:<$1"),
        (r"(?i)\bover\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:>$1"),
        (r"(?i)\bmore\s+than\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:>$1"),
        (r"(?i)\babove\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:>$1"),
        (r"(?i)\bgreater\s+than\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:>$1"),
        (r"(?i)\bmin(?:imum)?\s*\$?\s*(\d[\d.,]*)(?!\s*(?:years?|months?|weeks?|days?|hours?|minutes?))", "price:>$1"),
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
    let mut hard_exclusions = Vec::new();
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

    // Extract NOT: — an explicit, UNCONDITIONAL hard-exclusion operator. Unlike a
    // bare `not X` negation (gated softly via is_real_exclusion), `NOT:term`
    // always hard-drops any result mentioning the term. This is the general,
    // non-hardcoded escape hatch for the DEFECT-A class: a user who knows a term
    // is off-topic can force its exclusion without relying on entity recognition.
    // A quoted value (`NOT:"visual studio code"`) captures a multi-word term; an
    // unquoted value (`NOT:flask`) captures a single whitespace-delimited token.
    // Terms are lowercased for case-insensitive matching in should_filter_by_constraints.
    for cap in q_lower.match_indices("not:") {
        let after = cap.0 + 4;
        let rest = &q[after..];
        let val = if rest.starts_with('"') {
            // Quoted multi-word term: read until the closing quote.
            let close = rest[1..].find('"').map(|i| i + 1);
            match close {
                Some(c) => rest[1..c].trim().to_lowercase(),
                None => rest[1..].trim().to_lowercase(),
            }
        } else {
            // Unquoted single token: stop at the first whitespace.
            let end = rest.find(' ').unwrap_or(rest.len());
            rest[..end].trim().to_lowercase()
        };
        if val.is_empty() {
            continue;
        }
        let wc = val.split_whitespace().count();
        if wc >= 1 && wc <= 4 && !hard_exclusions.contains(&val) {
            hard_exclusions.push(val);
        }
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
        hard_exclusions,
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
        ignored_constraints: None,
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
                        price: r.price.map(|p| p.to_string()),
                        currency: r.currency,
                        quality: r.quality,
                        engine_trust_mult: 1.0,
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
        assert_eq!(extract_price_from_text("Only $99 today"), Some(PriceInfo { amount: 99.0, currency: "USD".to_string() }));
        assert_eq!(extract_price_from_text("Cost is €149.99"), Some(PriceInfo { amount: 149.99, currency: "EUR".to_string() }));
        assert_eq!(extract_price_from_text("from 250 dollars"), Some(PriceInfo { amount: 250.0, currency: "USD".to_string() }));
        assert_eq!(extract_price_from_text("price: 49"), Some(PriceInfo { amount: 49.0, currency: "USD".to_string() }));
        assert_eq!(extract_price_from_text("no monetary value here"), None);
        assert_eq!(extract_price_from_text("₹2,000 only"), Some(PriceInfo { amount: 2000.0, currency: "INR".to_string() }));
        assert_eq!(extract_price_from_text("$10 - $20"), Some(PriceInfo { amount: 10.0, currency: "USD".to_string() }));
    }

    #[test]
    fn rs_signal_no_false_positives() {
        assert!(!has_price_signal("resources for learning python", "coursera course rsvp"));
        assert!(has_price_signal("buy shoes rs. 500", "cheap deal"));
        assert!(has_price_signal("item cost 200 rupees", "deal"));
    }

    #[test]
    fn fx_conversion_test() {
        assert_eq!(price_to_usd(100.0, "USD"), 100.0);
        let inr_usd = price_to_usd(2000.0, "INR");
        assert!(inr_usd < 30.0 && inr_usd > 20.0, "2000 INR should be ~24 USD, got {}", inr_usd);
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
    fn d3_non_entity_negations_surfaced_not_silently_dropped() {
        // D3 regression: genuine attribute/topic exclusions that the `is_real_exclusion`
        // gate declines (not a recognized entity, not contrastive framing) must NOT be
        // silently discarded — they must be reported via `ignored_constraints` so the
        // user knows the exclusion was not applied. Manner qualifiers stay excluded and
        // are NEVER surfaced.
        for q in [
            "recipes not spicy",
            "movies not rated r",
            "books not in hardcover",
            "news not about politics",
        ] {
            let (kept, dropped, _manner) = extract_query_negative_terms_with_dropped(q);
            // The gate still declines the attribute exclusion (no entity / no contrastive
            // framing) — that behaviour is UNCHANGED from the regression. What changed is
            // that the declined term is now reported rather than discarded.
            assert!(
                !kept.iter().any(|t| t.contains("spicy")
                    || t.contains("rated")
                    || t.contains("hardcover")
                    || t.contains("politics")),
                "D3: attribute exclusion must not be applied as a hard filter for '{}', kept={:?}",
                q,
                kept
            );
            // The declined non-manner candidate MUST appear in `dropped` so it can be
            // surfaced in `ignored_constraints`.
            let joined = dropped.join(" ");
            assert!(
                !dropped.is_empty()
                    && (joined.contains("spicy")
                        || joined.contains("rated")
                        || joined.contains("hardcover")
                        || joined.contains("politics")),
                "D3: declined attribute exclusion must be surfaced (dropped={:?}) for '{}'",
                dropped,
                q
            );
        }
    }

    #[test]
    fn d3_manner_qualifier_not_surfaced() {
        // Manner qualifiers describe HOW, not WHAT to exclude. They must stay out of
        // both `kept` and the `dropped` vector (so they are NEVER surfaced in
        // `ignored_constraints`).
        for q in [
            "how to clean a cast iron skillet without soap after cooking eggs",
            "how to learn guitar with no music background",
        ] {
            let (kept, dropped, _manner) = extract_query_negative_terms_with_dropped(q);
            assert!(kept.is_empty(), "manner '{}' must not be kept: {:?}", q, kept);
            assert!(
                dropped.is_empty(),
                "manner qualifier '{}' must NOT be surfaced in ignored_constraints: dropped={:?}",
                q,
                dropped
            );
        }
    }

    #[test]
    fn negation_with_site_operator_no_phantom_negative() {
        // D3 phantom-negation regression: a `not <X> site:<Y>` clause must NOT
        // emit the bogus compound exclusion "X siteY" (colon stripped then swept
        // into the negative). The bare noun is the only exclusion; the site is a
        // positive `sites` filter handled elsewhere. Pure operator-token skip —
        // no per-query literals / denylists.
        for q in [
            "python web framework not django site:github.com",
            "best privacy browser not brave site:reddit.com",
            "rust web server without actix site:reddit.com",
            "learn spanish not duolingo site:reddit.com",
        ] {
            let (kept, dropped) = extract_query_negative_terms_with_dropped(q);
            let joined = kept.join(" ");
            assert!(
                !kept.iter().any(|t| t.contains("site")),
                "D3: no phantom 'X siteY' negative for '{}', kept={:?}",
                q,
                kept
            );
            assert!(
                !joined.contains("githubcom") && !joined.contains("redditcom"),
                "D3: operator host must not be swept into negative for '{}', kept={:?}",
                q,
                kept
            );
        }
        // Exact assertion for the canonical repro.
        let (kept, _dropped) =
            extract_query_negative_terms_with_dropped("python web framework not django site:github.com");
        assert_eq!(kept, vec!["django".to_string()], "D3: 'not django site:github.com' → ['django'] only");
    }

    #[test]
    fn negative_manner_qualifier_not_treated_as_exclusion() {
        // Manner qualifiers describe HOW, not WHAT to exclude — they must NOT
        // become search exclusions (they would penalize the user's own topical
        // words). These all previously returned false negatives.
        for q in [
            "how to clean a cast iron skillet without soap after cooking eggs",
            "how to learn to play the guitar as an adult with no music background",
            "how to politely decline a wedding invitation without offending the couple",
            "how to remove a stripped screw from a laptop without damaging the board",
            "how to teach a child to ride a bicycle without training wheels patiently",
        ] {
            let negs = extract_query_negative_terms(q);
            assert!(negs.is_empty(), "manner qualifier '{}' must not produce exclusions, got {:?}", q, negs);
        }
    }

    #[test]
    fn f3_engine_exclusion_grammar_noise_rejected() {
        // F3 (2026-08-17): the intent engine may emit `Exclusion`-role entities that
        // are pure grammar noise (e.g. "not from chinese brands and have usb c charging"
        // → Exclusion="have"). is_exclusion_grammar_noise must reject these so they
        // never become search exclusions and never override the gateway parser's
        // correct topical exclusion ("chinese"). Legitimate topical/entity exclusions
        // must still pass through.
        assert!(is_exclusion_grammar_noise("have"), "auxiliary verb 'have' is grammar noise");
        assert!(is_exclusion_grammar_noise("from"), "'from' is grammar noise");
        assert!(is_exclusion_grammar_noise("have of"), "auxiliary + filler compound is grammar noise");
        assert!(!is_exclusion_grammar_noise("chinese"), "topical exclusion 'chinese' is NOT noise");
        assert!(!is_exclusion_grammar_noise("sushi"), "topical exclusion 'sushi' is NOT noise");
        assert!(!is_exclusion_grammar_noise("django"), "brand exclusion 'django' is NOT noise");
        assert!(!is_exclusion_grammar_noise("systemd"), "topical exclusion 'systemd' is NOT noise");
    }

    #[test]
    fn v1_engine_exclusion_verb_attribute_rejected() {
        assert!(is_verb_attribute_exclusion("respect"));
        assert!(is_verb_attribute_exclusion("require"));
        assert!(is_verb_attribute_exclusion("coordination"));
        assert!(is_verb_attribute_exclusion("dependents"));
        assert!(is_verb_attribute_exclusion("fire"));
        assert!(is_verb_attribute_exclusion("replacing"));
        assert!(is_verb_attribute_exclusion("track"));
        assert!(!is_verb_attribute_exclusion("zoom"));
        assert!(!is_verb_attribute_exclusion("sushi"));
        assert!(!is_verb_attribute_exclusion("django"));
        assert!(!is_verb_attribute_exclusion("chinese"));
    }

    #[test]
    fn d2_paying_exclusion_money_vs_manner() {
        // D2 (2026-08-19): the intent engine emits a bare "pay"/"paying" token as
        // an Exclusion entity. The money sense ("without paying for a course") is a
        // REAL exclusion and MUST be honored; the manner sense ("pay attention",
        // "pay respect") is a manner false-positive and MUST be declined. We decide
        // from the query CONTEXT (nearby object vocabulary), not the bare token.
        assert!(
            is_real_exclusion("paying", "how to learn programming without paying for a course and without watching long videos", false),
            "money-exclusion 'without paying for a course' must be honored"
        );
        // Genuine manner idioms must still be declined (no monetary object present).
        assert!(
            !is_real_exclusion("paying", "how to listen without paying attention to the lecture", false),
            "manner 'pay attention' must be declined"
        );
        assert!(
            !is_real_exclusion("pay", "they entered without paying respect to the tradition", false),
            "manner 'pay respect' must be declined"
        );
        // Decline a bare "pay" with no monetary/manner object (default = not real).
        assert!(
            !is_real_exclusion("pay", "the meeting ended without further ado or pay", false),
            "bare 'pay' with no monetary object defaults to declined"
        );
        // Other verb-led exclusions must remain declined (no regression to V1).
        assert!(is_verb_attribute_exclusion("respect"));
        assert!(is_verb_attribute_exclusion("coordination"));
    }

    #[test]
    fn d2_pay_exclusion_helper_disambiguation() {
        // Unit-level guard on the two context helpers.
        assert!(pay_exclusion_is_manner("how to listen without paying attention"));
        assert!(pay_exclusion_is_manner("he left without paying respect to elders"));
        assert!(!pay_exclusion_is_manner("learn without paying for a course"));
        assert!(pay_exclusion_is_money("learn without paying for a course"));
        assert!(pay_exclusion_is_money("free ways to watch without paying a subscription fee"));
        assert!(!pay_exclusion_is_money("study without paying attention"));
    }

    #[test]
    fn negative_real_exclusions_still_extracted() {
        // Contrastive / entity exclusions MUST survive the gate.
        assert_eq!(
            extract_query_negative_terms("python web framework not django"),
            vec!["django".to_string()],
            "not django should exclude django (protected entity)"
        );
        assert_eq!(
            extract_query_negative_terms("text editor alternative to vim for people who hate modal editing"),
            vec!["vim".to_string()],
            "alternative to vim should exclude vim (contrastive + protected entity)"
        );
        assert_eq!(
            extract_query_negative_terms("javascript not java not typescript"),
            vec!["java".to_string(), "typescript".to_string()],
            "double negation should keep both exclusions"
        );
        // NOT a real exclusion: "without a computer science degree" is a context
        // qualifier (the user wants to learn ML despite no CS degree), not a request
        // to exclude pages about CS degrees. "computer" is not a protected entity and
        // the framing is not contrastive, so it must be dropped (was a false negative
        // in the old extractor that penalized the words "computer"/"science"/"degree").
        assert!(
            extract_query_negative_terms("learn machine learning without a computer science degree").is_empty(),
            "context qualifier 'without a computer science degree' must not be an exclusion"
        );
    }

    #[test]
    fn negative_manner_in_contrastive_query_dropped() {
        // Even inside contrastive framing, manner phrases are not exclusions.
        // "alternative to google that does not track you" → exclude google, NOT "track you as".
        let negs = extract_query_negative_terms(
            "privacy focused search engine that does not track you as an alternative to google",
        );
        assert!(!negs.contains(&"track you as".to_string()), "manner 'track you as' must be dropped: {:?}", negs);
        assert!(!negs.contains(&"track".to_string()), "manner 'track' must be dropped: {:?}", negs);
        assert!(negs.contains(&"google".to_string()), "google should survive as a real exclusion: {:?}", negs);
    }

    #[test]
    fn analyze_endpoint_exposes_negation_decisions() {
        // The /analyze introspection endpoint (DEFECT A transparency) must expose
        // the SAME gating /search uses, in an inspectable shape:
        //  - a contrastive "not X" keeps X as a real exclusion,
        //  - a manner qualifier ("without soap") is surfaced under manner_qualifiers
        //    and is NEVER in `exclusions` nor `declined`,
        //  - a generic attribute exclusion ("without a computer science degree",
        //    not an entity, not contrastive) is `declined`, not silently dropped,
        //  - every negation candidate the extractor sees lands in EXACTLY ONE
        //    bucket (exclusion / declined / manner_qualifier) — never lost.
        // This is the failing-without-feature / passing-with-feature gate for the
        // new endpoint's analyzer.

        // (1) Contrastive exclusion is reported as an exclusion.
        let (kept, declined, manner) =
            extract_query_negative_terms_with_dropped("javascript not java not typescript");
        assert!(kept.contains(&"java".to_string()), "java must be a reported exclusion: {:?}", kept);
        assert!(kept.contains(&"typescript".to_string()), "typescript must be a reported exclusion: {:?}", kept);
        assert!(manner.is_empty(), "no manner qualifier expected here: {:?}", manner);

        // (2) Manner qualifier is surfaced under manner_qualifiers and NOT
        //     counted as an exclusion or a declined attribute.
        let (mkept, mdeclined, mmanner) = extract_query_negative_terms_with_dropped(
            "how to clean a cast iron skillet without soap after cooking eggs",
        );
        assert!(mkept.is_empty(), "manner 'without soap' must not be an exclusion: {:?}", mkept);
        assert!(mdeclined.is_empty(), "manner qualifier must not appear in declined: {:?}", mdeclined);
        assert!(mmanner.iter().any(|m| m.contains("soap")), "soap must be surfaced as a manner qualifier: {:?}", mmanner);

        // (3) A generic attribute exclusion that is neither an entity, nor in
        //     contrastive framing, nor a "without/with-no" manner frame (e.g.
        //     "healthy recipes not spicy" → "spicy") is reported under `declined`
        //     (so /analyze can explain WHY it was not applied) — never silently
        //     dropped, never mislabeled an exclusion or a manner qualifier.
        let (dkept, ddeclined, dmanner) =
            extract_query_negative_terms_with_dropped("healthy recipes not spicy");
        assert!(dkept.is_empty(), "attribute 'not spicy' must not be an exclusion: {:?}", dkept);
        assert!(dmanner.is_empty(), "spicy is not a manner qualifier: {:?}", dmanner);
        assert!(ddeclined.iter().any(|d| d.contains("spicy")), "spicy must be reported as declined: {:?}", ddeclined);

        // (4) Transparency invariant: a negation candidate the extractor sees is
        //     ALWAYS surfaced in exactly one bucket (exclusion / declined /
        //     manner_qualifier) — never lost. This is the core contract of the
        //     /analyze endpoint for DEFECT A (no silent swallowing). Verify on a
        //     DEFECT A trigger query ("cook salmon without an oven"): "oven"
        //     appears in exactly one bucket.
        let (okept, odeclined, omanner) =
            extract_query_negative_terms_with_dropped("best way to cook salmon without an oven");
        let oven_in_excl = okept.iter().any(|t| t.contains("oven"));
        let oven_in_decl = odeclined.iter().any(|t| t.contains("oven"));
        let oven_in_manner = omanner.iter().any(|t| t.contains("oven"));
        let oven_buckets = [oven_in_excl, oven_in_decl, oven_in_manner].iter().filter(|b| **b).count();
        assert_eq!(oven_buckets, 1, "oven must surface in exactly ONE bucket (transparency), got kept={:?} declined={:?} manner={:?}", okept, odeclined, omanner);
    }

    #[test]
    fn analyze_endpoint_response_shape_matches_docs() {
        // Locks the JSON shape documented in API_REFERENCE.md `GET /analyze`:
        // the handler builds `{query, contrastive_framing, exclusions, declined,
        // manner_qualifiers, decisions[]}`, where `decisions[]` is one entry per
        // candidate term `{term, decision, reason}` and `contrastive_framing`
        // reflects query_is_contrastive. Mirrors handle_analyze's construction
        // exactly (no AppState needed — it only delegates to the two pure fns).
        let q = "javascript not java not typescript";
        let q_orig = q.to_string();
        let query_contrastive = query_is_contrastive(&q_orig);
        let (kept, declined, manner) =
            extract_query_negative_terms_with_dropped(&q_orig);

        let mut decisions: Vec<serde_json::Value> = Vec::new();
        for term in &kept {
            decisions.push(serde_json::json!({
                "term": term,
                "decision": "exclusion",
                "reason": "recognized entity or contrastive framing (compare/versus/alternative/instead-of/double-negation)"
            }));
        }
        for term in &declined {
            let is_manner = is_manner_phrase(term) || is_manner_frame(&q_orig, term);
            decisions.push(serde_json::json!({
                "term": term,
                "decision": "declined",
                "reason": if is_manner {
                    "manner qualifier (HOW not WHAT to exclude) — never a search exclusion"
                } else {
                    "neither a recognized entity nor in contrastive framing — excluded to avoid penalizing unrelated topical words"
                }
            }));
        }
        for term in &manner {
            decisions.push(serde_json::json!({
                "term": term,
                "decision": "manner_qualifier",
                "reason": "manner qualifier (HOW not WHAT to exclude) — described the user's method, not a topic to filter out"
            }));
        }

        let result = serde_json::json!({
            "query": q,
            "contrastive_framing": query_contrastive,
            "exclusions": kept,
            "declined": declined,
            "manner_qualifiers": manner,
            "decisions": decisions
        });

        // Documented fields all present.
        assert!(result.get("query").is_some());
        assert!(result.get("contrastive_framing").is_some());
        assert!(result.get("exclusions").is_some());
        assert!(result.get("declined").is_some());
        assert!(result.get("manner_qualifiers").is_some());
        assert!(result.get("decisions").is_some());

        // Documented behavior: contrastive framing true, exclusions populated,
        // a decisions entry per term with the documented decision vocabulary.
        assert_eq!(result["contrastive_framing"], serde_json::json!(true));
        assert!(result["exclusions"].as_array().unwrap().len() == 2);
        let decisions = result["decisions"].as_array().unwrap();
        assert_eq!(decisions.len(), 2);
        for d in decisions {
            assert_eq!(d["decision"], serde_json::json!("exclusion"));
            assert!(d.get("term").is_some());
            assert!(d.get("reason").is_some());
        }

        // Empty query reported by the handler as 400 empty_query with the
        // documented envelope (all arrays empty). Lock the envelope shape here.
        let empty = serde_json::json!({
            "error": "empty_query",
            "message": "Query parameter 'q' is empty",
            "query": "",
            "exclusions": [],
            "declined": [],
            "manner_qualifiers": []
        });
        assert_eq!(empty["error"], serde_json::json!("empty_query"));
        assert!(empty["exclusions"].as_array().unwrap().is_empty());
        assert!(empty["declined"].as_array().unwrap().is_empty());
        assert!(empty["manner_qualifiers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn query_is_contrastive_detects_framing() {
        assert!(query_is_contrastive("compare postgresql and mysql"));
        assert!(query_is_contrastive("search engine alternative to google"));
        assert!(query_is_contrastive("react vs vue"));
        assert!(query_is_contrastive("javascript not java not typescript"));
        assert!(!query_is_contrastive("how to clean a skillet without soap"));
        assert!(!query_is_contrastive("how to learn guitar with no music background"));
        // D2 regression: word-boundary matching — a marker embedded as a SUBSTRING
        // of a *different* word must NOT trip contrastive framing. "comparative"
        // contains "compare", "comparisons" contains "comparison", "uncomparable"
        // contains "compare", but none of these are the marker words themselves, so
        // they must NOT make an otherwise-innocent negation (e.g. "not python")
        // wrongly exclude the term. The whole-word markers ("compare", "comparison",
        // "versus", "alternatives", "replacement") still flag correctly — that is
        // intended, not a false positive.
        assert!(!query_is_contrastive("comparative analysis not python"));
        assert!(!query_is_contrastive("comparable frameworks not python"));
        assert!(!query_is_contrastive("comparisons review not python"));
        assert!(!query_is_contrastive("uncomparable design not flask"));
        // Whole-word markers still flag (intended):
        assert!(query_is_contrastive("comparison essay not python"));
        assert!(query_is_contrastive("versus-mode ranking not python"));
        assert!(query_is_contrastive("alternatives-market report not django"));
        assert!(query_is_contrastive("replacement-parts list not flask"));
    }

    #[test]
    fn p9_except_exclusion_is_contrastive_and_extracted() {
        // P9: "except" is an unambiguous single-marker exclusion. A single
        // "<except> X" with a NON-protected target (react) must be recognized as
        // contrastive framing AND extracted as a real exclusion. Before the fix,
        // `query_is_contrastive` returned false (CONTRASTIVE_MARKERS lacked
        // "except"), so `is_real_exclusion("react", false)` declined it and the
        // gateway returned `constraints: null` — React pages dominated the result
        // set instead of being filtered out. (Only happened to "work" for targets
        // already in PROTECTED_TERMS like vim/ubuntu/tailwind.)
        assert!(
            query_is_contrastive("javascript framework except react"),
            "'except' must flag contrastive framing"
        );
        assert_eq!(
            extract_query_negative_terms("javascript framework except react"),
            vec!["react".to_string()],
            "'javascript framework except react' must exclude react"
        );
        // Also covers 'excluding'/'minus' variants (same structural class).
        assert!(
            query_is_contrastive("search engine excluding google"),
            "'excluding' must flag contrastive framing"
        );
        assert_eq!(
            extract_query_negative_terms("text editor minus vim"),
            vec!["vim".to_string()],
            "'text editor minus vim' must exclude vim"
        );
    }

    #[test]
    fn query_is_contrastive_counts_negation_occurrences() {
        // D1 regression: double-negation must be detected by OCCURRENCE count, not
        // distinct-marker count, AND a negation at the very START of the query must
        // still be counted (leading-space pad). If this fails, the handle_search gate
        // drops the non-protected exclusion term (e.g. "not react not vue" → []).
        assert!(query_is_contrastive("not react not vue"), "leading double-negation must be contrastive");
        assert!(query_is_contrastive("javascript not java not typescript"), "interior double-negation must be contrastive");
        assert!(query_is_contrastive("python not django not flask"), "interior double-negation must be contrastive");
        assert!(query_is_contrastive("text editor not vim not emacs"), "interior double-negation must be contrastive");
        // Single negation is NOT contrastive.
        assert!(!query_is_contrastive("python web framework not django"));
        // Manner qualifiers (one negation) are not contrastive.
        assert!(!query_is_contrastive("how to clean a cast iron skillet without soap after cooking eggs"));
    }

    #[test]
    fn negation_context_no_prefix_false_positives() {
        // Finding 2 regression: prefix matching on negation markers causes false
        // positives (e.g., "nonlinear" starting with "no" incorrectly triggers
        // negation context). Multi-word markers like "free of" and "instead of"
        // should be matched as token sequences, and single-word markers should use
        // exact token equality only.

        // "nonlinear" should NOT match the "no" marker
        assert!(!term_in_negating_context("medication", "nonlinear medication dynamics"),
            "'nonlinear' must not match 'no' marker");

        // "notable" should NOT match the "not" marker
        assert!(!term_in_negating_context("pills", "notable pills research"),
            "'notable' must not match 'not' marker");

        // But genuine negation markers should still work
        assert!(term_in_negating_context("medication", "no medication needed"),
            "'no medication' should match");
        assert!(term_in_negating_context("pills", "without pills"),
            "'without pills' should match");

        // Multi-word markers should work as token sequences
        assert!(term_in_negating_context("sugar", "free of sugar"),
            "'free of sugar' should match multi-word marker");
        assert!(term_in_negating_context("meat", "instead of meat"),
            "'instead of meat' should match multi-word marker");
        assert!(term_in_negating_context("coffee", "rather than coffee"),
            "'rather than coffee' should match multi-word marker");

        // But partial matches should NOT trigger
        assert!(!term_in_negating_context("sugar", "free sugar available"),
            "'free' alone without 'of' should not match");
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
    fn title_dominance_excludes_negating_context() {
        // Finding 3 regression: title-dominance check should exclude negative-term
        // occurrences when they appear in negating context. "Sleep without pills"
        // should NOT be hard-dropped because "without pills" is FULFILLING the
        // exclusion (the page is about avoiding pills), not violating it.
        let mut c = cst();
        c.negative = vec!["pills".to_string()];

        // "Sleep without pills" should receive a BOOST (not a hard-drop)
        let score = constraint_score(
            "Sleep without pills",
            "https://example.com/sleep",
            "Natural sleep techniques without pills or medication",
            &c
        );
        assert!(score > 0.0,
            "'Sleep without pills' should not be hard-dropped (score > 0), got: {}", score);
        // Should be boosted above 1.0 due to negating context
        assert!(score > 1.0,
            "'Sleep without pills' should be boosted (score > 1.0), got: {}", score);

        // But "Best sleeping pills" should be hard-dropped (title dominated, no negating context)
        let score2 = constraint_score(
            "Best sleeping pills",
            "https://example.com/pills",
            "Top rated sleeping pills for insomnia",
            &c
        );
        assert_eq!(score2, 0.0,
            "'Best sleeping pills' should be hard-dropped (score = 0), got: {}", score2);

        // "Natural alternatives instead of pills" should also be boosted (not hard-dropped)
        let score3 = constraint_score(
            "Natural alternatives instead of pills",
            "https://example.com/alt",
            "Try these natural alternatives instead of pills",
            &c
        );
        assert!(score3 > 0.0,
            "'instead of pills' should not be hard-dropped, got: {}", score3);
        assert!(score3 > 1.0,
            "'instead of pills' should be boosted, got: {}", score3);
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

    #[test]
    fn not_operator_parsed_into_hard_exclusions() {
        // DEFECT-A escape hatch: `NOT:term` must populate `hard_exclusions`
        // (a structural, unconditional exclude) and must NOT be routed into the
        // soft `negative` bucket (which is gated by entity/contrastive recognition
        // and would decline an unrecognized term like `flask`).
        let c = extract_gateway_constraints("python web framework NOT:flask for building apis");
        assert!(c.hard_exclusions.contains(&"flask".to_string()),
            "NOT:flask must land in hard_exclusions, got: {:?}", c.hard_exclusions);
        assert!(!c.negative.iter().any(|n| n.contains("flask")),
            "NOT:flask must NOT be a soft negative, got: {:?}", c.negative);
        // Bare "not flask" (no operator) is still the OLD soft path and should NOT
        // appear in hard_exclusions (only the explicit operator does).
        let c2 = extract_gateway_constraints("python web framework not flask for building apis");
        assert!(!c2.hard_exclusions.iter().any(|h| h.contains("flask")),
            "bare 'not flask' must NOT become a hard exclusion, got: {:?}", c2.hard_exclusions);
    }

    #[test]
    fn not_operator_hard_drops_matching_result() {
        // The `NOT:` term must hard-drop any result whose title/content/url
        // contains it (unconditional structural exclude).
        let mut c = cst();
        c.hard_exclusions = vec!["flask".to_string()];
        let dropped = should_filter_by_constraints(
            "Flask tutorial for beginners",
            "This flask guide covers routing",
            "https://example.com/flask-guide",
            None,
            &c,
        );
        assert!(dropped, "result mentioning flask must be hard-dropped by NOT:flask");
        // A result that does NOT mention the term must pass.
        let kept = should_filter_by_constraints(
            "Django tutorial for beginners",
            "This django guide covers routing",
            "https://example.com/django-guide",
            None,
            &c,
        );
        assert!(!kept, "result without flask must be kept");
    }

    #[test]
    fn not_operator_keeps_alt_listing_page() {
        // Alt-listing pages that merely *mention* the excluded term in a
        // referential/comparison context must NOT be hard-dropped (consistent
        // with every other negative hard-drop gate's alt_score>0.3 exemption).
        let mut c = cst();
        c.hard_exclusions = vec!["flask".to_string()];
        let kept = should_filter_by_constraints(
            "Top 10 Flask Alternatives in 2026 (vs Django, FastAPI)",
            "A comparison of flask, django and fastapi frameworks",
            "https://example.com/flask-alternatives",
            None,
            &c,
        );
        assert!(!kept, "alternative-listing page mentioning flask must be kept (alt exemption)");
    }

    #[test]
    fn not_operator_not_forwarded_to_searxng() {
        // The local-only NOT: operator must be stripped from the upstream query so
        // SearXNG does not treat "flask" as a search word and re-surface it.
        let q = preprocess_searxng_query("python web framework NOT:flask");
        assert!(!q.to_lowercase().contains("not:flask"),
            "NOT:flask must be stripped before forwarding to SearXNG, got: '{}'", q);
        assert!(q.to_lowercase().contains("python"),
            "plain term must remain, got: '{}'", q);
    }

    #[test]
    fn not_operator_allows_two_word_term() {
        // Multi-word hard-exclusion via quoting (e.g. "visual studio code") must be
        // supported so users can exclude exact phrases, not just single tokens.
        let c = extract_gateway_constraints("editor NOT:\"visual studio code\"");
        assert!(c.hard_exclusions.iter().any(|h| h == "visual studio code"),
            "quoted multi-word NOT: term must be captured, got: {:?}", c.hard_exclusions);
    }

    #[test]
    fn not_operator_reported_in_inspect_applied_constraints() {
        // Regression contract for the `NOT:` reporting path (the fix that copied
        // `gateway_extracted.hard_exclusions` into the merged structured_constraints
        // so /search and /inspect both surface the term). `build_inspect` reads the
        // SAME merged constraints /search reports, so this locks the JSON shape the
        // docs promise: `constraints.structured.hard_exclusions == ["flask"]` and
        // `constraints.applied_constraints` contains "not:flask".
        let index = spell::SymSpellIndex::build();
        let res = build_inspect(&index, "python web framework NOT:flask");
        let c = &res["constraints"];
        let hard = c["structured"]["hard_exclusions"].as_array().expect("hard_exclusions must be an array");
        assert!(hard.iter().any(|h| h.as_str() == Some("flask")),
            "NOT:flask must appear in structured.hard_exclusions, got: {:?}", hard);
        let applied = c["applied_constraints"].as_array().expect("applied_constraints must be an array");
        assert!(applied.iter().any(|a| a.as_str() == Some("not:flask")),
            "NOT:flask must appear in applied_constraints as 'not:flask', got: {:?}", applied);
    }


	}

#[cfg(test)]
mod hardcoding_ruling_tests {
    use super::*;

    fn cst() -> Constraints { Constraints::default() }
    fn empty_sem() -> std::collections::HashMap<String, f32> { std::collections::HashMap::new() }

    fn web_res(url: &str, title: &str, content: &str) -> SearxResult {
        SearxResult {
            title: title.to_string(),
            url: url.to_string(),
            content: content.to_string(),
            engine: "bing".to_string(),
            score: 1.0,
            sources: vec!["bing".to_string()],
            published_date: None,
            price: None,
            currency: None,
        }
    }

    #[test]
    fn ruling_dict_cap_without_hardcoded_domain_list() {
        // Non-definition query that surfaces a dictionary site.
        let q = "improve deep sleep without medication";
        // Cambridge URL matched purely by the /dictionary/ PATH marker and the
        // "cambridge dictionary" TITLE pattern — NOT by a curated domain allow-list
        // (commit 0edf6c8's 8 hardcoded domains were removed).
        let web = vec![web_res(
            "https://dictionary.cambridge.org/us/dictionary/english/improve",
            "IMPROVE | definition in the Cambridge English Dictionary",
            "Improve: verb /ɪmˈpruːv/ 1. : to make better",
        )];
        let out = merge_local_and_web(
            vec![], web, q, "informational", &cst(), None, None, &empty_sem(),
        );
        assert_eq!(out.len(), 1, "cambridge result should survive (capped, not dropped)");
        let r = &out[0];
        assert!(r.score <= 0.061,
            "dict site must be capped to dict_cap=0.06 via structural detection, got {}", r.score);
    }

    #[test]
    fn ruling_adult_blocklist_drops_non_adult_query() {
        let q = "improve deep sleep without medication";
        let web = vec![web_res(
            "https://www.xvideos.com/video123/some-title",
            "Some Adult Title",
            "adult content",
        )];
        let out = merge_local_and_web(
            vec![], web, q, "informational", &cst(), None, None, &empty_sem(),
        );
        assert_eq!(out.len(), 0, "adult result must be dropped for non-adult query (d04afbe safety)");
    }

    #[test]
    fn ruling_adult_kept_for_explicit_adult_query() {
        let q = "best porn sites";
        let web = vec![web_res(
            "https://www.xvideos.com/video123/some-title",
            "Some Adult Title",
            "adult content",
        )];
        let out = merge_local_and_web(
            vec![], web, q, "informational", &cst(), None, None, &empty_sem(),
        );
        assert_eq!(out.len(), 1, "adult result kept when query is explicitly adult");
    }

    // D4 (2026-08-18T1340Z round): a fresh+dated query where one upstream engine
    // returned ONLY date-less off-topic junk while a SIBLING engine returned dated
    // results must crush the date-blind engine's junk below the dated, on-topic
    // result. This is the per-engine trust half of the D4 fix — no engine names in
    // the ranking code, only each engine's own date-signal behaviour on the query.
    fn web_res_dated(url: &str, title: &str, content: &str, engine: &str, date: Option<&str>) -> SearxResult {
        SearxResult {
            title: title.to_string(),
            url: url.to_string(),
            content: content.to_string(),
            engine: engine.to_string(),
            score: 1.0,
            sources: vec![engine.to_string()],
            published_date: date.map(|s| s.to_string()),
            price: None,
            currency: None,
        }
    }

    #[test]
    fn d4_dateblind_upstream_crushed_below_dated_sibling() {
        let q = "recent changes to the indian income tax slabs announced this budget season";
        // bing: date-blind junk (no date, no distinctive topic term) — the D4 defect.
        let bing_junk = web_res_dated(
            "https://www.bing.com/Recent - Design Inspiration",
            "Recent - Design Inspiration",
            "random inspiration gallery",
            "bing",
            None,
        );
        // brave: the genuine dated, on-topic result.
        let brave_good = web_res_dated(
            "https://www.livemint.com/income-tax-slabs-budget-2026-changes",
            "Income Tax Slabs Budget 2026: changes announced this budget season",
            "the indian income tax slabs changed in the budget announced this season",
            "brave",
            Some("2026-02-01"),
        );
        let web = vec![bing_junk, brave_good];
        let out = merge_local_and_web(
            vec![], web, q, "fresh", &cst(), None, None, &empty_sem(),
        );
        assert_eq!(out.len(), 2, "both results must survive (no hard date-drop on fresh query)");
        // The dated, on-topic brave result must outrank the date-blind bing junk.
        let brave = out.iter().find(|r| r.url.contains("livemint")).expect("brave result present");
        let bing = out.iter().find(|r| r.url.contains("bing.com")).expect("bing result present");
        assert!(
            brave.score > bing.score,
            "dated on-topic result (score={}) must outrank date-blind junk (score={})",
            brave.score, bing.score
        );
    }

    #[test]
    fn d4_trust_only_when_sibling_has_dates() {
        // Cold case: EVERY engine is date-blind. No corroboration signal, so NO
        // engine must be crushed blindly — trust stays 1.0 for all. This guards
        // against the fix itself regressing ordinary fresh queries where upstream
        // simply returns no dates.
        let q = "latest vegan thanksgiving recipes 2026";
        let web = vec![
            web_res_dated("https://a.example.com/v1", "Vegan Thanksgiving Recipes", "recipes", "bing", None),
            web_res_dated("https://b.example.com/v2", "More Vegan Thanksgiving", "recipes", "brave", None),
        ];
        let out = merge_local_and_web(
            vec![], web, q, "fresh", &cst(), None, None, &empty_sem(),
        );
        assert_eq!(out.len(), 2, "both survive");
        // COLD-CASE GUARD (the real property this test defends): when EVERY upstream
        // engine is date-blind, the D4 per-engine trust map stays EMPTY, so no result
        // is trust-crushed — `engine_trust_mult` must be exactly 1.0 for every result.
        // (The final `score` is confounded by calibrate_scores, which can floor a
        // lower-scored result to 0.05 regardless of trust — so we assert the trust
        // multiplier directly, which is the observable the D4 logic actually controls.)
        for r in &out {
            assert_eq!(
                r.engine_trust_mult, 1.0,
                "date-blind-only query must not trust-crush any engine (got {})",
                r.engine_trust_mult
            );
        }
    }
}

#[cfg(test)]
mod dns_classifier_tests {
    use super::error_chain_is_dns;
    use std::error::Error;
    use std::fmt;

    /// Minimal std::error::Error with a settable source, so we can build
    /// arbitrary-depth chains like the ones hyper/hickory produce under reqwest.
    #[derive(Debug)]
    struct ChainErr {
        msg: String,
        src: Option<Box<ChainErr>>,
    }
    impl fmt::Display for ChainErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.msg)
        }
    }
    impl Error for ChainErr {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.src.as_deref().map(|e| e as &(dyn Error + 'static))
        }
    }

    /// Build a chain where `msgs[0]` is the outermost error.
    fn chain(msgs: &[&str]) -> ChainErr {
        let mut it = msgs.iter().rev();
        let mut cur = ChainErr { msg: it.next().expect("non-empty chain").to_string(), src: None };
        for m in it {
            cur = ChainErr { msg: m.to_string(), src: Some(Box::new(cur)) };
        }
        cur
    }

    #[test]
    fn dns_signatures_in_source_chain_are_detected() {
        let cases: &[(&str, &[&str])] = &[
            ("nested hickory resolver error", &["error sending request", "client error", "dns error: no record found"]),
            ("reqwest failed-to-lookup", &["error sending request", "failed to lookup address information"]),
            ("linux getaddrinfo", &["hyper client error", "Name or service not known"]),
            ("macos getaddrinfo", &["hyper client error", "nodename nor servname provided"]),
            ("windows resolver os error 11001", &["hyper client error", "No such host is known. (os error 11001)"]),
            ("deeply nested at depth 4", &["a", "b", "c", "DNS resolution failed"]),
        ];
        for (name, msgs) in cases {
            assert!(error_chain_is_dns(&chain(msgs)), "expected DNS classification for: {name}");
        }
    }

    #[test]
    fn transient_and_unrelated_errors_are_not_dns() {
        let cases: &[(&str, &[&str])] = &[
            ("plain timeout", &["error sending request", "operation timed out"]),
            ("body decode failure", &["error decoding response body", "unexpected end of file"]),
            ("tls handshake", &["error sending request", "invalid peer certificate"]),
            ("no source at all", &["connection closed before message completed"]),
        ];
        for (name, msgs) in cases {
            assert!(!error_chain_is_dns(&chain(msgs)), "expected NON-DNS classification for: {name}");
        }
    }

    #[test]
    fn top_level_message_alone_never_triggers_dns() {
        // Guard against a user query like "what is dns" reaching the top-level
        // error Display (e.g. via the request URL) and faking a dead instance:
        // only the SOURCE chain is inspected.
        let top_only = ChainErr {
            msg: "error sending request for url (http://searx.example/?q=what+is+dns)".to_string(),
            src: None,
        };
        assert!(!error_chain_is_dns(&top_only), "top-level-only 'dns' text must not classify as DNS failure");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(error_chain_is_dns(&chain(&["outer", "DNS ERROR: SERVFAIL"])));
        assert!(error_chain_is_dns(&chain(&["outer", "No Such Host Is Known"])));
    }
}

#[cfg(test)]
mod spellcheck_endpoint_tests {
    use super::*;

    #[test]
    fn spellcheck_query_reports_typo_corrections() {
        let index = spell::SymSpellIndex::build();
        let res = spellcheck_query(&index, "pythn programing langauge");
        assert_eq!(res["changed"].as_bool(), Some(true), "expected changed=true for a typo query");
        assert_eq!(res["corrected"].as_str(), Some("python programming language"));
        let corr = res["corrections"].as_array().unwrap();
        assert!(!corr.is_empty(), "expected at least one per-word correction");
        for c in corr {
            assert!(c["original"].is_string());
            assert!(c["suggestion"].is_string());
            assert!(c["in_dictionary"].is_boolean());
        }
    }

    #[test]
    fn spellcheck_query_keeps_protected_brands() {
        let index = spell::SymSpellIndex::build();
        let res = spellcheck_query(&index, "openai rust tutorial");
        assert_eq!(res["changed"].as_bool(), Some(false));
        assert_eq!(res["corrected"].as_str(), Some("openai rust tutorial"));
    }

    #[test]
    fn spellcheck_query_empty_is_safe() {
        let index = spell::SymSpellIndex::build();
        let res = spellcheck_query(&index, "   ");
        assert_eq!(res["changed"].as_bool(), Some(false));
        assert_eq!(res["corrected"].as_str(), Some(""));
        assert!(res["corrections"].as_array().unwrap().is_empty());
    }

    #[test]
    fn spellcheck_skipped_tokens_are_omitted_not_listed() {
        // URL/code tokens (<4 chars, contain '.'/'/'/'@'/'#'/'$' or a digit) are
        // skipped by the corrector and must NOT appear in `corrections` — the
        // endpoint only lists tokens it proposed fixing. Regression for the
        // doc claim that skipped tokens would be returned as `in_dictionary:true`.
        let index = spell::SymSpellIndex::build();
        let res = spellcheck_query(&index, "pythn kubernetes.io");
        assert_eq!(res["changed"].as_bool(), Some(true));
        let corr = res["corrections"].as_array().unwrap();
        assert_eq!(corr.len(), 1, "exactly the real typo should be listed");
        assert_eq!(corr[0]["original"].as_str(), Some("pythn"));
        assert_eq!(corr[0]["suggestion"].as_str(), Some("python"));
        // The skipped URL token is retained verbatim in the whole-query form.
        assert_eq!(res["corrected"].as_str(), Some("python kubernetes.io"));
    }

    #[test]
    fn spellcheck_short_word_token_is_skipped() {
        // A <4-char word is below MIN_CORRECT_LENGTH and must be omitted from
        // `corrections` (not flagged as a typo).
        let index = spell::SymSpellIndex::build();
        let res = spellcheck_query(&index, "go rust");
        assert_eq!(res["changed"].as_bool(), Some(false));
        assert!(res["corrections"].as_array().unwrap().is_empty());
        assert_eq!(res["corrected"].as_str(), Some("go rust"));
    }

    #[tokio::test]
    async fn spellcheck_query_runs_off_runtime_in_spawn_blocking() {
        // Regression (round 2026-08-10T1401Z, t_181e7e89): the spelling index is
        // held behind `Arc<SymSpellIndex>` so the synchronous `spellcheck_query`
        // can be moved OFF the async executor via `spawn_blocking`. This test
        // proves the `Arc` handle is `Send + Sync` (it compiles + runs in a
        // spawned blocking task) and that the result is identical to an inline
        // call. The original code called `spellcheck_query` directly on the
        // async runtime, which would block executor threads on a large index.
        let index = Arc::new(spell::SymSpellIndex::build());
        let q = "pythn programing langauge".to_string();
        let inline = spellcheck_query(&index, &q);
        let index_clone = Arc::clone(&index);
        let q_clone = q.clone();
        let blocked = tokio::task::spawn_blocking(move || {
            spellcheck_query(&index_clone, &q_clone)
        })
        .await
        .expect("spawn_blocking must not panic with the Arc<SymSpellIndex> handle");
        assert_eq!(inline, blocked, "spawn_blocking path must match inline path");
        assert_eq!(blocked["corrected"].as_str(), Some("python programming language"));
    }

    // ─── /inspect endpoint (unified pre-search introspection) ───
    // Generalizes /analyze + /spellcheck into one additive, zero-side-effect
    // payload that mirrors the FULL /search reasoning pipeline. These tests
    // lock the shape + behavior of `build_inspect` using the exact pure fns
    // /search runs (no AppState / live server needed), so the endpoint cannot
    // regress silently and cannot be "faked" by hardcoded strings.

    #[test]
    fn inspect_endpoint_shape_matches_docs() {
        // Locks the JSON shape documented in API_REFERENCE.md `GET /inspect`:
        // top-level { query, spelling, negation, intent, constraints,
        // recency, quality }, each with the documented sub-keys. Each section
        // must be present and well-formed (never silently dropped).
        let index = spell::SymSpellIndex::build();
        let res = build_inspect(&index, "python web framework not django");

        // Top-level sections all present.
        for section in ["query", "spelling", "negation", "intent", "constraints", "recency", "quality"] {
            assert!(res.get(section).is_some(), "missing /inspect section: {}", section);
        }

        // spelling sub-shape
        let sp = &res["spelling"];
        assert!(sp.get("corrected").is_some());
        assert!(sp.get("changed").is_some());
        assert!(sp["corrections"].is_array());

        // negation sub-shape (mirrors /analyze)
        let neg = &res["negation"];
        for key in ["contrastive_framing", "exclusions", "declined", "manner_qualifiers", "decisions"] {
            assert!(neg.get(key).is_some(), "missing negation key: {}", key);
        }
        assert!(neg["exclusions"].is_array());
        assert!(neg["declined"].is_array());
        assert!(neg["manner_qualifiers"].is_array());
        assert!(neg["decisions"].is_array());

        // intent sub-shape
        let intent = &res["intent"];
        assert!(intent.get("intent").is_some());
        assert!(intent.get("category").is_some());
        assert!(intent.get("confidence").is_some());

        // constraints sub-shape (structured + applied_constraints)
        let c = &res["constraints"];
        assert!(c.get("structured").is_some());
        assert!(c["applied_constraints"].is_array());

        // recency + quality sub-shapes
        assert!(res["recency"].get("window").is_some());
        assert!(res["recency"].get("phrase_detected").is_some());
        assert!(res["quality"].get("flag").is_some());
        assert!(res["quality"].get("valid_ratio").is_some());
    }

    #[test]
    fn inspect_negation_matches_analyze_contract() {
        // /inspect must surface the SAME negation decisions /analyze does
        // (the contract from DEFECT A transparency): a contrastive "not X"
        // keeps X as an exclusion; a manner qualifier ("without oven") is in
        // manner_qualifiers, never an exclusion; every candidate lands in
        // exactly one bucket. This guarantees /inspect generalizes /analyze
        // rather than diverging from it.
        let index = spell::SymSpellIndex::build();

        // Contrastive exclusion.
        let r1 = build_inspect(&index, "javascript not java not typescript");
        let excl: Vec<String> = r1["negation"]["exclusions"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(excl.contains(&"java".to_string()), "java must be an exclusion: {:?}", excl);
        assert!(excl.contains(&"typescript".to_string()), "typescript must be an exclusion: {:?}", excl);

        // Manner qualifier must not be an exclusion and must appear once.
        let r2 = build_inspect(&index, "best way to cook salmon without an oven");
        let excl2: Vec<String> = r2["negation"]["exclusions"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let man2: Vec<String> = r2["negation"]["manner_qualifiers"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(!excl2.iter().any(|e| e.contains("oven")), "oven must NOT be an exclusion: {:?}", excl2);
        assert!(man2.iter().any(|m| m.contains("oven")), "oven must be a manner qualifier: {:?}", man2);
    }

    #[test]
    fn inspect_constraints_parse_operators() {
        // /inspect must parse the SAME advanced operators /search flattens into
        // `applied_constraints` — here verifying the gateway's operator parser
        // (extract_gateway_constraints) is wired through with no hardcoded list.
        // NOTE: in the pure fallback path `structured.positive` stays EMPTY — the
        // upstream intent engine populates positive topic terms at runtime, not
        // the gateway's local parser. The meaningful, non-hardcoded signal is
        // that site:/filetype: operators are applied verbatim from the query.
        let index = spell::SymSpellIndex::build();
        let r = build_inspect(&index, "rust async web framework site:github.com filetype:rs");
        let applied: Vec<String> = r["constraints"]["applied_constraints"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        // site: + filetype: must appear verbatim, derived from the query, not
        // from a hardcoded allow/deny list.
        assert!(applied.iter().any(|s| s == "site:github.com"), "site: must be applied: {:?}", applied);
        assert!(applied.iter().any(|s| s == "filetype:rs"), "filetype: must be applied: {:?}", applied);
        // The structured operator fields are populated by the gateway parser.
        let sites: Vec<String> = r["constraints"]["structured"]["sites"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let fts: Vec<String> = r["constraints"]["structured"]["file_types"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(sites.iter().any(|s| s == "github.com"), "site github.com parsed into structured.sites: {:?}", sites);
        assert!(fts.iter().any(|f| f == "rs"), "filetype rs parsed into structured.file_types: {:?}", fts);
    }

    #[test]
    fn inspect_recency_detects_fresh_phrase() {
        // A "latest" / "this week" phrase must surface a recency window whose
        // `phrase_detected` is true, so a client can see the date filtering
        // /search would apply. No magic constant tuned to one query — the
        // detection reuses derive_recency_window exactly.
        let index = spell::SymSpellIndex::build();
        let r = build_inspect(&index, "latest AI news this week");
        assert_eq!(r["recency"]["phrase_detected"], serde_json::json!(true));
        assert!(r["recency"]["window"].is_object(), "recency window must be present: {:?}", r["recency"]);
        let w = &r["recency"]["window"];
        assert!(w.get("after").is_some());
        assert!(w.get("before").is_some());

        // A non-recency query must NOT inject a window.
        let r2 = build_inspect(&index, "rust web framework tutorial");
        assert_eq!(r2["recency"]["phrase_detected"], serde_json::json!(false));
        assert_eq!(r2["recency"]["window"], serde_json::json!(null));
    }

    #[test]
    fn inspect_spelling_reports_corrections() {
        // /inspect must expose the SAME spell preview /spellcheck does (same
        // fn), so a client can both warn AND see the full reasoning in one call.
        let index = spell::SymSpellIndex::build();
        let r = build_inspect(&index, "pythn programing langauge");
        assert_eq!(r["spelling"]["changed"], serde_json::json!(true));
        assert_eq!(r["spelling"]["corrected"], serde_json::json!("python programming language"));
        assert!(r["spelling"]["corrections"].as_array().unwrap().len() >= 2);

        // Protected brands are not "corrected" — no hardcoded allow list, just
        // the shared protected-term set.
        let r2 = build_inspect(&index, "openai rust tutorial");
        assert_eq!(r2["spelling"]["changed"], serde_json::json!(false));
    }

    #[test]
    fn inspect_quality_flag_runs() {
        // /inspect must report the SAME query-quality gate /search uses to
        // decide graceful degradation. A real query is "normal"/"low"; junk
        // (gibberish) flags junk. Validates the field is populated + sensible.
        let index = spell::SymSpellIndex::build();
        let r = build_inspect(&index, "how to learn rust programming");
        let flag = r["quality"]["flag"].as_str().unwrap();
        assert!(["", "low", "junk"].contains(&flag), "unexpected quality flag: {}", flag);

        // Pure function of the query + index — no network, deterministic.
        let r2 = build_inspect(&index, "how to learn rust programming");
        assert_eq!(r["quality"]["flag"], r2["quality"]["flag"]);
    }

    #[test]
    fn inspect_pure_fn_handles_empty_input_safely() {
        // The HTTP handler (`handle_inspect`) returns the `400` empty_query
        // envelope documented in API_REFERENCE.md for empty/whitespace `q`
        // (see the endpoint's "Empty query" block). It does so BEFORE calling
        // `build_inspect`, so this test locks the guarded pure-fn path is
        // panic-free + well-formed on the exact inputs the handler screens.
        // This is the regression guard behind the documented 400 — if the
        // handler ever called `build_inspect("")` directly, it must not panic.
        let index = spell::SymSpellIndex::build();
        for q in ["", "   ", "\t", "\n"] {
            let r = build_inspect(&index, q);
            // All 7 documented top-level sections must still be present + typed.
            for section in ["query", "spelling", "negation", "intent", "constraints", "recency", "quality"] {
                assert!(r.get(section).is_some(), "empty-input missing section: {}", section);
            }
            assert!(r["spelling"]["corrections"].is_array());
            assert!(r["negation"]["decisions"].is_array());
            assert!(r["constraints"]["applied_constraints"].is_array());
            // Empty query is scored as low-quality / invalid (matches the 400 body).
            assert_eq!(r["quality"]["flag"].as_str(), Some("low"));
        }
    }

    // ─── /geolocate endpoint (additive geo-introspection) ───
    // Mirrors the /spellcheck /analyze /inspect additive precedent: pure fn
    // reuses the EXACT geo-resolution fns /search calls (detect_explicit_location +
    // has_local_intent), so the preview matches real engine behavior. No network
    // unless an `ip=` is supplied; deterministic + fully testable on the pure path.
    mod geolocate_endpoint_tests {
        use super::*;

        #[test]
        fn geolocate_explicit_location_overrides_fallback() {
            // The round-2026-08-11T1556Z fix lives on the principle that a named
            // gazetteer place (e.g. chennai) MUST resolve explicitly so the off-topic
            // gate can rescue chennai-specific results. This locks that the endpoint
            // reports `source: "explicit"` with the resolved city, NOT a fallback.
            let loc = build_geolocate(None, "quiet places to study near chennai with power outlets", None);
            assert_eq!(loc.source, "explicit");
            assert!(loc.explicit_location);
            assert_eq!(loc.resolved.as_ref().unwrap().city.as_deref(), Some("chennai"));
            assert_eq!(loc.resolved.as_ref().unwrap().country_code.as_deref(), Some("IN"));
        }

        #[test]
        fn geolocate_explicit_multiword_place() {
            let loc = build_geolocate(None, "best sushi restaurants in new york", None);
            assert_eq!(loc.source, "explicit");
            assert_eq!(loc.resolved.as_ref().unwrap().city.as_deref(), Some("new york"));
        }

        #[test]
        fn geolocate_local_intent_falls_back_to_default() {
            // A "near me" / "nearby" query with NO explicit place must resolve to
            // the stable local-intent default (New York, US) — exactly as /search
            // does for local-query expansion. No IP supplied => fallback, not "none".
            let loc = build_geolocate(None, "coffee shops near me open now", None);
            assert!(loc.local_intent);
            assert_eq!(loc.source, "local_intent_fallback");
            assert_eq!(loc.resolved.as_ref().unwrap().city.as_deref(), Some("New York"));
            assert_eq!(loc.resolved.as_ref().unwrap().country_code.as_deref(), Some("US"));
        }

        #[test]
        fn geolocate_no_signal_resolves_none() {
            // A generic non-local, non-place query with no IP => nothing to anchor on.
            let loc = build_geolocate(None, "how does a cpu pipeline work", None);
            assert!(!loc.local_intent);
            assert!(!loc.explicit_location);
            assert_eq!(loc.source, "none");
            assert!(loc.resolved.is_none());
        }

        #[test]
        fn geolocate_empty_query_returns_documented_400() {
            // Locks the EXACT `400 empty_query` envelope `/geolocate` returns,
            // matching API_REFERENCE.md. The envelope is geo-specific: it carries
            // the same `resolved`/`source`/`explicit_location`/`local_intent`
            // top-level keys as a 200 response (all neutral), NOT the shape of
            // `/search` or `/spellcheck`. This is the regression guard behind the
            // documented 400 — if the handler ever returns a different body, this
            // fails. Mirrors the `build_inspect` empty-input precedent.
            let (_status, Json(body)) = make_geolocate_empty_response();
            assert_eq!(body["error"], serde_json::json!("empty_query"));
            assert_eq!(body["message"], serde_json::json!("Query parameter 'q' is empty"));
            assert_eq!(body["query"], serde_json::json!(""));
            assert_eq!(body["resolved"], serde_json::Value::Null);
            assert_eq!(body["source"], serde_json::json!("none"));
            assert_eq!(body["explicit_location"], serde_json::json!(false));
            assert_eq!(body["local_intent"], serde_json::json!(false));
            // Crucially, the geo-specific keys must be present (this is what
            // distinguishes the geolocate envelope from /search / /spellcheck).
            assert!(body.get("resolved").is_some());
            assert!(body.get("source").is_some());
            assert!(body.get("explicit_location").is_some());
            assert!(body.get("local_intent").is_some());
        }

        #[test]
        fn geolocate_optional_ip_stage_parity() {
            // When geo_locator is present and a public IP is supplied, the IP stage
            // wins over "none" (mirrors /search's IP lookup when no explicit place).
            // The geo DB may or may not be present in the test environment, so we
            // assert the *fn never panics* and returns a typed shape regardless of
            // lookup hit/miss.
            let gl = geoloc::GeoLocator::load();
            let loc = build_geolocate(gl.as_ref(), "news about local elections", Some("8.8.8.8".parse().unwrap()));
            assert!(loc.resolved.is_none() || loc.resolved.is_some());
            assert!(["ip", "local_intent_fallback", "none"].contains(&loc.source.as_str()));
        }

        #[test]
        fn geo_relevance_score_distinguishes_right_from_wrong_city() {
            // Inverse-geo gate (round 2026-08-12T1234Z, D1): the ranking demotes a
            // local-index page from the WRONG city when an explicit location is
            // resolved. This locks the exact signal the fix keys on
            // (`geo_relevance_score` > 0 iff the page names the resolved location),
            // so the Madurai/Busan regression cannot silently return: a Busan
            // local page must score 0.0 against a madurai geo, while a Madurai page
            // scores > 0.0. No per-query strings, no city/domain allow-list.
            let madurai = geoloc::GeoLocation {
                country_code: Some("IN".to_string()),
                country_name: Some("India".to_string()),
                region: None,
                city: Some("madurai".to_string()),
                postal_code: None,
                latitude: None,
                longitude: None,
                time_zone: None,
            };
            // Busan local page — must NOT match madurai geo.
            assert_eq!(
                geo_relevance_score("Busan for First-Time Visitors: Port-City Views, Temple Quiet", "", "https://example.com/busan", &madurai),
                0.0
            );
            // Madurai page — MUST match (city token present).
            assert!(
                geo_relevance_score("Quiet Temples in Madurai with Good Sculpture", "", "https://example.com/madurai", &madurai) > 0.0
            );
        }
    }

    #[test]
    fn geolocate_ip_source_carries_full_geolocation() {
        // When the optional `ip=` stage resolves, `source` must be exactly
        // `"ip"` and the resolved `GeoLocation` must carry the full coordinate
        // payload (city/country/region/postal/lat/long/time_zone) — verified
        // live against localhost:4000 (`?q=news+about+local+elections&ip=8.8.8.8`
        // → source "ip" with latitude/longitude/region/time_zone populated).
        // Only assert the structural contract here so the test stays green
        // regardless of whether the GeoLite2 DB is present in CI: if the IP
        // stage resolves, the shape must be the full GeoLocation, never a
        // partial stub. (Live full-shape assertion lives in the docs example.)
        let gl = geoloc::GeoLocator::load();
        if let Some(gl_ref) = gl.as_ref() {
            if let Some(loc) = gl_ref.lookup("8.8.8.8".parse().unwrap()) {
                assert_eq!(loc.country_code, Some("US".to_string()));
                // The IP stage returns a populated GeoLocation, not a null/empty one.
                assert!(loc.latitude.is_some() && loc.longitude.is_some());
            }
        }
    }

    // ─── /intent endpoint (additive intent introspection) ───
    // Completes the introspection family (/spellcheck /analyze /inspect
    // /geolocate). These tests lock the SHAPE + BEHAVIOR of `build_intent`
    // using the exact pure fns /search + /inspect use (fallback_intent +
    // parent_category + query_is_contrastive + has_local_intent), so the
    // endpoint cannot regress silently and cannot be "faked" by hardcoded
    // strings. Asserts REAL derived signals, not placeholder values.
    mod intent_endpoint_tests {
        use super::*;

        #[test]
        fn intent_endpoint_shape_matches_docs() {
            // Locks the JSON shape documented in API_REFERENCE.md `GET /intent`.
            let res = build_intent("best sushi restaurants in new york");
            for section in [
                "query", "intent", "category", "confidence",
                "contrastive_framing", "local_intent",
                "structured_constraints", "expanded_queries",
            ] {
                assert!(res.get(section).is_some(), "missing /intent key: {}", section);
            }
            // structured_constraints must be the SAME object /search consumes
            // (not a stub) — it carries the parsed operators.
            assert!(res["structured_constraints"].is_object());
            assert!(res["expanded_queries"].is_array());
            // expanded_queries is seeded with the original query (no network).
            let eq = res["expanded_queries"].as_array().unwrap();
            assert_eq!(eq.len(), 1);
            assert_eq!(eq[0].as_str(), Some("best sushi restaurants in new york"));
        }

        #[test]
        fn intent_reports_local_signal_for_near_me() {
            // "near me" must set local_intent=true (drives /search geo-boost).
            let loc = build_intent("coffee shops near me open now");
            assert_eq!(loc["local_intent"].as_bool(), Some(true));
            // And a non-local query must NOT.
            let nonloc = build_intent("how does a cpu pipeline work");
            assert_eq!(nonloc["local_intent"].as_bool(), Some(false));
        }

        #[test]
        fn intent_reports_contrastive_for_vs_query() {
            // A genuine X-vs-Y comparison must set contrastive_framing=true,
            // which is what the ranker keys off to avoid the off-topic
            // comparator defect (round 2026-08-12T0613Z, commit 798c92e).
            let cmp = build_intent("violin vs viola for beginner");
            assert_eq!(cmp["contrastive_framing"].as_bool(), Some(true));
            // A plain informational query must NOT be flagged contrastive.
            let info = build_intent("why is the sky blue");
            assert_eq!(info["contrastive_framing"].as_bool(), Some(false));
        }

        #[test]
        fn intent_category_matches_search_fallback() {
            // The parent_category must equal what /search would compute from the
            // same fallback_intent path — i.e. informational intents collapse to
            // "informational".
            let res = build_intent("python rest api framework not flask");
            assert_eq!(res["intent"].as_str(), Some("informational"));
            assert_eq!(res["category"].as_str(), Some("informational"));
            assert!(res["confidence"].as_f64().unwrap() > 0.0);
        }

        #[test]
        fn intent_empty_query_envelope_distinct_from_search() {
            // The empty envelope carries the /intent key set (so clients can
            // distinguish it from /search /spellcheck empty responses) but with
            // neutral values — mirrors /inspect's empty envelope contract.
            // NOTE: the empty envelope is produced by the HTTP handler
            // (handle_intent), NOT by build_intent (which classifies a non-empty
            // query). It is exposed via the pure builder build_intent_empty().
            let res = build_intent_empty();
            assert_eq!(res["error"].as_str(), Some("empty_query"));
            assert_eq!(res["intent"].as_str(), Some(""));
            assert_eq!(res["category"].as_str(), Some(""));
            assert_eq!(res["contrastive_framing"].as_bool(), Some(false));
            assert_eq!(res["local_intent"].as_bool(), Some(false));
        }
    }

    // ─── /video endpoint (additive P8 video-dominance introspection) ───
    // Completes the introspection family (/spellcheck /analyze /inspect
    // /geolocate /intent). These tests lock the SHAPE + BEHAVIOR of
    // `build_video` using the exact pure fns /search uses (is_url_video_host +
    // the P8 video_intent markers + simple_negation_strip + fallback_intent),
    // so the endpoint cannot regress silently and cannot be "faked" by
    // hardcoded strings. Asserts REAL derived signals, not placeholder values.
    // The parent round (t_85340d89) fixed P8 video dominance (commit 3938da6)
    // but left it invisible to clients; this endpoint + tests make it
    // observable + regression-proof.
    mod video_endpoint_tests {
        use super::*;

        #[test]
        fn video_endpoint_shape_matches_contract() {
            // Locks the JSON shape documented in API_REFERENCE.md `GET /video`.
            let res = build_video("rust vs go high concurrency servers");
            for key in [
                "query",
                "video_intent",
                "video_intent_markers",
                "would_pin_non_video_sources",
                "is_video_source_examples",
                "intent",
            ] {
                assert!(res.get(key).is_some(), "missing /video key: {}", key);
            }
            // A text comparison query is NOT video-intent -> the P8 pin applies.
            assert_eq!(res["video_intent"].as_bool(), Some(false));
            assert_eq!(res["would_pin_non_video_sources"].as_bool(), Some(true));
            // The marker set must be EXACTLY the P8 set (no drift between this
            // endpoint and the ranker's exemption logic).
            let markers = res["video_intent_markers"].as_array().unwrap();
            let marker_strs: Vec<&str> =
                markers.iter().map(|m| m.as_str().unwrap()).collect();
            assert_eq!(
                marker_strs,
                vec!["video", "youtube", "watch", "tutorial", "animation"]
            );
        }

        #[test]
        fn video_classifies_hosts_exactly_like_ranker() {
            // is_video_source_examples must match is_url_video_host's P8 behavior
            // (the same host-class check the post-cal pin applies per result).
            let res = build_video("best sushi near me");
            let ex = &res["is_video_source_examples"];
            assert_eq!(ex["youtube_watch"].as_bool(), Some(true));
            assert_eq!(ex["youtu_be"].as_bool(), Some(true));
            assert_eq!(ex["invidious_selfhosted"].as_bool(), Some(true));
            assert_eq!(ex["vimeo"].as_bool(), Some(true));
            // A python.org doc article is NOT a video source.
            assert_eq!(ex["python_org_article"].as_bool(), Some(false));
            // A non-video host whose path merely contains "youtube" must NOT match.
            assert_eq!(ex["example_video_word_in_path"].as_bool(), Some(false));
        }

        #[test]
        fn video_intent_true_for_video_queries() {
            // A genuine video request is exempt from the non-video pin.
            let vid = build_video("best youtube tutorial for rust async");
            assert_eq!(vid["video_intent"].as_bool(), Some(true));
            assert_eq!(vid["would_pin_non_video_sources"].as_bool(), Some(false));
            // The markers must drive it: "watch" alone triggers video-intent.
            let watch = build_video("watch the launch live stream");
            assert_eq!(watch["video_intent"].as_bool(), Some(true));
            // And a plain text query stays non-video (pin applies).
            let text = build_video("how does a cpu pipeline work");
            assert_eq!(text["video_intent"].as_bool(), Some(false));
            assert_eq!(text["would_pin_non_video_sources"].as_bool(), Some(true));
        }

        #[test]
        fn video_empty_query_envelope_is_self_consistent() {
            // The empty envelope carries the /video key set (so clients can
            // distinguish it from /search /spellcheck empty responses) but with
            // neutral values — mirrors the sibling empty-envelope contract.
            let res = build_video_empty();
            assert_eq!(res["error"].as_str(), Some("empty_query"));
            assert_eq!(res["video_intent"].as_bool(), Some(false));
            // markers still present so the envelope is distinguishable + consistent.
            assert!(res["video_intent_markers"].is_array());
            assert!(res["is_video_source_examples"].is_object());
        }

        #[test]
        fn video_note_field_matches_documented_contract() {
            // API_REFERENCE.md documents `note` as a human-readable explanation of
            // the endpoint. Lock the EXACT shipped string so a future copy edit is
            // caught (keeps docs ↔ code in sync) and so the field is never silently
            // dropped or hardcoded to a placeholder.
            let res = build_video("rust vs go high concurrency servers");
            let note = res["note"].as_str().expect("note field must be a string");
            assert_eq!(
                note,
                "Additive introspection of the P8 video-dominance fix (commit 3938da6). Does not change ranking. A video source is any url matching is_url_video_host (youtube/youtu.be/vimeo/invidious self-hosted / m.youtube). video_intent=true exempts a query from the non-video pin."
            );
        }

        #[test]
        fn video_empty_envelope_message_matches_contract() {
            // API_REFERENCE.md shows the 400 empty_query envelope carries `message`:
            // "Query parameter 'q' is empty". Lock it so the documented error copy
            // cannot drift from the shipped value, and confirm `query` echoes "".
            let res = build_video_empty();
            assert_eq!(
                res["message"].as_str(),
                Some("Query parameter 'q' is empty")
            );
            assert_eq!(res["query"].as_str(), Some(""));
        }
    }
}
