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

// ─── Intent Categories ───────────────────────────────────────────────
const INTENT_CATEGORIES: &[&str] = &[
    "navigational",
    "informational",
    "technical",
    "how-to",
    "comparison",
    "transactional",
    "fresh",
];

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
    pub constraints: Vec<String>,
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

// ─── Layer 1: Rule-Based Pre-Classifier (< 1ms) ─────────────────────

struct RuleMatch {
    intent: &'static str,
    confidence: f32,
}

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

    // ── How-To ──
    if q.starts_with("how to ") || q.starts_with("how do i ")
        || q.starts_with("how can i ") || q.starts_with("how do you ")
        || q.starts_with("steps to ") || q.starts_with("guide to ")
        || q.starts_with("tutorial ")
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
    if q.contains("latest") || q.contains("recent") || q.contains("newest")
        || q.contains("today") || q.contains("this week")
        || q.contains("this month") || q.contains("2026")
        || q.contains("2025") || q.starts_with("news ")
        || q.contains(" update") || q.contains(" release")
        || q.contains(" cve") || q.contains(" vulnerability")
    {
        return Some(RuleMatch { intent: "fresh", confidence: 0.8 });
    }

    // ── Technical ──
    let tech_terms = [
        "api", "sdk", "library", "framework", "crate", "package", "module",
        "function", "method", "class", "interface", "struct", "enum",
        "trait", "impl", "syntax", "compiler", "runtime", "debug",
        "error", "bug", "fix", "issue", "version", "migration",
        "documentation", "docs", "reference", "manpage",
    ];
    let tech_languages = [
        "rust", "python", "javascript", "typescript", "go", "golang",
        "java", "c++", "cpp", "swift", "kotlin", "ruby",
        "php", "haskell", "elixir", "scala",
        "react", "vue", "angular", "svelte", "nextjs", "next.js",
        "django", "flask", "fastapi", "express", "axum", "tokio",
        "docker", "kubernetes", "k8s", "linux", "git", "nginx",
        "postgres", "mysql", "redis", "mongodb", "sqlite",
    ];

    let has_tech_term = tech_terms.iter().any(|t| q.contains(t));
    let has_tech_lang = tech_languages.iter().any(|l| {
        q.split_whitespace().any(|w| w == *l)
    });

    if has_tech_term || has_tech_lang {
        return Some(RuleMatch { intent: "technical", confidence: 0.75 });
    }

    // ── Informational ──
    if q.starts_with("what is ") || q.starts_with("what are ")
        || q.starts_with("what does ") || q.starts_with("explain ")
        || q.starts_with("why ") || q.starts_with("when ")
        || q.starts_with("where ") || q.starts_with("who is ")
        || q.starts_with("define ") || q.starts_with("meaning of ")
    {
        return Some(RuleMatch { intent: "informational", confidence: 0.8 });
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

// ─── Query Expansion (Dynamic, Not Hardcoded) ────────────────────────
// Generates query variations for broader recall across meta-search engines.
// Each variation captures a different phrasing of the same intent.

fn expand_queries(query: &str, intent: &str) -> Vec<String> {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();
    let mut expansions = vec![q.to_string()]; // always include original

    match intent {
        "how-to" => {
            // "how to deploy to aws" → "aws deployment guide", "deploy application aws tutorial"
            let stripped = q_lower
                .strip_prefix("how to ").or_else(|| q_lower.strip_prefix("how do i "))
                .or_else(|| q_lower.strip_prefix("how can i "))
                .unwrap_or(&q_lower);
            if stripped != q_lower {
                expansions.push(format!("{} guide", stripped));
                expansions.push(format!("{} tutorial", stripped));
            }
        }
        "comparison" => {
            // "rust vs go" → "rust go comparison", "rust compared to go"
            if q_lower.contains(" vs ") || q_lower.contains(" versus ") {
                let replaced = q_lower.replace(" versus ", " vs ");
                let parts: Vec<&str> = replaced.split(" vs ").collect();
                if parts.len() == 2 {
                    expansions.push(format!("{} {} comparison", parts[0].trim(), parts[1].trim()));
                    expansions.push(format!("{} compared to {}", parts[0].trim(), parts[1].trim()));
                }
            }
            // "best X 2025" → "top X 2025", "X recommendation 2025"
            if q_lower.starts_with("best ") || q_lower.starts_with("top ") {
                let core = if q_lower.starts_with("best ") {
                    q.strip_prefix("best ").or_else(|| q.strip_prefix("Best ")).unwrap_or(q)
                } else {
                    q.strip_prefix("top ").or_else(|| q.strip_prefix("Top ")).unwrap_or(q)
                };
                expansions.push(format!("top {}", core));
                expansions.push(format!("{} recommendation", core));
            }
        }
        "technical" => {
            // "rust async runtime" → "rust async programming", "rust async await docs"
            // Extract topic terms (skip generic ones)
            let stop = ["the","a","an","in","on","for","with","using","from","to","and","or","of","is","are"];
            let topic_words: Vec<&str> = words.iter()
                .filter(|w| w.len() > 2 && !stop.contains(w))
                .copied()
                .collect();
            if topic_words.len() >= 2 {
                // Use last 2-3 topic words + "documentation" or "docs"
                let tail: Vec<&str> = topic_words.iter().rev().take(3).rev().copied().collect();
                expansions.push(format!("{} documentation", tail.join(" ")));
                expansions.push(format!("{} examples", tail.join(" ")));
            }
        }
        "informational" => {
            // "what is machine learning" → "machine learning explained", "machine learning overview"
            let stripped = q_lower
                .strip_prefix("what is ").or_else(|| q_lower.strip_prefix("what are "))
                .or_else(|| q_lower.strip_prefix("explain "))
                .or_else(|| q_lower.strip_prefix("define "));
            if let Some(core) = stripped {
                if core.len() > 3 {
                    expansions.push(format!("{} explained", core));
                    expansions.push(format!("{} overview", core));
                }
            }
        }
        "fresh" => {
            // "latest rust release" → "rust release 2026", "rust new version"
            // Remove temporal words and rephrase
            let temporal = ["latest","recent","newest","today","this week","this month","new"];
            let mut core_words: Vec<&str> = Vec::new();
            for w in &words {
                if !temporal.iter().any(|t| *t == *w) && w.len() > 2 {
                    core_words.push(w);
                }
            }
            if !core_words.is_empty() {
                let core = core_words.join(" ");
                expansions.push(format!("{} 2026", core));
                expansions.push(format!("{} update", core));
            }
        }
        _ => {}
    }

    // Deduplicate and cap at 4
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for exp in expansions {
        let key = exp.to_lowercase();
        if seen.insert(key) && unique.len() < 4 {
            unique.push(exp);
        }
    }
    unique
}

// ─── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let device = Device::Cpu;

    // Model Paths (only MiniLM — Qwen removed)
    let bert_path = "./models/model.safetensors";
    let bert_config_path = "./models/config.json";
    let bert_tokenizer_path = "./models/tokenizer_embed.json";

    // Wait for models
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

    // Pre-compute intent category centroids (happens once at startup)
    tracing::info!("Computing intent category centroids...");
    let category_centroids = {
        let category_examples: Vec<Vec<&str>> = vec![
            vec!["python docs", "github login", "mdn web docs", "stackoverflow",
                 "rust book", "npm registry", "pypi", "crates.io", "docker hub",
                 "kubernetes documentation", "react official site", "vue.js homepage",
                 "typescript handbook", "go documentation", "linux man pages",
                 "arch wiki", "reddit", "wikipedia", "youtube", "twitter"],
            vec!["what is machine learning", "what is a neural network",
                 "explain quantum computing", "what does TCP do",
                 "how does a compiler work", "what is the internet",
                 "why is the sky blue", "what is photosynthesis",
                 "define algorithm", "meaning of recursion",
                 "what are design patterns", "what is REST API",
                 "explain blockchain", "what is DNS", "how wifi works"],
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
    tracing::info!("Intent Engine listening on {} (lightweight: rules + centroids, no LLM)", addr);

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

    // Check cache
    if let Some(cached) = state.intent_cache.get(&query_norm).await {
        return Json(cached);
    }

    // Layer 1: Rule-based pre-classifier (< 1ms)
    let result = if let Some(rule_match) = rule_based_classify(&params.q) {
        tracing::info!("Layer 1 (rules) -> {} (conf: {:.2})", rule_match.intent, rule_match.confidence);
        let expanded = expand_queries(&params.q, rule_match.intent);
        tracing::info!("Expanded to {} query variations", expanded.len());
        IntentResponse {
            query: params.q.clone(),
            intent: rule_match.intent.to_string(),
            confidence: rule_match.confidence,
            constraints: vec![],
            expanded_queries: expanded,
        }
    } else {
        // Layer 2: Embedding centroid classifier (~2ms)
        tracing::info!("Layer 1 ambiguous, using Layer 2 (centroids)");
        let bert_model = state.bert_model.lock().unwrap();
        match compute_embedding(&state.device, &*bert_model, &state.bert_tokenizer, &params.q) {
            Some(query_embedding) => {
                let (intent, confidence) = classify_by_centroids(&query_embedding, &state.category_centroids);
                tracing::info!("Layer 2 (centroids) -> {} (conf: {:.2})", intent, confidence);
                let expanded = expand_queries(&params.q, &intent);
                tracing::info!("Expanded to {} query variations", expanded.len());
                IntentResponse {
                    query: params.q.clone(),
                    intent,
                    confidence,
                    constraints: vec![],
                    expanded_queries: expanded,
                }
            }
            None => {
                tracing::warn!("Embedding failed, defaulting to informational");
                IntentResponse {
                    query: params.q.clone(),
                    intent: "informational".to_string(),
                    confidence: 0.3,
                    constraints: vec![],
                    expanded_queries: vec![params.q.clone()],
                }
            }
        }
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
