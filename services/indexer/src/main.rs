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
    #[serde(default)]
    authority: Option<f64>,
    #[serde(default)]
    quality: Option<f64>,
}

#[derive(Deserialize, Clone)]
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
    #[serde(default)]
    authority: f32,
    #[serde(default)]
    content: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let index_path = "./index_data";
    std::fs::create_dir_all(index_path)?;

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("url", STRING | STORED);
    schema_builder.add_text_field("title", STORED | TEXT);
    schema_builder.add_text_field("content", TEXT | STORED);
    schema_builder.add_u64_field("timestamp", INDEXED | FAST | STORED);
    schema_builder.add_bytes_field("embedding", STORED);
    schema_builder.add_f64_field("authority", STORED | FAST);
    
    let schema = schema_builder.build();

    let index = match Index::open_or_create(tantivy::directory::MmapDirectory::open(index_path)?, schema.clone()) {
        Ok(idx) => {
            if idx.schema() != schema {
                tracing::warn!("Schema mismatch detected, recreating index...");
                let _ = std::fs::remove_dir_all(index_path);
                std::fs::create_dir_all(index_path)?;
                Index::create_in_dir(index_path, schema.clone())?
            } else {
                idx
            }
        }
        Err(e) => {
            tracing::warn!("Failed to open index: {:?}, clearing directory and recreating...", e);
            let _ = std::fs::remove_dir_all(index_path);
            std::fs::create_dir_all(index_path)?;
            Index::create_in_dir(index_path, schema.clone())?
        }
    };

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
        .route("/urls", get(handle_list_urls))
        .route("/search", get(handle_search))
        .route("/stats", get(handle_stats))
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
    let authority_field = state.schema.get_field("authority").unwrap();

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

    // Store domain authority score if provided
    let auth = payload.authority.unwrap_or(0.5);
    doc.add_f64(authority_field, auth);

    // Delete existing document with the same URL to prevent duplicates
    let term = tantivy::Term::from_field_text(url_field, &payload.url);
    writer.delete_term(term);

    writer.add_document(doc).unwrap();
    writer.commit().unwrap();
    tracing::info!("Successfully committed: {}", payload.url);

    Json(serde_json::json!({ "status": "indexed", "url": payload.url }))
}

#[derive(Serialize)]
struct IndexedUrl {
    url: String,
    timestamp: u64,
}

async fn handle_list_urls(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<IndexedUrl>> {
    let searcher = state.reader.searcher();
    let url_field = state.schema.get_field("url").unwrap();
    let timestamp_field = state.schema.get_field("timestamp").unwrap();
    
    let query = tantivy::query::AllQuery;
    let top_docs = match searcher.search(&query, &tantivy::collector::TopDocs::with_limit(1000)) {
        Ok(docs) => docs,
        Err(_) => vec![],
    };
    
    let mut list = Vec::new();
    for (_, doc_address) in top_docs {
        if let Ok(retrieved_doc) = searcher.doc::<TantivyDocument>(doc_address) {
            let url = retrieved_doc.get_first(url_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let timestamp = retrieved_doc.get_first(timestamp_field).and_then(|v| v.as_u64()).unwrap_or(0);
            if !url.is_empty() {
                list.push(IndexedUrl { url, timestamp });
            }
        }
    }
    
    Json(list)
}

async fn handle_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<SearchResult>> {
    let state_clone = state.clone();
    let q = params.q.clone();
    let vector = params.vector.clone();
    let min_score = params.min_score;
    let freshness_boost = params.freshness_boost;

    let results = tokio::task::spawn_blocking(move || {
        let searcher = state_clone.reader.searcher();
        let url_field = state_clone.schema.get_field("url").unwrap();
        let title_field = state_clone.schema.get_field("title").unwrap();
        let timestamp_field = state_clone.schema.get_field("timestamp").unwrap();
        let embedding_field = state_clone.schema.get_field("embedding").unwrap();
        let authority_field = state_clone.schema.get_field("authority").unwrap();
        let content_field = state_clone.schema.get_field("content").unwrap();

        let query_vector: Option<Vec<f32>> = vector.and_then(|v_str| {
            serde_json::from_str::<Vec<f32>>(&v_str).ok()
        });

        let query_parser = tantivy::query::QueryParser::for_index(&state_clone.index, vec![title_field, state_clone.schema.get_field("content").unwrap()]);
        let title_query_parser = tantivy::query::QueryParser::for_index(&state_clone.index, vec![title_field]);

        let query: Box<dyn tantivy::query::Query> = if q.is_empty() {
            Box::new(tantivy::query::AllQuery)
        } else {
            match query_parser.parse_query(&q) {
                Ok(q) => q,
                Err(_) => Box::new(tantivy::query::AllQuery),
            }
        };

        let title_query: Box<dyn tantivy::query::Query> = if q.is_empty() {
            Box::new(tantivy::query::AllQuery)
        } else {
            match title_query_parser.parse_query(&q) {
                Ok(q) => q,
                Err(_) => Box::new(tantivy::query::AllQuery),
            }
        };

        let limit = 200;
        let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(limit)).unwrap_or_default();

        let title_hits: std::collections::HashSet<String> = searcher.search(&title_query, &tantivy::collector::TopDocs::with_limit(limit))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, addr)| {
                searcher.doc::<TantivyDocument>(addr).ok()
                    .and_then(|d| d.get_first(url_field).and_then(|v| v.as_str()).map(|s| s.to_string()))
            })
            .collect();

        let mut bm25_ranked = Vec::new();
        let mut semantic_ranked = Vec::new();
        let mut metadata: HashMap<String, (String, u64, f64, String)> = HashMap::new();
        let mut urls_without_embeddings = std::collections::HashSet::new();

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let threshold = min_score.unwrap_or(0.75);

        let mut semantic_pass_urls = std::collections::HashSet::new();
        let is_semantic = query_vector.is_some();

        for (_score, doc_address) in top_docs {
            let Ok(retrieved_doc) = searcher.doc::<TantivyDocument>(doc_address) else { continue; };
            let url = retrieved_doc.get_first(url_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = retrieved_doc.get_first(title_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let timestamp = retrieved_doc.get_first(timestamp_field).and_then(|v| v.as_u64()).unwrap_or(0);
            let authority = retrieved_doc.get_first(authority_field).and_then(|v| v.as_f64()).unwrap_or(0.5);
            let content = retrieved_doc.get_first(content_field).and_then(|v| v.as_str()).unwrap_or("").to_string();

            metadata.insert(url.clone(), (title.clone(), timestamp, authority, content));

            let mut has_embedding = false;
            if let Some(ref q_vec) = query_vector {
                if let Some(doc_bytes) = retrieved_doc.get_first(embedding_field).and_then(|v| v.as_bytes()) {
                    if doc_bytes.len() % 4 == 0 {
                        let doc_vec: Vec<f32> = doc_bytes
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                            .collect();

                        if doc_vec.len() == q_vec.len() {
                            has_embedding = true;
                            let dot_product: f32 = q_vec.iter().zip(doc_vec.iter()).map(|(a, b)| a * b).sum();
                            if dot_product >= threshold {
                                semantic_ranked.push((url.clone(), dot_product, title.clone(), timestamp));
                                semantic_pass_urls.insert(url.clone());
                            }
                        }
                    }
                }
            }

            if !has_embedding {
                urls_without_embeddings.insert(url.clone());
            }

            if !is_semantic || semantic_pass_urls.contains(&url) || !has_embedding {
                bm25_ranked.push(url.clone());
            }
        }

        semantic_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let k = 60.0;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();

        for (rank, url) in bm25_ranked.iter().enumerate() {
            if is_semantic && !semantic_pass_urls.contains(url) && !urls_without_embeddings.contains(url) { continue; }
            *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
        }

        for (rank, (url, _sim, _title, _ts)) in semantic_ranked.iter().enumerate() {
            *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
        }

        let mut results: Vec<SearchResult> = rrf_scores
            .into_iter()
            .map(|(url, score)| {
                let (title, ts, auth, content) = metadata.get(&url).cloned().unwrap_or(("No Title".to_string(), 0, 0.5, String::new()));
                let mut final_score = score;
                if freshness_boost.unwrap_or(false) && ts > 0 {
                    let age = now.saturating_sub(ts);
                    let scale = 86400.0 * 7.0;
                    let boost = scale / (scale + age as f32);
                    final_score *= 1.0 + (boost * 0.5);
                }
                final_score *= 1.0 + (auth as f32 * 0.3);

                if title_hits.contains(&url) {
                    final_score *= 2.0;
                }

                SearchResult {
                    url, title,
                    score: final_score,
                    authority: auth as f32,
                    content: if content.len() > 500 { content.chars().take(500).collect() } else { content },
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        const MAX_PER_DOMAIN: usize = 3;
        let mut domain_counts: HashMap<String, usize> = HashMap::new();
        let mut diverse_results: Vec<SearchResult> = Vec::new();
        for r in results {
            let domain = r.url
                .split("://")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .and_then(|s| s.split(':').next())
                .unwrap_or("")
                .to_lowercase();
            let count = domain_counts.entry(domain).or_insert(0);
            if *count < MAX_PER_DOMAIN {
                *count += 1;
                diverse_results.push(r);
            }
        }
        let mut results = diverse_results;

        if let Some(max_score) = results.iter().map(|r| r.score).fold(None, |acc, s| {
            Some(match acc { Some(m) => s.max(m), None => s })
        }) {
            if max_score > 0.0 {
                for r in results.iter_mut() {
                    r.score /= max_score;
                }
            }
        }

        results.truncate(10);
        results
    }).await.unwrap_or_default();

    Json(results)
}

async fn handle_stats(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let searcher = state.reader.searcher();
    let doc_count = searcher.num_docs();
    let segment_count = searcher.segment_readers().len();

    // Get index directory size
    let index_size = std::fs::read_dir("./index_data")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum::<u64>()
        })
        .unwrap_or(0);

    Json(serde_json::json!({
        "documents": doc_count,
        "segments": segment_count,
        "index_size_bytes": index_size,
        "index_size_mb": (index_size as f64 / 1_048_576.0).round() as u64,
    }))
}
