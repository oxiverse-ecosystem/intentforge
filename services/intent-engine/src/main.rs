use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokenizers::Tokenizer;
use tokio::sync::Semaphore;

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
    "local",
];

// ─── Entity Roles (Query Graph IR) ────────────────────────────────
// Instead of flat positive/negative constraints, entities have semantic roles
// that determine how they're used in expansion, retrieval, and ranking.
//
// "alternative to notion"    → notion is Reference (find things LIKE it)
// "better than chatgpt"      → chatgpt is Comparison (benchmark against)
// "without django"           → django is Exclusion (exclude from results)
// "nginx reverse proxy"      → nginx is Target (the thing we're searching about)

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EntityRole {
    /// The primary subject of the search
    Target,
    /// "things like X", "alternative to X" — find similar things
    Reference,
    /// "better than X", "faster than X" — benchmark against
    Comparison,
    /// "without X", "not X", "except X" — exclude from results
    Exclusion,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QueryEntity {
    pub text: String,
    pub role: EntityRole,
}

// ─── Structured Constraints ──────────────────────────────────────────
// Positive: terms the results MUST include/relate to
// Negative: terms the results MUST NOT include/relate to
// Entities: semantic entities with roles (Query Graph IR)
// Extracted algorithmically from query syntax, not hardcoded.

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Constraints {
    #[serde(default)]
    pub positive: Vec<String>,
    #[serde(default)]
    pub negative: Vec<String>,
    /// Semantic entities with roles — the Query Graph IR.
    /// Replaces the flat positive/negative model with structured entity semantics.
    #[serde(default)]
    pub entities: Vec<QueryEntity>,
    /// Detected programming language from the query.
    /// Used by the gateway for language-aware result scoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub file_types: Vec<String>,
    #[serde(default)]
    pub sites: Vec<String>,
    #[serde(default)]
    pub phrases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_date: Option<String>,
    #[serde(default)]
    pub intitle: Vec<String>,
    #[serde(default)]
    pub inurl: Vec<String>,
    #[serde(default)]
    pub intext: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_max: Option<f32>,
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
    #[serde(default)]
    pub distribution: std::collections::HashMap<String, f32>, // calibrated probability distribution
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
    pub bert_model: Arc<BertModel>,
    pub bert_tokenizer: Tokenizer,
    pub device: Device,
    pub intent_cache: Cache<String, IntentResponse>,
    pub embed_cache: Cache<String, Vec<f32>>,
    pub bert_semaphore: Semaphore,
}

// ─── Linear Probe Weights ──────────────────────────────────────────
// Trained via logistic regression on all-MiniLM-L6-v2 embeddings
// using calibration_benchmark_200.csv. No hardcoded patterns.
// Retrain: python3 services/intent-engine/train_linear_probe.py

#[derive(Debug, Deserialize, Clone)]
struct IntentWeights {
    labels: Vec<String>,
    weights: Vec<Vec<f64>>,
    bias: Vec<f64>,
    temperature: f64,
    confidence: ConfidenceConfig,
}

#[derive(Debug, Deserialize, Clone)]
struct ConfidenceConfig {
    base: f64,
    margin_multiplier: f64,
}

static CONFIG: OnceLock<IntentWeights> = OnceLock::new();

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

fn extract_and_strip_phrases(query: &str) -> (String, Vec<String>) {
    let mut phrases = Vec::new();
    let mut cleaned_query = String::new();
    let mut current_phrase = String::new();
    let mut inside_quotes = false;
    
    // Normalize smart quotes
    let normalized = query
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
        } else {
            cleaned_query.push(c);
        }
    }
    if inside_quotes && !current_phrase.is_empty() {
        cleaned_query.push_str(&current_phrase);
    }
    
    (cleaned_query, phrases)
}

fn parse_price_range(s: &str) -> Option<(Option<f32>, Option<f32>)> {
    let clean: String = s.chars().filter(|c| c.is_numeric() || *c == '-' || *c == '.').collect();
    if clean.contains('-') {
        let parts: Vec<&str> = clean.split('-').collect();
        if parts.len() == 2 {
            let pmin = parts[0].parse::<f32>().ok();
            let pmax = parts[1].parse::<f32>().ok();
            return Some((pmin, pmax));
        }
    }
    if let Ok(val) = clean.parse::<f32>() {
        return Some((None, Some(val)));
    }
    None
}

/// Translate spoken number words into digits so the downstream price
/// operators fire. Spelled prices like "four hundred dollars" or "two hundred
/// fifty dollars" were never matched by the digit-only `price:<` regexes, so
/// they leaked as junk positive constraints (e.g. +four +hundred +dollars)
/// and no price bound was ever extracted (P3 regression). Converting the words
/// to digits up front lets the existing `under <N>` / `below <N>` rules produce
/// a real `price:<N` constraint, which then feeds extraction. Currency-agnostic:
/// it only rewrites the number, never the currency word.
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
        if tok == "hundred" || tok == "thousand" {
            out.push(tok.clone());
            i += 1;
            continue;
        }
        let is_unit = units.iter().any(|(w, _)| w == tok);
        if is_unit {
            let mut j = i;
            let mut run: Vec<String> = Vec::new();
            while j < tokens.len() {
                let t = &tokens[j];
                let is_num = units.iter().any(|(w, _)| w == t) || t == "hundred" || t == "thousand";
                if !is_num { break; }
                run.push(t.clone());
                j += 1;
            }
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
                        current += v as i64;
                    } else {
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
/// so downstream extraction is surface-form agnostic. Pure, order-independent
/// regex-free string rewriting:
///   * "under $500" / "less than 100" / "below 50" / "cheaper than 30" -> `price:<N`
///   * "over $100" / "more than 200" / "above 50"                    -> `price:>N`
///   * "in url:github" / "inurl github"                             -> `inurl:github`
///   * "on site:reddit" / "on site reddit"                         -> `site:reddit`
/// Existing canonical operators (`site:`, `filetype:`, `price:`, ...) are left
/// untouched. Only whitespace-delimited surface forms are rewritten; this never
/// touches quoted phrases (they are stripped before this runs).
fn normalize_nl_operators(query: &str) -> String {
    // Spoken prices ("four hundred dollars") -> digits so the price regexes below
    // can rewrite them into `price:<N`. Must run before the digit-only rules.
    let query = normalize_spoken_numbers(query);
    let mut out = query.to_string();

    // Price: upper-bound forms.
    for (re_src, replacement) in [
        (r"(?i)\bunder\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bless\s+than\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bbelow\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bcheaper\s+than\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        (r"(?i)\bmax(?:imum)?\s*\$?\s*(\d[\d.,]*)", "price:<$1"),
        // Price: lower-bound forms.
        (r"(?i)\bover\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bmore\s+than\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\babove\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bgreater\s+than\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        (r"(?i)\bmin(?:imum)?\s*\$?\s*(\d[\d.,]*)", "price:>$1"),
        // Operator spacing: "in url:github" / "inurl github" -> "inurl:github"
        (r"(?i)\bin\s+url\s*:\s*", "inurl:"),
        (r"(?i)\binurl\s+", "inurl:"),
        // "on site:reddit" / "on site reddit" -> "site:reddit"
        (r"(?i)\bon\s+site\s*:\s*", "site:"),
        (r"(?i)\bonsite\s+", "site:"),
        // "in title:guide" / "in title guide" -> "intitle:guide"
        (r"(?i)\bin\s+title\s*:\s*", "intitle:"),
        (r"(?i)\bintitle\s+", "intitle:"),
        // "in text:foo" / "in text foo" -> "intext:foo"
        (r"(?i)\bin\s+text\s*:\s*", "intext:"),
        (r"(?i)\bintext\s+", "intext:"),
    ] {
        if let Ok(re) = regex::Regex::new(re_src) {
            out = re.replace_all(&out, replacement).to_string();
        }
    }
    out
}

fn extract_constraints(query: &str) -> Constraints {
    let (query_stripped_phrases_raw, phrases) = extract_and_strip_phrases(query);
    // Normalize natural-language constraint syntax into the canonical
    // operator tokens the rest of this function already parses. This lets
    // users type the way they speak:
    //   "under $500" / "less than 100" / "below 50"  -> price:<N
    //   "in url:github" / "on site:reddit"           -> inurl:/site:
    // Applied before operator scanning so `price:`, `site:`, `inurl:` are
    // extracted uniformly regardless of the surface form the user typed.
    let query_stripped_phrases = normalize_nl_operators(&query_stripped_phrases_raw);

    let mut file_types = Vec::new();
    let mut sites = Vec::new();
    let mut after_date = None;
    let mut before_date = None;
    let mut intitle = Vec::new();
    let mut inurl = Vec::new();
    let mut intext = Vec::new();
    let mut related = Vec::new();
    let mut price_min = None;
    let mut price_max = None;
    let mut language = None;

    // Negated site:/filetype: tokens (e.g. "-site:reddit.com") are collected
    // here during operator scanning below and later merged with the natural-
    // language negative constraints extracted in Phase 1.
    let mut negative: Vec<String> = Vec::new();

    let q_lower_full = query_stripped_phrases.to_lowercase();
    
    // Extract filetype:
    for cap in q_lower_full.match_indices("filetype:") {
        let after = cap.0 + 9;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if val.is_empty() {
            continue;
        }
        // Negated form "-filetype:x" is an EXCLUSION, not a positive filter.
        // Routing it into `file_types` would invert the intent (include x
        // instead of excluding it). Push to `negative` so the hard filter and
        // the graduated penalty both treat it as exclusion.
        let negated = cap.0 > 0 && q_lower_full.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            negative.push(format!("filetype:{}", val));
        } else {
            file_types.push(val);
        }
    }

    // Extract site:
    for cap in q_lower_full.match_indices("site:") {
        let after = cap.0 + 5;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if val.is_empty() {
            continue;
        }
        // Negated form "-site:x" is an EXCLUSION, not a positive filter.
        // A bare `site:` scan would otherwise swallow "-site:reddit.com"
        // into the positive `sites` list and return the very site the user
        // asked to exclude. Push to `negative` as a `site:` exclusion token.
        let negated = cap.0 > 0 && q_lower_full.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            negative.push(format!("site:{}", val));
        } else {
            sites.push(val);
        }
    }

    // Extract intitle:
    for cap in q_lower_full.match_indices("intitle:") {
        let after = cap.0 + 8;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            intitle.push(val);
        }
    }

    // Extract inurl:
    for cap in q_lower_full.match_indices("inurl:") {
        let after = cap.0 + 6;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            inurl.push(val);
        }
    }

    // Extract intext:
    for cap in q_lower_full.match_indices("intext:") {
        let after = cap.0 + 7;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            intext.push(val);
        }
    }

    // Extract related:
    for cap in q_lower_full.match_indices("related:") {
        let after = cap.0 + 8;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            related.push(val);
        }
    }

    // Extract price:
    for cap in q_lower_full.match_indices("price:") {
        let after = cap.0 + 6;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if let Some((pmin, pmax)) = parse_price_range(&val) {
            price_min = pmin.or(price_min);
            price_max = pmax.or(price_max);
        }
    }

    // Extract lang:
    for cap in q_lower_full.match_indices("lang:") {
        let after = cap.0 + 5;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            language = Some(val);
        }
    }
    
    // Extract after:
    if let Some(pos) = q_lower_full.find("after:") {
        let after = pos + 6;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            after_date = Some(val);
        }
    }
    
    // Extract before:
    if let Some(pos) = q_lower_full.find("before:") {
        let after = pos + 7;
        let rest = &query_stripped_phrases[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let val = rest[..end].trim().to_lowercase();
        if !val.is_empty() {
            before_date = Some(val);
        }
    }

    if language.is_none() {
        // Delegate to the evidence-based detector so the gateway doesn't get a
        // forced "en" for non-matching queries. Returns None when no language
        // clearly wins (caller then falls back to geo/IP-derived locale).
        language = detect_query_language(&q_lower_full);
    }
    
    // Clean the query of all operators for token extraction
    let mut query_clean = String::new();
    let words_full: Vec<&str> = query_stripped_phrases.split_whitespace().collect();
    for w in words_full {
        let wl = w.to_lowercase();
        if wl.starts_with("site:") || wl.starts_with("filetype:") || wl.starts_with("after:") || wl.starts_with("before:")
            || wl.starts_with("intitle:") || wl.starts_with("inurl:") || wl.starts_with("intext:")
            || wl.starts_with("related:") || wl.starts_with("price:") || wl.starts_with("lang:")
        {
            continue;
        }
        query_clean.push_str(w);
        query_clean.push(' ');
    }
    let query_clean = query_clean.trim().to_string();
    
    let q = query_clean;
    let q_lower = q.to_lowercase();
    let mut positive = Vec::new();
    // `negative` is already declared above (near the operator scanners) so that
    // negated site:/filetype: tokens collected there and natural-language
    // negatives from Phase 1 accumulate into the same vector.

    // ── Phase 1: Extract explicit negative constraints ──
    // Handles: "NOT X", "-X", "without X", "except X", "excluding X"
    // Also handles conjunctive lists: "excluding X and Y and Z"
    //
    // IMPORTANT: "alternative to X" and "similar to X" are NOT negative.
    // They indicate a Reference entity (find things LIKE X).
    // "better than X", "faster than X" indicate a Comparison entity.

    let negative_markers = [
        " not ", " nor ", " -", " without ", " except ", " excluding ",
        " but not ", " other than ", " minus ", " besides ", " no ",
    ];

    // Reference patterns: "alternative to X" means find things LIKE X, not exclude X
    let reference_markers = [
        " alternative to ", " alternatives to ",
        " similar to ", " comparable to ",
        " replacement for ", " substitute for ",
        " competitor of ", " competitors of ",
    ];

    // Comparison patterns: "better than X" means benchmark against X
    let comparison_markers = [
        " better than ", " faster than ", " cheaper than ",
        " lighter than ", " easier than ", " simpler than ",
        " more performant than ", " more efficient than ",
    ];

    // Also match negative markers at the start of the query (no leading space)
    // For "not django web framework", extract just "django" as negative,
    // not the entire phrase. The rest is context for the search.
    let negative_start_markers = [
        "not ", "- ", "without ", "except ", "excluding ",
        "minus ", "besides ", "no ",
    ];

    // Reference markers at the start of the query (no leading space)
    // "alternative to notion" → notion is a Reference entity
    let reference_start_markers = [
        "alternative to ", "alternatives to ",
        "similar to ", "comparable to ",
        "replacement for ", "substitute for ",
        "competitor of ", "competitors of ",
    ]; // Note: "like " is omitted — too common as a non-reference word

    // Generic function words must never become negative constraints. A bare
    // negation marker followed by a stopword ("how to ... without [how]",
    // "...that does not require...") is syntactic, not a topical exclusion —
    // extracting "how"/"require"/"use" as a negative constraint penalises
    // every otherwise-relevant page and collapses the result set. Generic
    // (no topic-specific blocklist): covers the common closed-class words.
    let is_generic_negatable = |w: &str| -> bool {
        const GENERIC: &[&str] = &[
            "how", "what", "why", "when", "where", "who", "which", "that", "this",
            "these", "those", "the", "a", "an", "and", "or", "but", "use", "using",
            "require", "required", "requires", "need", "needed", "needs", "do",
            "does", "did", "can", "could", "would", "should", "will", "with", "without",
            "from", "into", "onto", "upon", "over", "under", "before", "after", "than",
            "them", "they", "their", "our", "your", "its", "his", "her", "not", "no",
        ];
        GENERIC.contains(&w.to_lowercase().as_str())
    };

    // Process start-of-string markers first (negatives)
    for marker in &negative_start_markers {
        if q_lower.starts_with(marker) {
            let remaining = &q[marker.len()..];
            // Preserve phrases if possible for negatives.
            // Phase 5: negatives use max_words=1 (head-noun only) so
            // "without prior experience" → "prior" not "prior experience".
            let term = extract_constraint_term(remaining, 1);
            if !term.is_empty() && term.len() > 1 && !is_generic_negatable(&term) {
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
                // Phase 5: negatives max_words=1 (head-noun) — "without node and react" → ["node","react"].
                let terms = extract_conjunctive_terms(remaining, 1);
                for term in terms {
                    if !term.is_empty() && term.len() > 1 && !is_generic_negatable(&term) {
                        negative.push(term);
                    }
                }
            }
            search_from = after_marker;
        }
    }

    // ── Phase 1b: Extract Reference entities ──
    // "alternative to X" → X is a Reference (find things LIKE X)
    // These are NOT negative constraints — they're the seed for similarity search.
    let mut entities: Vec<QueryEntity> = Vec::new();

    // Process start-of-string reference markers first
    // "alternative to notion" → match without leading space
    for marker in &reference_start_markers {
        if q_lower.starts_with(marker) {
            let remaining = &q[marker.len()..];
            let term = extract_constraint_term(remaining, 2);
            if !term.is_empty() && term.len() > 1 {
                entities.push(QueryEntity {
                    text: term.clone(),
                    role: EntityRole::Reference,
                });
                if !positive.contains(&term) {
                    positive.push(term);
                }
            }
            break; // only one start marker can match
        }
    }

    for marker in &reference_markers {
        let mut search_from = 0;
        while let Some(pos) = q_lower[search_from..].find(marker) {
            let abs_pos = search_from + pos;
            let after_marker = abs_pos + marker.len();
            if after_marker < q_lower.len() {
                let remaining = &q[after_marker..];
                let term = extract_constraint_term(remaining, 2);
                if !term.is_empty() && term.len() > 1 {
                    // Don't add as negative — add as Reference entity
                    entities.push(QueryEntity {
                        text: term.clone(),
                        role: EntityRole::Reference,
                    });
                    // Also add as positive constraint so it's included in search
                    if !positive.contains(&term) {
                        positive.push(term);
                    }
                }
            }
            search_from = after_marker;
        }
    }

    // ── Phase 1b2: "alternative to X" / "instead of X" ⇒ NEGATIVE (BUG #3) ──
    // "alternative to X" means the user wants something OTHER than X, so X is an
    // EXCLUSION constraint, NOT a positive Reference. This corrects the prior
    // behavior where "search engine alternative to google" returned Google as a
    // top result (Google was being added as a positive/Reference entity).
    // We extract X, push it to `negative`, and remove any Reference entity /
    // positive constraint that the earlier Reference phase (Phase 1b) created.
    let alt_neg_markers = [" alternative to ", " alternatives to ", " instead of "];
    let alt_neg_start_markers = ["alternative to ", "alternatives to ", "instead of "];

    let mut alt_terms: Vec<String> = Vec::new();
    for marker in &alt_neg_start_markers {
        if q_lower.starts_with(marker) {
            // Phase 5: negatives head-noun only (max_words=1)
            let term = extract_constraint_term(&q[marker.len()..], 1);
            if !term.is_empty() && term.len() > 1 {
                alt_terms.push(term);
            }
            break; // only one start marker can match
        }
    }
    for marker in &alt_neg_markers {
        let mut sf = 0;
        while let Some(pos) = q_lower[sf..].find(marker) {
            let ap = sf + pos + marker.len();
            if ap < q_lower.len() {
                // Phase 5: negatives head-noun only (max_words=1)
                let term = extract_constraint_term(&q[ap..], 1);
                if !term.is_empty() && term.len() > 1 {
                    alt_terms.push(term);
                }
            }
            sf = ap;
        }
    }
    for term in &alt_terms {
        if !negative.contains(term) {
            negative.push(term.clone());
        }
        // Remove from positive constraints and Reference entities that Phase 1b added
        positive.retain(|p| p != term);
        entities.retain(|e| !(e.role == EntityRole::Reference && &e.text == term));
    }

    // ── Phase 1c: Extract Comparison entities ──
    // "better than X" → X is a Comparison (benchmark against X)
    for marker in &comparison_markers {
        let mut search_from = 0;
        while let Some(pos) = q_lower[search_from..].find(marker) {
            let abs_pos = search_from + pos;
            let after_marker = abs_pos + marker.len();
            if after_marker < q_lower.len() {
                let remaining = &q[after_marker..];
                let term = extract_constraint_term(remaining, 2);
                if !term.is_empty() && term.len() > 1 {
                    entities.push(QueryEntity {
                        text: term.clone(),
                        role: EntityRole::Comparison,
                    });
                    // Also add as positive constraint so it's included in search
                    if !positive.contains(&term) {
                        positive.push(term);
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
    // These are explicit positive filters the user wants applied. Mirror the
    // negation handling and host validation from `extract_constraints` so a
    // negated "-site:x"/"-filetype:x" is NOT swallowed into `positive` (which
    // would invert the exclusion into an inclusion) and a malformed token such
    // as ".edu" (leading dot) is normalized/dropped instead of becoming a
    // literal (and zero-resulting) host filter.
    for cap in q_lower.match_indices("site:") {
        let after = cap.0 + 5; // skip "site:"
        let rest = &q[after..];
        // Take until next space or end
        let end = rest.find(' ').unwrap_or(rest.len());
        let site_val = &rest[..end];
        if site_val.is_empty() {
            continue;
        }
        // Leading '-' => this is a negation; do not push to positive.
        let negated = cap.0 > 0 && q_lower.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            continue;
        }
        // Normalize bare TLDs (edu/gov/...) to ".edu"/".gov"; drop leading-dot
        // or other non-host tokens so they don't zero out the query.
        let is_valid_host = site_val.contains('.') || site_val == "localhost";
        if !is_valid_host {
            let bare_tlds = ["edu","gov","org","com","net","io","dev","ai","co","us","uk","de","fr","es","nl","ru","cn","jp","in"];
            if bare_tlds.contains(&site_val) {
                positive.push(format!("site:.{}", site_val));
            }
            continue;
        }
        positive.push(format!("site:{}", site_val));
    }
    for cap in q_lower.match_indices("filetype:") {
        let after = cap.0 + 9; // skip "filetype:"
        let rest = &q[after..];
        let end = rest.find(' ').unwrap_or(rest.len());
        let ft_val = &rest[..end];
        if ft_val.is_empty() {
            continue;
        }
        let negated = cap.0 > 0 && q_lower.as_bytes().get(cap.0 - 1) == Some(&b'-');
        if negated {
            continue;
        }
        positive.push(format!("filetype:{}", ft_val));
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
    // Fires when Phase 2 marker-based extraction produced fewer than 3 terms.
    // Phase 2 often stops early at stop words (e.g. "with postgres on ubuntu 22.04"
    // extracts only "+postgres" because "on" terminates term extraction).
    if positive.len() < 3 {
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

        let mut neg_set: std::collections::HashSet<String> = negative.iter().cloned().collect();
        // Also add individual words from multi-word negatives to prevent leakage.
        // "django orm" -> also add "django" and "orm" so Phase 5 doesn't
        // add them back as positives.
                // Also add individual words from multi-word negatives to prevent leakage,
        // BUT only if the word doesn't appear elsewhere in the query as a standalone
        // term. This prevents removing legitimate positives: in "python async orm
        // not django orm", "orm" appears twice (positive + negative context) so it
        // stays positive, but "django" only appears in the negative phrase so it's
        // excluded from positives.
        let word_counts: std::collections::HashMap<String, usize> = {
            let mut counts = std::collections::HashMap::new();
            for w in q_lower.split_whitespace() {
                let clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                if !clean.is_empty() {
                    *counts.entry(clean).or_insert(0) += 1;
                }
            }
            counts
        };
        for neg in &negative {
            if neg.split_whitespace().count() > 1 {
                for word in neg.split_whitespace() {
                    let wl = word.to_lowercase();
                    // Only add to neg_set if the word appears only once in the query
                    // (i.e., only within the negative phrase, not as a standalone positive)
                    if word_counts.get(&wl).copied().unwrap_or(0) <= 1 {
                        neg_set.insert(wl);
                    }
                }
            }
        }
        let pos_set: std::collections::HashSet<String> = positive.iter().cloned().collect();

        // Words that appear as explicit OR/AND alternatives (e.g. "best OR worst",
        // "python and java") are deliberate comparison constraints. They must NOT
        // be silently dropped just because a quality adjective like "best"/"free"
        // is in `stop_words` — otherwise "best OR worst" collapses to just "worst".
        // We collect the operand words up front and exempt them from the stop-word
        // filter below (negation still wins, so "-best" stays excluded).
        let mut alt_operands: std::collections::HashSet<String> = std::collections::HashSet::new();
        for conj in [" or ", " and "] {
            for segment in q_lower.split(conj) {
                let operand = segment.trim().trim_end_matches(['.', ',', ';', '!', '?'])
                    .split_whitespace().next().unwrap_or("").to_string();
                if operand.len() >= 2 {
                    alt_operands.insert(operand);
                }
            }
        }

        // Extract candidate topic words from the query
        // Prefer keeping phrase groups intact so hyphenated/slashed negative terms
        // like "react/vue/nextjs" survive as whole phrases and won't leak back
        // into positives after alnum-filtering.
        let words: Vec<&str> = q_lower.split_whitespace().collect();
        for w in &words {
            let mut w_clean: String = w.chars()
                .map(|c| if c.is_alphanumeric() { c } else if c == '/' || c == '-' || c == '_' { c } else { ' ' })
                .collect();
            w_clean = w_clean.split_whitespace().collect::<Vec<_>>().join(" ");
            if w_clean.is_empty() { continue; }
            if w_clean.len() < 2 { continue; }
            // Explicit OR/AND operands survive the stop-word filter (see above).
            if !alt_operands.contains(w_clean.as_str()) && stop_words.contains(w_clean.as_str()) { continue; }
            // Use lowercase string forms for set lookups (HashSet<String>).
            let w_lower: String = w.to_lowercase();
            if neg_set.contains(&w_lower) { continue; }
            if pos_set.contains(&w_lower) { continue; }
            if consumed_words.contains(&w_lower) { continue; }
            // If the raw token matched a negative phrase exactly, skip adding it as a positive.
            if negative.iter().any(|n| n == &w_lower) { continue; }
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

    let language = language.or_else(|| detect_query_language(&q_lower));

    // ── Phase 7: Promote negative terms to Exclusion entities ──
    // Any term that wasn't already captured as Reference/Comparison gets Exclusion role.
    for neg_term in &negative {
        if !entities.iter().any(|e| e.text == *neg_term) {
            entities.push(QueryEntity {
                text: neg_term.clone(),
                role: EntityRole::Exclusion,
            });
        }
    }

    // De-duplicate and unify constraint term casing so downstream
    // matching/telemetry sees one canonical form for each term
    // (e.g. "+pinecone" / "-Pinecone" collapse to "pinecone").
    let normalize_terms = |terms: Vec<String>| -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::with_capacity(terms.len());
        for t in terms {
            let key = t.to_lowercase();
            if seen.insert(key.clone()) {
                out.push(key);
            }
        }
        out
    };
    let positive = normalize_terms(positive);
    let negative = normalize_terms(negative);

    Constraints {
        positive,
        negative,
        entities,
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
    }
}

fn detect_query_language(q_lower: &str) -> Option<String> {
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    // Each language mapped to a set of its characteristic (function/stop) words.
    // Detection is evidence-based: we count how many query words belong to each
    // language's signature. A language is returned only when it clearly wins
    // (strict majority of query words, or any distinctive non-cognate marker),
    // otherwise we return None and let the caller avoid forcing a wrong locale.
    // This fixes the prior behaviour where any non-matching query was silently
    // pinned to "en" and forwarded as a hard language filter.
    let fr_words = ["de", "la", "le", "les", "des", "et", "recette", "gateau", "pour", "dans", "une", "que", "qui", "est", "avec"];
    let de_words = ["der", "die", "das", "und", "ist", "rezept", "kuchen", "fur", "mit", "wie", "man", "lernt", "ein", "eine", "nicht", "sich", "auf", "von"];
    let es_words = ["el", "la", "los", "las", "y", "en", "para", "con", "mejor", "zapatos", "baratos", "comprar", "que", "uno", "una", "por", "como", "mas"];
    let nl_words = ["van", "het", "een", "en", "koptelefoon", "voor", "de", "die", "met", "is", "op", "een"];

    let score_for = |sig: &[&str]| -> usize {
        words.iter().filter(|w| sig.contains(w)).count()
    };
    let fr = score_for(&fr_words);
    let de = score_for(&de_words);
    let es = score_for(&es_words);
    let nl = score_for(&nl_words);

    // Distinctive markers that are unambiguous (not shared English cognates).
    let fr_hit = words.iter().any(|w| ["recette", "gateau", "pour", "dans", "une"].contains(w));
    let de_hit = words.iter().any(|w| ["wie", "man", "lernt", "rezept", "kuchen", "fur"].contains(w));
    let es_hit = words.iter().any(|w| ["mejor", "zapatos", "baratos", "comprar", "para"].contains(w));
    let nl_hit = words.iter().any(|w| ["koptelefoon", "voor", "van"].contains(w));

    // Pick the language with the highest signature overlap; require it to be a
    // strict majority of the query words OR carry a distinctive marker, so that
    // e.g. "python" or "best laptop" (English-passing words) do not get mis-tagged.
    let best = [(fr as i32, "fr"), (de as i32, "de"), (es as i32, "es"), (nl as i32, "nl")]
        .iter()
        .copied()
        .max_by_key(|&(s, _)| s)
        .unwrap();
    let total = words.len() as i32;
    let (best_score, best_lang) = best;
    let distinctive = fr_hit || de_hit || es_hit || nl_hit;
    if best_score >= 2 && (best_score * 2 > total || distinctive) {
        let lang = if fr_hit { "fr" } else if de_hit { "de" } else if es_hit { "es" } else if nl_hit { "nl" } else { best_lang };
        return Some(lang.to_string());
    }
    // No clear non-English signal: do NOT force "en". Return None so the
    // gateway falls back to geo/IP-derived language rather than an asserted en.
    None
}

/// Extract multiple terms connected by "and" or "or" from a negated context.
/// "mysql and sqlite" → ["mysql", "sqlite"]
/// "mysql or sqlite" → ["mysql", "sqlite"]
/// "react" → ["react"]
/// max_words controls how many words per term (1 for negatives, 2 for positives).
fn extract_conjunctive_terms(text: &str, max_words: usize) -> Vec<String> {
    // Stop words/connectors that terminate the negated chain, except "or" — handled
    // alongside "and" so exclusion lists like "without X or Y" are captured cleanly.
    let stop_at = [" but ", " for ", " with ", " that ", " which ",
                   " not ", " without ", " except ", " excluding ", " other than ",
                   ".", ",", ";", "?", "!", " site:", " after:", " before:", " -"];
    // Find the end of the negated phrase.
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

    // Split on " and " / " or " to get individual terms.
    // This intentionally treats both conjunctions the same in constraint lists.
    let combined_parts: Vec<&str> = phrase.split(" and ")
        .flat_map(|part| part.split(" or "))
        .collect();

    if combined_parts.len() > 1 {
        combined_parts.iter()
            .map(|p| {
                let cleaned = strip_neg(p);
                // OR/AND clauses are union/comparison terms. The generic
                // extractor drops a clause that is a *pure* quality adjective
                // (e.g. "best", "free") because quality adjectives are stripped
                // as modifiers — but as an explicit OR operand the word IS the
                // constraint the user wants. So if extraction yields nothing,
                // fall back to the literal clause token (minus the negation
                // prefix) so operands like "best OR worst" both survive.
                let extracted = extract_constraint_term(&cleaned, max_words);
                if extracted.is_empty() {
                    let t = cleaned.trim().to_lowercase();
                    if !t.is_empty() && t != "and" && t != "or" {
                        t
                    } else {
                        String::new()
                    }
                } else {
                    extracted
                }
            })
            .filter(|t| !t.is_empty())
            .collect()
    } else {
        vec![extract_constraint_term(&strip_neg(phrase), max_words)]
    }
}

/// Extract a constraint term from the text after a marker.
/// Takes up to `max_words` words, stops at punctuation, conjunctions, or quality adjectives.
/// For negatives (max_words=1): "not vim" → "vim" (single word only)
/// For positives (max_words=2): "for game engine" → "game engine", "for beginners fast" → "beginners"
fn extract_constraint_term(text: &str, max_words: usize) -> String {
    let mut stop_words: Vec<&str> = vec![
        "and", "or", "but", "the", "a", "an", "is", "are", "in", "on",
        "for", "with", "from", "to", "of", "at", "by", "as", "via",
        "under", "over", "about", "into", "through", "between", "after", "before",
        "during", "since", "until", "above", "below", "per", "up", "down", "out",
        "off", "set", "how", "what", "where", "when", "why", "which", "who",
        "programming", "framework", "library", "language", "tool", "database",
        "server", "client", "application", "app", "software", "system",
        "platform", "service", "tutorial", "guide", "documentation", "docs",
        "2026", "2025", "2024", "2023", "2022", "2021", "2020",
        "privacy", "private", "secure", "security",
        "small", "startup", "startups", "indie",
        "open-source", "opensource", "foss", "floss", "free", "libre",
        "self-hosted", "selfhosted", "offline", "local",
        "lightweight", "minimal", "minimalist",
        "ubuntu", "debian", "linux", "mac", "macos", "windows", "android", "ios",
        "not", "nor", "no", "without", "except", "excluding", "minus", "besides", "instead",
    ];
    // Allow constraints to absorb one fewer stop word after the first term
    // so multi-word constraints like "type safety" aren't broken by "safety".
    if max_words > 1 {
        let extra = [
            "fast","modern","quick","lightweight","simple","easy","powerful",
            "popular","efficient","cheap","free","secure","safe","reliable",
            "scalable","flexible","extensible","portable","robust","minimal",
            "minimalist","production","ready","mature","stable","great",
        ];
        stop_words.extend_from_slice(&extra);
    }
    let quality_adjectives = [
        "fast", "modern", "quick", "lightweight", "simple", "easy", "powerful",
        "popular", "efficient", "cheap", "free", "secure", "safe", "reliable",
        "scalable", "flexible", "extensible", "portable", "robust", "minimal",
        "minimalist", "open-source",
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


// ─── Linear Probe Classifier ──────────────────────────────────────
// Trained via logistic regression on all-MiniLM-L6-v2 embeddings.
// Weights loaded from config/intent_weights.json at startup.

fn linear_classify(
    query_embedding: &[f32],
    weights: &IntentWeights,
) -> (String, f32, std::collections::HashMap<String, f32>) {
    let n_classes = weights.labels.len();
    let dim = weights.weights[0].len();

    let mut logits = weights.bias.clone();
    for c in 0..n_classes {
        for j in 0..dim.min(query_embedding.len()) {
            logits[c] += weights.weights[c][j] * query_embedding[j] as f64;
        }
    }

    let temp = (weights.temperature as f64).max(1.2); // Phase 3: soften softmax (was ~0.5) so peaks stop saturating at 1.0
    let max_logit = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let shifted: Vec<f64> = logits.iter().map(|l| (l - max_logit) / temp).collect();
    let exp_shifted: Vec<f64> = shifted.iter().map(|s| s.exp()).collect();
    let exp_sum: f64 = exp_shifted.iter().sum();
    let probs: Vec<f64> = if exp_sum > 1e-12 {
        exp_shifted.iter().map(|e| e / exp_sum).collect()
    } else {
        vec![1.0 / n_classes as f64; n_classes]
    };

    let mut distribution = std::collections::HashMap::new();
    for (i, label) in weights.labels.iter().enumerate() {
        distribution.insert(label.clone(), probs[i] as f32);
    }

    let mut sorted: Vec<(usize, f64)> = probs.iter().enumerate().map(|(i, p)| (i, *p)).collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let (winner_idx, top1) = sorted[0];
    let top2 = sorted.get(1).map(|(_, p)| *p).unwrap_or(0.0);
    let margin = top1 - top2;
    let intent = weights.labels[winner_idx].clone();

    let conf = &weights.confidence;
    let confidence = (conf.base as f32 + margin as f32 * conf.margin_multiplier as f32).clamp(0.0, 1.0);

    let mut intent = weights.labels[winner_idx].clone();

    // Phase 3: confidence floor fallback. When the model is genuinely
    // uncertain (low confidence), default to informational rather than
    // emitting an over-confident misclassification (e.g. navigational).
    // Raised from 0.35 to 0.55: a navigational that wins at only 0.37 is a
    // weak over-prediction (see audit — "kubernetes ingress tls configuration"
    // was classified navigational @0.374 with how-to 0.24 right behind it).
    if confidence < 0.55 && intent != "informational" {
        intent = "informational".to_string();
    }

    tracing::info!(
        "linear_classify: intent={} (conf={:.3}) margin={:.3} probs=[{}]",
        intent, confidence, margin,
        weights.labels.iter().enumerate().map(|(i, l)| format!("{}={:.3}", l, probs[i])).collect::<Vec<_>>().join(" ")
    );

    (intent, confidence, distribution)
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

/// Phase 2 (B3): detect whether a query contains a brand / proper noun.
/// Used to gate "official <q>" injection in `expand_queries` so that only
/// genuine navigational-brand queries ("github", "openai") get the boost,
/// not over-predicted navigational chatter ("python", "rust", "a").
///
/// Adaptive (no hardcoded brand list): a token counts as brand/proper-noun
/// when it has high character-level entityness (rare bigrams = coined name)
/// OR is capitalized in the ORIGINAL query (proper-noun signal).
fn contains_brand_or_proper_noun(query: &str) -> bool {
    // Capitalization check on the original (un-normalized) query.
    // "OpenAI pricing" → "OpenAI" is capitalized → brand present.
    for tok in query.split_whitespace() {
        if tok.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return true;
        }
    }
    // Character-statistics check on lowercased tokens.
    let ql = query.to_lowercase();
    for tok in ql.split_whitespace() {
        // Skip pure stopwords / very short tokens — they're never brands.
        if tok.len() < 3 { continue; }
        if !tok.chars().all(|c| c.is_alphabetic()) { continue; }
        if token_entityness(tok) >= 0.5 {
            return true;
        }
    }
    false
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


// ─── Regional / Multilingual Junk Normalization (Phase 9) ──────────
// Strips locale tags (en-US, en_GB, fr-FR, pt-BR...), maps "in <large-region>"
// to a geo hint (returned separately), normalizes full-width / curly punctuation
// to ASCII. Keeps the language token itself when it is a real query word
// (e.g. "rust" is not a locale tag).
//
// Returns (cleaned_query, optional_geo_region).

fn normalize_regional_junk(query: &str) -> (String, Option<String>) {
    // Normalize exotic unicode punctuation to ASCII so downstream tokenization
    // behaves consistently (full-width colon, chinese comma, etc.)
    let ascii_norm = query
        .replace('：', ":")
        .replace('，', ", ")
        .replace('、', ", ")
        .replace('｜', " | ")
        .replace('－', "-")
        .replace('—', " - ")
        .replace('’', "'")
        .replace('‘', "'")
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('　', " ");

    // Locale tag regex: `(?i)\b[a-z]{2}[_-][A-Z]{2}\b` e.g. en-US, en_GB, pt-BR
    let locale_re = regex::Regex::new(r"(?i)\b[a-z]{2}[_-][A-Za-z]{2}\b").unwrap();
    let stripped = locale_re.replace_all(&ascii_norm, " ").to_string();

    // "in <region>" → geo. Only treat as geo when the region is a known large
    // region/country, not a random noun.
    let known_regions: &[&str] = &[
        "us", "usa", "united states", "uk", "united kingdom", "england", "scotland",
        "canada", "australia", "india", "germany", "france", "spain", "italy",
        "japan", "china", "brazil", "mexico", "russia", "korea", "europe", "asia",
        "africa", "latin america", "south america", "north america", "eu", "europe",
        "singapore", "indonesia", "philippines", "vietnam", "thailand", "netherlands",
        "ireland", "sweden", "norway", "denmark", "finland", "poland", "portugal",
    ];
    let mut geo: Option<String> = None;
    let mut cleaned = stripped.clone();
    let in_re = regex::Regex::new(r"(?i)\bin\s+([a-z][a-z\s]*?)(?:\s+in\s|\s*$|,|\.)").unwrap();
    if let Some(cap) = in_re.captures(&stripped) {
        let region = cap[1].trim().to_lowercase();
        if known_regions.iter().any(|r| *r == region.as_str()) {
            geo = Some(region);
            cleaned = in_re.replace(&stripped, " ").to_string();
        }
    }

    // Collapse the extra whitespace we may have introduced.
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (cleaned, geo)
}

// ─── Query Normalization (De-stutter, Collapse Repeats) ─────────────
// "how how to to set configure setup redis cluster" → "configure redis cluster"
// Removes duplicate tokens, collapses repeated n-grams, normalizes casing.

fn normalize_query(query: &str) -> String {
    let q = query.trim();
    if q.is_empty() { return q.to_string(); }

    // Step 1: Collapse repeated adjacent tokens
    // "how how to to set" → "how to set"
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let mut deduped: Vec<&str> = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        if i == 0 || tok.to_lowercase() != tokens[i - 1].to_lowercase() {
            deduped.push(*tok);
        }
    }

    // Step 2: Collapse repeated adjacent bigrams
    // "set up set up" → "set up"
    let mut collapsed: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < deduped.len() {
        if i + 3 < deduped.len()
            && deduped[i].to_lowercase() == deduped[i + 2].to_lowercase()
            && deduped[i + 1].to_lowercase() == deduped[i + 3].to_lowercase()
        {
            // Skip the repeated bigram
            collapsed.push(deduped[i]);
            collapsed.push(deduped[i + 1]);
            i += 4; // skip both bigrams (we keep one)
        } else {
            collapsed.push(deduped[i]);
            i += 1;
        }
    }

    // Step 3: Remove filler words that appear multiple times
    // "how to how to configure" → "how to configure"
    let fillers: std::collections::HashSet<&str> = [
        "how", "to", "the", "a", "an", "is", "are", "was", "were",
        "be", "been", "being", "do", "does", "did", "will", "would",
        "could", "should", "can", "may", "might", "shall",
    ].iter().copied().collect();

    let mut seen_filler: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<&str> = Vec::new();
    for tok in collapsed {
        let lower = tok.to_lowercase();
        if fillers.contains(lower.as_str()) {
            if seen_filler.contains(&lower) {
                continue; // skip duplicate filler
            }
            seen_filler.insert(lower);
        }
        result.push(tok);
    }

    result.join(" ")
}

// ─── Query Compression (IDF-Weighted Term Extraction) ───────────────
// For long queries (>8 words), extract the most informative terms.
// Preserves concepts, removes syntax, preserves intent.
// "what monitoring stack should a small startup use for kubernetes
//  microservices running on aws" → "kubernetes monitoring stack aws startup"

fn compress_query(query: &str) -> String {
    compress_query_with_negatives(query, &[])
}

/// Compress a long query to its most informative terms, excluding any
/// negative constraint terms. This is critical because the naive
/// compress_query strips "not" (a stop word) but keeps the terms after
/// "not" — making search engines search FOR the excluded items.
fn strip_negations_from_query(query: &str, negative: &[String]) -> String {
    let mut cleaned_words: Vec<String> = query.split_whitespace().map(|s| s.to_string()).collect();
    let neg_triggers: std::collections::HashSet<String> = [
        "not", "nor", "no", "without", "except", "excluding", "minus", "besides", "other", "than", "but", "-"
    ].iter().map(|s| s.to_string()).collect();

    // Sort negatives by word count descending to remove longer phrases first
    let mut sorted_negatives = negative.to_vec();
    sorted_negatives.sort_by(|a, b| b.split_whitespace().count().cmp(&a.split_whitespace().count()));

    for neg in &sorted_negatives {
        let neg_words: Vec<String> = neg.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        if neg_words.is_empty() {
            continue;
        }

        // Try to find the sequence of neg_words in cleaned_words
        let mut i = 0;
        while i + neg_words.len() <= cleaned_words.len() {
            let matches = neg_words.iter().enumerate().all(|(idx, word)| {
                let qw = cleaned_words[i + idx].to_lowercase();
                let qw_clean: String = qw.chars().filter(|c| c.is_alphanumeric()).collect();
                let w_clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
                qw_clean == w_clean || qw_clean.contains(&w_clean)
            });

            if matches {
                // Look backwards for a negation trigger
                let mut remove_start = i;
                if i > 0 {
                    let prev_word = cleaned_words[i - 1].to_lowercase();
                    let prev_clean: String = prev_word.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
                    if neg_triggers.contains(&prev_clean) || prev_clean == "-" {
                        remove_start = i - 1;
                        // Handle "but not", "other than" etc.
                        if remove_start > 0 {
                            let prev_prev_word = cleaned_words[remove_start - 1].to_lowercase();
                            let prev_prev_clean: String = prev_prev_word.chars().filter(|c| c.is_alphanumeric()).collect();
                            if (prev_prev_clean == "but" && prev_clean == "not")
                                || (prev_prev_clean == "other" && prev_clean == "than")
                            {
                                remove_start = remove_start - 1;
                            }
                        }
                    }
                }
                
                // Remove the range of words
                cleaned_words.drain(remove_start..(i + neg_words.len()));
                // Reset search index
                i = 0;
            } else {
                i += 1;
            }
        }
    }

    cleaned_words.join(" ")
}

/// Compress a long query to its most informative terms, excluding any
/// negative constraint terms. This is critical because the naive
/// compress_query strips "not" (a stop word) but keeps the terms after
/// "not" — making search engines search FOR the excluded items.
fn compress_query_with_negatives(query: &str, negative: &[String]) -> String {
    let cleaned_query = if !negative.is_empty() {
        let stripped = strip_negations_from_query(query, negative);
        if stripped.trim().is_empty() {
            query.to_string()
        } else {
            stripped
        }
    } else {
        query.to_string()
    };

    let words: Vec<&str> = cleaned_query.split_whitespace().collect();
    if words.len() <= 8 {
        return cleaned_query; // short enough, don't compress
    }

    // Stop words — common English words with low information value
    let stop_words: std::collections::HashSet<&str> = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "can", "may", "might", "shall", "must",
        "in", "on", "at", "to", "for", "of", "with", "from", "by", "as",
        "into", "through", "during", "before", "after", "above", "below",
        "between", "out", "off", "over", "under", "again", "further",
        "and", "but", "or", "nor", "not", "so", "yet",
        "i", "me", "my", "we", "our", "you", "your", "he", "she", "it",
        "they", "them", "their", "this", "that", "these", "those",
        "what", "which", "who", "whom", "when", "where", "why", "how",
        "all", "each", "every", "both", "few", "more", "most", "other",
        "some", "such", "no", "only", "own", "same", "than", "too",
        "very", "just", "about", "above", "after", "again", "also",
        "any", "because", "before", "being", "between", "does",
        "during", "each", "few", "from", "further", "get", "got",
        "here", "into", "just", "keep", "like", "make", "many",
        "might", "more", "most", "much", "must", "never", "new",
        "now", "old", "one", "only", "other", "our", "out", "over",
        "own", "part", "put", "same", "see", "shall", "should",
        "since", "still", "take", "than", "that", "their", "them",
        "then", "there", "these", "they", "this", "those", "through",
        "together", "too", "under", "until", "upon", "very", "was",
        "well", "were", "what", "when", "where", "which", "while",
        "who", "whom", "why", "will", "with", "within", "without",
        "would", "yet", "you", "your",
        // Question/filler patterns
        "how", "what", "when", "where", "why", "which", "who",
        "should", "would", "could", "can", "do", "does", "did",
        "use", "using", "used", "getting", "started",
        "need", "want", "looking", "try", "trying", "work", "working",
    ].iter().copied().collect();

    // Technical terms — high information value, boost these
    let tech_terms: std::collections::HashSet<&str> = [
        "api", "sdk", "library", "framework", "crate", "package", "module",
        "price", "pricing", "cost", "cheap", "affordable", "budget", "deal", "deals", "offer", "offers", "discount",
        "function", "method", "class", "interface", "struct", "enum",
        "database", "db", "cache", "queue", "stream", "pipeline",
        "server", "client", "proxy", "load", "balancer", "gateway",
        "container", "docker", "kubernetes", "k8s", "pod", "node",
        "cluster", "microservice", "microservices", "monolith",
        "ci", "cd", "devops", "sre", "observability", "monitoring",
        "logging", "tracing", "metrics", "alerting",
        "authentication", "authorization", "auth", "oauth", "jwt",
        "encryption", "tls", "ssl", "https", "cors", "csrf",
        "rest", "graphql", "grpc", "websocket", "sse",
        "react", "vue", "angular", "svelte", "nextjs", "next.js",
        "nuxt", "remix", "astro", "gatsby",
        "typescript", "javascript", "python", "rust", "go", "golang",
        "java", "kotlin", "swift", "ruby", "php", "c++", "cpp", "c#",
        "elasticsearch", "solr", "lucene", "meilisearch", "typesense",
        "redis", "memcached", "postgres", "postgresql", "mysql",
        "mongodb", "dynamodb", "cassandra", "cockroachdb", "sqlite",
        "kafka", "rabbitmq", "nats", "pulsar",
        "nginx", "apache", "caddy", "traefik", "envoy", "haproxy",
        "terraform", "ansible", "pulumi", "cloudformation",
        "aws", "gcp", "azure", "vercel", "netlify", "fly.io", "railway",
        "linux", "ubuntu", "debian", "alpine", "arch",
        "git", "github", "gitlab", "bitbucket",
        "prometheus", "grafana", "datadog", "newrelic", "sentry",
        "webpack", "vite", "rollup", "esbuild", "turbopack",
        "tailwind", "bootstrap", "css", "html", "dom", "bom",
        "http", "tcp", "udp", "ip", "dns", "dhcp",
        "json", "yaml", "toml", "xml", "csv", "protobuf",
        "regex", "parsing", "token", "ast", "lexer", "compiler",
        "machine", "learning", "ml", "ai", "llm", "neural",
        "vector", "embedding", "transformer", "attention",
    ].iter().copied().collect();

    // Score each token
    let mut scored: Vec<(usize, &str, f32)> = Vec::new(); // (original_index, word, score)
    for (i, word) in words.iter().enumerate() {
        let lower = word.to_lowercase();
        let clean: String = lower.chars().filter(|c| c.is_alphanumeric() || *c == '.' || *c == '+' || *c == '#').collect();

        if clean.is_empty() { continue; }

        let mut score: f32 = if stop_words.contains(clean.as_str()) {
            0.0
        } else {
            1.0
        };

        // Technical term boost: +2.0
        if tech_terms.contains(clean.as_str()) {
            score += 2.0;
        }

        // Entity boost: capitalized words (not at sentence start) or ALL CAPS
        let is_sentence_start = i == 0 || words[i - 1].ends_with('.') || words[i - 1].ends_with('!') || words[i - 1].ends_with('?');
        if !is_sentence_start && word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            score += 1.5; // likely proper noun / entity
        }
        if word.len() >= 2 && word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
            score += 1.5; // acronym (AWS, API, HTTP)
        }

        // Length heuristic: longer words tend to be more specific
        if clean.len() >= 6 {
            score += 0.5;
        }

        // Position bias: earlier terms slightly more important
        let position_boost = 1.0 - (i as f32 / words.len() as f32) * 0.3;
        score *= position_boost;

        if score > 0.0 {
            scored.push((i, *word, score));
        }
    }

    // Sort by score descending, then by position ascending (stable)
    scored.sort_by(|a, b| {
        b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    // Take top N terms (10-14 depending on query length)
    // 8 was too aggressive — dropping terms like "self" from "self hosted" hurts SearXNG matching
    let max_terms = if words.len() > 20 { 14 } else { 10 };
    let mut selected: Vec<(usize, &str)> = scored.iter()
        .take(max_terms)
        .map(|(i, w, _)| (*i, *w))
        .collect();

    // Restore original order for readability
    selected.sort_by_key(|(i, _)| *i);

    let compressed = selected.iter().map(|(_, w)| *w).collect::<Vec<&str>>().join(" ");

    // If compression produced something reasonable, use it, otherwise fall back to cleaned_query
    if !compressed.trim().is_empty() {
        compressed
    } else {
        cleaned_query
    }
}

// ─── Query Expansion (Dynamic, Not Hardcoded) ────────────────────────

/// Function words that carry no standalone search signal. When they end up at
/// the start or end of a generated phrase — usually because their object token
/// was stripped upstream (e.g. a numeric price, a stop word) — they leave a
/// dangling preposition/article/conjunction that pollutes the query
/// ("cheap used cars under tutorial"). This set is used to trim those ends.
/// It is intentionally small and closed-class (English function words), so it
/// generalises to any query rather than any specific one.
fn is_dangling_function_word(w: &str) -> bool {
    matches!(w,
        "a" | "an" | "the" | "to" | "for" | "in" | "on" | "of" | "and" | "or"
        | "with" | "from" | "by" | "under" | "over" | "about" | "into" | "than"
        | "as" | "at" | "but" | "nor" | "so" | "vs" | "versus" | "up" | "off"
        | "out" | "per" | "via" | "onto" | "upon"
    )
}

/// Collapse consecutive duplicate tokens (case-insensitive). Fixes artifacts
/// like "step step" (from stripping "by" out of "step by step") and
/// "2026 2026" (from appending a year that is already present). Only ADJACENT
/// duplicates are collapsed so legitimate repeats ("new york new york") in
/// distant positions are preserved.
fn collapse_adjacent_dups(phrase: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for w in phrase.split_whitespace() {
        if out.last().map(|p: &&str| p.eq_ignore_ascii_case(w)).unwrap_or(false) {
            continue;
        }
        out.push(w);
    }
    out.join(" ")
}

/// Trim leading and trailing dangling function words from a token slice.
/// Interior function words are preserved (they connect real terms).
fn trim_dangling<'a>(words: &[&'a str]) -> Vec<&'a str> {
    let mut start = 0usize;
    let mut end = words.len();
    while start < end && is_dangling_function_word(&words[start].to_lowercase()) {
        start += 1;
    }
    while end > start && is_dangling_function_word(&words[end - 1].to_lowercase()) {
        end -= 1;
    }
    words[start..end].to_vec()
}

/// Normalise a generated expansion: collapse adjacent duplicates, then trim
/// dangling function words from both ends. Applied to every expansion so the
/// fix is general rather than case-specific.
fn sanitize_expansion(phrase: &str) -> String {
    let collapsed = collapse_adjacent_dups(phrase);
    let words: Vec<&str> = collapsed.split_whitespace().collect();
    trim_dangling(&words).join(" ")
}

/// Bare ambiguous programming-language names that collide with a common English
/// word or game title (e.g. "rust" → survival game, "go" → verb, "java" → island).
/// When such a name appears as a standalone comparison entity without
/// disambiguating context in the original query, we append the suffix so the
/// generated search expansions find the language, not the game/word.
/// Data-driven and shared in spirit with the gateway's LANGUAGE_DISAMBIGUATION.
fn ambiguous_lang_suffix(entity: &str) -> Option<&'static str> {
    let table: &[(&str, &str)] = &[
        ("go", " programming"),
        ("rust", " programming"),
        ("ruby", " programming"),
        ("swift", " programming"),
        ("java", " programming"),
        ("c", " programming"),
    ];
    let e = entity.trim().to_lowercase();
    table.iter().find(|(name, _)| *name == e).map(|(_, s)| *s)
}

/// If `entity` is a bare ambiguous language name that the original query has not
/// already disambiguated, return its disambiguated form (e.g. "rust programming").
/// Otherwise return the entity unchanged. Used by comparison expansion so
/// entities like "rust"/"go" resolve to the language rather than a game.
fn disambiguate_comparison_entity(entity: &str, q_lower: &str) -> String {
    let e_trim = entity.trim();
    if let Some(suffix) = ambiguous_lang_suffix(e_trim) {
        // Skip if the user already provided explicit disambiguation.
        if q_lower.contains(&format!("{} programming", e_trim))
            || q_lower.contains(&format!("{} language", e_trim))
            || q_lower.contains(&format!("{} tutorial", e_trim))
            || q_lower.contains(&format!("{} guide", e_trim))
            || q_lower.contains(&format!("{} framework", e_trim))
        {
            return e_trim.to_string();
        }
        return format!("{}{}", e_trim, suffix);
    }
    e_trim.to_string()
}

fn expand_queries(query: &str, intent: &str, confidence: f32, brand_hint: bool, constraints: &Constraints) -> Vec<String> {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    let mut expansions = vec![q.to_string()]; // always include original

    // A word is positive if it is present in the input query (which is already negation-stripped)
    let is_positive = |w: &str| -> bool {
        let w_clean: String = w.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        words.iter().any(|qw| {
            let qw_clean: String = qw.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
            qw_clean == w_clean
        })
    };

    // Build set of negative constraint terms to exclude from expansions
    // "not django" should NOT generate "django documentation" as an expansion
    let neg_set: std::collections::HashSet<String> = constraints.negative.iter()
        .map(|n| n.to_lowercase())
        .collect();
    // NOTE: "alternative", "alternatives" are NOT negative triggers.
    // "alternative to X" means find things LIKE X — the Reference entity handles this.
    let neg_triggers: std::collections::HashSet<&str> = [
        "not", "no", "without", "except", "excluding", "but", "minus",
        "other", "than", "instead",
    ].iter().copied().collect();

    // ── Query Graph IR: Extract Reference entities ──
    // Reference entities are things like "notion" in "alternative to notion".
    // They need special expansion: "apps like X", "X competitor", "X replacement".
    let reference_entities: Vec<&QueryEntity> = constraints.entities.iter()
        .filter(|e| e.role == EntityRole::Reference)
        .collect();

    // If we have Reference entities, generate similarity-based expansions
    if !reference_entities.is_empty() {
        for entity in &reference_entities {
            let ref_text = entity.text.to_lowercase();
            // "apps like notion"
            expansions.push(format!("apps like {}", ref_text));
            // "notion competitor"
            expansions.push(format!("{} competitor", ref_text));
            // "notion alternative open source"
            expansions.push(format!("{} alternative open source", ref_text));
            // "self-hosted notion replacement"
            expansions.push(format!("self-hosted {} replacement", ref_text));
            // "free alternative to notion"
            expansions.push(format!("free alternative to {}", ref_text));
        }
    }

    let core = extract_core_topic(&q_lower, intent);
    let core_trimmed = core.trim();

    match intent {
        "how-to" => {
            if core_trimmed.len() > 3 {
                // Filter stop words to get meaningful topic words for variations
                let howto_stop = ["a","an","the","to","for","in","on","of","and","or","is","are","with","from","by"];
                let mut seen_topic = std::collections::HashSet::new();
                let topic_words: Vec<&str> = core_trimmed.split_whitespace()
                    .filter(|w| w.len() > 1 && !howto_stop.contains(w) && !w.parse::<f64>().is_ok())
                    .filter(|w| {
                        let w_lower = w.to_lowercase();
                        let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                        (!neg_set.contains(w_stripped) || is_positive(w_stripped))
                            && (!neg_set.contains(&w_lower) || is_positive(&w_lower))
                            && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    })
                    .filter(|w| seen_topic.insert(w.to_lowercase()))
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
                let parts: Vec<&str> = replaced.split(" vs ").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                if parts.len() == 2 {
                    let (a, b) = (parts[0], parts[1]);
                    let a_d = disambiguate_comparison_entity(a, &q_lower);
                    let b_d = disambiguate_comparison_entity(b, &q_lower);
                    expansions.push(format!("{} {} comparison", a_d, b_d));
                    expansions.push(format!("{} compared to {}", a_d, b_d));
                    if let Some(year) = extract_year(&q_lower) {
                        expansions.push(format!("{} vs {} {}", a_d, b_d, year));
                    }
                } else if parts.len() >= 3 {
                    // N-way comparison ("openai vs anthropic vs google ai models",
                    // "rust vs go vs python performance comparison guide").
                    // The last part carries trailing context words (e.g.
                    // "google ai models") — split its first token as the final
                    // entity and keep the remainder as shared context so every
                    // generated variation stays on-topic.
                    let last = *parts.last().unwrap();
                    let mut last_words = last.split_whitespace();
                    let last_entity = last_words.next().unwrap_or(last);
                    let context: String = last_words.collect::<Vec<&str>>().join(" ");

                    // Build the clean entity list: all middle parts are single
                    // entities; the first token of the last part is the final one.
                    let mut entities: Vec<String> = parts[..parts.len() - 1].iter()
                        .map(|e| disambiguate_comparison_entity(e, &q_lower))
                        .collect();
                    entities.push(disambiguate_comparison_entity(last_entity, &q_lower));

                    let ctx = |base: String| -> String {
                        if context.is_empty() { base } else { format!("{} {}", base, context) }
                    };

                    // Whole-set comparison variations.
                    let joined = entities.join(" ");
                    expansions.push(ctx(format!("{} comparison", joined)));
                    if let Some(year) = extract_year(&q_lower) {
                        expansions.push(ctx(format!("{} comparison {}", joined, year)));
                    }
                    // Adjacent-pair variations so search engines can find the
                    // pairwise comparison articles that dominate this query class.
                    for pair in entities.windows(2) {
                        expansions.push(ctx(format!("{} vs {}", pair[0], pair[1])));
                    }
                    // First-vs-each variations to surface head-to-head content
                    // that skips the middle entity ("openai vs google").
                    if entities.len() >= 3 {
                        for other in &entities[2..] {
                            expansions.push(ctx(format!("{} vs {}", entities[0], other)));
                        }
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
            let mut seen_topic = std::collections::HashSet::new();
            let topic_words: Vec<&str> = words.iter()
                .filter(|w| w.len() > 2 && !stop.contains(w))
                .filter(|w| {
                    // Filter out negative constraint terms from expansions
                    let w_lower = w.to_lowercase();
                    let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                    (!neg_set.contains(w_stripped) || is_positive(w_stripped))
                        && (!neg_set.contains(&w_lower) || is_positive(&w_lower))
                        && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                })
                // Dedup to avoid "orm orm" when a word appears before and after a negated term
                .filter(|w| seen_topic.insert(w.to_lowercase()))
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
                expansions.push(format!("what is {}", core_trimmed));
                expansions.push(format!("{} for beginners", core_trimmed));
                expansions.push(format!("{} examples", core_trimmed));
                expansions.push(format!("learn {}", core_trimmed));
                expansions.push(format!("{} best resources", core_trimmed));
                expansions.push(core_trimmed.to_string());
            }
        }
        "fresh" => {
            let temporal = ["latest","recent","newest","today","this week","this month","new"];
            let year = extract_year(&q_lower).unwrap_or("2026");
            let mut core_words: Vec<&str> = Vec::new();
            for w in &words {
                if !temporal.iter().any(|t| *t == *w) && w.len() >= 2 {
                    // Skip year tokens to avoid "2026 2026" duplication
                    if w.len() == 4 && w.starts_with("20") && w.parse::<u32>().ok().map_or(false, |y| (2020..=2029).contains(&y)) {
                        continue;
                    }
                    let w_lower = w.to_lowercase();
                    let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                    if (!neg_set.contains(w_stripped) || is_positive(w_stripped))
                        && (!neg_set.contains(&w_lower) || is_positive(&w_lower))
                        && !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    {
                        core_words.push(w);
                    }
                }
            }
            if !core_words.is_empty() {
                let core_str = core_words.join(" ");
                expansions.push(format!("{} {}", core_str, year));
                expansions.push(format!("{} update", core_str));
                if core_str.contains("release") || core_str.contains("version") {
                    expansions.push(format!("{} changelog", core_str));
                }
            }
        }
        "navigational" => {
            // Phase 2 (B3): only inject "official <q>" when the query is a
            // high-confidence navigational AND contains a brand/proper noun.
            // Over-predicted navigational (chitchat, "python tutorial", "a")
            // must NOT get "official" prepended — it pollutes results.
            let already_official = q_lower.contains("official");
            if !already_official && confidence >= 0.80 && brand_hint {
                expansions.push(format!("official {}", q));
            }
        }
        "local" => {
            // For local intent, generate queries that work well for geo-aware search.
            // Preserve "near me" patterns if present, otherwise focus on the core topic.
            let without_near = q_lower
                .strip_suffix("near me").or_else(|| q_lower.strip_suffix("near me?"))
                .or_else(|| q_lower.strip_suffix("nearby"))
                .or_else(|| q_lower.strip_suffix(" open now"))
                .or_else(|| q_lower.strip_suffix(" tonight"))
                .or_else(|| q_lower.strip_suffix(" today"))
                .or_else(|| q_lower.strip_suffix("this weekend"))
                .map(|s| s.trim())
                .unwrap_or(&q_lower);
            if !without_near.is_empty() && without_near != q_lower {
                expansions.push(without_near.to_string());
            }
            // Add directional variants
            if q_lower.contains(" near ") || q_lower.contains("nearby") {
                expansions.push(format!("{} nearby", without_near));
            }
            // Extract "in <place>" and add location-focused variant
            if let Some(in_idx) = q_lower.rfind(" in ") {
                let place = q_lower[in_idx + 4..].trim();
                let topic = q_lower[..in_idx].trim();
                if !place.is_empty() && !topic.is_empty() {
                    expansions.push(format!("{} near {}", topic, place));
                    expansions.push(format!("{} {}", topic, place));
                }
            }
        }
        _ => {}
    }

    // ── Negative-aware "alternatives" expansion ──
    // When the query has negative constraints (e.g., "not django not sqlalchemy"),
    // generate an expansion that excludes both negation triggers AND the negated terms,
    // then frames as "alternatives". Without this, the gateway bypass strips only
    // trigger words and passes excluded terms as positive search signals.
    //
    // ALSO: when negative constraints dominate, add a stripped version of the
    // ORIGINAL query (without any negated terms) as an expansion so search engines
    // can actually find relevant results instead of searching for excluded items.
    if !constraints.negative.is_empty() && reference_entities.is_empty() {
        // Always add the negation-stripped original as the FIRST expansion after
        // the original query. This ensures search engines search for what the user
        // actually wants, not the things they want to exclude.
        let neg_set_lower: std::collections::HashSet<String> = constraints.negative.iter()
            .flat_map(|n| n.to_lowercase().split_whitespace()
                .map(|w| w.to_string())
                .collect::<Vec<_>>())
            .collect();
        let stripped_core: Vec<&str> = words.iter()
            .filter(|w| {
                let w_lower = w.to_lowercase();
                let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                (!neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str()))
                    && (!neg_set.contains(w_stripped) || is_positive(w_stripped))
                    && (!neg_set.contains(&w_lower) || is_positive(&w_lower))
                    && (!neg_set_lower.contains(w_stripped) || is_positive(w_stripped))
                    && (!neg_set_lower.contains(&w_lower) || is_positive(&w_lower))
            })
            .copied()
            .collect();
        if !stripped_core.is_empty() {
            let stripped_query = stripped_core.join(" ");
            if stripped_query != q && !expansions.contains(&stripped_query) {
                // Insert right after the original so it's the primary fallback
                expansions.insert(1, stripped_query.clone());
            }
        }
    }

    // ── Negative-aware "alternatives" expansion ──
    if !constraints.negative.is_empty() && reference_entities.is_empty() {
        let neg_word_set: std::collections::HashSet<&str> = constraints.negative.iter()
            .flat_map(|n| n.split_whitespace())
            .collect();
        let mut seen_alt = std::collections::HashSet::new();
        let alt_words: Vec<&str> = words.iter()
            .filter(|w| w.len() > 1)
            .filter(|w| {
                let w_lower = w.to_lowercase();
                let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    && (!neg_set.contains(w_stripped) || is_positive(w_stripped))
                    && (!neg_set.contains(&w_lower) || is_positive(&w_lower))
                    && (!neg_word_set.contains(w_stripped) || is_positive(w_stripped))
                    && (!neg_word_set.contains(w_lower.as_str()) || is_positive(w_lower.as_str()))
            })
            .filter(|w| seen_alt.insert(w.to_lowercase()))
            .copied()
            .collect();
        if !alt_words.is_empty() {
            let alt_text = alt_words.join(" ");
            // Check if any existing expansion already conveys "alternatives"
            let has_alt_concept = expansions.iter().any(|e| {
                let e_low = e.to_lowercase();
                e_low.contains("alternative") || e_low.contains("instead of")
                    || e_low.contains("replacement") || e_low.contains("competitor")
            });
            if !has_alt_concept {
                expansions.push(format!("{} alternatives", alt_text));
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for exp in expansions {
        // Sanitize every expansion: collapse adjacent duplicate tokens
        // ("step step", "2026 2026") and trim dangling function words at the
        // ends ("cheap used cars under" → "cheap used cars"). This is applied
        // uniformly so no individual expansion branch can emit a malformed query.
        let cleaned = sanitize_expansion(&exp);
        let cleaned = if cleaned.trim().is_empty() { exp } else { cleaned };
        let key = cleaned.to_lowercase();
        if seen.insert(key) {
            unique.push(cleaned);
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

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // ── Load linear probe weights ──
    let config_path = "./config/intent_weights.json";
    tracing::info!("Loading linear probe weights from {}...", config_path);
    let config: IntentWeights = serde_json::from_reader(
        std::fs::File::open(config_path)
            .map_err(|e| anyhow::anyhow!("Failed to open weights {}: {}", config_path, e))?
    ).map_err(|e| anyhow::anyhow!("Failed to parse weights {}: {}", config_path, e))?;
    CONFIG.set(config)
        .map_err(|_| anyhow::anyhow!("CONFIG already initialized"))?;
    tracing::info!("Linear probe weights loaded successfully");

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

    let intent_cache = Cache::builder()
        .max_capacity(2000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let embed_cache = Cache::builder()
        .max_capacity(2000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let state = Arc::new(AppState {
        bert_model: Arc::new(bert_model),
        bert_tokenizer,
        device,
        intent_cache,
        embed_cache,
        // Concurrency for the CPU-bound BERT forward pass. The model is
        // immutable (`forward` takes &self), so there is no shared-mutable
        // state to serialize — this semaphore alone bounds true parallelism.
        // Tunable via INTENT_MAX_CONCURRENCY (default 4, matching the
        // RAYON_NUM_THREADS budget). The previous Semaphore::new(2) throttled
        // inference so hard that burst traffic queued past the gateway's
        // intent budget, surfacing as "Intent Engine unreachable" blips.
        bert_semaphore: Semaphore::new(
            std::env::var("INTENT_MAX_CONCURRENCY")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|&n| n >= 1 && n <= 16)
                .unwrap_or(4),
        ),
    });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/analyze", get(analyze_query))
        .route("/embed", get(embed_text))
        .route("/embed_batch", post(embed_batch))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3005));
    tracing::info!("Intent Engine listening on {} (linear probe + constraint extraction)", addr);

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

    // ── Step 1: Normalize query ──
    // Phase 9: strip regional/multilingual junk (locale tags like en-US,
    // full-width punctuation) and pull a geo region out of "in <region>".
    let (regional_cleaned, _geo) = normalize_regional_junk(&params.q);
    if _geo.is_some() || regional_cleaned.trim() != params.q.trim() {
        tracing::info!("Regional junk normalized: {:?} → {:?} (geo={:?})",
            params.q.trim(), regional_cleaned, _geo);
    }
    // "how how to to set configure setup redis cluster" → "how to configure redis cluster"
    let normalized = normalize_query(&regional_cleaned);
    if normalized != params.q.trim() {
        tracing::info!("Query normalized: {:?} → {:?}", params.q.trim(), normalized);
    }

    // Extract constraints from the NORMALIZED query
    let structured = extract_constraints(&normalized);
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

    // ── Get embedding for linear probe (offloaded to blocking thread pool) ──
    let query_embedding = {
        let _permit = state.bert_semaphore.acquire().await.unwrap();
        let state_clone = state.clone();
        let query_text = params.q.clone();
        tokio::task::spawn_blocking(move || {
            compute_embedding(&state_clone.device, &state_clone.bert_model, &state_clone.bert_tokenizer, &query_text)
        }).await.unwrap_or(None)
    };

    // ── Linear probe classification ──
    // Uses logistic regression weights trained on calibration_benchmark_200.csv
    let weights = CONFIG.get().expect("weights not loaded");
    let (intent, confidence, distribution) = query_embedding.as_ref()
        .map(|emb| linear_classify(emb, weights))
        .unwrap_or_else(|| {
            let mut d = std::collections::HashMap::new();
            d.insert("informational".to_string(), 1.0);
            ("informational".to_string(), 0.3, d)
        });

    // ── Lexical intent overrides (BUG #4 + ISSUE #7) ──
    // The ML linear probe is strong but occasionally misranks clear lexical
    // signals (e.g. "react vs vue vs angular" was classified navigational at
    // 0.877; "how to deploy a docker container" got only 0.272 confidence).
    // When a high-precision lexical marker is present we override/boost the
    // model so the API contract holds: "vs" ⇒ comparison, "how to" ⇒ how-to.
    let mut intent = intent;
    let mut confidence = confidence;
    let ql = normalized.to_lowercase();
    let has_vs = ql.contains(" vs ") || ql.contains(" versus ")
        || ql.starts_with("vs ") || ql.starts_with("versus ");
    // At least two distinct candidate entities separated by "or" / "vs"
    // (e.g. "react or vue", "x vs y") ⇒ comparison, not informational/navigational.
    let has_or_compare = ql.contains(" or ") && ql.split(" or ").count() >= 2;
    if has_vs || has_or_compare {
        if intent != "comparison" {
            tracing::info!("Lexical override: 'vs'/'or' marker ⇒ comparison (was {})", intent);
            intent = "comparison".to_string();
            // High confidence: lexical comparison markers are unambiguous.
            confidence = confidence.max(0.9);
        }
    }
    // "how to" / "how do i" / "how can i" / "how do you" ⇒ how-to.
    // Raise confidence so it isn't misranked behind informational/navigational.
    let howto_markers = ["how to ", "how do i ", "how do you ", "how can i ",
                         "how can i ", "how to", "steps to ", "tutorial for "];
    let is_howto = howto_markers.iter().any(|m| ql.contains(m))
        || ql.starts_with("how to") || ql.starts_with("how do");
    if is_howto && intent == "how-to" {
        // Boost weak how-to confidence to a more usable level.
        confidence = confidence.max(0.6);
        tracing::info!("Lexical boost: how-to confidence raised to {:.3}", confidence);
    } else if is_howto && intent != "how-to" {
        // Model missed the how-to signal — override when the lexical marker is clear.
        if ql.contains("how to") || ql.starts_with("how do") || ql.contains("steps to ") {
            tracing::info!("Lexical override: how-to marker ⇒ how-to (was {})", intent);
            intent = "how-to".to_string();
            confidence = confidence.max(0.6);
        }
    }

    // Phase 3: chitchat lexical override. The linear probe over-predicts
    // navigational for short social utterances ("how are you", "tell me a
    // joke", "who are you"). Clear conversational markers must pin chitchat
    // so they don't get "official" injection or a navigational half-life.
    let chitchat_markers = [
        "how are you", "how r you", "how you doing", "how's it going", "hows it going",
        "tell me a joke", "tell me joke", "a joke", "who are you", "who r you",
        "what are you", "are you real", "are you human", "good morning", "good evening",
        "good afternoon", "good night", "thank you", "thanks", "hey", "hi there",
        "hello there", "what's up", "whats up", "nice to meet you", "can you help",
        "make me laugh", "sing a song", "what is your name", "whats your name",
        "meaning of life", "purpose of life", "why are we here", "why do we exist",
        "what is the meaning", "point of life",
    ];
    let is_chitchat = chitchat_markers.iter().any(|m| ql.contains(m))
        || ql.trim() == "hi" || ql.trim() == "hello" || ql.trim() == "hey" || ql.trim() == "yo"
        || ql.trim() == "sup" || ql.trim() == "thanks" || ql.trim() == "thank you";
    if is_chitchat {
        if intent != "chitchat" {
            tracing::info!("Lexical override: chitchat marker ⇒ chitchat (was {})", intent);
            intent = "chitchat".to_string();
            confidence = confidence.max(0.7);
        }
    }

    // Phase 4: temporal/freshness override. Queries like "latest ai news 2026"
    // or "recent rust releases" carry an explicit recency signal. The linear
    // probe never emits "fresh", so we force it here so the gateway applies a
    // short (6h) freshness half-life. Requires a temporal marker AND a
    // non-temporal topic token to avoid matching "what's new in python".
    let temporal_markers = ["latest", "recent", "newest", "this week", "this month",
                            "past week", "past month", "last week", "last month",
                            "today", "2026 news", "2025 news", "new release",
                            "new version", "current", "upcoming"];
    let has_temporal = temporal_markers.iter().any(|t| ql.contains(t))
        || ql.contains("news 202") || ql.contains("releases 202");
    let topic_token_count = ql.split_whitespace()
        .filter(|w| w.len() >= 3 && !temporal_markers.contains(w)
                && !["latest","recent","newest","today","this","week","month","past","last","current","new"].contains(w)
                && !["the","for","in","on","of","a","an","and","with"].contains(w))
        .count();
    if has_temporal && topic_token_count >= 1 && intent != "fresh" {
        tracing::info!("Temporal override: recency marker + topic ⇒ fresh (was {})", intent);
        intent = "fresh".to_string();
        confidence = confidence.max(0.7);
    }

    // Phase 3b: technical / documentation override. Developer doc queries like
    // "kubernetes ingress tls configuration" or "python asyncio event loop
    // explained" were being classified navigational at low confidence (0.37)
    // because a title-case token (e.g. "Kubernetes", "Python") trips the
    // brand/proper-noun navigational signal. A doc/reference intent should win
    // when a technical topic token appears in a multi-token doc phrase (a bare
    // single brand like "docker" alone stays navigational — that's correct).
    // High-precision lexical override, mirrors the how-to/comparison overrides.
    let tech_markers = [
        "docs", "doc", "api", "reference", "configure", "configuration",
        "config", "tls", "ssl", "setup", "install", "tutorial", "guide",
        "explained", "example", "examples", "implementation", "architecture",
        "spec", "specification", "cli", "sdk", "library", "framework", "syntax",
        "async", "event loop", "ingress", "handler", "middleware", "deployment",
        "pooling", "module", "pattern", "patterns", "cluster", "index", "query",
        "schema", "migration", "cache", "benchmark", "benchmarking", "networking",
        "connection", "authentication", "authorization", "routing", "logging",
        "metrics", "pipeline", "state", "thread", "process", "memory", "build",
        "compile", "debug", "test", "testing", "deploy", "scale", "scaling",
    ];
    let has_tech_marker = tech_markers.iter().any(|m| ql.contains(m));
    // Known code/infra tokens: a bare single one may be brand-navigational
    // (e.g. "docker"), but combined with other tokens it's a doc query.
    const CODE_TOKENS: &[&str] = &[
        "kubernetes", "docker", "nginx", "postgresql", "postgres", "redis",
        "terraform", "kafka", "rabbitmq", "graphql", "grpc", "rust", "python",
        "golang", "typescript", "javascript", "react", "vue", "node", "nodejs",
        "elasticsearch", "mongodb", "mysql", "sqlite", "cassandra", "prometheus",
        "grafana", "etcd", "consul", "vault", "aws", "gcp", "azure", "linux",
        "windows", "macos", "kotlin", "swift", "java", "scala", "ruby", "php",
        "django", "flask", "fastapi", "spring", "express", "rails",
    ];
    let code_token_count = ql.split_whitespace().filter(|w| {
        let wl = w.trim_end_matches('s');
        CODE_TOKENS.contains(&wl) || CODE_TOKENS.contains(&w)
    }).count();
    let token_count = ql.split_whitespace().count();
    // Fire when a code token is present AND either (a) a doc marker word is
    // present, or (b) it's a multi-token phrase (>=2 tokens) — i.e. not a bare
    // brand search. This flips "kubernetes ingress tls configuration",
    // "postgresql connection pooling", "terraform aws vpc module",
    // "redis pub sub patterns" to technical while leaving "docker" navigational.
    let tech_trigger = code_token_count >= 1 && (has_tech_marker || token_count >= 2);
    if tech_trigger && intent != "technical" {
        tracing::info!("Lexical override: technical marker ⇒ technical (was {})", intent);
        intent = "technical".to_string();
        confidence = confidence.max(0.6);
    }

    // ── Step 2: Compress long queries before expansion ──

    // "what monitoring stack should a small startup use for kubernetes
    //  microservices running on aws" → "kubernetes monitoring stack aws startup"
    // Uses negative-aware compression so excluded terms ("not chrome")
    // don't leak into the compressed query and pollute search results.
    let expansion_input = compress_query_with_negatives(&normalized, &structured.negative);
    if expansion_input != normalized {
        tracing::info!("Query compressed: {:?} → {:?} (negation-aware)", normalized, expansion_input);
    }

    let expanded = expand_queries(&expansion_input, &intent, confidence, contains_brand_or_proper_noun(&params.q), &structured);
    tracing::info!("Expanded to {} query variations", expanded.len());

    let result = IntentResponse {
        query: params.q.clone(),
        intent,
        confidence,
        constraints: flat_constraints,
        structured_constraints: structured,
        expanded_queries: expanded,
        distribution,
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
        let _permit = state.bert_semaphore.acquire().await.unwrap();
        let state_clone = state.clone();
        let text = params.text.clone();
        tokio::task::spawn_blocking(move || {
            compute_embedding(&state_clone.device, &state_clone.bert_model, &state_clone.bert_tokenizer, &text)
                .unwrap_or_else(|| vec![0.0; 384])
        }).await.unwrap_or_else(|_| vec![0.0; 384])
    };

    state.embed_cache.insert(text_norm, embedding_vec.clone()).await;
    Json(EmbedResponse {
        embedding: embedding_vec,
    })
}

/// Batch embedding endpoint. Accepts a JSON list of texts and returns the
/// matching list of embeddings in a SINGLE model pass per text (cached). This
/// lets the gateway embed many web-result snippets with ONE round-trip instead
/// of N calls to /embed, so web-result semantic scoring can reuse the same
/// deployed MiniLM model that already powers intent classification and the
/// local index. Fail-closed: any text that fails to embed returns a zero
/// vector (cosine 0) so a partial failure never poisons the whole batch.
#[derive(Deserialize)]
pub struct EmbedBatchRequest {
    pub texts: Vec<String>,
}

#[derive(Serialize)]
pub struct EmbedBatchResponse {
    pub embeddings: Vec<Vec<f32>>,
}

async fn embed_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmbedBatchRequest>,
) -> Json<EmbedBatchResponse> {
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(req.texts.len());
    for text in req.texts {
        let text_norm = text.trim().to_lowercase();
        let vec = if let Some(cached) = state.embed_cache.get(&text_norm).await {
            cached
        } else {
            let _permit = state.bert_semaphore.acquire().await.unwrap();
            let state_clone = state.clone();
            let t = text.clone();
            let computed = tokio::task::spawn_blocking(move || {
                compute_embedding(&state_clone.device, &state_clone.bert_model, &state_clone.bert_tokenizer, &t)
                    .unwrap_or_else(|| vec![0.0; 384])
            }).await.unwrap_or_else(|_| vec![0.0; 384]);
            state.embed_cache.insert(text_norm, computed.clone()).await;
            computed
        };
        embeddings.push(vec);
    }
    Json(EmbedBatchResponse { embeddings })
}
