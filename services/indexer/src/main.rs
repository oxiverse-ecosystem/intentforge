use ax_ext::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use axum as ax_ext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

struct AppState {
    index: Index,
    reader: IndexReader,
    writer: Arc<tokio::sync::Mutex<IndexWriter>>,
    schema: Schema,
}

#[derive(Deserialize, Serialize)]
struct IngestRequest {
    url: String,
    title: String,
    content: String,
    #[serde(default)]
    timestamp: Option<u64>,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default)]
    vector: Option<String>,
    #[serde(default)]
    min_score: Option<f32>, // Semantic threshold
    #[serde(default)]
    freshness_boost: Option<bool>,
}

#[derive(Serialize)]
struct SearchResult {
    url: String,
    title: String,
    score: f32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let index_path = "./index_data";
    std::fs::create_dir_all(index_path)?;

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("url", STORED | TEXT);
    schema_builder.add_text_field("title", STORED | TEXT);
    schema_builder.add_text_field("content", TEXT);
    schema_builder.add_u64_field("timestamp", INDEXED | FAST | STORED);
    schema_builder.add_bytes_field("embedding", STORED);
    
    let schema = schema_builder.build();

    let index = Index::open_or_create(tantivy::directory::MmapDirectory::open(index_path)?, schema.clone())?;

    let writer = index.writer(50_000_000)?; 
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;

    let state = Arc::new(AppState {
        index,
        reader,
        writer: Arc::new(tokio::sync::Mutex::new(writer)),
        schema,
    });

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/index", post(handle_ingest))
        .route("/search", get(handle_search))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 6000));
    tracing::info!("Indexer listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    ax_ext::serve(listener, app).await.unwrap();

    Ok(())
}

async fn handle_ingest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IngestRequest>,
) -> Json<serde_json::Value> {
    tracing::info!("Ingesting URL: {}", payload.url);
    let url_field = state.schema.get_field("url").unwrap();
    let title_field = state.schema.get_field("title").unwrap();
    let content_field = state.schema.get_field("content").unwrap();
    let timestamp_field = state.schema.get_field("timestamp").unwrap();
    let embedding_field = state.schema.get_field("embedding").unwrap();

    let mut writer = state.writer.lock().await;
    let mut doc = TantivyDocument::default();
    doc.add_text(url_field, payload.url.clone());
    doc.add_text(title_field, payload.title.clone());
    doc.add_text(content_field, payload.content);
    
    let ts = payload.timestamp.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });
    doc.add_u64(timestamp_field, ts);

    if let Some(vec) = payload.embedding {
        tracing::info!("Adding embedding ({} dims)", vec.len());
        let bytes: Vec<u8> = vec.iter().flat_map(|&f| f.to_le_bytes()).collect();
        doc.add_bytes(embedding_field, bytes);
    }

    writer.add_document(doc).unwrap();
    writer.commit().unwrap();
    tracing::info!("Successfully committed: {}", payload.url);

    Json(serde_json::json!({ "status": "indexed", "url": payload.url }))
}

async fn handle_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<SearchResult>> {
    let searcher = state.reader.searcher();
    let url_field = state.schema.get_field("url").unwrap();
    let title_field = state.schema.get_field("title").unwrap();
    let timestamp_field = state.schema.get_field("timestamp").unwrap();
    let embedding_field = state.schema.get_field("embedding").unwrap();

    let query_vector: Option<Vec<f32>> = params.vector.and_then(|v_str| {
        serde_json::from_str::<Vec<f32>>(&v_str).ok()
    });

    let query_parser = tantivy::query::QueryParser::for_index(&state.index, vec![title_field, state.schema.get_field("content").unwrap()]);
    let query = if params.q.is_empty() {
        Box::new(tantivy::query::AllQuery) as Box<dyn tantivy::query::Query>
    } else {
        match query_parser.parse_query(&params.q) {
            Ok(q) => q,
            Err(_) => Box::new(tantivy::query::AllQuery) as Box<dyn tantivy::query::Query>,
        }
    };

    let limit = 100;
    let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(limit)).unwrap();

    let mut bm25_ranked = Vec::new();
    let mut semantic_ranked = Vec::new();
    let mut metadata: HashMap<String, (String, u64)> = HashMap::new();
    
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let threshold = params.min_score.unwrap_or(0.75);

    let mut semantic_pass_urls = std::collections::HashSet::new();
    let is_semantic = query_vector.is_some();

    for (_score, doc_address) in top_docs {
        let retrieved_doc: TantivyDocument = searcher.doc(doc_address).unwrap();
        let url = retrieved_doc.get_first(url_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let title = retrieved_doc.get_first(title_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let timestamp = retrieved_doc.get_first(timestamp_field).and_then(|v| v.as_u64()).unwrap_or(0);
        
        metadata.insert(url.clone(), (title.clone(), timestamp));

        if let Some(ref q_vec) = query_vector {
            if let Some(doc_bytes) = retrieved_doc.get_first(embedding_field).and_then(|v| v.as_bytes()) {
                let doc_vec: Vec<f32> = doc_bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                    .collect();
                
                if doc_vec.len() == q_vec.len() {
                    let dot_product: f32 = q_vec.iter().zip(doc_vec.iter()).map(|(a, b)| a * b).sum();
                    if dot_product >= threshold {
                        semantic_ranked.push((url.clone(), dot_product, title.clone(), timestamp));
                        semantic_pass_urls.insert(url.clone());
                    }
                }
            }
        }
        
        // Only add to BM25 rank if it's not a semantic search OR if it passed semantic threshold
        // If it's NOT a semantic search, we add everything.
        // If it IS a semantic search, we only add it if it has an embedding AND passed, OR if it has NO embedding (fallback)
        if !is_semantic || semantic_pass_urls.contains(&url) {
             bm25_ranked.push(url.clone());
        }
    }

    semantic_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let k = 60.0;
    let mut rrf_scores: HashMap<String, f32> = HashMap::new();

    for (rank, url) in bm25_ranked.iter().enumerate() {
        if is_semantic && !semantic_pass_urls.contains(url) { continue; }
        *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
    }

    for (rank, (url, _sim, _title, _ts)) in semantic_ranked.iter().enumerate() {
        *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
    }

    let mut results: Vec<SearchResult> = rrf_scores
        .into_iter()
        .map(|(url, score)| {
            let (title, ts) = metadata.get(&url).cloned().unwrap_or(("No Title".to_string(), 0));
            let mut final_score = score;
            if params.freshness_boost.unwrap_or(false) && ts > 0 {
                let age = now.saturating_sub(ts);
                let scale = 86400.0 * 7.0;
                let boost = scale / (scale + age as f32);
                final_score *= 1.0 + (boost * 0.5);
            }
            SearchResult { url, title, score: final_score }
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(10);

    Json(results)
}
