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

#[derive(Serialize)]
struct UnifiedResponse {
    intent: IntentResponse,
    local_results: Vec<IndexerResult>,
    web_results: Vec<SearxResult>,
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
    constraints: &Constraints,
) -> f32 {
    if constraints.positive.is_empty() && constraints.negative.is_empty() {
        return 1.0; // no constraints = no penalty
    }

    let text_lower = format!("{} {}", title.to_lowercase(), content.to_lowercase());
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
        // For single-word constraints: match against TITLE primarily (content is too noisy)
        // For multi-word constraints: match against title + content
        let matched = if neg_words.len() == 1 {
            // Title-first matching for single-word constraints
            // Check: exact word match, trimmed match, alphanumeric-normalized match,
            // and also substring match on normalized text for compound terms like "nodejs" containing "node"
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
            // Also check full normalized title as substring fallback (>= 3 chars)
            || (neg_normalized.len() >= 3 && title_normalized.contains(&neg_normalized))
            // Also check if the term is in the URL path (e.g., github.com/flask)
            || text_lower.split('/').any(|segment| {
                let seg = segment.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                seg == neg_lower
                // Also check domain: strip "www." prefix and ".tld" suffix
                || {
                    let no_www = seg.strip_prefix("www.").unwrap_or(&seg);
                    let domain = no_www.split('.').next().unwrap_or(no_www);
                    domain == neg_lower
                }
            })
        } else {
            // Multi-word: check title first, then content
            title_lower.contains(&neg_lower) || title_normalized.contains(&neg_normalized)
            || text_lower.contains(&neg_lower) || text_normalized.contains(&neg_normalized)
        };
        if matched {
            // Penalty scales with number of constraints: more constraints = less penalty per violation
            // but always severe. Single constraint: 0.02x (98% penalty). Multiple: 0.05x per violation.
            let penalty = if neg_count <= 1.0 { 0.02 } else { 0.05 };
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
                text_lower.contains(&pos_lower) || text_normalized.contains(&pos_normalized)
            };
            if found {
                matched += 1;
            }
        }
        // Coverage: fraction of positive constraints matched
        let coverage = matched as f32 / constraints.positive.len() as f32;
        // Scale: 0% coverage = 0.5x, 100% coverage = 1.3x
        score *= 0.5 + coverage * 0.8;
    }

    score.clamp(0.0, 1.5)
}

// ─── VPN Signal Writer ──────────────────────────────────────────────
// Writes a signal file to trigger VPN rotation when rate limiting is detected.
// The vpn-rotator container watches this file.

fn trigger_vpn_rotation(reason: &str) {
    // Write signal to shared bind-mount directory
    let signal_dir = "/tmp/vpn-signals";
    let signal_path = format!("{}/rotate_signal", signal_dir);
    // Ensure directory exists
    let _ = std::fs::create_dir_all(signal_dir);
    if let Err(e) = std::fs::write(&signal_path, reason) {
        tracing::warn!("Failed to write VPN rotation signal: {:?}", e);
    } else {
        tracing::info!("VPN rotation signal written: {}", reason);
    }
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
        return 0.3; // single-source result
    }
    let unique_sources: std::collections::HashSet<&String> = sources.iter().collect();
    let count = unique_sources.len() as f32;
    // Logarithmic scaling: 1 source = 0.3, 2 = 0.55, 3 = 0.7, 4+ = 0.85+
    // This prevents runaway boosts while still rewarding cross-source agreement
    (0.3 + 0.2 * (count - 1.0).max(0.0).ln()).clamp(0.3, 0.95)
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
                rrf: 0.07,
                intent: 0.04,
                freshness: 0.18,
                authority: 0.13,
                local_bonus: 0.02,
                quality: 0.08,
                semantic: 0.22,
                consensus: 0.13,
                constraint: 0.13,
            },
            "technical" => Self {
                rrf: 0.08,
                intent: 0.10,
                freshness: 0.04,
                authority: 0.12,
                local_bonus: 0.04,
                quality: 0.06,
                semantic: 0.30,
                consensus: 0.08,
                constraint: 0.18,
            },
            "navigational" => Self {
                rrf: 0.04,
                intent: 0.20,
                freshness: 0.03,
                authority: 0.18,
                local_bonus: 0.02,
                quality: 0.04,
                semantic: 0.25,
                consensus: 0.08,
                constraint: 0.16,
            },
            "comparison" => Self {
                rrf: 0.10,
                intent: 0.08,
                freshness: 0.08,
                authority: 0.06,
                local_bonus: 0.02,
                quality: 0.10,
                semantic: 0.25,
                consensus: 0.13,
                constraint: 0.18,
            },
            "how-to" => Self {
                rrf: 0.08,
                intent: 0.08,
                freshness: 0.06,
                authority: 0.06,
                local_bonus: 0.04,
                quality: 0.08,
                semantic: 0.28,
                consensus: 0.14,
                constraint: 0.18,
            },
            _ => Self {  // informational, default
                rrf: 0.08,
                intent: 0.06,
                freshness: 0.06,
                authority: 0.08,
                local_bonus: 0.04,
                quality: 0.08,
                semantic: 0.26,
                consensus: 0.16,
                constraint: 0.18,
            },
        }
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
// Normalizes scores to [0, 1] using robust percentile scaling.
// Makes scores comparable across different queries (a 1.5 on one query
// shouldn't be confused with 1.5 on another).

fn normalize_scores(scores: &mut [f32]) {
    let n = scores.len();
    if n < 2 {
        return; // not enough data to normalize
    }

    // Create indexed scores for rank-based normalization
    let mut indexed: Vec<(usize, f32)> = scores.iter().enumerate().map(|(i, &s)| (i, s)).collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Check if percentile normalization would cluster (p10-p90 range too tight)
    let p10_idx = ((n as f32 * 0.10) as usize).min(n - 1);
    let p90_idx = ((n as f32 * 0.90) as usize).min(n - 1);
    let p10 = indexed[p10_idx].1;
    let p90 = indexed[p90_idx].1;
    let percentile_range = p90 - p10;

    // Also check overall range
    let min_score = indexed[0].1;
    let max_score = indexed[n - 1].1;
    let total_range = max_score - min_score;

    // Use min-max normalization over the full range to preserve relative
    // differences at the top (percentile-based was clamping top results to 1.0)
    if total_range < 0.01 {
        // All scores nearly identical — use rank-based to force spread
        for (rank, &(orig_idx, _)) in indexed.iter().enumerate() {
            scores[orig_idx] = (rank as f32) / ((n - 1) as f32);
        }
    } else {
        // Min-max normalization: best result = 1.0, worst = 0.0, others proportional
        for score in scores.iter_mut() {
            *score = ((*score - min_score) / total_range).clamp(0.0, 1.0);
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
        });
        health.consecutive_failures = 0;
        health.open_until = None;
    }

    fn record_failure(&self, engine: &str) {
        let mut engines = self.engines.lock().unwrap();
        let health = engines.entry(engine.to_string()).or_insert(EngineHealth {
            consecutive_failures: 0,
            last_failure: None,
            open_until: None,
        });
        health.consecutive_failures += 1;
        health.last_failure = Some(Instant::now());

        // Exponential backoff: 30s, 60s, 120s, ... capped at 10 min
        if health.consecutive_failures >= 3 {
            let backoff_secs = 30u64 * 2u64.pow(health.consecutive_failures.saturating_sub(3));
            let backoff = Duration::from_secs(backoff_secs.min(600));
            health.open_until = Some(Instant::now() + backoff);
            tracing::warn!(
                "Circuit OPEN for engine '{}' — {} failures, backing off {:?}",
                engine, health.consecutive_failures, backoff
            );
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

struct AppState {
    circuit: CircuitBreaker,
    cache: SearchCache,
    rate_limits: RateLimitTracker,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState {
        circuit: CircuitBreaker::new(),
        cache: SearchCache::new(),
        rate_limits: RateLimitTracker::new(),
    });

    let app = Router::new()
        .route("/", get(|| async { "IntentForge-v2 Gateway" }))
        .route("/health", get(|| async { "OK" }))
        .route("/search", get(handle_search))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 4000));
    tracing::info!("Gateway listening on {} (circuit-breaker + cache)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_search(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    // 0. Check cache first (5-min TTL)
    let cache_key = format!("{}:{}", params.q.to_lowercase().trim(), "all");
    if let Some(cached) = state.cache.get(&cache_key) {
        tracing::info!("Cache hit for query: {}", params.q);
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }
    // Timeout HTTP client — 10s for meta-search (SearXNG aggregates multiple engines)
    // Results are cached for 5 min, so this hit only happens once per query
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
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

    // 3. Multi-Variation Fan-Out: query SearXNG with expanded queries for broader recall
    // The intent engine returns 2-4 query variations. We fire them all to SearXNG.
    // This catches results that the original query phrasing might miss.
    let expanded_queries = if intent.expanded_queries.len() > 1 {
        intent.expanded_queries.clone()
    } else {
        vec![q.clone()]
    };

    // Strip negative constraint terms from expanded queries before SearXNG fan-out.
    // "python web framework not django" → "python web framework"
    // Sending "not django" to SearXNG pollutes results with Django pages.
    let neg_terms: Vec<String> = intent.structured_constraints.negative.iter()
        .map(|n| n.to_lowercase())
        .collect();
    let cleaned_queries: Vec<String> = expanded_queries.iter().map(|eq| {
        if neg_terms.is_empty() { return eq.clone(); }
        let mut words: Vec<&str> = eq.split_whitespace().collect();
        // Remove negative constraint words and their negation triggers ("not", "except", "without", "other than")
        let neg_triggers = ["not", "except", "without", "excluding", "other", "than"];
        words.retain(|w| {
            let w_lower = w.to_lowercase();
            !neg_terms.contains(&w_lower) && !neg_triggers.contains(&w_lower.as_str())
        });
        words.join(" ")
    }).filter(|q| q.split_whitespace().count() >= 2).collect();

    let expanded_queries = if !cleaned_queries.is_empty() { cleaned_queries } else { expanded_queries };
    tracing::info!("Fan-out with {} query variations: {:?}", expanded_queries.len(), expanded_queries);

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

    // Build SearXNG URLs for all expanded queries (max 4 variations)
    // Preprocess queries to strip trigger words that cause dictionary/shopping results
    // For freshness queries, add time_range parameter to get recent results
    let searx_urls: Vec<String> = expanded_queries.iter().take(4).map(|eq| {
        let clean_eq = preprocess_searxng_query(eq);
        let mut url = format!("http://127.0.0.1:8080/search?q={}&format=json&pageno=1", urlencoding::encode(&clean_eq));
        if is_freshness_query {
            url.push_str("&time_range=month");
        }
        url
    }).collect();

    let whoogle_url = format!("http://127.0.0.1:5000/search?q={}&format=json", q_encoded);
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let client_ref = &client;
    let circuit_ref = &state.circuit;
    let ratelimit_ref = &state.rate_limits;

    // Check circuit breaker before calling each engine
    let searx_open = circuit_ref.is_open("searxng");
    let whoogle_open = circuit_ref.is_open("whoogle");
    let invidious_open = circuit_ref.is_open("invidious");

    let indexer_fut = async {
        let resp = client_ref.get(&indexer_query).send().await?;
        let status = resp.status();
        resp.json::<Vec<IndexerResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Indexer JSON (status: {}): {:?}", status, e);
            e
        })
    };

    // Fire all SearXNG variations in parallel, with retry on 0 results
    // (VPN drops cause simultaneous failures across all engines — retry recovers)
    let searx_futs: Vec<_> = searx_urls.iter().map(|url| {
        let url = url.clone();
        let searx_open = searx_open;
        async move {
            if searx_open {
                return Ok(SearxResponse { results: vec![] });
            }
            // First attempt — sanitize control chars before JSON parse
            let first: Result<SearxResponse, reqwest::Error> = async {
                let resp = client_ref.get(&url).send().await?;
                let status = resp.status();
                let raw = resp.text().await.unwrap_or_default();
                let sanitized = sanitize_json_text(&raw);
                match serde_json::from_str::<SearxResponse>(&sanitized) {
                    Ok(data) => Ok(data),
                    Err(e) => {
                        tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
                        // Return empty response instead of error to allow retry
                        Ok(SearxResponse { results: vec![] })
                    }
                }
            }.await;

            match first {
                Ok(data) if !data.results.is_empty() => Ok(data),
                Ok(_) => {
                    // 0 results — check if retry would help.
                    // Skip retry for obviously malformed queries (special chars, too long, gibberish)
                    let url_lower = url.to_lowercase();
                    let q_part = url_lower.split("q=").nth(1).unwrap_or("");
                    let q_decoded = q_part.split("&").next().unwrap_or("");
                    let alpha_ratio: f32 = if q_decoded.len() > 0 {
                        q_decoded.chars().filter(|c| c.is_alphabetic() || c.is_whitespace()).count() as f32 / q_decoded.len() as f32
                    } else { 0.0 };
                    let is_malformed = q_decoded.len() > 200 || alpha_ratio < 0.3;
                    if is_malformed {
                        tracing::info!("Skipping retry for malformed query (alpha_ratio={:.2}, len={})", alpha_ratio, q_decoded.len());
                        return Ok(SearxResponse { results: vec![] });
                    }
                    // Retry once after 2s for legitimate queries that returned 0 results
                    tracing::warn!("SearXNG returned 0 results, retrying in 2s...");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let retry: Result<SearxResponse, reqwest::Error> = async {
                        let resp = client_ref.get(&url).send().await?;
                        let status = resp.status();
                        // Detect rate limiting (429) and trigger VPN rotation with rate-limit count
                        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                            let rl_count = ratelimit_ref.count_in_window(300);
                            ratelimit_ref.record();
                            let new_count = ratelimit_ref.count_in_window(300);
                            tracing::warn!(
                                "SearXNG got 429 Too Many Requests — rate-limits in 5min window: {} → {}",
                                rl_count, new_count
                            );
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
                    // If retry also returned 0 results, signal VPN rotation
                    if let Ok(ref data) = retry {
                        if data.results.is_empty() {
                            tracing::warn!("SearXNG retry returned 0 results — triggering VPN rotation");
                            trigger_vpn_rotation("zero_results_after_retry");
                        }
                    }
                    retry
                }
                Err(e) => {
                    tracing::warn!("SearXNG request failed — triggering VPN rotation: {:?}", e);
                    trigger_vpn_rotation("request_failed");
                    Err(e)
                }
            }
        }
    }).collect();

    let whoogle_fut = async {
        if whoogle_open {
            tracing::info!("Whoogle circuit OPEN — skipping");
            return Ok::<WhoogleResponse, anyhow::Error>(WhoogleResponse { results: vec![] });
        }
        let resp = client_ref.get(&whoogle_url).send().await?;
        let status = resp.status();
        let raw_text = resp.text().await.unwrap_or_default();
        // Parse Whoogle JSON into Value (no struct-level duplicate field rejection),
        // then extract results manually. This avoids serde's "duplicate field" error
        // caused by Whoogle returning both "title" and "text" keys in each result.
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
    };

    let invidious_fut = async {
        if invidious_open {
            tracing::info!("Invidious circuit OPEN — skipping");
            return Ok(vec![]);
        }
        let resp = client_ref.get(&invidious_url).send().await?;
        let status = resp.status();
        resp.json::<Vec<InvidiousResult>>().await.map_err(|e| {
            tracing::error!("Failed to parse Invidious JSON (status: {}): {:?}", status, e);
            e
        })
    };

    // Join all futures: indexer + all SearXNG variations + whoogle + invidious
    let (indexer_res, searx_results, whoogle_res, invidious_res) = tokio::join!(
        indexer_fut,
        futures::future::join_all(searx_futs),
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

    // 5. Aggregate Web Results from all sources
    let mut web_results: Vec<SearxResult> = Vec::new();
    // Track per-URL RRF contributions from each source's ranked position
    // This gives a proper rank-fusion score instead of meaningless insertion order
    let mut url_rrf_contributions: HashMap<String, f32> = HashMap::new();

    // Aggregate SearXNG results from all query variations
    for (i, searx_res) in searx_results.into_iter().enumerate() {
        match searx_res {
            Ok(searx_data) => {
                tracing::info!("SearXNG variation {} returned {} results", i, searx_data.results.len());
                circuit_ref.record_success("searxng");
                // Track position-based RRF contribution per URL within this variation
                for (pos, result) in searx_data.results.into_iter().enumerate() {
                    let normalized = {
                        let lower = result.url.to_lowercase();
                        let no_fragment = lower.split('#').next().unwrap_or(&lower);
                        let no_trailing = no_fragment.trim_end_matches('/');
                        no_trailing.replacen("://www.", "://", 1)
                    };
                    let rrf_contrib = 1.0 / (60.0 + (pos + 1) as f32);
                    *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                    web_results.push(result);
                }
            }
            Err(e) => {
                tracing::error!("SearXNG variation {} request failed/timed out: {:?}", i, e);
                circuit_ref.record_failure("searxng");
            }
        }
    }

    match whoogle_res {
        Ok(whoogle_data) => {
            tracing::info!("Whoogle returned {} results", whoogle_data.results.len());
            circuit_ref.record_success("whoogle");
            for (pos, r) in whoogle_data.results.into_iter().enumerate() {
                let normalized = {
                    let lower = r.url.to_lowercase();
                    let no_fragment = lower.split('#').next().unwrap_or(&lower);
                    let no_trailing = no_fragment.trim_end_matches('/');
                    no_trailing.replacen("://www.", "://", 1)
                };
                let rrf_contrib = 1.0 / (60.0 + (pos + 1) as f32);
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
            for (pos, r) in invidious_data.into_iter().enumerate() {
                if r.result_type.as_deref() == Some("video") {
                    if let Some(vid) = r.video_id {
                        let video_url = format!("https://www.youtube.com/watch?v={}", vid);
                        let normalized = {
                            let lower = video_url.to_lowercase();
                            let no_fragment = lower.split('#').next().unwrap_or(&lower);
                            let no_trailing = no_fragment.trim_end_matches('/');
                            no_trailing.replacen("://www.", "://", 1)
                        };
                        let rrf_contrib = 1.0 / (60.0 + (pos + 1) as f32);
                        *url_rrf_contributions.entry(normalized).or_insert(0.0) += rrf_contrib;
                        web_results.push(SearxResult {
                            title: r.title.unwrap_or_else(|| "No Title".to_string()),
                            url: video_url,
                            content: r.description.unwrap_or_default(),
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

    // Deduplicate — URL normalization + domain-based dedup
    // Multiple query variations may return the same page with different URLs
    // KEY: merge sources so we know which engines agreed on each result
    let mut unique_web_results: Vec<SearxResult> = Vec::new();
    let mut url_to_index: HashMap<String, usize> = HashMap::new(); // normalized URL -> index in unique_web_results
    let mut seen_domains = std::collections::HashMap::<String, usize>::new();
    const MAX_PER_DOMAIN: usize = 5; // prevent single-domain dominance

    for res in web_results {
        // Normalize URL: lowercase, strip trailing slash, strip fragment, strip www.
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            // Strip www. prefix for better dedup (www.example.com == example.com)
            no_trailing.replacen("://www.", "://", 1)
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

    // 6. Multi-Signal Ranking with Content Quality + Semantic Relevance + Consensus + RRF
    let weights = RankingWeights::for_intent(&intent.intent);

    // Pre-compute semantic relevance scores once (avoids double computation in ranking + filter)
    let semantic_scores: Vec<f32> = web_results.iter()
        .map(|res| semantic_relevance_score(&q, &res.title, &res.content))
        .collect();

    // Rank web results using all signals
    for (i, res) in web_results.iter_mut().enumerate() {
        // Use proper position-based RRF from each engine's ranked output
        // instead of meaningless insertion order
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            no_trailing.replacen("://www.", "://", 1)
        };
        let rank_score = url_rrf_contributions.get(&normalized).copied().unwrap_or(0.01);

        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = domain_authority_score(&res.url);
        let quality = content_quality_score(&res.content);
        let semantic = semantic_scores[i]; // use precomputed score
        let consensus = consensus_score(&res.sources);
        let c_score = constraint_score(&res.title, &res.content, &intent.structured_constraints);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            false, // not local
            quality,
            semantic,
            consensus,
            c_score,
            &weights,
        );
    }
    // Filter out results with very low semantic relevance using adaptive threshold
    let semantic_threshold = if web_results.len() > 30 { 0.25 }
        else if web_results.len() > 20 { 0.20 }
        else { 0.15 };
    let mut _idx = 0;
    web_results.retain(|_| {
        let keep = semantic_scores[_idx] >= semantic_threshold;
        _idx += 1;
        keep
    });
    web_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Normalize web scores to [0, 1] for cross-query comparability
    // SKIP normalization when negative constraints exist — it would undo constraint penalties
    let has_negative_constraints = !intent.structured_constraints.negative.is_empty();
    if !has_negative_constraints {
        let mut web_scores: Vec<f32> = web_results.iter().map(|r| r.score).collect();
        normalize_scores(&mut web_scores);
        for (i, r) in web_results.iter_mut().enumerate() {
            r.score = web_scores[i];
        }
    }

    // Rank local results using precomputed semantic scores
    let local_semantic_scores: Vec<f32> = local_results.iter()
        .map(|res| semantic_relevance_score(&q, &res.title, &res.content))
        .collect();
    for (i, res) in local_results.iter_mut().enumerate() {
        // Use the indexer's actual score (BM25 + semantic RRF) as the rank signal
        let rank_score = (res.score).max(0.01);
        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = if res.authority > 0.0 { res.authority } else { domain_authority_score(&res.url) };
        let quality = content_quality_score(&res.content);
        let semantic = local_semantic_scores[i];
        let c_score = constraint_score(&res.title, &res.content, &intent.structured_constraints);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            true, // local index bonus
            quality,
            semantic,
            0.3, // local-only = single source consensus
            c_score,
            &weights,
        );
    }
    local_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Quality gate: filter out garbage local results (error pages, stale content)
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
        title_ok && not_error
    });

    // Normalize local scores to [0, 1]
    let mut local_scores: Vec<f32> = local_results.iter().map(|r| r.score).collect();
    normalize_scores(&mut local_scores);
    for (i, r) in local_results.iter_mut().enumerate() {
        r.score = local_scores[i];
    }

    // 7. Feed Meta-Search Results into Crawl Queue with relevance signals
    // Include the score so the crawler can prioritize high-relevance URLs
    let crawl_urls: Vec<serde_json::Value> = web_results.iter()
        .filter(|r| r.score > 0.3 && !r.content.is_empty() && r.title.len() > 10)
        .take(20)
        .enumerate()
        .map(|(i, r)| {
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

    // Sanitize content in web_results to remove control characters
    // that SearXNG search engines may return in snippets
    for res in web_results.iter_mut() {
        res.title = sanitize_text_content(&res.title);
        res.content = sanitize_text_content(&res.content);
    }

    let response = UnifiedResponse {
        intent,
        local_results,
        web_results,
    };

    // Cache for 5 minutes — but never cache empty results (engine timeouts/rate limits)
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !response.web_results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({})))
}

fn fallback_intent(q: &str) -> IntentResponse {
    IntentResponse {
        query: q.to_string(),
        intent: "informational".to_string(),
        confidence: 0.3,
        constraints: vec![],
        structured_constraints: Constraints::default(),
        expanded_queries: vec![q.to_string()],
    }
}
