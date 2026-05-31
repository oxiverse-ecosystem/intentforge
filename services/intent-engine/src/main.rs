use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokenizers::Tokenizer;

// ─── Intent Categories (legacy, kept for reference) ────────────────
#[allow(dead_code)]
const INTENT_CATEGORIES: &[&str] = &[
    "navigational",
    "informational",
    "technical",
    "how-to",
    "comparison",
    "transactional",
    "fresh",
];

// ─── Structured Constraints ──────────────────────────────────────────
// Positive: terms the results MUST include/relate to
// Negative: terms the results MUST NOT include/relate to
// Extracted algorithmically from query syntax, not hardcoded.

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Constraints {
    #[serde(default)]
    pub positive: Vec<String>,
    #[serde(default)]
    pub negative: Vec<String>,
}

// ─── API Types ───────────────────────────────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct AnalyzeParams {
    pub q: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntentResponse {
    #[serde(default)]
    pub query: String,
    pub intent: String,
    pub confidence: f32,
    #[serde(default)]
    pub constraints: Vec<String>,       // legacy flat list (kept for compat)
    #[serde(default)]
    pub structured_constraints: Constraints, // new: positive + negative
    #[serde(default)]
    pub expanded_queries: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct EmbedParams {
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
}

// ─── App State (no more Qwen!) ──────────────────────────────────────

pub struct AppState {
    pub bert_model: Mutex<BertModel>,
    pub bert_tokenizer: Tokenizer,
    pub device: Device,
    pub intent_cache: Cache<String, IntentResponse>,
    pub embed_cache: Cache<String, Vec<f32>>,
    pub category_centroids: Vec<Vec<f32>>,
}

// ─── Constraint Extraction (Algorithmic) ────────────────────────────
// Extracts positive and negative constraints from natural language queries.
//
// Negative patterns detected:
//   - "NOT X", "-X", "without X", "except X", "excluding X"
//   - "no X" (when not at start of sentence as "no" = "number")
//   - "but not X", "other than X", "minus X"
//
// Positive patterns detected:
//   - "for X", "with X", "that is X", "must be X"
//   - Comma-separated list items after the core topic
//   - Adjective-like modifiers: "async", "lightweight", "fast", etc.
//
// Strategy: split query into constraint segments using punctuation and
// constraint trigger words, classify each segment as positive or negative.

fn extract_constraints(query: &str) -> Constraints {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let mut positive = Vec::new();
    let mut negative = Vec::new();

    // ── Phase 1: Extract explicit negative constraints ──
    // Handles: "NOT X", "-X", "without X", "except X", "excluding X"
    // Also handles conjunctive lists: "excluding X and Y and Z"

    let negative_markers = [
        " not ", " -", " without ", " except ", " excluding ",
        " but not ", " other than ", " minus ", " besides ", " no ",
        " alternative to ", " alternatives to ",
        " instead of ", " replacement for ",
    ];

    // Also match negative markers at the start of the query (no leading space)
    // For "not django web framework", extract just "django" as negative,
    // not the entire phrase. The rest is context for the search.
    let negative_start_markers = [
        "not ", "- ", "without ", "except ", "excluding ",
        "minus ", "besides ", "no ",
    ];

    // Process start-of-string markers first
    for marker in &negative_start_markers {
        if q_lower.starts_with(marker) {
            let remaining = &q[marker.len()..];
            // Take only the first 1-2 words as the negative term
            // (not the whole remaining query)
            let term = extract_constraint_term(remaining, 1);
            if !term.is_empty() && term.len() > 1 {
                negative.push(term);
            }
            break; // only one start marker can match
        }
    }

    // Known compound terms that start with "no " but aren't negations
    // "no sql" → "nosql", "no code" → "nocode", "no loss" → "noloss"
    let no_compounds = [
        "sql", "code", "loss", "ops", "code", "block",
    ];

    for marker in &negative_markers {
        let mut search_from = 0;
        while let Some(pos) = q_lower[search_from..].find(marker) {
            let abs_pos = search_from + pos;
            let after_marker = abs_pos + marker.len();
            if after_marker < q_lower.len() {
                // Special case: " no <term>" where no+term is a compound domain term
                if *marker == " no " {
                    let remaining_after_no = &q_lower[after_marker..];
                    let next_word = remaining_after_no.split_whitespace().next().unwrap_or("");
                    if no_compounds.contains(&next_word) {
                        // Push the compound form as positive: "no sql" → "nosql"
                        let compound = format!("no{}", next_word);
                        if !positive.contains(&compound) {
                            positive.push(compound);
                        }
                        search_from = after_marker + next_word.len();
                        continue;
                    }
                }
                let remaining = &q[after_marker..];
                // Extract multiple terms connected by "and"
                let terms = extract_conjunctive_terms(remaining, 1);
                for term in terms {
                    if !term.is_empty() && term.len() > 1 {
                        negative.push(term);
                    }
                }
            }
            search_from = after_marker;
        }
    }

    // ── Phase 2: Extract positive constraints ──
    // Look for requirement signals in the query
    // "for <X>", "with <X>", "that is <X>", "must be <X>"
    // Stop at negative markers to avoid capturing negated content

    let positive_markers = [
        " for ", " with ", " that is ", " that are ", " must be ",
        " must have ", " needs to ", " should be ", " which is ",
        " which are ",
    ];

    let negative_starts = [" not ", " without ", " except ", " excluding ",
                           " but not ", " other than ", " minus ", " besides ", " no ",
                           " alternative to ", " alternatives to ",
                           " instead of ", " replacement for "];

    for marker in &positive_markers {
        if let Some(pos) = q_lower.find(marker) {
            let after = pos + marker.len();
            if after < q_lower.len() {
                let remaining = &q[after..];
                // Stop at the first negative marker
                let end = negative_starts.iter()
                    .filter_map(|nm| remaining.to_lowercase().find(nm))
                    .min()
                    .unwrap_or(remaining.len());
                let clean_remaining = &remaining[..end];
                // Use extract_conjunctive_terms to handle "X and Y" lists
                let terms = extract_conjunctive_terms(clean_remaining, 2);
                for term in terms {
                    if !term.is_empty() && term.len() > 1 {
                        positive.push(term);
                    }
                }
            }
        }
    }

    // ── Phase 2.5: Extract site: and filetype: as positive constraints ──
    // These are explicit positive filters the user wants applied
    for cap in q_lower.match_indices("site:") {
        let after = cap.0 + 5; // skip "site:"
        let rest = &q[after..];
        // Take until next space or end
        let end = rest.find(' ').unwrap_or(rest.len());
        let site_val = &rest[..end];
        if !site_val.is_empty() {
            positive.push(format!("site:{}", site_val));
        }
    }
    for cap in q_lower.match_indices("filetype:") {
        let after = cap.0 + 9; // skip "filetype:"
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let ft_val = &rest[..end];
        if !ft_val.is_empty() {
            positive.push(format!("filetype:{}", ft_val));
        }
    }

    // ── Phase 3: Comma-separated constraint list ──
    // "rust framework, async, lightweight, no macros"
    // The first segment is the main query, subsequent ones are constraints.
    let segments: Vec<&str> = q.split(',').collect();
    if segments.len() >= 2 {
        for segment in &segments[1..] {
            let seg_trimmed = segment.trim();
            let seg_lower = seg_trimmed.to_lowercase();

            // Check if this segment starts with a negative marker
            let is_negative = seg_lower.starts_with("no ")
                || seg_lower.starts_with("not ")
                || seg_lower.starts_with("without ")
                || seg_lower.starts_with("except ")
                || seg_lower.starts_with("excluding ")
                || seg_lower.starts_with("besides ")
                || seg_lower.starts_with("-");

            if is_negative {
                // Special case: "no <compound>" where no+term is a domain term
                if seg_lower.starts_with("no ") {
                    let after_no = &seg_lower[3..];
                    let next_word = after_no.split_whitespace().next().unwrap_or("");
                    if no_compounds.contains(&next_word) {
                        // Push compound form as positive: "no sql" → "nosql"
                        let compound = format!("no{}", next_word);
                        if !positive.contains(&compound) {
                            positive.push(compound);
                        }
                        continue;
                    }
                }
                // Extract only the first word after the negative marker
                // "no heavy macros" → "macros" (not "heavy macros")
                let term_start = seg_lower.find(' ').map(|p| p + 1).unwrap_or(0);
                if term_start < seg_trimmed.len() {
                    let rest = &seg_trimmed[term_start..].trim();
                    let term = rest.split_whitespace().next().unwrap_or("")
                        .trim_matches(|c: char| c == ',' || c == '.' || c == ';')
                        .to_string();
                    if !term.is_empty() && term.len() > 1 {
                        negative.push(term);
                    }
                }
            } else {
                // It's a positive constraint
                let cleaned = seg_trimmed.trim_matches(|c: char| c.is_whitespace() || c == '.');
                if !cleaned.is_empty() && cleaned.len() > 1 {
                    positive.push(cleaned.to_string());
                }
            }
        }
    }

    // ── Phase 4: "vs" comparison constraints ──
    // "rust vs go" → don't treat as constraints, but "X without Y" in comparisons
    // is already handled by Phase 1

    // ── Phase 5: Implicit positive constraints from topic nouns ──
    // Extract remaining topic nouns as implicit positives when marker-based
    // extraction didn't capture enough constraints.
    // "python web framework not django" → remaining: [python, web, framework] → +python
    // "rust async web framework" (no markers) → remaining: [rust, async, web, framework] → +rust, +async
    // "lightweight javascript bundler" → remaining: [lightweight, javascript, bundler] → +lightweight, +javascript
    // Fires when: (a) positive list is empty (no markers matched), OR
    //              (b) negatives exist but few positives (marker-based missed them).
    if positive.is_empty() || (!negative.is_empty() && positive.len() < 2) {
        let stop_words: std::collections::HashSet<&str> = [
            "the","a","an","in","on","for","with","using","from","to",
            "and","or","of","is","are","was","were","be","been","has","have","had",
            "do","does","did","will","would","could","should","may","might",
            "how","what","where","when","why","which","who","this","that","these",
            "those","it","its","i","me","my","we","our","you","your","he","she","they",
            "be","as","at","by","not","but","if","so","than","too","very","can","just",
            "best","top","new","old","good","bad","big","small","first","last",
            "most","more","less","many","few","each","every","all","any","some",
            "quick","simple","easy","great","popular","powerful",
            "no","without","except","excluding","besides","other","than","minus",
            "that","which","must","needs","should","can","vs","v","versus",
            // Prepositions, conjunctions, articles, pronouns that slip through
            "set","up","down","out","off","over","under","about","into","through",
            "between","after","before","during","since","until","above","below",
            "per","via","way","ways","thing","things","type","kind","sort",
            // Domain-generic words that aren't useful as constraints
            "framework","library","language","tool","editor","database",
            "generator","server","client","application","app","software",
            "system","platform","service","api","sdk","package","module",
            "bundler","runtime","programming","tutorial","tutorials","guide",
            "documentation","docs","learn","getting","started","introduction",
            "explained","overview","comparison","compared","production",
            "deployment","alternative","alternatives","option","options",
            "solution","solutions","recommendation","recommendations",
            // Year numbers (extracted as constraints but pollute ranking)
            "2024","2025","2026","2023","2022","2021","2020",
            // Measurement/quantity words
            "price","cost","budget","cheap","free","under","over","less","more",
            "best","top","good","great","popular","recommended",
        ].iter().copied().collect();

        // Build set of words already consumed as part of compound terms
        // (e.g., "sql" consumed by "nosql" compound)
        let mut consumed_words: std::collections::HashSet<String> = std::collections::HashSet::new();
        for pos in &positive {
            if pos.starts_with("no") && pos.len() > 2 {
                let component = &pos[2..];
                consumed_words.insert(component.to_string());
            }
        }

        let neg_set: std::collections::HashSet<String> = negative.iter().cloned().collect();
        let pos_set: std::collections::HashSet<String> = positive.iter().cloned().collect();

        // Extract candidate topic words from the query
        let words: Vec<&str> = q_lower.split_whitespace().collect();
        for w in &words {
            let w_clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
            if w_clean.len() < 2 { continue; }
            if stop_words.contains(w_clean.as_str()) { continue; }
            if neg_set.contains(&w_clean) { continue; }
            if pos_set.contains(&w_clean) { continue; }
            if consumed_words.contains(&w_clean) { continue; }
            // Only add as implicit positive if it looks like a topic noun
            // (not a generic adjective or verb)
            positive.push(w_clean);
        }
    }

    // Deduplicate
    positive.sort();
    positive.dedup();
    negative.sort();
    negative.dedup();

    // Remove any term that appears in both — negative takes priority
    // (if user says "not vim", we don't also boost for "vim")
    let neg_set: std::collections::HashSet<String> = negative.iter().cloned().collect();
    positive.retain(|p| !neg_set.contains(p));
    // negative stays as-is — explicit exclusions always win

    // Remove very short or very long terms (likely parsing errors)
    positive.retain(|t| t.len() >= 2 && t.len() <= 50);
    negative.retain(|t| t.len() >= 2 && t.len() <= 50);

    Constraints { positive, negative }
}

/// Extract multiple terms connected by "and" from a negated context.
/// "mysql and sqlite" → ["mysql", "sqlite"]
/// "react" → ["react"]
/// max_words controls how many words per term (1 for negatives, 2 for positives).
fn extract_conjunctive_terms(text: &str, max_words: usize) -> Vec<String> {
    // Stop words that terminate the conjunctive chain.
    // NOTE: "not", "without", "except" etc. are NOT stop words here
    // because "and not X" is a valid conjunctive negative pattern.
    let stop_at = [" but ", " or ", " for ", " with ", " that ", " which ",
                   " not ", " without ", " except ", " excluding ", " other than ",
                   ".", ",", ";", "?", "!", " site:", " after:", " before:", " -"];
    // Find the end of the negated phrase
    let end = stop_at.iter()
        .filter_map(|s| text.to_lowercase().find(s))
        .min()
        .unwrap_or(text.len());
    let phrase = &text[..end];

    // Strip leading negative markers from individual parts
    // so "and not deno" → "deno", "and without X" → "X"
    let neg_prefixes = ["not ", "no ", "without ", "except ", "excluding ", "minus ", "-"];
    let strip_neg = |s: &str| -> String {
        let trimmed = s.trim();
        for prefix in &neg_prefixes {
            if trimmed.to_lowercase().starts_with(prefix) {
                return trimmed[prefix.len()..].trim().to_string();
            }
        }
        trimmed.to_string()
    };

    // Split on " and " to get individual terms
    let parts: Vec<&str> = phrase.split(" and ").collect();
    if parts.len() > 1 {
        // Multiple terms connected by "and"
        parts.iter()
            .map(|p| extract_constraint_term(&strip_neg(p), max_words))
            .filter(|t| !t.is_empty())
            .collect()
    } else {
        // Single term
        vec![extract_constraint_term(&strip_neg(phrase), max_words)]
    }
}

/// Extract a constraint term from the text after a marker.
/// Takes up to `max_words` words, stops at punctuation, conjunctions, or quality adjectives.
/// For negatives (max_words=1): "not vim" → "vim" (single word only)
/// For positives (max_words=2): "for game engine" → "game engine", "for beginners fast" → "beginners"
fn extract_constraint_term(text: &str, max_words: usize) -> String {
    let stop_words = ["and", "or", "but", "the", "a", "an", "is", "are", "in", "on",
        "for", "with", "from", "to", "of", "at", "by", "as", "via",
        "under", "over", "about", "into", "through", "between", "after", "before",
        "during", "since", "until", "above", "below", "per", "up", "down", "out",
        "off", "set", "how", "what", "where", "when", "why", "which", "who",
        // Domain-generic words that aren't useful as extracted constraints
        "programming", "framework", "library", "language", "tool", "database",
        "server", "client", "application", "app", "software", "system",
        "platform", "service", "tutorial", "guide", "documentation", "docs"];
    // Quality adjectives/modifiers that terminate extraction after the first content word.
    // "for beginners fast modern" → "beginners" (stops at "fast")
    // "for game engine lightweight" → "game engine" (stops at "lightweight")
    let quality_adjectives = [
        "fast", "modern", "quick", "lightweight", "simple", "easy", "powerful",
        "popular", "efficient", "cheap", "free", "secure", "safe", "reliable",
        "scalable", "flexible", "extensible", "portable", "robust", "minimal",
        "minimalist", "beginner-friendly", "user-friendly", "open-source",
        "cross-platform", "high-performance", "production-ready", "mature",
        "stable", "fastest", "lightest", "newest", "latest", "greatest",
        "best", "top", "new", "old", "good", "great", "small", "big",
        "alternative", "alternatives", "recommended", "suggested",
    ];
    let mut words = Vec::new();

    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| c == ',' || c == '.' || c == ';' || c == ':');
        if clean.is_empty() {
            break;
        }
        let lower = clean.to_lowercase();
        // Standard stop words (always stop)
        if stop_words.contains(&lower.as_str()) && !words.is_empty() {
            break;
        }
        // Quality adjectives stop extraction only after we've collected at least one word.
        // This prevents "fast framework" from being truncated to just "fast",
        // but "beginners fast modern" correctly yields "beginners".
        if !words.is_empty() && quality_adjectives.contains(&lower.as_str()) {
            break;
        }
        words.push(clean);
        if words.len() >= max_words {
            break;
        }
    }

    words.join(" ")
}


// ─── Evidence-Based Intent Classification ──────────────────────────
// Replaces the cascade of IF rule THEN intent with an evidence accumulator.
// Each detector contributes independent signals across all 7 intent categories.
// The final intent emerges from the weighted combination — no single detector
// can force an outcome, and competing signals coexist.

#[derive(Clone, Debug)]
struct IntentEvidence {
    navigational: f32,
    informational: f32,
    technical: f32,
    how_to: f32,
    comparison: f32,
    transactional: f32,
    fresh: f32,
}

impl IntentEvidence {
    fn zero() -> Self {
        Self {
            navigational: 0.0, informational: 0.0, technical: 0.0,
            how_to: 0.0, comparison: 0.0, transactional: 0.0, fresh: 0.0,
        }
    }

    fn scores(&self) -> [(&'static str, f32); 7] {
        [
            ("navigational", self.navigational),
            ("informational", self.informational),
            ("technical", self.technical),
            ("how-to", self.how_to),
            ("comparison", self.comparison),
            ("transactional", self.transactional),
            ("fresh", self.fresh),
        ]
    }

    fn argmax(&self) -> (&'static str, f32) {
        self.scores().iter()
            .cloned()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(("informational", 0.0))
    }

    fn merge(a: &Self, weight_a: f32, b: &Self, weight_b: f32) -> Self {
        let total = weight_a + weight_b;
        if total < 1e-8 { return a.clone(); }
        let wa = weight_a / total;
        let wb = weight_b / total;
        Self {
            navigational:    a.navigational    * wa + b.navigational    * wb,
            informational:   a.informational   * wa + b.informational   * wb,
            technical:       a.technical        * wa + b.technical       * wb,
            how_to:          a.how_to           * wa + b.how_to          * wb,
            comparison:      a.comparison       * wa + b.comparison      * wb,
            transactional:   a.transactional    * wa + b.transactional   * wb,
            fresh:           a.fresh            * wa + b.fresh           * wb,
        }
    }
}

// ─── Individual Detectors ─────────────────────────────────────────
// Each detector returns IntentEvidence with scores across all categories.
// Detectors are independent — no detector reads another's output.

fn question_detector(query: &str) -> IntentEvidence {
    let q = query.trim().to_lowercase();
    let mut ev = IntentEvidence::zero();

    if q.starts_with("how to ") || q.starts_with("how do i ")
        || q.starts_with("how can i ") || q.starts_with("how do you ")
        || q.starts_with("how does ") || q.starts_with("how do ")
        || q.starts_with("steps to ") || q.starts_with("guide to ")
        || q.starts_with("tutorial ") || q.contains(" tutorial")
    {
        ev.how_to = 0.85;
        ev.informational = 0.25;
    } else if q.starts_with("what is ") || q.starts_with("what are ")
        || q.starts_with("what does ") || q.starts_with("explain ")
        || q.starts_with("who is ")
        || q.starts_with("define ") || q.starts_with("meaning of ")
    {
        ev.informational = 0.80;
    } else if q.starts_with("why ") || q.starts_with("when ")
        || q.starts_with("where ")
    {
        // "why use rust over c++" — "why" can be informational or comparison.
        // If "vs"/"over"/"better" present, lean comparison.
        if q.contains(" vs ") || q.contains(" versus ") || q.contains(" over ")
            || q.contains(" better ") || q.starts_with("which ")
        {
            ev.comparison = 0.65;
            ev.informational = 0.35;
        } else {
            ev.informational = 0.70;
        }
    }

    ev
}

fn domain_detector(query: &str) -> IntentEvidence {
    let q = query.trim().to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut ev = IntentEvidence::zero();

    // TLD patterns: strongest navigational signal
    if q.contains(".com") || q.contains(".org") || q.contains(".net")
        || q.contains(".io") || q.contains(".dev") || q.contains(".rs")
        || q.contains(".py") || q.contains(".go") || q.contains(".edu")
        || q.contains(".gov")
    {
        ev.navigational = 0.90;
        return ev;
    }

    // Detect competing intent signals — when these are present,
    // the user is asking ABOUT a platform, not going TO it.
    let has_how_to = q.starts_with("how to ") || q.starts_with("how do i ")
        || q.starts_with("how can i ") || q.starts_with("how do you ")
        || q.starts_with("steps to ") || q.starts_with("guide to ")
        || q.starts_with("tutorial ") || q.contains(" tutorial");
    let has_question = q.starts_with("what is ") || q.starts_with("what are ")
        || q.starts_with("what does ") || q.starts_with("explain ")
        || q.starts_with("why ") || q.starts_with("define ");
    let has_comparison = q.contains(" vs ") || q.contains(" versus ")
        || q.starts_with("best ") || q.starts_with("top ")
        || q.starts_with("compare ") || q.starts_with("which ");
    let has_transactional = q.starts_with("buy ") || q.starts_with("download ")
        || q.starts_with("install ") || q.starts_with("purchase ")
        || q.contains(" pricing") || q.contains(" price");
    let has_fresh = q.contains("latest") || q.contains("recent")
        || q.contains("newest") || q.contains("new ");

    let competing_intent = has_how_to || has_question || has_comparison
        || has_transactional || has_fresh;

    // Known platform destinations
    let known_platforms: &[&str] = &[
        "github", "gitlab", "bitbucket", "stackoverflow", "stackexchange",
        "reddit", "twitter", "x", "facebook", "instagram", "linkedin",
        "youtube", "twitch", "discord", "slack", "notion", "figma",
        "dribbble", "behance", "medium", "substack", "mastodon",
        "docker", "npm", "pypi", "crates", "rubygems",
        "vercel", "netlify", "heroku", "digitalocean", "linode",
        "stripe", "twilio", "sendgrid", "mailchimp",
        "jira", "confluence", "trello", "asana", "linear",
        "spotify", "netflix", "hulu", "plex",
        "wikipedia", "wikimedia", "archive",
        "google", "bing", "duckduckgo",
        "apple", "microsoft", "amazon", "aws", "gcp", "azure",
        "openai", "anthropic", "huggingface",
    ];

    let known_compound: &[(&str, &str)] = &[
        ("docker", "hub"), ("docker", "desktop"),
        ("aws", "console"), ("gcp", "console"), ("azure", "portal"),
        ("google", "docs"), ("google", "drive"), ("google", "cloud"),
        ("github", "actions"), ("github", "pages"), ("github", "copilot"),
        ("apple", "music"), ("apple", "tv"),
        ("microsoft", "teams"), ("microsoft", "365"),
        ("stack", "overflow"), ("stack", "exchange"),
    ];

    // Check compound matches first
    if words.len() >= 2 {
        for (a, b) in known_compound {
            if words.contains(a) && words.contains(b) {
                // When competing intent is present, reduce navigational signal.
                // "how to use docker hub" → nav=0.30, not 0.85
                // "docker hub" alone → nav=0.85
                ev.navigational = if competing_intent { 0.30 } else { 0.85 };
                return ev;
            }
        }
    }

    // Single platform name match — only when query is SHORT (1-2 tokens)
    // AND no competing intent signal is present.
    if words.len() <= 2 && !competing_intent {
        for word in &words {
            if known_platforms.contains(word) {
                ev.navigational = 0.75;
                return ev;
            }
        }
    }

    // Platform mentioned in longer query with competing intent → weak signal
    if competing_intent {
        for word in &words {
            if known_platforms.contains(word) {
                ev.navigational = ev.navigational.max(0.25);
            }
        }
    }

    // Explicit navigational keywords
    if q.starts_with("official ") || q.contains(" homepage")
        || q.contains(" login") || q.contains(" sign in")
        || q.contains(" sign up") || q.starts_with("go to ")
        || q.starts_with("open ")
    {
        ev.navigational = 0.80;
    } else if q.contains("terms of service") || q.contains("privacy policy")
        || q.ends_with(" tos") || q.contains(" tos ")
    {
        ev.navigational = 0.75;
    }

    ev
}

fn tech_detector(query: &str) -> IntentEvidence {
    let q = query.trim().to_lowercase();
    let mut ev = IntentEvidence::zero();

    let tech_terms = [
        "api", "sdk", "library", "framework", "crate", "package", "module",
        "function", "method", "class", "interface", "struct", "enum",
        "trait", "impl", "syntax", "compiler", "runtime", "debug",
        "error", "bug", "fix", "issue", "version", "migration",
        "documentation", "docs", "reference", "manpage", "engine",
        "editor", "programming", "algorithm", "data structure",
    ];
    let tech_languages = [
        "rust", "python", "javascript", "typescript", "go", "golang",
        "java", "c++", "cpp", "swift", "kotlin", "ruby",
        "php", "haskell", "elixir", "scala",
        "react", "vue", "angular", "svelte", "nextjs", "next.js",
        "django", "flask", "fastapi", "express", "axum", "tokio",
        "docker", "kubernetes", "k8s", "linux", "git", "nginx",
        "postgres", "mysql", "redis", "mongodb", "sqlite",
        "css", "html", "sql", "nosql", "graphql", "grpc", "rest",
        "webpack", "vite", "tailwind", "bootstrap", "sass",
        "aws", "gcp", "azure", "terraform", "ansible",
        "tcp", "udp", "http", "https", "ssh", "ftp", "dns", "dhcp",
        "json", "yaml", "xml", "csv", "jwt", "oauth", "cors", "csrf",
    ];

    let has_tech_term = tech_terms.iter().any(|t| q.contains(t));
    let has_tech_lang = tech_languages.iter().any(|l| {
        q.split_whitespace().any(|w| w == *l)
    });

    if has_tech_term || has_tech_lang {
        ev.technical = 0.65;
        // Tech terms with action words lean how-to
        if q.starts_with("install ") || q.starts_with("set up ")
            || q.starts_with("configure ") || q.starts_with("deploy ")
            || q.starts_with("build ")
        {
            ev.how_to = 0.45;
        }
    }

    ev
}

fn commercial_detector(query: &str) -> IntentEvidence {
    let q = query.trim().to_lowercase();
    let mut ev = IntentEvidence::zero();

    // Transactional signals
    if q.starts_with("buy ") || q.starts_with("download ")
        || q.starts_with("install ") || q.starts_with("get ")
        || q.starts_with("purchase ") || q.starts_with("order ")
        || q.starts_with("subscribe ") || q.starts_with("sign up for ")
        || q.contains(" pricing") || q.contains(" free download")
    {
        ev.transactional = 0.80;
    }

    // Comparison signals
    if q.contains(" vs ") || q.contains(" versus ")
        || q.starts_with("best ") || q.starts_with("top ")
        || q.starts_with("compare ") || q.contains(" comparison")
        || q.starts_with("which ") || q.starts_with("better ")
    {
        ev.comparison = 0.75;
    }

    ev
}

fn freshness_detector(query: &str) -> IntentEvidence {
    let q = query.trim().to_lowercase();
    let mut ev = IntentEvidence::zero();

    let has_year = q.split_whitespace().any(|w| {
        w.len() == 4 && w.chars().all(|c| c.is_ascii_digit())
            && w.parse::<u32>().map_or(false, |y| y >= 2020 && y <= 2040)
    });
    if q.contains("latest") || q.contains("recent") || q.contains("newest")
        || q.contains("today") || q.contains("this week")
        || q.contains("this month") || has_year || q.starts_with("news ")
        || q.contains(" update") || q.contains(" release")
        || q.contains(" changes") || q.contains(" changelog")
        || q.contains(" cve") || q.contains(" vulnerability")
        || q.starts_with("new ")
    {
        ev.fresh = 0.75;
    }

    ev
}

fn entity_detector(
    query: &str,
    query_embedding: &[f32],
    centroids: &[Vec<f32>],
) -> IntentEvidence {
    // Entity detection using character-level signals (bigram rarity,
    // abbreviation patterns, concept fraction). This is the evidence-based
    // replacement for the old Layer 1.5 navigational override.
    //
    // Instead of IF score > threshold THEN navigational, it contributes
    // a navigational signal that competes with other evidence.

    let q_lower = query.trim().to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    let mut ev = IntentEvidence::zero();
    if words.is_empty() { return ev; }

    // ── Per-token character analysis ──
    let mut entity_scores: Vec<f32> = Vec::new();
    let mut abbr_scores: Vec<f32> = Vec::new();
    for word in &words {
        entity_scores.push(token_entityness(word));
        abbr_scores.push(abbreviation_score(word));
    }

    let max_entity: f32 = entity_scores.iter().cloned().fold(0.0f32, f32::max);
    let entity_density: f32 = entity_scores.iter().sum::<f32>()
        / entity_scores.len().max(1) as f32;
    let max_abbr: f32 = abbr_scores.iter().cloned().fold(0.0f32, f32::max);

    // ── Abbreviation discount ──
    let mut abbr_driven = false;
    for (i, word) in words.iter().enumerate() {
        let chars: Vec<char> = word.chars().collect();
        let vowels = chars.iter().filter(|c| "aeiou".contains(**c)).count();
        let vratio = if chars.is_empty() { 0.0 } else { vowels as f32 / chars.len() as f32 };
        if vratio < 0.15 && abbr_scores[i] > 0.6 && entity_scores[i] > 0.4 {
            abbr_driven = true;
        }
    }

    // ── Tech-term discount ──
    let tech_terms_nav = [
        "api", "sdk", "css", "html", "sql", "nosql", "graphql", "grpc",
        "webpack", "vite", "tailwind", "bootstrap", "sass", "terraform",
        "ansible", "docker", "kubernetes", "k8s", "nginx", "redis",
        "postgres", "mysql", "mongodb", "sqlite", "axum", "tokio",
    ];
    let mut tech_driven = false;
    for (i, word) in words.iter().enumerate() {
        if entity_scores[i] > 0.4 && tech_terms_nav.contains(word) {
            tech_driven = true;
        }
    }

    // ── Bigram rarity ──
    let best_rarity = words.iter()
        .map(|w| bigram_rarity(w))
        .fold(0.0f32, f32::max);

    // ── Concept fraction ──
    let stopwords = ["what", "is", "are", "the", "a", "an", "how", "to", "do", "does",
                     "can", "for", "in", "on", "of", "and", "or", "vs", "versus",
                     "best", "top", "cheap", "buy", "price", "near", "me", "free",
                     "online", "download", "install", "use", "using"];
    let mut concept_tokens = 0usize;
    let mut total_tokens = 0usize;
    for (i, word) in words.iter().enumerate() {
        if stopwords.contains(word) { continue; }
        if abbr_scores[i] > 0.5 { continue; }
        total_tokens += 1;
        let rarity = bigram_rarity(word);
        if rarity < 0.90 {
            concept_tokens += 1;
        }
    }
    let concept_fraction = if total_tokens > 0 {
        concept_tokens as f32 / total_tokens as f32
    } else { 0.0 };
    let entity_only_factor = (1.0 - concept_fraction).max(0.0);

    // ── Embedding signals ──
    let sims: Vec<f32> = centroids.iter()
        .map(|c| cosine_similarity(query_embedding, c))
        .collect();
    let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_sim = sims.iter().cloned().fold(f32::INFINITY, f32::min);
    let discriminability = (max_sim - min_sim).max(0.0);

    let dim = centroids.first().map(|c| c.len()).unwrap_or(384);
    let mut generic_centroid = vec![0.0f32; dim];
    for centroid in centroids {
        for (i, v) in centroid.iter().enumerate() { generic_centroid[i] += v; }
    }
    let n = centroids.len() as f32;
    if n > 0.0 {
        for v in generic_centroid.iter_mut() { *v /= n; }
        let norm: f32 = generic_centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 { for v in generic_centroid.iter_mut() { *v /= norm; } }
    }
    let dist_from_generic = 1.0 - cosine_similarity(query_embedding, &generic_centroid);

    let entropy = {
        let shift = if min_sim < 0.01 { -min_sim + 0.01 } else { 0.0 };
        let shifted: Vec<f32> = sims.iter().map(|s| s + shift).collect();
        let total: f32 = shifted.iter().sum();
        if total < 1e-8 { 0.0 } else {
            let mut h = 0.0f32;
            for &s in &shifted {
                let p = s / total;
                if p > 1e-8 { h -= p * p.log2(); }
            }
            h / (sims.len() as f32).log2().max(1.0)
        }
    };

    // ── Combine into navigational evidence ──
    let mut effective_entity = max_entity * 0.6 + entity_density * 0.4;
    if abbr_driven { effective_entity *= 0.6; }
    if tech_driven { effective_entity *= 0.5; }

    let entity_with_rarity = (effective_entity * 0.55 + best_rarity * 0.45).min(1.0);
    let char_nav = entity_with_rarity.max(max_abbr * 0.5);
    let emb_nav_raw = entropy * 0.5 + dist_from_generic.max(0.0) * 0.3
        + (1.0 - max_sim).max(0.0) * 0.2;
    let disc_penalty = (discriminability / 0.10).min(1.0);
    let emb_nav = emb_nav_raw * disc_penalty;

    let alpha = if discriminability < 0.08 && effective_entity > 0.40 { 0.80 }
        else if discriminability < 0.08 { 0.65 }
        else if discriminability > 0.15 && effective_entity < 0.40 { 0.30 }
        else if effective_entity > 0.55 { 0.60 }
        else { 0.45 };

    let nav_raw = alpha * char_nav + (1.0 - alpha) * emb_nav;
    let nav_score = nav_raw * entity_only_factor;

    ev.navigational = nav_score.clamp(0.0, 1.0);

    tracing::info!(
        "entity_detector: eff_entity={:.3} best_rarity={:.3} concepts={}/{} \
         alpha={:.2} disc={:.3} → nav={:.3}",
        effective_entity, best_rarity, concept_tokens, total_tokens,
        alpha, discriminability, ev.navigational
    );

    ev
}

fn embedding_detector(
    query_embedding: &[f32],
    centroids: &[Vec<f32>],
) -> IntentEvidence {
    // Pure embedding signal: cosine similarity to each category centroid.
    // Uses softmax with temperature to amplify small differences in the
    // 0.91-0.97 similarity range that MiniLM produces.

    let mut ev = IntentEvidence::zero();
    let sims: Vec<f32> = centroids.iter()
        .map(|c| cosine_similarity(query_embedding, c))
        .collect();

    if sims.is_empty() { return ev; }

    let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_sim = sims.iter().cloned().fold(f32::INFINITY, f32::min);
    let discriminability = (max_sim - min_sim).max(0.0);

    // Softmax with temperature to amplify small differences.
    // Uses log-sum-exp trick for numerical stability.
    // Lower temperature = sharper distribution (trusts small differences more).
    let temp = 0.01f32;
    let max_s = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let shifted: Vec<f32> = sims.iter().map(|s| (s - max_s) / temp).collect();
    let exp_shifted: Vec<f32> = shifted.iter().map(|s| s.exp()).collect();
    let exp_sum: f32 = exp_shifted.iter().sum();
    let softmax: Vec<f32> = if exp_sum > 1e-8 {
        exp_shifted.iter().map(|e| e / exp_sum).collect()
    } else {
        vec![1.0 / sims.len() as f32; sims.len()]
    };

    // Confidence scaling: how much the best category stands out.
    // disc < 0.05 → weak signal, scale down
    // disc > 0.10 → strong signal, use full softmax
    let confidence = (discriminability / 0.08).clamp(0.2, 1.0);

    let categories = [
        "navigational", "informational", "technical",
        "how-to", "comparison", "transactional", "fresh",
    ];

    for (i, cat) in categories.iter().enumerate() {
        if i < softmax.len() {
            let score = softmax[i] * confidence;
            match *cat {
                "navigational" => ev.navigational = score,
                "informational" => ev.informational = score,
                "technical" => ev.technical = score,
                "how-to" => ev.how_to = score,
                "comparison" => ev.comparison = score,
                "transactional" => ev.transactional = score,
                "fresh" => ev.fresh = score,
                _ => {}
            }
        }
    }

    tracing::info!(
        "embedding_detector: max_sim={:.4} disc={:.4} confidence={:.3} softmax=[{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}]",
        max_sim, discriminability, confidence,
        softmax.get(0).unwrap_or(&0.0), softmax.get(1).unwrap_or(&0.0),
        softmax.get(2).unwrap_or(&0.0), softmax.get(3).unwrap_or(&0.0),
        softmax.get(4).unwrap_or(&0.0), softmax.get(5).unwrap_or(&0.0),
        softmax.get(6).unwrap_or(&0.0)
    );

    ev
}

fn evidence_classify(
    query: &str,
    query_embedding: Option<&Vec<f32>>,
    centroids: &[Vec<f32>],
) -> (String, f32) {
    // Evidence accumulation: each detector contributes independent signals.
    // No detector can force an outcome — the final intent emerges from
    // the weighted combination of all evidence.

    let q_evidence = question_detector(query);
    let d_evidence = domain_detector(query);
    let t_evidence = tech_detector(query);
    let c_evidence = commercial_detector(query);
    let f_evidence = freshness_detector(query);

    // Entity detector needs embeddings for centroid distance signals
    let e_evidence = if let Some(emb) = query_embedding {
        entity_detector(query, emb, centroids)
    } else {
        IntentEvidence::zero()
    };

    // Merge all detectors with their weights
    // Weights reflect how much we trust each signal type
    let mut combined = IntentEvidence::zero();
    let mut total_weight = 0.0f32;

    let detectors: &[(IntentEvidence, f32)] = &[
        (q_evidence, 1.0),   // Question patterns are very reliable
        (d_evidence, 1.0),   // Domain/TLD patterns are very reliable
        (t_evidence, 0.7),   // Tech terms are moderate signal
        (c_evidence, 0.9),   // Commercial patterns are reliable
        (f_evidence, 0.8),   // Freshness patterns are fairly reliable
        (e_evidence, 0.6),   // Entity detection is noisier
    ];

    for (ev, weight) in detectors {
        if *weight > 0.0 {
            combined = IntentEvidence::merge(&combined, total_weight, ev, *weight);
            total_weight += *weight;
        }
    }

    // Embedding detector (if available) as tiebreaker
    if let Some(emb) = query_embedding {
        let emb_ev = embedding_detector(emb, centroids);
        combined = IntentEvidence::merge(&combined, total_weight, &emb_ev, 0.4);
    }

    let (intent, score) = combined.argmax();

    tracing::info!(
        "evidence_classify: nav={:.3} info={:.3} tech={:.3} how={:.3} \
         comp={:.3} txn={:.3} fresh={:.3} → {} ({:.3})",
        combined.navigational, combined.informational, combined.technical,
        combined.how_to, combined.comparison, combined.transactional,
        combined.fresh, intent, score
    );

    (intent.to_string(), score.clamp(0.0, 1.0))
}

// ─── Layer 1: Rule-Based Pre-Classifier (LEGACY — replaced by evidence detectors) ──

#[allow(dead_code)]
struct RuleMatch {
    intent: &'static str,
    confidence: f32,
}

#[allow(dead_code)]
fn rule_based_classify(query: &str) -> Option<RuleMatch> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(RuleMatch { intent: "informational", confidence: 0.5 });
    }

    // ── Navigational: user wants a specific site/page ──
    if q.contains(".com") || q.contains(".org") || q.contains(".net")
        || q.contains(".io") || q.contains(".dev") || q.contains(".rs")
        || q.contains(".py") || q.contains(".go") || q.contains(".edu")
        || q.contains(".gov")
    {
        return Some(RuleMatch { intent: "navigational", confidence: 0.9 });
    }
    if q.starts_with("official ") || q.contains(" homepage")
        || q.contains(" login") || q.contains(" sign in")
        || q.contains(" sign up") || q.starts_with("go to ")
        || q.starts_with("open ")
    {
        return Some(RuleMatch { intent: "navigational", confidence: 0.85 });
    }
    // Legal/policy pages: "oxiverse terms of service", "github privacy policy"
    // These are navigational — user wants a specific site's legal page.
    // Question patterns (what is, explain) are checked above and take priority.
    if q.contains("terms of service") || q.contains("privacy policy")
        || q.ends_with(" tos") || q.contains(" tos ")
    {
        return Some(RuleMatch { intent: "navigational", confidence: 0.80 });
    }

    // ── How-To ──
    if q.starts_with("how to ") || q.starts_with("how do i ")
        || q.starts_with("how can i ") || q.starts_with("how do you ")
        || q.starts_with("steps to ") || q.starts_with("guide to ")
        || q.starts_with("tutorial ") || q.contains(" tutorial")
    {
        return Some(RuleMatch { intent: "how-to", confidence: 0.9 });
    }

    // ── Transactional ──
    if q.starts_with("buy ") || q.starts_with("download ")
        || q.starts_with("install ") || q.starts_with("get ")
        || q.starts_with("purchase ") || q.starts_with("order ")
        || q.starts_with("subscribe ") || q.starts_with("sign up for ")
        || q.contains(" pricing") || q.contains(" free download")
    {
        return Some(RuleMatch { intent: "transactional", confidence: 0.85 });
    }

    // ── Comparison ──
    if q.contains(" vs ") || q.contains(" versus ")
        || q.starts_with("best ") || q.starts_with("top ")
        || q.starts_with("compare ") || q.contains(" comparison")
        || q.starts_with("which ") || q.starts_with("better ")
    {
        return Some(RuleMatch { intent: "comparison", confidence: 0.8 });
    }

    // ── Fresh/News ──
    // Algorithmic year detection: any 4-digit number in [2020, 2040] range
    let has_year = q.split_whitespace().any(|w| {
        w.len() == 4 && w.chars().all(|c| c.is_ascii_digit())
            && w.parse::<u32>().map_or(false, |y| y >= 2020 && y <= 2040)
    });
    if q.contains("latest") || q.contains("recent") || q.contains("newest")
        || q.contains("today") || q.contains("this week")
        || q.contains("this month") || has_year || q.starts_with("news ")
        || q.contains(" update") || q.contains(" release")
        || q.contains(" cve") || q.contains(" vulnerability")
    {
        return Some(RuleMatch { intent: "fresh", confidence: 0.8 });
    }

    // ── Informational (question patterns) ──
    // Must be checked BEFORE technical — "what is python" should be
    // informational, not technical, even though "python" is a tech term.
    if q.starts_with("what is ") || q.starts_with("what are ")
        || q.starts_with("what does ") || q.starts_with("explain ")
        || q.starts_with("why ") || q.starts_with("when ")
        || q.starts_with("where ") || q.starts_with("who is ")
        || q.starts_with("define ") || q.starts_with("meaning of ")
    {
        return Some(RuleMatch { intent: "informational", confidence: 0.8 });
    }

    // ── Technical ──
    let tech_terms = [
        "api", "sdk", "library", "framework", "crate", "package", "module",
        "function", "method", "class", "interface", "struct", "enum",
        "trait", "impl", "syntax", "compiler", "runtime", "debug",
        "error", "bug", "fix", "issue", "version", "migration",
        "documentation", "docs", "reference", "manpage", "engine",
        "editor", "programming", "algorithm", "data structure",
    ];
    let tech_languages = [
        "rust", "python", "javascript", "typescript", "go", "golang",
        "java", "c++", "cpp", "swift", "kotlin", "ruby",
        "php", "haskell", "elixir", "scala",
        "react", "vue", "angular", "svelte", "nextjs", "next.js",
        "django", "flask", "fastapi", "express", "axum", "tokio",
        "docker", "kubernetes", "k8s", "linux", "git", "nginx",
        "postgres", "mysql", "redis", "mongodb", "sqlite",
        "css", "html", "sql", "nosql", "graphql", "grpc", "rest",
        "webpack", "vite", "tailwind", "bootstrap", "sass",
        "aws", "gcp", "azure", "terraform", "ansible",
        "tcp", "udp", "http", "https", "ssh", "ftp", "dns", "dhcp",
        "json", "yaml", "xml", "csv", "jwt", "oauth", "cors", "csrf",
    ];

    let has_tech_term = tech_terms.iter().any(|t| q.contains(t));
    let has_tech_lang = tech_languages.iter().any(|l| {
        q.split_whitespace().any(|w| w == *l)
    });

    if has_tech_term || has_tech_lang {
        return Some(RuleMatch { intent: "technical", confidence: 0.75 });
    }

    // No strong signal → Layer 2
    None
}

// ─── Layer 2: Embedding Centroid Classifier (~2ms) ──────────────────

fn compute_centroid(model: &BertModel, tokenizer: &Tokenizer, device: &Device, examples: &[&str]) -> Vec<f32> {
    let dim = 384; // MiniLM-L6-v2
    let mut sum = vec![0.0f32; dim];
    let mut count = 0usize;

    for text in examples {
        if let Some(emb) = compute_embedding(device, model, tokenizer, text) {
            for (i, v) in emb.iter().enumerate() {
                sum[i] += v;
            }
            count += 1;
        }
    }

    if count == 0 {
        return vec![0.0; dim];
    }

    // Average
    for v in sum.iter_mut() {
        *v /= count as f32;
    }

    // L2 normalize
    let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for v in sum.iter_mut() {
            *v /= norm;
        }
    }

    sum
}

fn compute_embedding(device: &Device, model: &BertModel, tokenizer: &Tokenizer, text: &str) -> Option<Vec<f32>> {
    let tokens = tokenizer.encode(text, true).ok()?;
    let token_ids = Tensor::new(tokens.get_ids(), device).ok()?.unsqueeze(0).ok()?;
    let token_type_ids = Tensor::new(tokens.get_type_ids(), device).ok()?.unsqueeze(0).ok()?;

    let embeddings = model.forward(&token_ids, &token_type_ids, None).ok()?;
    let (_n_batch, n_tokens, _n_emb) = embeddings.dims3().ok()?;

    let embedding = (embeddings.sum(1).ok()? / (n_tokens as f64)).ok()?;
    let norm = embedding.sqr().ok()?.sum_all().ok()?.sqrt().ok()?.to_vec0::<f32>().ok()?;
    if norm < 1e-8 {
        return None;
    }
    let normalized = (embedding / (norm as f64)).ok()?;
    Some(normalized.to_vec2::<f32>().ok()?[0].clone())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-8 || norm_b < 1e-8 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[allow(dead_code)]
fn classify_by_centroids(query_embedding: &[f32], centroids: &[Vec<f32>]) -> (String, f32) {
    let mut best_intent = "informational";
    let mut best_score = -1.0f32;

    for (i, centroid) in centroids.iter().enumerate() {
        let sim = cosine_similarity(query_embedding, centroid);
        if sim > best_score {
            best_score = sim;
            if i < INTENT_CATEGORIES.len() {
                best_intent = INTENT_CATEGORIES[i];
            }
        }
    }

    (best_intent.to_string(), best_score)
}

// ─── Layer 1.5: Embedding-Based Navigational Detection ─────────────
// Addresses the failure mode where rule-based classifies "oxiverse tos"
// as "technical" and centroid confirms it.
//
// Three continuous signals, no hardcoded lists:
//   1. entityness: per-token distance from generic centroid + distribution entropy
//   2. domain_matchability: character-level check if tokens look like domain segments
//   3. abbreviation_score: vowel ratio + length + consonant cluster patterns
//
// Combined multiplicatively into a navigational score.
// Only overrides non-navigational rule results when score is high.

fn bigram_rarity(token: &str) -> f32 {
    // Computes how rare the character bigrams in a token are compared to
    // English text. Coined brand names (oxiverse, netflix, github) use
    // rare bigrams (ox, xi, iv, nf, tf) while natural English words use
    // common bigrams (th, he, in, er, an, on, at).
    //
    // This is the core signal that distinguishes brands from dictionary words.
    // No word lists — pure character statistics from English corpus frequencies.
    let lower = token.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    if chars.len() < 2 { return 0.5; }

    // Log-frequency of common English bigrams (COCA/Brown corpus).
    // Higher value = more common in English = less "domain-like".
    // Only stored for common bigrams — rare ones default to 0.05.
    let bigram_freq: std::collections::HashMap<&str, f32> = [
        // Top-tier: very common English bigrams
        ("th", 3.7), ("he", 3.1), ("in", 2.7), ("er", 2.5), ("an", 2.3),
        ("re", 2.2), ("on", 2.1), ("at", 2.0), ("en", 2.0), ("nd", 1.9),
        ("ti", 1.9), ("es", 1.8), ("or", 1.8), ("te", 1.7), ("of", 1.7),
        ("ed", 1.6), ("is", 1.6), ("it", 1.6), ("al", 1.6), ("ar", 1.5),
        ("st", 1.5), ("to", 1.5), ("nt", 1.5), ("ng", 1.5), ("se", 1.5),
        ("ha", 1.5), ("as", 1.4), ("ou", 1.4), ("io", 1.4), ("le", 1.4),
        // Common: frequent in everyday English
        ("li", 1.3), ("ve", 1.3), ("co", 1.3), ("me", 1.3), ("de", 1.3),
        ("ne", 1.2), ("ri", 1.2), ("ro", 1.2), ("ic", 1.2), ("ce", 1.1),
        ("la", 1.1), ("ta", 1.1), ("ma", 1.1), ("ra", 1.1), ("ec", 1.1),
        ("si", 1.0), ("id", 1.0), ("ol", 1.0), ("ur", 1.0), ("ch", 1.0),
        ("ly", 0.9), ("ot", 0.9), ("ut", 0.9), ("mi", 0.9), ("pe", 0.9),
        ("tr", 0.9), ("ct", 0.9), ("ge", 0.9), ("no", 0.9), ("il", 0.9),
        ("pa", 0.8), ("nc", 0.8), ("el", 0.8), ("di", 0.8), ("ac", 0.8),
        ("ns", 0.8), ("ab", 0.7), ("po", 0.7), ("ca", 0.7), ("ho", 0.7),
        ("om", 0.7), ("ie", 0.7), ("hi", 0.7), ("ig", 0.6), ("ss", 0.6),
        ("pr", 0.6), ("wh", 0.6), ("un", 0.6), ("im", 0.6), ("os", 0.6),
        ("lo", 0.6), ("su", 0.5), ("wi", 0.5), ("be", 0.5), ("ph", 0.5),
        ("cr", 0.5), ("ni", 0.5), ("bl", 0.5), ("pl", 0.5), ("sh", 0.5),
        ("mo", 0.5), ("vi", 0.5), ("fr", 0.4), ("sp", 0.4), ("rs", 0.4),
        ("ts", 0.4), ("gr", 0.4), ("tw", 0.4), ("ep", 0.4), ("sc", 0.4),
        ("hu", 0.4), ("sm", 0.3), ("sw", 0.3), ("dw", 0.3), ("kn", 0.3),
        ("gn", 0.3), ("wr", 0.3), ("pn", 0.3), ("ps", 0.3),
        // Moderate: less common but still normal English bigrams
        ("oo", 0.7), ("ok", 0.5), ("ks", 0.4), ("ey", 0.5), ("bo", 0.5),
        ("oa", 0.5), ("ke", 0.5), ("ek", 0.3), ("ak", 0.3), ("rn", 0.4),
        ("rd", 0.5), ("ld", 0.5), ("lk", 0.3), ("rk", 0.3), ("wn", 0.4),
        ("wl", 0.2), ("rm", 0.4), ("mp", 0.5), ("nk", 0.3), ("sk", 0.3),
        ("ck", 0.4), ("ff", 0.3), ("ll", 0.5), ("tt", 0.4), ("dd", 0.2),
        ("pp", 0.3), ("bb", 0.2), ("nn", 0.3), ("mm", 0.2), ("rr", 0.1),
        ("ee", 0.5), ("ea", 0.8), ("ai", 0.4), ("ei", 0.3), ("oi", 0.3),
        ("au", 0.3), ("ua", 0.3), ("ue", 0.4), ("ow", 0.5), ("ew", 0.4),
        ("aw", 0.3), ("ay", 0.4), ("oy", 0.3), ("wo", 0.3), ("wa", 0.5),
        ("gi", 0.1), ("ug", 0.3), ("ag", 0.3), ("og", 0.3), ("eg", 0.3),
        ("up", 0.4), ("op", 0.4), ("ip", 0.3), ("ap", 0.4),
        ("uf", 0.2), ("of", 1.7), ("af", 0.2), ("if", 0.4), ("ef", 0.3),
        ("ub", 0.3), ("ib", 0.3), ("ob", 0.3), ("eb", 0.3),
        ("uv", 0.2), ("ov", 0.3), ("av", 0.3), ("ev", 0.4),
        ("uz", 0.1), ("iz", 0.1), ("az", 0.1), ("ez", 0.1),
        // Very rare: strong signal for coined/brand names
        ("mn", 0.2), ("nm", 0.2), ("xu", 0.2), ("xz", 0.1), ("ox", 0.1),
        ("xi", 0.05), ("nf", 0.05), ("tf", 0.01), ("zx", 0.01),
        ("qw", 0.01), ("vk", 0.01), ("gl", 0.2), ("gg", 0.1), ("fb", 0.1),
        ("gm", 0.1), ("xb", 0.05), ("xc", 0.1), ("xd", 0.05), ("xf", 0.05),
        ("xh", 0.05), ("xj", 0.01), ("xk", 0.01), ("xl", 0.05), ("xm", 0.05),
        ("xn", 0.05), ("xp", 0.1), ("xq", 0.01), ("xr", 0.05), ("xs", 0.1),
        ("xt", 0.1), ("xv", 0.05), ("xw", 0.01), ("xx", 0.01), ("xy", 0.05),
        ("yf", 0.1), ("yg", 0.1), ("yh", 0.05), ("yj", 0.01), ("yk", 0.1),
        ("yl", 0.2), ("ym", 0.2), ("yn", 0.2), ("yp", 0.2), ("yq", 0.01),
        ("yr", 0.1), ("ys", 0.3), ("yt", 0.2), ("yv", 0.1), ("yw", 0.05),
        ("yx", 0.05), ("yy", 0.05), ("yz", 0.05), ("za", 0.1), ("zb", 0.05),
        ("zc", 0.05), ("zd", 0.05), ("ze", 0.2), ("zf", 0.05), ("zg", 0.05),
        ("zh", 0.05), ("zi", 0.1), ("zj", 0.01), ("zk", 0.05), ("zl", 0.05),
        ("zm", 0.05), ("zn", 0.05), ("zo", 0.1), ("zp", 0.05), ("zq", 0.01),
        ("zr", 0.05), ("zs", 0.05), ("zt", 0.05), ("zu", 0.1), ("zv", 0.05),
        ("zw", 0.05), ("zy", 0.1), ("zz", 0.1),
    ].iter().copied().collect();

    let max_freq = 3.7f32; // "th" is the most common English bigram
    let mut rarity_sum = 0.0f32;
    let mut count = 0usize;
    for i in 0..chars.len()-1 {
        let bg: String = chars[i..i+2].iter().collect();
        let freq = bigram_freq.get(bg.as_str()).copied().unwrap_or(0.05);
        rarity_sum += (max_freq - freq) / max_freq;
        count += 1;
    }
    if count == 0 { return 0.5; }
    (rarity_sum / count as f32).clamp(0.0, 1.0)
}

fn unigram_rarity(token: &str) -> f32 {
    // How rare are the individual characters? Rare letters (j, x, q, z, k)
    // boost the score. Common letters (e, t, a, o, i, n) lower it.
    let lower = token.to_lowercase();
    if lower.is_empty() { return 0.5; }
    let unigram_freq: std::collections::HashMap<char, f32> = [
        ('e', 2.8), ('t', 2.7), ('a', 2.5), ('o', 2.4), ('i', 2.4),
        ('n', 2.3), ('s', 2.2), ('h', 2.1), ('r', 2.1), ('d', 1.8),
        ('l', 1.7), ('c', 1.5), ('u', 1.4), ('m', 1.3), ('w', 1.2),
        ('f', 1.1), ('g', 1.0), ('y', 1.0), ('p', 1.0), ('b', 0.9),
        ('v', 0.6), ('k', 0.4), ('j', 0.1), ('x', 0.1), ('q', 0.1),
        ('z', 0.1),
    ].iter().copied().collect();
    let max_freq = 2.8f32; // 'e' is most common
    let mut rarity_sum = 0.0f32;
    for c in lower.chars() {
        let freq = unigram_freq.get(&c).copied().unwrap_or(0.05);
        rarity_sum += (max_freq - freq) / max_freq;
    }
    (rarity_sum / lower.chars().count() as f32).clamp(0.0, 1.0)
}

fn token_entityness(token: &str) -> f32 {
    // How entity-like is this token? Uses bigram rarity — the character-level
    // signal that distinguishes coined brand names from dictionary words.
    //
    // "oxiverse" → high rarity (ox, xi, iv are rare bigrams) → ~0.62
    // "photosynthesis" → low rarity (ph, ho, to, os are common) → ~0.35
    // "search" → moderate rarity (se, ea, rc, ch) → ~0.53
    // "intentforge" → moderate (in, nt, te common; tf, or rare) → ~0.36
    //
    // No thresholds, no lists — a continuous function of character statistics.
    let lower = token.to_lowercase();
    if lower.len() < 2 { return 0.3; }
    if !lower.chars().all(|c| c.is_alphabetic()) { return 0.3; }

    let br = bigram_rarity(token);
    let ur = unigram_rarity(token);

    let mut raw = br * 0.6 + ur * 0.4;

    // Short token bonus: abbreviations and short brand names get a bump
    if lower.len() <= 4 { raw *= 1.15; }
    // Long natural word penalty: 10+ letter words are almost always dictionary words
    if lower.len() >= 10 { raw *= 0.7; }

    raw.clamp(0.0, 1.0)
}

#[allow(dead_code)]
fn domain_matchability(token: &str) -> f32 {
    // Wrapper — entityness IS domain matchability now.
    // High entityness = rare character patterns = could appear in a domain.
    token_entityness(token)
}

fn abbreviation_score(token: &str) -> f32 {
    // Continuous function detecting abbreviation-like character patterns.
    // No predefined abbreviation list — pure character statistics.
    //
    // Signals:
    //   - Vowel ratio: abbreviations have fewer vowels (~25-33%) vs English (~40%)
    //   - Length: abbreviations are typically 2-5 characters
    //   - Consonant clusters: abbreviations often have 3+ consecutive consonants
    //
    // Returns [0.0, 1.0]: higher = more likely an abbreviation.
    if token.len() < 2 || token.len() > 8 { return 0.0; }
    let lower = token.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();

    // All non-alpha = not an abbreviation (numbers, symbols)
    if !chars.iter().all(|c| c.is_alphabetic()) { return 0.0; }

    let vowels = chars.iter().filter(|c| "aeiou".contains(**c)).count();
    let vowel_ratio = vowels as f32 / chars.len() as f32;

    // Length factor: peaks at 2-4 chars, decays after
    let length_factor = match chars.len() {
        2 => 0.9,
        3 => 1.0,
        4 => 0.85,
        5 => 0.6,
        _ => 0.3,
    };

    // Vowel sparsity: lower vowel ratio → higher score
    // English avg ~0.40, abbreviations ~0.25-0.33
    let vowel_sparsity = (1.0 - (vowel_ratio / 0.45).min(1.0)).max(0.0);

    // Consonant cluster density: count runs of 3+ consecutive consonants
    let mut cluster_count = 0usize;
    let mut consecutive_consonants = 0usize;
    for &c in &chars {
        if !("aeiou".contains(c)) {
            consecutive_consonants += 1;
            if consecutive_consonants >= 3 {
                cluster_count += 1;
            }
        } else {
            consecutive_consonants = 0;
        }
    }
    let cluster_density = (cluster_count as f32) / (chars.len() as f32).max(1.0);

    (vowel_sparsity * 0.5 + length_factor * 0.3 + cluster_density * 0.2).clamp(0.0, 1.0)
}

#[allow(dead_code)]
fn compute_navigational_score(
    query: &str,
    query_embedding: &[f32],
    centroids: &[Vec<f32>],
) -> f32 {
    // v4: BLENDED ADAPTIVE NAVIGATIONAL SCORING
    //
    // Addresses ALL failure modes from user feedback:
    //   1. Continuum entityness (not binary OOV) — uses bigram rarity
    //   2. "photosynthesis" false positive — bigram rarity naturally low
    //   3. Short informational queries — adaptive alpha trusts embeddings
    //   4. Abbreviation discount — zero-vowel abbrevs (tcp, css) get discounted
    //
    // KEY DESIGN: score = alpha * char_signals + (1-alpha) * embedding_signals
    // where alpha adapts based on signal strength.
    // Multiplicative combination failed because ALL signals had to agree.
    // Additive blending lets strong signals compensate for weak ones.

    let q_lower = query.trim().to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    if words.is_empty() { return 0.0; }

    // ── Step 1: Generic centroid ──
    let dim = centroids.first().map(|c| c.len()).unwrap_or(384);
    let mut generic_centroid = vec![0.0f32; dim];
    for centroid in centroids {
        for (i, v) in centroid.iter().enumerate() {
            generic_centroid[i] += v;
        }
    }
    let n = centroids.len() as f32;
    if n > 0.0 {
        for v in generic_centroid.iter_mut() { *v /= n; }
        let norm: f32 = generic_centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 {
            for v in generic_centroid.iter_mut() { *v /= norm; }
        }
    }

    // ── Step 2: Per-token character-level analysis ──
    let mut entity_scores: Vec<f32> = Vec::new();
    let mut abbr_scores: Vec<f32> = Vec::new();

    for word in &words {
        let ent = token_entityness(word);
        let abbr = abbreviation_score(word);
        entity_scores.push(ent);
        abbr_scores.push(abbr);
    }

    let entity_density: f32 = if entity_scores.is_empty() { 0.0 }
        else { entity_scores.iter().sum::<f32>() / entity_scores.len() as f32 };
    let max_entity: f32 = entity_scores.iter().cloned().fold(0.0f32, f32::max);
    let max_abbr: f32 = abbr_scores.iter().cloned().fold(0.0f32, f32::max);

    // ── Step 3: Abbreviation discount ──
    // Zero-vowel abbreviations (tcp, css, html, js) have high abbreviation_score
    // but are NOT navigational. Discount entity signals driven purely by
    // abbreviation patterns when the token has near-zero vowel ratio.
    let mut abbr_driven_entity = 0.0f32;
    for (i, word) in words.iter().enumerate() {
        let chars: Vec<char> = word.chars().collect();
        let vowels = chars.iter().filter(|c| "aeiou".contains(**c)).count();
        let vowel_ratio = if chars.is_empty() { 0.0 } else { vowels as f32 / chars.len() as f32 };
        if vowel_ratio < 0.15 && abbr_scores[i] > 0.6 && entity_scores[i] > 0.4 {
            abbr_driven_entity = abbr_driven_entity.max(entity_scores[i]);
        }
    }

    // ── Step 3b: Tech-term discount ──
    // Known tech terms (nosql, css, graphql) have high bigram_rarity because
    // they're coined technical jargon, not brand names. If a high-entity token
    // is a known tech term, discount it the same way abbreviations are discounted.
    let tech_terms_nav = [
        "api", "sdk", "css", "html", "sql", "nosql", "graphql", "grpc",
        "webpack", "vite", "tailwind", "bootstrap", "sass", "terraform",
        "ansible", "docker", "kubernetes", "k8s", "nginx", "redis",
        "postgres", "mysql", "mongodb", "sqlite", "axum", "tokio",
    ];
    let mut tech_driven_entity = 0.0f32;
    for (i, word) in words.iter().enumerate() {
        if entity_scores[i] > 0.4 && tech_terms_nav.contains(word) {
            tech_driven_entity = tech_driven_entity.max(entity_scores[i]);
        }
    }

    // ── Step 4: Embedding-level signals ──
    let sims: Vec<f32> = centroids.iter()
        .map(|c| cosine_similarity(query_embedding, c))
        .collect();
    let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_sim = sims.iter().cloned().fold(f32::INFINITY, f32::min);

    let entropy = {
        let shift = if min_sim < 0.01 { -min_sim + 0.01 } else { 0.0 };
        let shifted: Vec<f32> = sims.iter().map(|s| s + shift).collect();
        let total: f32 = shifted.iter().sum();
        if total < 1e-8 { 0.0 } else {
            let mut h = 0.0f32;
            for &s in &shifted {
                let p = s / total;
                if p > 1e-8 { h -= p * p.log2(); }
            }
            h / (sims.len() as f32).log2().max(1.0)
        }
    };

    let dist_from_generic = 1.0 - cosine_similarity(query_embedding, &generic_centroid);

    // ── Step 5: Entity-aware structure bonus ──
    // If query has a strong entity token, don't penalize for extra common words.
    // "oxiverse terms of service" → entity present → keep structure high
    let structure_bonus = if max_entity > 0.50 {
        0.85  // entity present → high structure regardless of length
    } else if max_entity > 0.40 {
        0.75
    } else {
        match words.len() {
            1 => 1.0,
            2 => 0.95,
            3 => 0.80,
            4..=5 => 0.60,
            _ => 0.40,
        }
    };

    // ── Step 6: Compute char-level and embedding-level nav signals ──
    let mut effective_entity = max_entity * 0.6 + entity_density * 0.4;
    // Apply abbreviation discount
    if abbr_driven_entity > 0.5 {
        effective_entity *= 0.6;
    }
    // Apply tech-term discount: known tech jargon with high rarity isn't a brand
    if tech_driven_entity > 0.5 {
        effective_entity *= 0.5;
    }

    // Bigram rarity: coined brand names (oxiverse, netflix) have rare bigrams.
    // Dictionary words (photosynthesis, machine) have common bigrams.
    // This separates brands from English words at the character level.
    let bigram_rarity_score = if !words.is_empty() {
        let best = words.iter().map(|w| bigram_rarity(w)).fold(0.0f32, f32::max);
        best
    } else { 0.0 };

    // Char nav: entity signal boosted by bigram rarity, abbreviation as tiebreaker.
    // When both entity and bigram_rarity agree (coined brand), score is high.
    // When entity is moderate but bigram_rarity is low (dictionary word), score stays low.
    let entity_with_rarity = (effective_entity * 0.55 + bigram_rarity_score * 0.45).min(1.0);
    let char_nav = (entity_with_rarity.max(max_abbr * 0.5)) * structure_bonus;
    let emb_nav_raw = entropy * 0.5 + dist_from_generic.max(0.0) * 0.3 + (1.0 - max_sim).max(0.0) * 0.2;

    // Penalize emb_nav when discriminability is low.
    // When max_sim and min_sim are nearly identical, the embedding can't
    // distinguish between categories. The signal is noise.
    // "discriminability" = max_sim - min_sim. For well-separated queries
    // this is 0.1+, for uniform queries it's <0.05.
    let discriminability = (max_sim - min_sim).max(0.0);
    let disc_penalty = (discriminability / 0.10).min(1.0); // 0.0 at spread=0, 1.0 at spread≥0.10
    let emb_nav = emb_nav_raw * disc_penalty;

    // ── Step 7: Adaptive alpha (the key innovation) ──
    // USES DISCRIMINABILITY, not max_sim.
    //
    // max_sim > 0.6 is ALWAYS true (0.91-0.97 for all queries with MiniLM).
    // What matters is whether embeddings can DISTINGUISH categories:
    //   discriminability = max_sim - min_sim
    //   < 0.08 → embeddings are noise (entropy=1.0, all sims ~equal)
    //   > 0.15 → embeddings can distinguish (one category clearly closer)
    //
    // When discriminability is LOW, character signals MUST dominate.
    // When discriminability is HIGH, embeddings help suppress false positives.
    // Effective entity adds a secondary boost for strong brand signals.
    let alpha = if discriminability < 0.08 && effective_entity > 0.40 {
        0.80  // embeddings are noise, entity present → trust char
    } else if discriminability < 0.08 {
        0.65  // embeddings are noise, no entity → still lean char
    } else if discriminability > 0.15 && effective_entity < 0.40 {
        0.30  // embeddings discriminative, weak entity → trust embeddings
    } else if effective_entity > 0.55 {
        0.60  // strong entity signal → lean char regardless
    } else {
        0.45  // balanced zone
    };

    let nav_raw = alpha * char_nav + (1.0 - alpha) * emb_nav;

    // ── Step 8: Entity-only factor ──
    // Pure entity queries (brand/site names) have almost no "concept tokens"
    // (generic English words like "database", "hooks", "tutorial").
    // When concept tokens are present alongside an entity, the query is less
    // navigational — the user is asking ABOUT something, not going TO it.
    //
    // KEY: concept detection uses bigram_rarity, NOT entityness.
    // Entityness fails for words like "hooks" (consonant clusters → moderate entityness)
    // even though they're common English words. Bigram rarity correctly identifies them:
    //   "hooks"    → bigrams ho,oo,ok,ks → all common → low rarity → concept ✓
    //   "database" → bigrams da,at,ta,ab,ba,as,se → common → low rarity → concept ✓
    //   "oxiverse" → bigrams ox,xi,iv,ve,rs,se → rare → high rarity → entity ✓
    //   "react"    → bigrams re,ea,ac,ct → common → low rarity → concept ✓
    //
    // This means "react hooks" = 2 concepts = eof=0.0 → score=0 → not navigational
    // while "intentforge tos" = 0 concepts (tos=abbr, intentforge=rare bigrams) → navigational
    let stopwords = ["what", "is", "are", "the", "a", "an", "how", "to", "do", "does",
                     "can", "for", "in", "on", "of", "and", "or", "vs", "versus",
                     "best", "top", "cheap", "buy", "price", "near", "me", "free",
                     "online", "download", "install", "use", "using"];
    let mut concept_tokens = 0usize;
    let mut total_tokens = 0usize;
    let mut _dbg_rarities: Vec<(String, f32, f32)> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        if stopwords.contains(word) { continue; }
        // Skip abbreviations — they're modifiers (tos, css), not concept words
        if abbr_scores[i] > 0.5 { continue; }
        total_tokens += 1;
        let rarity = bigram_rarity(word);
        _dbg_rarities.push((word.to_string(), entity_scores[i], rarity));
        // A "concept token" is a generic English word with common bigrams.
        // The bigram_rarity function maps: "th"→0.0, "ox"→0.97, "xi"→0.99.
        // Common English words (hooks, database, tutorial) have rarity ~0.70-0.88.
        // Coined brand names (oxiverse, netflix, intentforge) have rarity > 0.90.
        // Threshold at 0.90 separates the two populations.
        if rarity < 0.90 {
            concept_tokens += 1;
        }
    }
    let concept_fraction = if total_tokens > 0 {
        concept_tokens as f32 / total_tokens as f32
    } else { 0.0 };
    let entity_only_factor = (1.0 - concept_fraction).max(0.0);

    let score = nav_raw * entity_only_factor;

    let dbg_str: String = _dbg_rarities.iter()
        .map(|(w, e, r)| format!("{}(ent={:.2},rare={:.2})", w, e, r))
        .collect::<Vec<_>>()
        .join(", ");
    tracing::info!(
        "NAV SCORE v5: eff_entity={:.3} max_entity={:.3} max_abbr={:.3} abbr_discount={} \
         entropy={:.3} dist_generic={:.3} max_sim={:.3} \
         structure={:.3} char_nav={:.3} emb_nav={:.3} alpha={:.2} \
         concepts={}/{} eof={:.2} tokens=[{}] → score={:.3}",
        effective_entity, max_entity, max_abbr, abbr_driven_entity > 0.5,
        entropy, dist_from_generic, max_sim,
        structure_bonus, char_nav, emb_nav, alpha,
        concept_tokens, total_tokens, entity_only_factor, dbg_str, score
    );

    score.clamp(0.0, 1.0)
}

// ─── Query Expansion (Dynamic, Not Hardcoded) ────────────────────────

fn expand_queries(query: &str, intent: &str, constraints: &Constraints) -> Vec<String> {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    let mut expansions = vec![q.to_string()]; // always include original

    // Build set of negative constraint terms to exclude from expansions
    // "not django" should NOT generate "django documentation" as an expansion
    let neg_set: std::collections::HashSet<String> = constraints.negative.iter()
        .map(|n| n.to_lowercase())
        .collect();
    let neg_triggers: std::collections::HashSet<&str> = [
        "not", "no", "without", "except", "excluding", "but", "minus",
        "other", "than", "alternative", "alternatives", "instead",
    ].iter().copied().collect();

    let core = extract_core_topic(&q_lower, intent);
    let core_trimmed = core.trim();

    match intent {
        "how-to" => {
            if core_trimmed.len() > 3 {
                // Filter stop words to get meaningful topic words for variations
                let howto_stop = ["a","an","the","to","for","in","on","of","and","or","is","are","with","from","by"];
                let topic_words: Vec<&str> = core_trimmed.split_whitespace()
                    .filter(|w| w.len() > 1 && !howto_stop.contains(w) && !w.parse::<f64>().is_ok())
                    .filter(|w| {
                        let w_lower = w.to_lowercase();
                        let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                        !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
                            && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    })
                    .collect();
                if topic_words.len() >= 2 {
                    // Cap at 5 words to keep queries concise for SearXNG
                    let max_words = 5;
                    let capped: Vec<&str> = topic_words.iter().take(max_words).copied().collect();
                    expansions.push(format!("{} tutorial", capped.join(" ")));
                    expansions.push(format!("{} guide", capped.join(" ")));
                } else if topic_words.len() == 1 {
                    expansions.push(format!("{} tutorial", topic_words[0]));
                    expansions.push(format!("{} guide", topic_words[0]));
                }
                // Cap core_trimmed for step-by-step too
                let core_capped: String = core_trimmed.split_whitespace().take(6).collect::<Vec<&str>>().join(" ");
                expansions.push(format!("{} step by step", core_capped));
            }
        }
        "comparison" => {
            if q_lower.contains(" vs ") || q_lower.contains(" versus ") {
                let replaced = q_lower.replace(" versus ", " vs ");
                let parts: Vec<&str> = replaced.split(" vs ").collect();
                if parts.len() == 2 {
                    let (a, b) = (parts[0].trim(), parts[1].trim());
                    expansions.push(format!("{} {} comparison", a, b));
                    expansions.push(format!("{} compared to {}", a, b));
                    if let Some(year) = extract_year(&q_lower) {
                        expansions.push(format!("{} vs {} {}", a, b, year));
                    }
                }
            }
            if q_lower.starts_with("best ") || q_lower.starts_with("top ") {
                let prefix = if q_lower.starts_with("best ") { "best" } else { "top" };
                let rest = q.strip_prefix(&format!("{} ", prefix)).unwrap_or(q);
                expansions.push(format!("top {}", rest));
                expansions.push(format!("{} recommendation", rest));
                expansions.push(rest.to_string());
            }
        }
        "technical" => {
            let stop = ["the","a","an","in","on","for","with","using","from","to","and","or","of","is","are",
                        "excluding","without","except","other","than",
                        "not","no","but","minus"];
            let topic_words: Vec<&str> = words.iter()
                .filter(|w| w.len() > 2 && !stop.contains(w))
                .filter(|w| {
                    // Filter out negative constraint terms from expansions
                    let w_lower = w.to_lowercase();
                    let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                    !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
                        && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                })
                .copied()
                .collect();
            if topic_words.len() >= 2 {
                // Cap at 5 words to keep queries concise for SearXNG
                // Long queries (>6 words) cause 0 results on most engines
                let max_words = 5;
                let capped: Vec<&str> = topic_words.iter().take(max_words).copied().collect();
                expansions.push(format!("{} documentation", capped.join(" ")));
                expansions.push(format!("{} examples", capped.join(" ")));
                if !q_lower.contains("programming") {
                    expansions.push(format!("{} programming", capped.join(" ")));
                }
            }
        }
        "informational" => {
            if core_trimmed.len() > 3 {
                expansions.push(format!("{} explained", core_trimmed));
                expansions.push(format!("{} overview", core_trimmed));
                expansions.push(core_trimmed.to_string());
            }
        }
        "fresh" => {
            let temporal = ["latest","recent","newest","today","this week","this month","new"];
            let mut core_words: Vec<&str> = Vec::new();
            for w in &words {
                if !temporal.iter().any(|t| *t == *w) && w.len() > 2 {
                    let w_lower = w.to_lowercase();
                    let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                    if !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
                        && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    {
                        core_words.push(w);
                    }
                }
            }
            if !core_words.is_empty() {
                let core_str = core_words.join(" ");
                expansions.push(format!("{} 2026", core_str));
                expansions.push(format!("{} update", core_str));
                if core_str.contains("release") || core_str.contains("version") {
                    expansions.push(format!("{} changelog", core_str));
                }
            }
        }
        "navigational" => {
            if !q_lower.contains("official") {
                expansions.push(format!("official {}", q));
            }
        }
        _ => {}
    }

    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for exp in expansions {
        let key = exp.to_lowercase();
        if seen.insert(key) && unique.len() < 2 {
            unique.push(exp);
        }
    }
    unique
}

fn extract_core_topic<'a>(q_lower: &'a str, intent: &str) -> &'a str {
    match intent {
        "how-to" => {
            q_lower
                .strip_prefix("how to ").or_else(|| q_lower.strip_prefix("how do i "))
                .or_else(|| q_lower.strip_prefix("how can i "))
                .or_else(|| q_lower.strip_prefix("how do you "))
                .or_else(|| q_lower.strip_prefix("steps to "))
                .or_else(|| q_lower.strip_prefix("guide to "))
                .or_else(|| q_lower.strip_prefix("tutorial "))
                .unwrap_or(q_lower)
        }
        "informational" => {
            q_lower
                .strip_prefix("what is ").or_else(|| q_lower.strip_prefix("what are "))
                .or_else(|| q_lower.strip_prefix("what does "))
                .or_else(|| q_lower.strip_prefix("explain "))
                .or_else(|| q_lower.strip_prefix("define "))
                .or_else(|| q_lower.strip_prefix("meaning of "))
                .unwrap_or(q_lower)
        }
        "transactional" => {
            q_lower
                .strip_prefix("buy ").or_else(|| q_lower.strip_prefix("download "))
                .or_else(|| q_lower.strip_prefix("install "))
                .or_else(|| q_lower.strip_prefix("get "))
                .or_else(|| q_lower.strip_prefix("purchase "))
                .unwrap_or(q_lower)
        }
        _ => q_lower,
    }
}

fn extract_year(text: &str) -> Option<&str> {
    for word in text.split_whitespace() {
        if word.len() == 4 && word.starts_with("20") {
            if let Ok(year) = word.parse::<u32>() {
                if year >= 2020 && year <= 2029 {
                    return Some(word);
                }
            }
        }
    }
    None
}

// ─── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let device = Device::Cpu;

    let bert_path = "./models/model.safetensors";
    let bert_config_path = "./models/config.json";
    let bert_tokenizer_path = "./models/tokenizer_embed.json";

    for path in &[bert_path, bert_config_path, bert_tokenizer_path] {
        let mut retry_count = 0;
        while !std::path::Path::new(path).exists() && retry_count < 60 {
            tracing::info!("Waiting for {} to appear...", path);
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            retry_count += 1;
        }
    }

    tracing::info!("Loading MiniLM-L6-v2 model (embeddings + intent classification)...");
    let bert_config: BertConfig = serde_json::from_reader(std::fs::File::open(bert_config_path)?)?;
    let bert_vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[bert_path], DType::F32, &device)?
    };
    let bert_model = BertModel::load(bert_vb, &bert_config)?;

    tracing::info!("Loading tokenizer...");
    let bert_tokenizer = Tokenizer::from_file(bert_tokenizer_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!("Computing intent category centroids...");
    let category_centroids = {
        let category_examples: Vec<Vec<&str>> = vec![
            // 0: Navigational — user wants a SPECIFIC site/page (not a topic)
            vec!["reddit", "wikipedia", "youtube", "twitter", "facebook",
                 "instagram", "github login", "gmail", "netflix", "spotify",
                 "amazon", "stackoverflow", "discord", "twitch", "linkedin",
                 "apple", "microsoft", "google docs", "dropbox", "figma"],
            // 1: Informational — user wants to LEARN about a topic
            vec![
                 "what is machine learning", "what is a neural network",
                 "explain quantum computing", "what does TCP do",
                 "how does a compiler work", "what is the internet",
                 "why is the sky blue", "what is photosynthesis",
                 "define algorithm", "meaning of recursion",
                 "what are design patterns", "what is REST API",
                 "explain blockchain", "what is DNS", "how wifi works",
                 // Non-question informational: seeking content/info without question words
                 "healthy breakfast recipes", "python tutorials for beginners",
                 "travel tips for europe", "gardening guide for spring",
                 "best hiking trails near seattle", "history of ancient rome",
                 "climate change effects on agriculture", "beginner yoga poses",
                 "nosql database overview", "css layout guide",
                 "javascript closures explained", "database indexing strategies"],
            vec!["rust async runtime", "python requests library",
                 "javascript fetch API", "go goroutines",
                 "docker compose volumes", "kubernetes pods",
                 "react hooks useState", "django ORM queries",
                 "tokio spawn", "axum extractors",
                 "postgres indexes", "redis pub sub",
                 "nginx reverse proxy", "git rebase vs merge",
                 "typescript generics", "cargo features"],
            vec!["how to install rust", "how to set up docker",
                 "how to deploy to aws", "how to create a git branch",
                 "how to use async await", "how to configure nginx",
                 "how to write unit tests", "how to build a REST API",
                 "how to connect to postgres", "how to set up ci cd",
                 "how to implement authentication", "how to use redis cache",
                 "how to optimize sql queries", "how to handle errors in rust",
                 "how to create a react app"],
            vec!["rust vs go performance", "react vs vue",
                 "postgres vs mysql", "docker vs kubernetes",
                 "best programming language 2026",
                 "top web frameworks", "compare aws vs gcp",
                 "which database to use", "best ide for python",
                 "typescript vs javascript", "nginx vs apache",
                 "grpc vs rest", "mongodb vs postgresql",
                 "kubernetes vs docker swarm", "vim vs neovim"],
            vec!["buy mechanical keyboard", "download vscode",
                 "install python", "get started with rust",
                 "purchase domain name", "subscribe to spotify",
                 "sign up for github", "order pizza online",
                 "buy cloud hosting", "download docker desktop",
                 "install ubuntu", "get ssl certificate",
                 "purchase api key", "subscribe to newsletter",
                 "buy raspberry pi"],
            vec!["latest rust release", "new python version",
                 "cve 2026", "security vulnerability update",
                 "recent ai news", "today's tech news",
                 "this week in programming", "new javascript framework",
                 "latest docker update", "rust 1.80 release",
                 "python 3.13 features", "react 19 changes",
                 "linux kernel update", "github copilot news",
                 "ai regulation 2026"],
        ];

        let mut centroids = Vec::new();
        for examples in &category_examples {
            centroids.push(compute_centroid(&bert_model, &bert_tokenizer, &device, examples));
        }
        tracing::info!("Centroids computed for {} categories", centroids.len());
        centroids
    };

    let intent_cache = Cache::builder()
        .max_capacity(2000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let embed_cache = Cache::builder()
        .max_capacity(2000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let state = Arc::new(AppState {
        bert_model: Mutex::new(bert_model),
        bert_tokenizer,
        device,
        intent_cache,
        embed_cache,
        category_centroids,
    });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/analyze", get(analyze_query))
        .route("/embed", get(embed_text))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3005));
    tracing::info!("Intent Engine listening on {} (rules + centroids + constraint extraction)", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

// ─── /analyze endpoint ───────────────────────────────────────────────

async fn analyze_query(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AnalyzeParams>,
) -> Json<IntentResponse> {
    let query_norm = params.q.trim().to_lowercase();

    if let Some(cached) = state.intent_cache.get(&query_norm).await {
        return Json(cached);
    }

    // Extract constraints from the query
    let structured = extract_constraints(&params.q);
    tracing::info!(
        "Constraints extracted: positive={:?}, negative={:?}",
        structured.positive, structured.negative
    );

    // Build flat constraints list for backward compatibility
    let mut flat_constraints: Vec<String> = Vec::new();
    for c in &structured.positive {
        flat_constraints.push(format!("+{}", c));
    }
    for c in &structured.negative {
        flat_constraints.push(format!("-{}", c));
    }

    // ── Get embedding for evidence detectors ──
    let query_embedding = {
        let bert_model = state.bert_model.lock().unwrap();
        compute_embedding(&state.device, &*bert_model, &state.bert_tokenizer, &params.q)
    };

    // ── Evidence-based classification ──
    // All detectors run independently. No cascade, no overrides.
    // The intent emerges from weighted evidence accumulation.
    let (intent, confidence) = evidence_classify(
        &params.q,
        query_embedding.as_ref(),
        &state.category_centroids,
    );

    let expanded = expand_queries(&params.q, &intent, &structured);
    tracing::info!("Expanded to {} query variations", expanded.len());

    let result = IntentResponse {
        query: params.q.clone(),
        intent,
        confidence,
        constraints: flat_constraints,
        structured_constraints: structured,
        expanded_queries: expanded,
    };

    state.intent_cache.insert(query_norm, result.clone()).await;
    Json(result)
}

// ─── /embed endpoint ─────────────────────────────────────────────────

async fn embed_text(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EmbedParams>,
) -> Json<EmbedResponse> {
    let text_norm = params.text.trim().to_lowercase();

    if let Some(cached) = state.embed_cache.get(&text_norm).await {
        return Json(EmbedResponse { embedding: cached });
    }

    let embedding_vec = {
        let bert_model = state.bert_model.lock().unwrap();
        compute_embedding(&state.device, &*bert_model, &state.bert_tokenizer, &params.text)
            .unwrap_or_else(|| vec![0.0; 384])
    };

    state.embed_cache.insert(text_norm, embedding_vec.clone()).await;
    Json(EmbedResponse {
        embedding: embedding_vec,
    })
}
