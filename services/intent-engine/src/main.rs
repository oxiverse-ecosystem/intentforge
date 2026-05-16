use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_qwen2::ModelWeights as QwenWeights;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokenizers::Tokenizer;

#[derive(Deserialize, Serialize, Clone)]
pub struct AnalyzeParams {
    pub q: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntentResponse {
    #[serde(default)]
    pub query: String,
    pub intent: String,
    #[serde(default, deserialize_with = "deserialize_maybe_list")]
    pub constraints: Vec<String>,
    #[serde(alias = "expandedQueries", default, deserialize_with = "deserialize_maybe_list")]
    pub expanded_queries: Vec<String>,
}

fn deserialize_maybe_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match v {
        serde_json::Value::Array(arr) => Ok(arr.into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()),
        serde_json::Value::String(s) => Ok(vec![s]),
        _ => Ok(vec![]),
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct EmbedParams {
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
}

pub struct AppState {
    pub qwen_model: Mutex<QwenWeights>,
    pub bert_model: Mutex<BertModel>,
    pub qwen_tokenizer: Tokenizer,
    pub bert_tokenizer: Tokenizer,
    pub device: Device,
    pub cache: Cache<String, IntentResponse>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let device = Device::Cpu;
    
    // Model Paths
    let qwen_path = "./models/qwen2.5-1.5b-instruct-q4_k_m.gguf";
    let qwen_tokenizer_path = "./models/tokenizer.json";
    let bert_path = "./models/model.safetensors";
    let bert_config_path = "./models/config.json";
    let bert_tokenizer_path = "./models/tokenizer_embed.json";

    // Wait for models
    let paths = vec![qwen_path, qwen_tokenizer_path, bert_path, bert_config_path, bert_tokenizer_path];
    for path in paths {
        let mut retry_count = 0;
        while !std::path::Path::new(path).exists() && retry_count < 60 {
            tracing::info!("Waiting for {} to appear...", path);
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            retry_count += 1;
        }
    }

    tracing::info!("Loading Qwen model...");
    let qwen_model = {
        let file = std::fs::File::open(qwen_path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let mut reader = std::io::Cursor::new(&mmap);
        let content = candle_core::quantized::gguf_file::Content::read(&mut reader)?;
        QwenWeights::from_gguf(content, &mut reader, &device)?
    };
    
    tracing::info!("Loading BERT model...");
    let bert_config: BertConfig = serde_json::from_reader(std::fs::File::open(bert_config_path)?)?;
    let bert_vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[bert_path], DType::F32, &device)?
    };
    let bert_model = BertModel::load(bert_vb, &bert_config)?;

    tracing::info!("Loading tokenizers...");
    let qwen_tokenizer = Tokenizer::from_file(qwen_tokenizer_path).map_err(|e| anyhow::anyhow!(e))?;
    let bert_tokenizer = Tokenizer::from_file(bert_tokenizer_path).map_err(|e| anyhow::anyhow!(e))?;

    let cache = Cache::builder()
        .max_capacity(1000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let state = Arc::new(AppState {
        qwen_model: Mutex::new(qwen_model),
        bert_model: Mutex::new(bert_model),
        qwen_tokenizer,
        bert_tokenizer,
        device,
        cache,
    });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/analyze", get(analyze_query))
        .route("/embed", get(embed_text))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Intent Engine listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn analyze_query(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AnalyzeParams>,
) -> Json<IntentResponse> {
    let query_norm = params.q.trim().to_lowercase();
    
    if let Some(cached) = state.cache.get(&query_norm).await {
        return Json(cached);
    }

    let generated_text = {
        let mut model = state.qwen_model.lock().unwrap();
        
        let prompt = format!(
            "<|im_start|>system\nYou are a high-precision search intent analyzer. Output valid JSON only.
Goals:
1. Identify Intent: [technical, how-to, comparison, conceptual, navigation, transactional]
2. Extract Constraints: list of core requirements (include version, platform, avoidances).
3. Expanded Queries: list of 2 queries that would yield the most precise results.

Disambiguation Rules:
- If 'Rust' is queried without context, assume programming unless game-specific terms (survival, raid, monument) are present.
- Capture 'avoid', 'exclude', 'no' as negative constraints (e.g., \"exclude: web\").

Example:
User: latest rust updates security avoid web
Assistant: {{\"intent\": \"technical\", \"constraints\": [\"rust language\", \"security\", \"exclude: web frameworks\", \"published: this week\"], \"expanded_queries\": [\"rust programming language security updates May 2026\", \"rust memory safety CVE 2026\"]}}<|im_end|>
<|im_start|>user\n{}<|im_end|>
<|im_start|>assistant\n{{",
            params.q
        );

        let tokens = state.qwen_tokenizer.encode(prompt, true).unwrap();
        let mut tokens = tokens.get_ids().to_vec();
        let mut text = String::from("{");
        let mut logits_processor = LogitsProcessor::new(42, Some(0.0), None);

        let mut pos = 0;
        let input = Tensor::new(&tokens[..], &state.device).unwrap().unsqueeze(0).unwrap();
        let mut logits = model.forward(&input, pos).unwrap().squeeze(0).unwrap();
        pos += tokens.len();

        for _ in 0..150 {
            let next_token = logits_processor.sample(&logits).unwrap();
            tokens.push(next_token);

            if next_token == 151643 || next_token == 151645 { break; }

            let decoded = state.qwen_tokenizer.decode(&[next_token], true).unwrap();
            text.push_str(&decoded);
            
            if text.contains('}') && text.len() > 10 { break; }

            let input = Tensor::new(&[next_token], &state.device).unwrap().unsqueeze(0).unwrap();
            logits = model.forward(&input, pos).unwrap().squeeze(0).unwrap();
            pos += 1;
        }
        text
    };
    
    tracing::info!("Generated text: {}", generated_text);

    // Loose parsing fallback
    let mut parsed: IntentResponse = serde_json::from_str(&generated_text).unwrap_or_else(|_| {
        IntentResponse {
            query: params.q.clone(),
            intent: "conceptual".to_string(),
            constraints: vec![],
            expanded_queries: vec![params.q.clone()],
        }
    });
    parsed.query = params.q;

    state.cache.insert(query_norm, parsed.clone()).await;
    Json(parsed)
}

async fn embed_text(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EmbedParams>,
) -> Json<EmbedResponse> {
    let embedding_vec = {
        let bert_model = state.bert_model.lock().unwrap();
        
        let tokens = state.bert_tokenizer.encode(params.text, true).unwrap();
        let token_ids = Tensor::new(tokens.get_ids(), &state.device).unwrap().unsqueeze(0).unwrap();
        let token_type_ids = Tensor::new(tokens.get_type_ids(), &state.device).unwrap().unsqueeze(0).unwrap();
        
        let embeddings = bert_model.forward(&token_ids, &token_type_ids, None).unwrap();
        
        let (_n_batch, n_tokens, _n_emb) = embeddings.dims3().unwrap();
        let embedding = (embeddings.sum(1).unwrap() / (n_tokens as f64)).unwrap();
        
        let norm = embedding.sqr().unwrap().sum_all().unwrap().sqrt().unwrap().to_vec0::<f32>().unwrap();
        (embedding / (norm as f64)).unwrap().to_vec2::<f32>().unwrap()[0].clone()
    };

    Json(EmbedResponse {
        embedding: embedding_vec,
    })
}
