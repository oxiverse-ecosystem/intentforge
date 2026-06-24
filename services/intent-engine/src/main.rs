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
    pub bert_model: Mutex<BertModel>,
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

fn extract_constraints(query: &str) -> Constraints {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let mut positive = Vec::new();
    let mut negative: Vec<String> = Vec::new();

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
        " similar to ", " like ", " comparable to ",
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

    // Process start-of-string markers first (negatives)
    for marker in &negative_start_markers {
        if q_lower.starts_with(marker) {
            let remaining = &q[marker.len()..];
            // Preserve phrases if possible for negatives.
            let term = extract_constraint_term(remaining, 2);
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

        let neg_set: std::collections::HashSet<String> = negative.iter().cloned().collect();
        let pos_set: std::collections::HashSet<String> = positive.iter().cloned().collect();

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
            if stop_words.contains(w_clean.as_str()) { continue; }
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

    let language = detect_query_language(&q_lower);

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

    Constraints { positive, negative, entities, language }
}

/// Detect programming language mentioned in a query.
/// Returns the canonical language name if found, None otherwise.
///
/// Uses context-aware matching:
/// - Long language names (>= 4 chars): exact word match is sufficient
/// - Short names (go, r, c): require context clues like "X framework", "X library",
///   "in X", "for X", "X programming", or disambiguation like "golang"
fn detect_query_language(q_lower: &str) -> Option<String> {
    let words: Vec<&str> = q_lower.split_whitespace().collect();

    // Canonical language name → (aliases, min_confidence_without_context)
    // Short names need context; long names are self-disambiguating.
    let languages: &[(&str, &[&str])] = &[
        ("go", &["go", "golang"]),
        ("rust", &["rust"]),
        ("python", &["python", "python3"]),
        ("javascript", &["javascript", "js"]),
        ("typescript", &["typescript", "ts"]),
        ("java", &["java"]),
        ("c++", &["c++", "cpp"]),
        ("c#", &["c#", "csharp"]),
        ("ruby", &["ruby"]),
        ("php", &["php"]),
        ("swift", &["swift"]),
        // Disambiguation for "swift" when it appears in storage context:
        // OpenStack Swift (object storage) is NOT the Swift programming language.
        // Storage context clues: ring, container, object storage, cluster, proxy, account
        // These are checked in detect_query_language via additional context logic below.
        ("kotlin", &["kotlin"]),
        ("scala", &["scala"]),
        ("haskell", &["haskell"]),
        ("elixir", &["elixir"]),
        ("clojure", &["clojure"]),
        ("r", &["r"]),
        ("lua", &["lua"]),
        ("perl", &["perl"]),
        ("dart", &["dart"]),
        ("zig", &["zig"]),
        ("nim", &["nim"]),
        ("ocaml", &["ocaml"]),
        ("erlang", &["erlang"]),
        ("fortran", &["fortran"]),
        ("cobol", &["cobol"]),
        ("assembly", &["assembly", "asm"]),
    ];

    // Context clues that confirm a word is a language reference
    let context_clues = [
        "framework", "library", "package", "module", "crate", "tutorial",
        "guide", "documentation", "docs", "programming", "developer",
        "async", "http", "api", "server", "cli", "web", "backend", "frontend",
        "install", "setup", "configure", "build", "compile", "run",
        "performance", "benchmark", "vs", "versus", "alternative",
        "best", "top", "learn", "course", "book", "example",
        "syntax", "error", "debug", "test", "deploy", "migrate",
        "database", "orm", "template", "parser", "regex",
    ];

    for &(canonical, aliases) in languages {
        for alias in aliases {
            // Long names (>= 4 chars): exact word match is sufficient
            if alias.len() >= 4 {
                if words.iter().any(|w| *w == *alias) {
                    // Disambiguation: "swift" in OpenStack storage context is NOT a programming language.
                    // Check for storage-related terms that indicate OpenStack Swift object storage.
                    if *alias == "swift" {
                        let storage_context = ["ring", "container", "object", "storage", "cluster",
                            "proxy", "account", "tenant", "replication", "consistency",
                            "openstack", "keystone", "glance", "nova", "cinder", "horizon",
                            "swiftstack", "mid-range", "midrange", "block", "backup",
                            "availability zone", "storage policy", "object-store"];
                        if storage_context.iter().any(|sc| q_lower.contains(sc)) {
                            return None; // This is OpenStack Swift, not the programming language
                        }
                    }
                    return Some(canonical.to_string());
                }
                continue;
            }

            // Short names (< 4 chars): need context clues OR other languages present
            if !words.iter().any(|w| *w == *alias) {
                continue;
            }

            // Check for context clues in the query
            let has_context = words.iter().any(|w| context_clues.contains(w))
                || q_lower.contains(&format!("{} framework", alias))
                || q_lower.contains(&format!("{} library", alias))
                || q_lower.contains(&format!("{} package", alias))
                || q_lower.contains(&format!("in {}", alias))
                || q_lower.contains(&format!("for {}", alias))
                || q_lower.contains(&format!("with {}", alias))
                || q_lower.contains(&format!("{} programming", alias))
                || q_lower.contains(&format!("{} developer", alias))
                || q_lower.contains(&format!("{} tutorial", alias))
                || q_lower.contains(&format!("{} code", alias))
                || q_lower.contains(&format!("{} project", alias));

            // Also check if another language is mentioned (comparison context)
            let has_other_lang = languages.iter().any(|&(other_canon, other_aliases)| {
                if other_canon == canonical { return false; }
                other_aliases.iter().any(|oa| words.iter().any(|w| *w == *oa))
            });

            if has_context || has_other_lang {
                return Some(canonical.to_string());
            }
        }
    }

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
            .map(|p| extract_constraint_term(&strip_neg(p), max_words))
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
        "platform", "service", "tutorial", "guide", "documentation", "docs"];
    // Allow constraints to absorb one fewer stop word after the first term
    // so multi-word constraints like "type safety" aren't broken by "safety".
    if max_words > 1 {
        let mut extra = [
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

    let temp = weights.temperature as f64;
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
fn compress_query_with_negatives(query: &str, negative: &[String]) -> String {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.len() <= 8 {
        return query.to_string(); // short enough, don't compress
    }

    // Build a set of negative terms to exclude from compressed output.
    // Each negative constraint may be multi-word ("google workspace"), so
    // we collect individual words from all constraints.
    let neg_word_set: std::collections::HashSet<String> = negative.iter()
        .flat_map(|n| n.to_lowercase().split_whitespace()
            .map(|w| w.to_string())
            .collect::<Vec<_>>())
        .collect();

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
        // Skip words that appear in negative constraints
        let word_lower = word.to_lowercase();
        if !negative.is_empty() && neg_word_set.contains(&word_lower) {
            continue;
        }
        let lower = word.to_lowercase();
        let clean: String = lower.chars().filter(|c| c.is_alphanumeric() || *c == '.' || *c == '+' || *c == '#').collect();

        if clean.is_empty() { continue; }

        let mut score: f32 = 0.0;

        // Base score: stop words get 0, others get 1.0
        if stop_words.contains(clean.as_str()) {
            score = 0.0;
        } else {
            score = 1.0;
        }

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

    // If compression produced something reasonable, use it
    if compressed.split_whitespace().count() >= 3 {
        compressed
    } else {
        // Fallback: if negative filtering made the query too short, try
        // compression without negatives (better than returning the original
        // which includes negation noise).
        if !negative.is_empty() {
            compress_query(query)
        } else {
            query.to_string()
        }
    }
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
                        !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
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
            let mut seen_topic = std::collections::HashSet::new();
            let topic_words: Vec<&str> = words.iter()
                .filter(|w| w.len() > 2 && !stop.contains(w))
                .filter(|w| {
                    // Filter out negative constraint terms from expansions
                    let w_lower = w.to_lowercase();
                    let w_stripped = w_lower.strip_prefix('-').unwrap_or(&w_lower);
                    !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
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
                expansions.push(core_trimmed.to_string());
            }
        }
        "fresh" => {
            let temporal = ["latest","recent","newest","today","this week","this month","new"];
            let year = extract_year(&q_lower).unwrap_or("2026");
            let mut core_words: Vec<&str> = Vec::new();
            for w in &words {
                if !temporal.iter().any(|t| *t == *w) && w.len() > 2 {
                    // Skip year tokens to avoid "2026 2026" duplication
                    if w.len() == 4 && w.starts_with("20") && w.parse::<u32>().ok().map_or(false, |y| (2020..=2029).contains(&y)) {
                        continue;
                    }
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
                expansions.push(format!("{} {}", core_str, year));
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
                !neg_triggers.contains(w_stripped) && !neg_triggers.contains(w_lower.as_str())
                    && !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
                    && !neg_set_lower.contains(w_stripped) && !neg_set_lower.contains(&w_lower)
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
                    && !neg_set.contains(w_stripped) && !neg_set.contains(&w_lower)
                    && !neg_word_set.contains(w_stripped) && !neg_word_set.contains(w_lower.as_str())
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
        let key = exp.to_lowercase();
        if seen.insert(key) {
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
        bert_model: Mutex::new(bert_model),
        bert_tokenizer,
        device,
        intent_cache,
        embed_cache,
        bert_semaphore: Semaphore::new(2),
    });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/analyze", get(analyze_query))
        .route("/embed", get(embed_text))
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

    // ── Step 1: Normalize query (de-stutter, collapse repeats) ──
    // "how how to to set configure setup redis cluster" → "how to configure redis cluster"
    let normalized = normalize_query(&params.q);
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
            let bert_model = state_clone.bert_model.lock().unwrap();
            compute_embedding(&state_clone.device, &*bert_model, &state_clone.bert_tokenizer, &query_text)
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

    // ── Step 2: Compress long queries before expansion ──
    // "what monitoring stack should a small startup use for kubernetes
    //  microservices running on aws" → "kubernetes monitoring stack aws startup"
    // Uses negative-aware compression so excluded terms ("not chrome")
    // don't leak into the compressed query and pollute search results.
    let expansion_input = compress_query_with_negatives(&normalized, &structured.negative);
    if expansion_input != normalized {
        tracing::info!("Query compressed: {:?} → {:?} (negation-aware)", normalized, expansion_input);
    }

    let expanded = expand_queries(&expansion_input, &intent, &structured);
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
            let bert_model = state_clone.bert_model.lock().unwrap();
            compute_embedding(&state_clone.device, &*bert_model, &state_clone.bert_tokenizer, &text)
                .unwrap_or_else(|| vec![0.0; 384])
        }).await.unwrap_or_else(|_| vec![0.0; 384])
    };

    state.embed_cache.insert(text_norm, embedding_vec.clone()).await;
    Json(EmbedResponse {
        embedding: embedding_vec,
    })
}
