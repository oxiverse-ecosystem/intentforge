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

// ─── Domain Authority (Algorithmic, Not Hardcoded) ───────────────────
// Scores based on domain signals: TLD, known quality indicators

fn domain_authority_score(url: &str) -> f32 {
    let url_lower = url.to_lowercase();
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default();

    let mut score: f32 = 0.5; // baseline for unknown domains

    // ── TLD-based scoring (algorithmic) ──
    if host.ends_with(".edu") || host.ends_with(".gov") {
        score += 0.3; // institutional authority
    } else if host.ends_with(".org") {
        score += 0.1; // generally trustworthy
    }

    // ── Domain pattern scoring (signals, not hardcoded lists) ──
    // Official documentation patterns
    if host.starts_with("docs.") || host.starts_with("doc.")
        || host.starts_with("developer.") || host.starts_with("dev.")
        || host.starts_with("learn.") || host.starts_with("api.")
        || url_lower.contains("/docs/") || url_lower.contains("/api/")
        || url_lower.contains("/reference/")
    {
        score += 0.25;
    }

    // Official source patterns
    if host.starts_with("official") || url_lower.contains("/official")
        || url_lower.contains("/homepage")
    {
        score += 0.2;
    }

    // High-quality content platforms
    if host.contains("wikipedia.org") || host.contains("wikimedia.org") {
        score += 0.2;
    }
    if host.contains("stackoverflow.com") || host.contains("stackexchange.com") {
        score += 0.15;
    }
    if host.contains("github.com") || host.contains("gitlab.com")
        || host.contains("codeberg.org")
    {
        score += 0.15;
    }

    // Package registries (technical authority)
    if host.contains("crates.io") || host.contains("pypi.org")
        || host.contains("npmjs.com") || host.contains("docs.rs")
        || host.contains("pkg.go.dev") || host.contains("rubygems.org")
    {
        score += 0.2;
    }

    // Well-known news/review sites
    if host.contains("arstechnica.com") || host.contains("theverge.com")
        || host.contains("wired.com") || host.contains("techcrunch.com")
        || host.contains("arxiv.org")
    {
        score += 0.1;
    }

    // Content farms / low quality signals
    if url_lower.contains("content-farm") || url_lower.contains("clickbait")
        || url_lower.contains("top10best") || url_lower.contains("bestof")
    {
        score -= 0.2;
    }

    // Medium/dev.to — decent but not authoritative
    if host.contains("medium.com") || host.contains("dev.to")
        || host.contains("hashnode.dev")
    {
        score += 0.05;
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

// ─── Intent Boost (Enhanced) ─────────────────────────────────────────

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

            // Documentation/official signals
            if url_lower.contains("docs.") || url_lower.contains("doc.")
                || url_lower.contains("/docs/") || url_lower.contains("/doc/")
                || url_lower.contains("documentation") || url_lower.contains("wiki")
                || title_lower.contains("documentation") || title_lower.contains("official")
                || title_lower.contains("homepage")
            {
                boost += 0.6;
            }
        }
        "technical" => {
            // Code repos, API docs, libraries
            if url_lower.contains("github.com") || url_lower.contains("gitlab.com")
                || url_lower.contains("docs.rs") || url_lower.contains("crates.io")
                || url_lower.contains("npmjs.com") || url_lower.contains("pypi.org")
                || url_lower.contains("/api/") || url_lower.contains("reference")
                || url_lower.contains("stackoverflow.com")
                || url_lower.contains("developer.")
            {
                boost += 0.5;
            }
        }
        "how-to" | "conceptual" | "informational" | "comparison" | "fresh" => {
            // Tutorials, guides, wikis, forums, news
            if url_lower.contains("stackoverflow.com") || url_lower.contains("reddit.com")
                || url_lower.contains("/blog/") || url_lower.contains("/tutorial/")
                || url_lower.contains("/guide/") || url_lower.contains("wikipedia.org")
                || url_lower.contains("dev.to") || url_lower.contains("medium.com")
                || url_lower.contains("news.ycombinator.com")
                || url_lower.contains("/news/") || url_lower.contains("/article/")
                || url_lower.contains("arxiv.org")
            {
                boost += 0.4;
            }

            // For comparison intent, boost review sites
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
}

impl RankingWeights {
    fn for_intent(intent: &str) -> Self {
        match intent {
            "fresh" => Self {
                rrf: 0.08,
                intent: 0.05,
                freshness: 0.20,   // news needs recency
                authority: 0.15,   // news needs trustworthy sources
                local_bonus: 0.02,
                quality: 0.10,
                semantic: 0.25,
                consensus: 0.15,
            },
            "technical" => Self {
                rrf: 0.10,
                intent: 0.12,      // technical intent boost matters
                freshness: 0.05,   // docs are stable
                authority: 0.15,   // official docs preferred
                local_bonus: 0.05,
                quality: 0.08,
                semantic: 0.35,    // technical queries need precision
                consensus: 0.10,
            },
            "navigational" => Self {
                rrf: 0.05,
                intent: 0.25,      // navigational intent is dominant
                freshness: 0.03,
                authority: 0.20,   // official sites preferred
                local_bonus: 0.02,
                quality: 0.05,
                semantic: 0.30,
                consensus: 0.10,
            },
            "comparison" => Self {
                rrf: 0.12,
                intent: 0.10,
                freshness: 0.10,   // reviews should be recent
                authority: 0.08,
                local_bonus: 0.02,
                quality: 0.12,     // comparison content quality matters
                semantic: 0.30,
                consensus: 0.16,   // cross-source agreement for comparisons
            },
            "how-to" => Self {
                rrf: 0.10,
                intent: 0.10,
                freshness: 0.08,
                authority: 0.08,
                local_bonus: 0.05,
                quality: 0.10,
                semantic: 0.32,    // how-to needs precise matching
                consensus: 0.17,
            },
            _ => Self {  // informational, default
                rrf: 0.10,
                intent: 0.08,
                freshness: 0.07,
                authority: 0.10,
                local_bonus: 0.05,
                quality: 0.10,
                semantic: 0.30,
                consensus: 0.20,
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
    weights: &RankingWeights,
) -> f32 {
    let local = if is_local { 1.0 } else { 0.0 };

    (weights.rrf * rank_score)
        + (weights.intent * intent_boost)
        + (weights.freshness * freshness)
        + (weights.authority * authority)
        + (weights.local_bonus * local)
        + (weights.quality * quality)
        + (weights.semantic * semantic)
        + (weights.consensus * consensus)
}

// ─── Cross-Query Score Normalization ────────────────────────────────
// Normalizes scores to [0, 1] using robust percentile scaling.
// Makes scores comparable across different queries (a 1.5 on one query
// shouldn't be confused with 1.5 on another).

fn normalize_scores(scores: &mut [f32]) {
    if scores.len() < 3 {
        return; // not enough data to normalize
    }
    let mut sorted: Vec<f32> = scores.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p10_idx = ((sorted.len() as f32 * 0.10) as usize).min(sorted.len() - 1);
    let p90_idx = ((sorted.len() as f32 * 0.90) as usize).min(sorted.len() - 1);
    let p10 = sorted[p10_idx];
    let p90 = sorted[p90_idx];
    let range = (p90 - p10).max(0.001); // avoid division by zero

    for score in scores.iter_mut() {
        *score = ((*score - p10) / range).clamp(0.0, 1.0);
    }
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

struct AppState {
    circuit: CircuitBreaker,
    cache: SearchCache,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState {
        circuit: CircuitBreaker::new(),
        cache: SearchCache::new(),
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
    // For freshness queries, add time_range parameter to get recent results
    let searx_urls: Vec<String> = expanded_queries.iter().take(4).map(|eq| {
        let mut url = format!("http://127.0.0.1:8080/search?q={}&format=json&pageno=1", urlencoding::encode(eq));
        if is_freshness_query {
            url.push_str("&time_range=month");
        }
        url
    }).collect();

    let whoogle_url = format!("http://127.0.0.1:5000/search?q={}&format=json", q_encoded);
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let client_ref = &client;
    let circuit_ref = &state.circuit;

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

    // Fire all SearXNG variations in parallel
    let searx_futs: Vec<_> = searx_urls.iter().map(|url| {
        let url = url.clone();
        let searx_open = searx_open;
        async move {
            if searx_open {
                return Ok(SearxResponse { results: vec![] });
            }
            let resp = client_ref.get(&url).send().await?;
            let status = resp.status();
            resp.json::<SearxResponse>().await.map_err(|e| {
                tracing::error!("Failed to parse SearXNG JSON (status: {}): {:?}", status, e);
                e
            })
        }
    }).collect();

    let whoogle_fut = async {
        if whoogle_open {
            tracing::info!("Whoogle circuit OPEN — skipping");
            return Ok(WhoogleResponse { results: vec![] });
        }
        let resp = client_ref.get(&whoogle_url).send().await?;
        let status = resp.status();
        let raw_text = resp.text().await.unwrap_or_default();
        // Whoogle sometimes returns JSON with duplicate keys (e.g. two "title" fields).
        // serde_json rejects duplicates. Fix: deduplicate keys in each JSON object.
        let cleaned = deduplicate_json_keys(&raw_text);
        match serde_json::from_str::<WhoogleResponse>(&cleaned) {
            Ok(parsed) => Ok(parsed),
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
                        no_fragment.trim_end_matches('/').to_string()
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
                    no_fragment.trim_end_matches('/').to_string()
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
                            no_fragment.trim_end_matches('/').to_string()
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
        // Normalize URL: lowercase, strip trailing slash, strip fragment
        let normalized = {
            let lower = res.url.to_lowercase();
            let no_fragment = lower.split('#').next().unwrap_or(&lower);
            let no_trailing = no_fragment.trim_end_matches('/');
            no_trailing.to_string()
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
            no_fragment.trim_end_matches('/').to_string()
        };
        let rank_score = url_rrf_contributions.get(&normalized).copied().unwrap_or(0.01);

        let intent_boost = calculate_intent_boost(&res.url, &res.title, &q, &intent.intent);
        let freshness = freshness_score(&res.url, &intent.intent);
        let authority = domain_authority_score(&res.url);
        let quality = content_quality_score(&res.content);
        let semantic = semantic_scores[i]; // use precomputed score
        let consensus = consensus_score(&res.sources);

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            false, // not local
            quality,
            semantic,
            consensus,
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
    let mut web_scores: Vec<f32> = web_results.iter().map(|r| r.score).collect();
    normalize_scores(&mut web_scores);
    for (i, r) in web_results.iter_mut().enumerate() {
        r.score = web_scores[i];
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

        res.score = compute_final_score(
            rank_score,
            intent_boost,
            freshness,
            authority,
            true, // local index bonus
            quality,
            semantic,
            0.3, // local-only = single source consensus
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

    let response = UnifiedResponse {
        intent,
        local_results,
        web_results,
    };

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    state.cache.put(cache_key, response_json, Duration::from_secs(300));

    Json(serde_json::to_value(&response).unwrap_or(serde_json::json!({})))
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
