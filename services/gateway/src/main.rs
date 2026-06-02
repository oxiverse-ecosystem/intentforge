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

// Helper: deserialize null/missing string fields as empty String
fn deserialize_null_as_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where D: serde::Deserializer<'de> {
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct WhoogleResult {
    #[serde(alias = "href", alias = "link")]
    url: String,
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
    #[serde(default)]
    content: String,
}

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
        "informational" | "technical" | "how-to" | "comparison" | "fresh" => "informational",
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
    sources: Vec<String>,  // e.g. ["local", "bing", "brave", "whoogle"]
    #[serde(default)]
    is_local: bool,
}

#[derive(Serialize)]
struct UnifiedResponse {
    intent: String,
    #[serde(default)]
    category: String,
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
    results: Vec<MergedResult>,
}

// ─── Domain Authority (Fully Algorithmic) ────────────────────────────
// Scores based purely on URL structure signals — no hardcoded domain lists.
// Signals: TLD trust, subdomain patterns, path patterns, URL complexity.

fn domain_authority_score(url: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();

    let mut score: f32 = 0.5; // baseline for unknown domains

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
    let path = reqwest::Url::parse(url).ok().map(|u| u.path().to_lowercase()).unwrap_or_default();
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path_segments.len() >= 2 {
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
    let mut estimated_age_hours: f32 = 168.0; // default: 7 days (less aggressive decay)

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

// ─── Constraint Scoring (Dynamic, Query-Derived) ───────────────────
// Applies structured constraints from intent analysis:
// - Positive constraints: boost results that match constraint terms
// - Negative constraints: penalize results that match negative terms
// NOT hardcoded — constraints come from the query itself.
// Score range: 0.0 (violates negative) to 1.0 (matches all positives, no negatives)

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
    let neg_count = constraints.negative.len() as f32;
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
    for neg in &constraints.negative {
        let neg_lower = neg.to_lowercase();
        let neg_normalized: String = neg_lower.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
        let neg_words: Vec<&str> = neg_lower.split_whitespace().collect();
        // Match against title + content + URL for all constraint lengths
        // Content matching uses word boundaries to reduce noise
        let matched = if neg_words.len() == 1 {
            // Title matching: exact word, trimmed, normalized
            title_lower.split_whitespace().any(|w| {
                w == neg_lower
                || w.trim_matches(|c: char| !c.is_alphanumeric()) == neg_lower
                || {
                    let w_alpha: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                    w_alpha == neg_normalized || w_alpha.contains(&neg_normalized)
                }
            })
            || title_normalized.split_whitespace().any(|w| {
                let w_clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                w_clean == neg_normalized || w_clean.contains(&neg_normalized)
            })
            || (neg_normalized.len() >= 3 && title_normalized.contains(&neg_normalized))
            // Content matching: word boundary match in first 500 chars to limit noise
            || {
                let content_prefix: String = content.chars().take(500).collect();
                let content_lower = content_prefix.to_lowercase();
                content_lower.split_whitespace().any(|w| {
                    w == neg_lower
                    || w.trim_matches(|c: char| !c.is_alphanumeric()) == neg_lower
                    || {
                        let w_alpha: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                        w_alpha == neg_normalized
                        // Compound-word awareness: "tailwindcss" starts with "tailwind"
                        // Length ratio guard: constraint must be ≥60% of word length to avoid
                        // false positives like "reactive" matching "react"
                        || (w_alpha.len() > neg_normalized.len()
                            && neg_normalized.len() >= 3
                            && w_alpha.starts_with(&neg_normalized)
                            && neg_normalized.len() as f32 / w_alpha.len() as f32 >= 0.6)
                        || w_alpha.contains(&neg_normalized)
                    }
                })
            }
            // URL path matching (e.g., github.com/reactjs, geeksforgeeks.org/reactjs/)
            || text_lower.split('/').any(|segment| {
                let seg = segment.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                seg == neg_lower
                || {
                    let no_www = seg.strip_prefix("www.").unwrap_or(&seg);
                    let domain = no_www.split('.').next().unwrap_or(no_www);
                    domain == neg_lower
                }
                // Compound-word prefix in URL segments: "reactjs" starts with "react"
                || {
                    let seg_alpha: String = seg.chars().filter(|c| c.is_alphanumeric()).collect();
                    seg_alpha.len() > neg_normalized.len()
                        && neg_normalized.len() >= 3
                        && seg_alpha.starts_with(&neg_normalized)
                        && neg_normalized.len() as f32 / seg_alpha.len() as f32 >= 0.6
                }
            })
        } else {
            // Multi-word: check title first, then content
            title_lower.contains(&neg_lower) || title_normalized.contains(&neg_normalized)
            || text_lower.contains(&neg_lower) || text_normalized.contains(&neg_normalized)
        };
        if matched {
            // Penalty scales gradually with constraint count to avoid compounding:
            // 1 constraint: 0.02 (98% penalty — severe for single exclusions)
            // 2 constraints: 0.10 per violation (0.10^2 = 0.01 if both hit)
            // 3 constraints: 0.16 per violation (0.16^3 = 0.004 if all hit)
            // 4+ constraints: 0.20 per violation (0.20^4 = 0.002 if all hit)
            // This prevents score collapse while still heavily penalizing violations.
            let penalty = (0.02 + (neg_count - 1.0) * 0.06).clamp(0.02, 0.20);
            tracing::info!("CONSTRAINT HIT: '{}' in title='{}' → penalty={}", neg, &title[..title.char_indices().nth(50).map(|(i,_)| i).unwrap_or(title.len())], penalty);
            score *= penalty;
        } else {
            tracing::info!("CONSTRAINT MISS: '{}' not in '{}'", neg, &text_lower[..text_lower.char_indices().nth(60).map(|(i,_)| i).unwrap_or(text_lower.len())]);
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
        let coverage = matched as f32 / constraints.positive.len() as f32;
        // Scale: 0% coverage = 0.3x (penalty), 100% coverage = 1.8x (strong boost)
        // This ensures results matching user-specified requirements rank prominently
        score *= 0.3 + coverage * 1.5;
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

    score.clamp(0.0, 2.0)
}

// ─── VPN Signal Writer ──────────────────────────────────────────────
// Writes a signal file to trigger VPN rotation when rate limiting is detected.
// The vpn-rotator container watches this file.

fn trigger_vpn_rotation(reason: &str) {
    tracing::info!("VPN rotation triggered: {}", reason);
    // Write signal to shared bind-mount directory (legacy, for vpn-rotator script)
    let signal_dir = "/tmp/vpn-signals";
    let signal_path = format!("{}/rotate_signal", signal_dir);
    let _ = std::fs::create_dir_all(signal_dir);
    let _ = std::fs::write(&signal_path, reason);
    // Direct VPN control via Gluetun API (gateway shares gluetun's network namespace)
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::new();
        // Stop VPN
        let _ = client.put("http://127.0.0.1:8000/v1/vpn/status")
            .json(&serde_json::json!({"status": "stopped"}))
            .timeout(std::time::Duration::from_secs(10))
            .send();
        std::thread::sleep(std::time::Duration::from_secs(5));
        // Start VPN
        let _ = client.put("http://127.0.0.1:8000/v1/vpn/status")
            .json(&serde_json::json!({"status": "running"}))
            .timeout(std::time::Duration::from_secs(10))
            .send();
        std::thread::sleep(std::time::Duration::from_secs(15));
        // Verify new IP
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
    let q_lower = query.to_lowercase();
    let t_lower = title.to_lowercase();
    let c_lower = content.to_lowercase();

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
            .filter(|w| w.len() > 2 && !stop_words.contains(w.as_str()))
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
    // to avoid false positives. Longer queries can tolerate partial matches.
    let min_coverage = match query_terms.len() {
        1 => 1.0,   // single term must match exactly
        2 => 0.45,  // at least 1 of 2
        3 => 0.30,  // at least 1 of 3
        _ => 0.20,  // 4+ terms: lenient
    };

    if coverage < min_coverage {
        if coverage < 0.10 {
            return 0.01; // essentially irrelevant
        }
        return (combined * 0.3).clamp(0.0, 0.15);
    }

    combined.clamp(0.0, 1.0)
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
        let labels = ["informational", "technical", "navigational", "comparison", "how-to", "fresh", "transactional"];

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
        if margin > 0.3 {
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

fn compute_final_score(
    rank_score: f32,
    intent_boost: f32,
    freshness: f32,
    authority: f32,
    is_local: bool,
    quality: f32,
    semantic: f32,
    consensus: f32,
    constraint: f32,
    weights: &RankingWeights,
) -> f32 {
    let local = if is_local { 1.0 } else { 0.0 };

    let base_score = (weights.rrf * rank_score)
        + (weights.intent * intent_boost)
        + (weights.freshness * freshness)
        + (weights.authority * authority)
        + (weights.local_bonus * local)
        + (weights.quality * quality)
        + (weights.semantic * semantic)
        + (weights.consensus * consensus);

    // Constraint score acts as a GLOBAL multiplier:
    // - 1.0 = no constraints or all satisfied
    // - <1.0 = negative constraint violated (severe penalty)
    // - 0.5-1.3 = positive constraint coverage
    // This ensures negative constraints actually demote violating results
    // instead of only affecting the small constraint weight component.
    base_score * constraint
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

fn normalize_scores(scores: &mut [f32]) {
    // Find the raw max to detect if any scores exceed 1.0
    let raw_max = scores.iter().cloned().fold(0.0f32, f32::max);
    let cap = 1.0f32;

    if raw_max <= cap {
        // No overflow — just clamp the floor
        for score in scores.iter_mut() {
            *score = score.clamp(0.05, cap);
        }
        return;
    }

    // Over-1.0 cluster exists. Compress the tail into [cap - spread, cap]
    // using log-scaling so the #1 result is still distinguishable from #2.
    // spread = 0.05 means the #1 result gets 1.000 and the worst over-1.0
    // result gets ~0.950 — visible but not dominant.
    let spread = 0.05f32;
    let log_max = (1.0 + (raw_max - cap)).ln(); // ln(1 + excess)

    for score in scores.iter_mut() {
        if *score > cap {
            let excess = *score - cap;
            let compressed = spread * (1.0 + excess).ln() / log_max;
            *score = cap - spread + compressed; // maps [cap, raw_max] → [cap - spread, cap]
        } else {
            *score = score.clamp(0.05, cap);
        }
    }
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
    let q = query.trim();
    let q_lower = q.to_lowercase();
    
    // Prefixes that trigger dictionary/definition results on Bing
    let prefix_triggers = [
        "comparing ", "compare ", "compared ", "comparison of ",
        "explanation of ", "definition of ",
        "implications of ", "analysis of ", "overview of ",
        "understanding ", "introduction to ",
    ];
    let mut cleaned = q_lower.to_string();
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
    
    // Strip noise adjectives that search engines misinterpret as nouns.
    // "fast" → speed tests (fast.com), "modern" → dictionary definitions,
    // "quick" → speed tests, "simple" → dictionary, etc.
    // These words have strong standalone noun meanings that override their
    // intended use as descriptors. They're safe to strip because:
    //   1. They're filtered as stop words in semantic_relevance_score() already
    //   2. They don't change the topical intent ("fast web framework python"
    //      and "web framework python" return the same relevant results)
    //   3. Positive constraint checking still verifies them in results
    // Algorithmic: derived from the semantic scoring stop_words set plus
    // adjectives with strong alternate noun senses (modern=era, quick=speed).
    let noise_adjectives: std::collections::HashSet<&str> = [
        "fast", "quick", "slow", "modern", "simple", "easy", "hard",
        "best", "top", "good", "bad", "new", "old", "big", "small",
        "great", "awesome", "cool", "popular", "powerful", "amazing",
    ].iter().copied().collect();
    
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.len() > 2 {
        // Only strip noise adjectives if the query has enough remaining content
        let filtered: Vec<&str> = words.iter()
            .filter(|w| !noise_adjectives.contains(*w))
            .copied()
            .collect();
        if filtered.len() >= 2 {
            cleaned = filtered.join(" ");
        }
        // If stripping would leave < 2 words, keep original (query is too short)
    }
    
    // If cleaned is empty or too short, fall back to original
    if cleaned.len() < 3 {
        return q.to_string();
    }
    
    // Preserve dotted names like "node.js", "deno.js", "c++" — don't strip dots
    // that are part of technology names
    cleaned
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
        let mut engines = self.engines.lock().unwrap();
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

        // Exponential backoff: 15s, 30s, 60s, ... capped at 5 min
        if health.consecutive_failures >= 2 {
            let backoff_secs = 15u64 * 2u64.pow(health.consecutive_failures.saturating_sub(2));
            let backoff = Duration::from_secs(backoff_secs.min(300));
            health.open_until = Some(Instant::now() + backoff);
            tracing::warn!(
                "Circuit OPEN for engine '{}' — {} failures, backing off {:?}",
                engine, health.consecutive_failures, backoff
            );
        }
    }

    // Record how many results an engine returned (for weight calculation)
    fn record_results(&self, engine: &str, count: u64) {
        let mut engines = self.engines.lock().unwrap();
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
        let engines = self.engines.lock().unwrap();
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

    // Navigational domain boost: if intent is navigational and the query
    // looks like a platform name (1-2 tokens), boost results whose host
    // matches the query. This fixes "github" → github.com subpages being
    // ranked below irrelevant content.
    let nav_query_domain: Option<String> = if intent == "navigational" {
        let q_words: Vec<&str> = query.split_whitespace().collect();
        if q_words.len() <= 2 {
            // Check if query looks like a domain name (no spaces, alphanumeric)
            let joined = q_words.join("").to_lowercase();
            if joined.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.') {
                Some(joined)
            } else { None }
        } else { None }
    } else { None };

    for r in merged.iter_mut() {
        let semantic = semantic_relevance_score(query, &r.title, &r.content);
        let intent_boost = calculate_intent_boost(&r.url, &r.title, query, intent);
        let freshness = freshness_score(&r.url, intent);
        let quality = content_quality_score(&r.content);
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

        let local_bonus = if r.is_local { 1.0 } else { 0.0 };
        let base = (weights.semantic * semantic)
            + (weights.intent * intent_boost)
            + (weights.freshness * freshness)
            + (weights.authority * r.authority)
            + (weights.quality * quality)
            + (weights.consensus * consensus)
            + (weights.local_bonus * local_bonus)
            + nav_domain_boost;
        r.score = base * c_score;
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

    // 5. Normalize scores to [0, 1]
    let mut scores: Vec<f32> = merged.iter().map(|r| r.score).collect();
    normalize_scores(&mut scores);
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
        let mut events = self.events.lock().unwrap();
        let now = Instant::now();
        // Prune events older than 5 minutes
        events.retain(|e| now.duration_since(*e) < Duration::from_secs(300));
        events.push(now);
    }

    fn count_in_window(&self, window_secs: u64) -> usize {
        let events = self.events.lock().unwrap();
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
        let mut engines = self.engines.lock().unwrap();
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
            let mut events = self.degradation_events.lock().unwrap();
            let now = Instant::now();
            events.retain(|e| now.duration_since(*e) < Duration::from_secs(300));
            events.push(now);
        }

        is_degraded
    }

    // Count degradation events in the last N seconds
    fn degradation_count(&self, window_secs: u64) -> usize {
        let events = self.degradation_events.lock().unwrap();
        let now = Instant::now();
        events
            .iter()
            .filter(|e| now.duration_since(**e) < Duration::from_secs(window_secs))
            .count()
    }

    // Get expected result count for an engine (rolling average)
    fn expected_count(&self, engine: &str) -> f64 {
        let engines = self.engines.lock().unwrap();
        engines
            .get(engine)
            .filter(|v| v.rolling_count >= 2.0)
            .map(|v| v.rolling_sum / v.rolling_count)
            .unwrap_or(10.0) // default expectation
    }
}

struct AppState {
    circuit: CircuitBreaker,
    cache: SearchCache,
    rate_limits: RateLimitTracker,
    volume_tracker: ResultVolumeTracker,
    http_client: reqwest::Client,
    searxng2_url: Option<String>,
}

async fn handle_images(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    let cache_key = format!("images:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Fan-out to both SearXNG instances in parallel (VPN + Tor)
    let searx_url = format!(
        "http://127.0.0.1:8080/search?q={}&format=json&categories=images&pageno=1",
        q_encoded
    );

    let searx2_url = state.searxng2_url.as_ref().map(|base| {
        format!("{}/search?q={}&format=json&categories=images&pageno=1", base, q_encoded)
    });

    let parse_images = |raw: String| -> Vec<ImageResult> {
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxImageResponse>(&sanitized) {
            Ok(data) => data.results.into_iter().map(|r| {
                let thumb = if !r.thumbnail.is_empty() { r.thumbnail.clone() }
                    else if !r.thumbnail_src.is_empty() { r.thumbnail_src.clone() }
                    else { r.source.clone() };
                ImageResult {
                    title: r.title,
                    url: r.url,
                    image_url: if r.img_src.is_empty() { thumb.clone() } else { r.img_src },
                    thumbnail_url: thumb,
                    description: r.content,
                    source: r.engine,
                }
            }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG image parse error: {}", e);
                vec![]
            }
        }
    };

    let searx1_fut = async {
        match tokio::time::timeout(Duration::from_secs(6), state.http_client.get(&searx_url).send()).await {
            Ok(Ok(resp)) => match resp.text().await {
                Ok(raw) => parse_images(raw),
                Err(e) => { tracing::warn!("SearXNG1 image body read error: {}", e); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG1 image request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG1 image timed out after 6s"); vec![] }
        }
    };

    let searx2_fut = async {
        let url = match searx2_url {
            Some(u) => u,
            None => return vec![],
        };
        match tokio::time::timeout(Duration::from_secs(8), state.http_client.get(&url).send()).await {
            Ok(Ok(resp)) => match resp.text().await {
                Ok(raw) => parse_images(raw),
                Err(e) => { tracing::warn!("SearXNG2 image body read error: {}", e); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG2 image request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG2 image timed out after 8s"); vec![] }
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

    Json(response)
}

async fn handle_videos(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    let cache_key = format!("videos:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Query both Invidious and SearXNG (categories=videos) in parallel
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);
    let searx_video_url = format!(
        "http://127.0.0.1:8080/search?q={}&format=json&categories=videos&pageno=1",
        q_encoded
    );

    let (invidious_fut, searx_fut) = tokio::join!(
        async {
            match tokio::time::timeout(Duration::from_secs(3), state.http_client.get(&invidious_url).send()).await {
                Ok(Ok(resp)) => match resp.json::<Vec<InvidiousResult>>().await {
                    Ok(data) => data.into_iter()
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
                            })
                        })
                        .collect::<Vec<_>>(),
                    Err(e) => { tracing::warn!("Invidious parse error: {}", e); vec![] }
                },
                Ok(Err(e)) => { tracing::warn!("Invidious request error: {}", e); vec![] }
                Err(_) => { tracing::warn!("Invidious timed out after 3s"); vec![] }
            }
        },
        async {
            match tokio::time::timeout(Duration::from_secs(4), state.http_client.get(&searx_video_url).send()).await {
                Ok(Ok(resp)) => match resp.text().await {
                    Ok(raw) => {
                        let sanitized = sanitize_json_text(&raw);
                        match serde_json::from_str::<SearxVideoResponse>(&sanitized) {
                            Ok(data) => data.results.into_iter().map(|r| {
                                let thumbnail = if !r.thumbnail.is_empty() { r.thumbnail.clone() }
                                    else if !r.img_src.is_empty() { r.img_src.clone() }
                                    else { String::new() };
                                VideoResult {
                                    title: r.title,
                                    url: r.url,
                                    description: r.content,
                                    video_id: String::new(),
                                    thumbnail,
                                    source: r.engine,
                                }
                            }).collect::<Vec<_>>(),
                            Err(e) => { tracing::warn!("SearXNG video parse error: {}", e); vec![] }
                        }
                    }
                    Err(e) => { tracing::warn!("SearXNG video body read error: {}", e); vec![] }
                },
                Ok(Err(e)) => { tracing::warn!("SearXNG video request error: {}", e); vec![] }
                Err(_) => { tracing::warn!("SearXNG video timed out after 4s"); vec![] }
            }
        }
    );

    // Merge results: SearXNG first (more reliable), then Invidious
    let mut results = searx_fut;
    results.extend(invidious_fut);

    // Deduplicate by URL
    let mut seen = std::collections::HashSet::new();
    results.retain(|r| seen.insert(r.url.clone()));

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    Json(response)
}

async fn handle_news(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    let cache_key = format!("news:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Fan-out to both SearXNG instances in parallel (VPN + Tor)
    let searx_url = format!(
        "http://127.0.0.1:8080/search?q={}&format=json&categories=news&pageno=1",
        q_encoded
    );

    let searx2_url = state.searxng2_url.as_ref().map(|base| {
        format!("{}/search?q={}&format=json&categories=news&pageno=1", base, q_encoded)
    });

    let parse_news = |raw: String| -> Vec<NewsResult> {
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxNewsResponse>(&sanitized) {
            Ok(data) => data.results.into_iter().map(|r| NewsResult {
                title: r.title,
                url: r.url,
                description: r.content,
                published_at: r.published_date.unwrap_or_default(),
                source: r.engine,
            }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG news parse error: {}", e);
                vec![]
            }
        }
    };

    let searx1_fut = async {
        match tokio::time::timeout(Duration::from_secs(6), state.http_client.get(&searx_url).send()).await {
            Ok(Ok(resp)) => match resp.text().await {
                Ok(raw) => parse_news(raw),
                Err(e) => { tracing::warn!("SearXNG1 news body read error: {}", e); vec![] }
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
        match tokio::time::timeout(Duration::from_secs(8), state.http_client.get(&url).send()).await {
            Ok(Ok(resp)) => match resp.text().await {
                Ok(raw) => parse_news(raw),
                Err(e) => { tracing::warn!("SearXNG2 news body read error: {}", e); vec![] }
            },
            Ok(Err(e)) => { tracing::warn!("SearXNG2 news request error: {}", e); vec![] }
            Err(_) => { tracing::warn!("SearXNG2 news timed out after 8s"); vec![] }
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

    Json(response)
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
        rate_limits: RateLimitTracker::new(),
        volume_tracker: ResultVolumeTracker::new(),
        http_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(3))  // Engines that can't respond in 3s are degraded
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .pool_max_idle_per_host(20)
            .connect_timeout(Duration::from_secs(1))
            .build()
            .unwrap(),
        searxng2_url,
    });

    // Prewarm: poll intent engine until ready (handles container restart ordering).
    // Gateway starts fast; intent engine takes 5-15s to load BERT + compute centroids.
    let prewarm_client = state.http_client.clone();
    let prewarm_searxng2 = state.searxng2_url.clone();
    tokio::spawn(async move {
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
        tracing::info!("Prewarm complete");
    });

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search))
        .route("/search/fast", get(handle_search_fast))
        .route("/images", get(handle_images))
        .route("/videos", get(handle_videos))
        .route("/news", get(handle_news))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {} (circuit-breaker + cache)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_search(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    // 0. Validate query — reject empty or whitespace-only queries
    let q_trimmed = params.q.trim();
    if q_trimmed.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Missing or empty query parameter 'q'",
                "results": [],
                "count": 0,
            })),
        );
    }
    // Reject queries that are only special characters (no alphanumeric content)
    let alpha_count = q_trimmed.chars().filter(|c| c.is_alphanumeric()).count();
    if alpha_count == 0 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Query must contain at least one alphanumeric character",
                "results": [],
                "count": 0,
            })),
        );
    }

    // 0b. Check cache first (5-min TTL)
    let cache_key = format!("{}:{}", params.q.to_lowercase().trim(), "all");
    if let Some(cached) = state.cache.get(&cache_key) {
        tracing::info!("Cache hit for query: {}", params.q);
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return (axum::http::StatusCode::OK, Json(value));
    }
    // Use shared HTTP client from AppState (connection pooling across requests)
    let client = state.http_client.clone();

    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // 1. Run Intent Analysis (with retry) and Embedding in parallel
    let intent_url = format!("http://127.0.0.1:3005/analyze?q={}", q_encoded);
    let embed_url = format!("http://127.0.0.1:3005/embed?text={}", q_encoded);

    // Retry intent engine up to 2 extra times with backoff.
    // Handles cold-start after container restart (model load takes 5-15s).
    let intent_fut = async {
        let delays = [0u64, 500, 1000]; // 0ms, 500ms, 1000ms
        for (attempt, delay_ms) in delays.iter().enumerate() {
            if *delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
            }
            match client.get(&intent_url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.json::<IntentResponse>().await {
                        Ok(parsed) => return Ok(parsed),
                        Err(e) => {
                            tracing::warn!("Intent parse failed (attempt {}, status: {}): {:?}",
                                attempt + 1, status, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Intent Engine request failed (attempt {}): {:?}", attempt + 1, e);
                }
            }
        }
        Err::<IntentResponse, ()>(())
    };
    let embed_fut = client.get(&embed_url).send();

    // ─── Build engine URLs with raw query (no intent dependency) ────
    // Engines fire immediately in parallel with intent analysis.
    // Intent results are used post-hoc for scoring, not for query construction.
    let searx_base_urls: Vec<&str> = if state.searxng2_url.is_some() {
        vec!["http://127.0.0.1:8080", state.searxng2_url.as_deref().unwrap()]
    } else {
        vec!["http://127.0.0.1:8080"]
    };

    // Build SearXNG URLs: one per instance, raw query (no expanded queries yet)
    let searx_urls: Vec<String> = searx_base_urls.iter().enumerate().map(|(i, base_url)| {
        let clean_q = preprocess_searxng_query(&q);
        format!("{}/search?q={}&format=json&pageno=1", base_url, urlencoding::encode(&clean_q))
    }).collect();

    let whoogle_url = format!("http://127.0.0.1:5000/search?q={}&format=json", q_encoded);
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let indexer_query_raw = format!("http://127.0.0.1:6000/search?q={}", q_encoded);

    let client_ref = &client;
    let circuit_ref = &state.circuit;
    let ratelimit_ref = &state.rate_limits;

    // Check circuit breaker before calling each engine
    let searx_instance_keys: Vec<String> = searx_base_urls.iter().enumerate().map(|(i, _)| {
        format!("searxng{}", i)
    }).collect();
    let searx_instance_open: Vec<bool> = searx_instance_keys.iter()
        .map(|k| circuit_ref.is_open(k))
        .collect();
    let all_searx_open = searx_instance_open.iter().all(|&o| o);
    let whoogle_open = circuit_ref.is_open("whoogle");
    let invidious_open = circuit_ref.is_open("invidious");

    let indexer_fut = async {
        let resp = client_ref.get(&indexer_query_raw).send().await?;
        let status = resp.status();
        resp.json::<Vec<IndexerResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Indexer JSON (status: {}): {:?}", status, e);
            e
        })
    };

    // Fire all SearXNG instances in parallel with retry on 0 results
    let searx_instance_keys_ref = &searx_instance_keys;
    let searx_futs: Vec<_> = searx_urls.iter().enumerate().map(|(i, url)| {
        let url = url.clone();
        let instance_key = searx_instance_keys_ref[i].clone();
        let is_open = searx_instance_open[i];
        async move {
            if is_open {
                return Ok(SearxResponse { results: vec![] });
            }
            // Per-instance timeout: 2s for the request itself, so slow backends
            // (e.g. tor2) don't block the entire fan-out waiting for the outer 3s.
            let first: Result<SearxResponse, reqwest::Error> = async {
                let resp = match tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    client_ref.get(&url).send()
                ).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        tracing::warn!("SearXNG instance request timed out (2s): {}", &url[..url.find('?').unwrap_or(url.len())]);
                        return Ok(SearxResponse { results: vec![] });
                    }
                };
                let status = resp.status();
                let raw = match tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    resp.text()
                ).await {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        tracing::warn!("SearXNG instance body read timed out (2s)");
                        return Ok(SearxResponse { results: vec![] });
                    }
                };
                let sanitized = sanitize_json_text(&raw);
                match serde_json::from_str::<SearxResponse>(&sanitized) {
                    Ok(data) => Ok(data),
                    Err(e) => {
                        tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
                        Ok(SearxResponse { results: vec![] })
                    }
                }
            }.await;

            match first {
                Ok(data) if !data.results.is_empty() => Ok(data),
                Ok(_) => {
                    let url_lower = url.to_lowercase();
                    let q_part = url_lower.split("q=").nth(1).unwrap_or("");
                    let q_decoded = q_part.split("&").next().unwrap_or("");
                    let alpha_ratio: f32 = if q_decoded.len() > 0 {
                        q_decoded.chars().filter(|c| c.is_alphabetic() || c.is_whitespace()).count() as f32 / q_decoded.len() as f32
                    } else { 0.0 };
                    let is_malformed = q_decoded.len() > 200 || alpha_ratio < 0.3;
                    if is_malformed {
                        return Ok(SearxResponse { results: vec![] });
                    }
                    tracing::warn!("SearXNG returned 0 results, retrying in 500ms...");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let retry: Result<SearxResponse, reqwest::Error> = async {
                        let resp = client_ref.get(&url).send().await?;
                        let status = resp.status();
                        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                            let rl_count = ratelimit_ref.count_in_window(300);
                            ratelimit_ref.record();
                            let new_count = ratelimit_ref.count_in_window(300);
                            tracing::warn!("SearXNG got 429 — rate-limits in 5min: {} → {}", rl_count, new_count);
                            trigger_vpn_rotation(&format!("429_rate_limit_{}", new_count));
                        }
                        let raw = resp.text().await.unwrap_or_default();
                        let sanitized = sanitize_json_text(&raw);
                        match serde_json::from_str::<SearxResponse>(&sanitized) {
                            Ok(data) => Ok(data),
                            Err(e) => {
                                tracing::error!("SearXNG retry parse failed (status: {}): {:?}", status, e);
                                Ok(SearxResponse { results: vec![] })
                            }
                        }
                    }.await;
                    if let Ok(ref data) = retry {
                        if data.results.is_empty() {
                            tracing::warn!("SearXNG retry returned 0 results — triggering VPN rotation");
                            trigger_vpn_rotation("zero_results_after_retry");
                        }
                    }
                    retry
                }
                Err(e) => {
                    tracing::warn!("SearXNG request failed (local error, no VPN rotation): {:?}", e);
                    Err(e)
                }
            }
        }
    }).collect();

    let whoogle_fut = async {
        if whoogle_open {
            return Ok::<WhoogleResponse, anyhow::Error>(WhoogleResponse { results: vec![] });
        }
        match tokio::time::timeout(Duration::from_secs(2), client_ref.get(&whoogle_url).send()).await {
            Ok(Ok(resp)) => {
                let status = resp.status();
                let raw_text = resp.text().await.unwrap_or_default();
                match serde_json::from_str::<serde_json::Value>(&raw_text) {
                    Ok(val) => {
                        let results: Vec<WhoogleResult> = val
                            .get("results")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter().filter_map(|item| {
                                    let url = item.get("href").or_else(|| item.get("link"))
                                        .and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let title = item.get("title").or_else(|| item.get("text"))
                                        .and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let description = item.get("content").or_else(|| item.get("desc"))
                                        .or_else(|| item.get("snippet"))
                                        .and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    if url.is_empty() { return None; }
                                    Some(WhoogleResult { url, title, description: Some(description) })
                                }).collect()
                            })
                            .unwrap_or_default();
                        Ok(WhoogleResponse { results })
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse Whoogle JSON (status: {}): {:?}", status, e);
                        Err(e.into())
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Whoogle request failed: {}", e);
                Err(e.into())
            }
            Err(_) => {
                tracing::warn!("Whoogle timed out after 2s");
                Ok(WhoogleResponse { results: vec![] })
            }
        }
    };

    let invidious_fut = async {
        if invidious_open {
            return Ok(vec![]);
        }
        match tokio::time::timeout(Duration::from_secs(2), client_ref.get(&invidious_url).send()).await {
            Ok(Ok(resp)) => {
                let status = resp.status();
                resp.json::<Vec<InvidiousResult>>().await.map_err(|e| {
                    tracing::error!("Failed to parse Invidious JSON (status: {}): {:?}", status, e);
                    e
                })
            }
            Ok(Err(e)) => {
                tracing::warn!("Invidious request failed: {}", e);
                Err(e.into())
            }
            Err(_) => {
                tracing::warn!("Invidious timed out after 2s");
                Ok(vec![])
            }
        }
    };

    // Conditional media fan-out based on raw query signals (no intent dependency)
    let q_lower = q.to_lowercase();
    let is_news_intent = q_lower.contains("news") || q_lower.contains("latest");
    let is_image_intent = q_lower.contains("image") || q_lower.contains("photo")
        || q_lower.contains("picture");

    let news_fut = async {
        if !is_news_intent || all_searx_open {
            return Ok(SearxNewsResponse { results: vec![] }) as Result<SearxNewsResponse, anyhow::Error>;
        }
        let news_url = format!(
            "http://127.0.0.1:8080/search?q={}&format=json&categories=news&pageno=1",
            q_encoded
        );
        let resp = client_ref.get(&news_url).send().await?;
        let raw = resp.text().await.unwrap_or_default();
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxNewsResponse>(&sanitized) {
            Ok(data) => Ok(data),
            Err(e) => {
                tracing::warn!("SearXNG news fan-out parse error: {}", e);
                Ok(SearxNewsResponse { results: vec![] })
            }
        }
    };

    let image_fut = async {
        if !is_image_intent || all_searx_open {
            return Ok(SearxImageResponse { results: vec![] }) as Result<SearxImageResponse, anyhow::Error>;
        }
        let image_url = format!(
            "http://127.0.0.1:8080/search?q={}&format=json&categories=images&pageno=1",
            q_encoded
        );
        let resp = client_ref.get(&image_url).send().await?;
        let raw = resp.text().await.unwrap_or_default();
        let sanitized = sanitize_json_text(&raw);
        match serde_json::from_str::<SearxImageResponse>(&sanitized) {
            Ok(data) => Ok(data),
            Err(e) => {
                tracing::warn!("SearXNG image fan-out parse error: {}", e);
                Ok(SearxImageResponse { results: vec![] })
            }
        }
    };

    let searx_fut_with_timeout = async {
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),  // Per-instance timeout is 2s; cut the join at 2s
            futures::future::join_all(searx_futs),
        ).await {
            Ok(results) => results,
            Err(_) => {
                tracing::warn!("SearXNG fan-out timed out after 2s — returning partial results");
                vec![]
            }
        }
    };

    // ─── SINGLE PARALLEL JOIN: intent + engines fire simultaneously ───
    // This eliminates the sequential intent→engines pipeline.
    // Engines start fetching immediately; intent runs in parallel.
    // Latency = max(intent, engines) instead of intent + engines.
    let (intent_result, embed_res, indexer_res, searx_results, whoogle_res, invidious_res, news_res, image_res) = tokio::join!(
        intent_fut,
        embed_fut,
        indexer_fut,
        searx_fut_with_timeout,
        whoogle_fut,
        invidious_fut,
        news_fut,
        image_fut,
    );

    // 2. Process Intent & Embedding (now available alongside engine results)
    let intent: IntentResponse = match intent_result {
        Ok(parsed) => parsed,
        Err(()) => {
            tracing::error!("Intent Engine unreachable after 3 attempts — using fallback");
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

    // 3. Intent-based post-processing: freshness, expanded queries, secondary fan-out
    let freshness_keywords = ["latest", "recent", "week", "month", "today", "newest", "cve", "vulnerability"];
    let is_freshness_query = intent.constraints.iter().any(|c| {
        let c_low = c.to_lowercase();
        freshness_keywords.iter().any(|&k| c_low.contains(k))
    }) || q.to_lowercase().contains("latest") || q.to_lowercase().contains("recent")
      || intent.intent == "fresh";

    // Secondary fan-out with expanded queries if initial results are sparse
    let expanded_queries = if intent.expanded_queries.len() > 1 {
        intent.expanded_queries.clone()
    } else {
        vec![q.clone()]
    };
    let neg_terms: Vec<String> = intent.structured_constraints.negative.iter()
        .map(|n| n.to_lowercase())
        .collect();
    let cleaned_queries: Vec<String> = expanded_queries.iter().map(|eq| {
        if neg_terms.is_empty() { return eq.clone(); }
        let mut words: Vec<&str> = eq.split_whitespace().collect();
        let neg_triggers = ["not", "except", "without", "excluding", "other", "than"];
        words.retain(|w| {
            let w_lower = w.to_lowercase();
            let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower).to_string();
            !neg_terms.contains(&w_stripped) && !neg_terms.contains(&w_lower) && !neg_triggers.contains(&w_stripped.as_str()) && !neg_triggers.contains(&w_lower.as_str())
        });
        words.join(" ")
    }).filter(|q| !q.trim().is_empty()).collect();
    let expanded_queries = if !cleaned_queries.is_empty() { cleaned_queries } else { expanded_queries };

    // TODO: Secondary fan-out with expanded queries if searx_results are sparse
    // For now, scoring uses intent-based weighting on the raw query results

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
    // Track per-URL RRF contributions from each source's ranked position
    // This gives a proper rank-fusion score instead of meaningless insertion order
    let mut url_rrf_contributions: HashMap<String, f32> = HashMap::new();

    // Aggregate SearXNG results from all query variations
    // Track per-engine result counts for degradation detection
    let mut engine_counts: HashMap<String, u64> = HashMap::new();
    for (i, searx_res) in searx_results.into_iter().enumerate() {
        let instance_key = &searx_instance_keys[i];
        match searx_res {
            Ok(searx_data) => {
                tracing::info!("SearXNG variation {} returned {} results", i, searx_data.results.len());
                circuit_ref.record_success(instance_key);
                circuit_ref.record_results(instance_key, searx_data.results.len() as u64);
                // Track per-engine result counts for degradation detection
                for r in &searx_data.results {
                    *engine_counts.entry(r.engine.clone()).or_insert(0) += 1;
                }
                // Track position-based RRF contribution per URL within this variation
                // Weight by engine reliability (dynamic learning)
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
                    web_results.push(result);
                }
            }
            Err(e) => {
                tracing::error!("SearXNG variation {} request failed/timed out: {:?}", i, e);
                circuit_ref.record_failure(instance_key);
            }
        }
    }

    // ─── Per-Engine Degradation Detection ──────────────────────────
    // Record per-engine result counts and detect silent degradation.
    // If an engine returns <50% of its rolling average, it's likely rate-limited.
    let mut degraded_engines = 0usize;
    for (engine, &count) in &engine_counts {
        let is_degraded = state.volume_tracker.record(engine, count);
        if is_degraded {
            degraded_engines += 1;
        }
    }

    // Cross-request degradation correlation: if 3+ degradation events in 5 min,
    // trigger proactive VPN rotation before the circuit breaker even opens.
    let recent_degradations = state.volume_tracker.degradation_count(300);
    if recent_degradations >= 3 {
        tracing::warn!(
            "PROACTIVE VPN ROTATION: {} engine degradations in 5-min window",
            recent_degradations
        );
        trigger_vpn_rotation(&format!("proactive_{}_degradations_5min", recent_degradations));
    }

    // Smart retry: if overall results are significantly below expected,
    // try a different expanded query variation on the BEST available instance.
    // The initial fan-out uses the raw query (== expanded_queries[0]) on all instances,
    // so we've used exactly 1 query variation — the retry should skip it.
    let total_results = web_results.len();
    let expected_min = if expanded_queries.len() > 1 { 15 } else { 10 };
    if total_results < expected_min && !expanded_queries.is_empty() && searx_base_urls.len() > 1 {
        // Pick the first expanded query variation we haven't tried yet (skip index 0 = raw query)
        let retry_query_idx = if expanded_queries.len() > 1 { 1 } else { 0 };
        if let Some(retry_eq) = expanded_queries.get(retry_query_idx) {
            // Prefer instance 0 (usually VPN/healthy) over instance 1 (often tor/degraded)
            let retry_instance_idx = 0usize;
            if let Some(retry_base) = searx_base_urls.get(retry_instance_idx) {
                let clean_eq = preprocess_searxng_query(retry_eq);
                let retry_url = format!("{}/search?q={}&format=json&pageno=1", retry_base, urlencoding::encode(&clean_eq));
                let retry_key = format!("searxng{}", retry_instance_idx);
                if !circuit_ref.is_open(&retry_key) {
                    tracing::info!(
                        "SMART RETRY: {} results < {} expected — trying variation '{}' on instance {}",
                        total_results, expected_min, clean_eq, retry_instance_idx
                    );
                    match tokio::time::timeout(
                        Duration::from_secs(2),
                        client_ref.get(&retry_url).send(),
                    ).await {
                        Ok(Ok(resp)) => {
                            let raw = resp.text().await.unwrap_or_default();
                            let sanitized = sanitize_json_text(&raw);
                            if let Ok(data) = serde_json::from_str::<SearxResponse>(&sanitized) {
                                let retry_count = data.results.len();
                                tracing::info!("Smart retry returned {} results", retry_count);
                                circuit_ref.record_success(&retry_key);
                                circuit_ref.record_results(&retry_key, retry_count as u64);
                                for (pos, result) in data.results.into_iter().enumerate() {
                                    let engine_weight = circuit_ref.weight(&result.engine);
                                    let normalized = {
                                        let lower = result.url.to_lowercase();
                                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                                        let no_trailing = no_fragment.trim_end_matches('/');
                                        let no_www = no_trailing.replacen("://www.", "://", 1);
                                        let no_mobile = no_www
                                            .replacen("://m.", "://", 1)
                                            .replacen("://mobile.", "://", 1);
                                        strip_tracking_params(&no_mobile)
                                    };
                                    if !url_rrf_contributions.contains_key(&normalized) {
                                        let rrf_contrib = engine_weight / (60.0 + (pos + 1) as f32);
                                        *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                                        web_results.push(result);
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("Smart retry request failed: {:?}", e);
                        }
                        Err(_) => {
                            tracing::warn!("Smart retry timed out");
                        }
                    }
                }
            }
        }
    }

    match whoogle_res {
        Ok(whoogle_data) => {
            tracing::info!("Whoogle returned {} results", whoogle_data.results.len());
            circuit_ref.record_success("whoogle");
            circuit_ref.record_results("whoogle", whoogle_data.results.len() as u64);
            let whoogle_weight = circuit_ref.weight("whoogle");
            for (pos, r) in whoogle_data.results.into_iter().enumerate() {
                let normalized = {
                    let lower = r.url.to_lowercase();
                    let no_fragment = lower.split('#').next().unwrap_or(&lower);
                    let no_trailing = no_fragment.trim_end_matches('/');
                    let no_www = no_trailing.replacen("://www.", "://", 1);
                    strip_tracking_params(&no_www)
                };
                let rrf_contrib = whoogle_weight / (60.0 + (pos + 1) as f32);
                *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                web_results.push(SearxResult {
                    title: r.title,
                    url: r.url,
                    content: r.description.unwrap_or_default(),
                    engine: "whoogle".to_string(),
                    score: 0.0,
                    sources: vec!["whoogle".to_string()],
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
            circuit_ref.record_results("invidious", invidious_data.len() as u64);
            let invidious_weight = circuit_ref.weight("invidious");
            for (pos, r) in invidious_data.into_iter().enumerate() {
                if r.result_type.as_deref() == Some("video") {
                    if let Some(vid) = r.video_id {
                        let desc = r.description.unwrap_or_default();
                        // Skip Invidious results with empty description — they provide
                        // no content for semantic scoring and degrade result quality.
                        // Video metadata alone (title + thumbnail) isn't useful in search.
                        if desc.trim().is_empty() {
                            tracing::debug!("Skipping Invidious result with empty content: {:?}", r.title);
                            continue;
                        }
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
                            title: r.title.unwrap_or_else(|| "No Title".to_string()),
                            url: video_url,
                            content: desc,
                            engine: "invidious".to_string(),
                            score: 0.0,
                            sources: vec!["invidious".to_string()],
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
    let is_garbage_cluster = best_score < 0.15 && mean_score < 0.10;

    tracing::info!(
        "Relevance distribution: best={:.3}, mean={:.3}, var={:.3}, confidence={:.3}, garbage_cluster={}",
        best_score, mean_score, score_variance, relevance_confidence, is_garbage_cluster
    );

    // If we detected a garbage cluster AND have expanded queries to try, retry
    if is_garbage_cluster && expanded_queries.len() > 1 && !all_searx_open {
        tracing::warn!(
            "GARBAGE CLUSTER DETECTED (best={:.3}, mean={:.3}) — retrying with expanded queries",
            best_score, mean_score
        );
        // Try each expanded query variation (skip index 0 = raw query already tried)
        for (eq_idx, eq) in expanded_queries.iter().enumerate().skip(1) {
            if eq_idx > 3 { break; } // limit retries
            let clean_eq = preprocess_searxng_query(eq);
            if clean_eq.to_lowercase() == q.to_lowercase() { continue; } // skip duplicates
            for (inst_idx, base_url) in searx_base_urls.iter().enumerate() {
                let retry_url = format!(
                    "{}/search?q={}&format=json&pageno=1",
                    base_url, urlencoding::encode(&clean_eq)
                );
                let retry_key = format!("searxng{}", inst_idx);
                if circuit_ref.is_open(&retry_key) { continue; }
                tracing::info!("Retry variation '{}' on instance {}", clean_eq, inst_idx);
                match tokio::time::timeout(Duration::from_secs(3), client_ref.get(&retry_url).send()).await {
                    Ok(Ok(resp)) => {
                        let raw = resp.text().await.unwrap_or_default();
                        let sanitized = sanitize_json_text(&raw);
                        if let Ok(data) = serde_json::from_str::<SearxResponse>(&sanitized) {
                            let retry_scores: Vec<f32> = data.results.iter()
                                .map(|r| semantic_relevance_score(&q, &r.title, &r.content))
                                .collect();
                            let retry_best = retry_scores.iter().cloned().fold(0.0f32, f32::max);
                            let retry_mean = if !retry_scores.is_empty() {
                                retry_scores.iter().sum::<f32>() / retry_scores.len() as f32
                            } else { 0.0 };
                            tracing::info!(
                                "Retry variation '{}' got {} results, best={:.3}, mean={:.3}",
                                clean_eq, data.results.len(), retry_best, retry_mean
                            );
                            // If this variation is significantly better, use it
                            if retry_best > best_score + 0.1 || retry_mean > mean_score + 0.1 {
                                tracing::info!(
                                    "Retry variation '{}' is BETTER — replacing results",
                                    clean_eq
                                );
                                circuit_ref.record_success(&retry_key);
                                circuit_ref.record_results(&retry_key, data.results.len() as u64);
                                for (pos, result) in data.results.into_iter().enumerate() {
                                    let engine_weight = circuit_ref.weight(&result.engine);
                                    let normalized = {
                                        let lower = result.url.to_lowercase();
                                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                                        let no_trailing = no_fragment.trim_end_matches('/');
                                        let no_www = no_trailing.replacen("://www.", "://", 1);
                                        let no_mobile = no_www
                                            .replacen("://m.", "://", 1)
                                            .replacen("://mobile.", "://", 1);
                                        strip_tracking_params(&no_mobile)
                                    };
                                    let rrf_contrib = engine_weight / (60.0 + (pos + 1) as f32);
                                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                                    web_results.push(result);
                                }
                                // Re-score after adding new results
                                break; // stop retrying this variation
                            }
                        }
                    }
                    _ => {} // timeout or error — skip
                }
            }
        }
    }

    // Adaptive threshold: higher when we have many results, lower when few
    // No positional exceptions — rank #1 can still be garbage
    let semantic_threshold = if web_results.len() > 30 { 0.18 }
        else if web_results.len() > 20 { 0.15 }
        else if web_results.len() > 10 { 0.12 }
        else { 0.08 };
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

    // Hard filter: remove web results that violate negative constraints
    if !intent.structured_constraints.negative.is_empty() {
        let before_count = web_results.len();
        web_results.retain(|r| {
            let c_score = constraint_score(&r.title, &r.content, &r.url, &intent.structured_constraints);
            c_score >= 0.15
        });
        let removed = before_count.saturating_sub(web_results.len());
        if removed > 0 {
            tracing::info!("Negative constraint hard filter: removed {}/{} web results", removed, before_count);
        }
        if web_results.is_empty() && before_count > 0 {
            tracing::warn!("Negative constraint filter removed all web results — relaxing threshold");
            // Re-add with relaxed threshold
            web_results = web_results.into_iter().filter(|r| {
                let c_score = constraint_score(&r.title, &r.content, &r.url, &intent.structured_constraints);
                c_score >= 0.01
            }).collect();
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
            tracing::debug!("Indexer result filtered (sem={:.3}): {}", sem_score, &r.title[..r.title.len().min(50)]);
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
    let mut results = merge_local_and_web(
        local_results,
        web_results,
        &q,
        &intent.intent,
        &intent.structured_constraints,
        Some(&intent.distribution),
    );

    // Sanitize content for safe JSON serialization
    for r in results.iter_mut() {
        r.title = sanitize_text_content(&r.title);
        r.content = sanitize_text_content(&r.content);
    }

    let response = UnifiedResponse {
        intent: intent.intent.clone(),
        category: parent_category(&intent.intent),
        confidence: intent.confidence,
        constraints: intent.constraints.clone(),
        structured_constraints: intent.structured_constraints.clone(),
        expanded_queries: intent.expanded_queries.clone(),
        distribution: intent.distribution.clone(),
        results,
    };

    // Cache for 5 minutes — but never cache empty results
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !response.results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    (axum::http::StatusCode::OK, Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({}))))
}

fn fallback_intent(q: &str) -> IntentResponse {
    IntentResponse {
        query: q.to_string(),
        intent: "informational".to_string(),
        confidence: 0.3,
        constraints: vec![],
        structured_constraints: Constraints::default(),
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
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // Check cache first
    let cache_key = format!("fast:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Query local index only — no network calls except to indexer
    let indexer_url = format!("http://127.0.0.1:6000/search?q={}", q_encoded);
    let results = match state.http_client.get(&indexer_url).send().await {
        Ok(resp) => {
            match resp.json::<Vec<IndexerResult>>().await {
                Ok(indexer_results) => {
                    // Convert to MergedResult format
                    indexer_results.into_iter().map(|r| MergedResult {
                        url: r.url,
                        title: r.title,
                        content: r.content,
                        score: r.score,
                        authority: r.authority,
                        sources: vec!["local".to_string()],
                        is_local: true,
                    }).collect::<Vec<_>>()
                }
                Err(_) => vec![]
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

    Json(response)
}
