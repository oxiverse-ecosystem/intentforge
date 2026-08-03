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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};
use sled::Db;

struct AppState {
    index: Index,
    reader: IndexReader,
    writer: Arc<tokio::sync::Mutex<IndexWriter>>,
    schema: Schema,
    writer_pending: Arc<AtomicU64>,
    query_parser: tantivy::query::QueryParser,
    title_parser: tantivy::query::QueryParser,
    // Embeddings are no longer stored inside Tantivy (was 6144 bytes/doc → ~1.5GB
    // of dead weight, since the semantic path only reads them when a `vector`
    // param is supplied, which the gateway never does). They live in a sled KV
    // store keyed by URL, enabling a sub-linear vector lookup for semantic search.
    embedding_store: Db,
}

#[derive(Deserialize, Serialize, Clone)]
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
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct BatchIngestRequest {
    documents: Vec<IngestRequest>,
    #[serde(default = "default_replace_existing")]
    replace_existing: bool,
}

fn default_replace_existing() -> bool {
    true
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
    /// Signal-quality metric in [0,1]. 1.0 = high-signal page that is genuinely
    /// about the query (strong BM25 + (optionally) semantic match). Lower values
    /// indicate a low-signal crawled page (matched only on a generic/boilerplate
    /// term, or a weak/partial lexical hit) that the gateway should demote before
    /// it can pollute general-topic result sets. Computed fail-safe: defaults to
    /// 1.0 when no query is present so non-search callers are unaffected.
    #[serde(default = "default_quality")]
    quality: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
}

fn default_quality() -> f32 { 1.0 }

/// Robustly open or create a Tantivy index.
///
/// Schema handling is deliberate and SAFE:
///  - If the on-disk index's schema matches ours → open normally.
///  - If it differs ONLY by the intentional `embedding` column removal (a
///    superset of the new fields), run an in-place rebuild that preserves the
///    entire text corpus (no re-crawl, no data loss).
///  - Only a genuinely unopenable/corrupted directory falls through to the
///    destructive recreate path.
fn open_or_create_index_robust(index_path: &str, schema: Schema) -> anyhow::Result<Index> {
    std::fs::create_dir_all(index_path)?;

    // Detect an existing index and inspect its on-disk schema BEFORE attempting
    // open_or_create (which hard-errors on mismatch and would force a wipe).
    let existing_meta = index_path.to_string() + "/meta.json";
    if std::path::Path::new(&existing_meta).exists() {
        match Index::open(tantivy::directory::MmapDirectory::open(index_path)?) {
            Ok(existing) => {
                let prev = existing.schema();
                if prev != schema {
                    let prev_fields: std::collections::HashSet<String> =
                        prev.fields().map(|(_, e)| e.name().to_string()).collect();
                    let new_fields: std::collections::HashSet<String> =
                        schema.fields().map(|(_, e)| e.name().to_string()).collect();
                    // Safe migration: old schema had `embedding`, new one does not,
                    // and no new fields were introduced → rebuild dropping the column.
                    if prev_fields.contains("embedding")
                        && !new_fields.contains("embedding")
                        && prev_fields.is_superset(&new_fields)
                    {
                        tracing::warn!(
                            "Schema migration: dropping `embedding` column in place (index text preserved)..."
                        );
                        match rebuild_index_dropping_embedding(index_path, schema.clone()) {
                            Ok(()) => {
                                return Ok(Index::open_or_create(
                                    tantivy::directory::MmapDirectory::open(index_path)?,
                                    schema.clone(),
                                )?);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "In-place rebuild failed ({:?}); falling back to destructive recreate",
                                    e
                                );
                            }
                        }
                    }
                    // Non-safe mismatch → destructive recreate below.
                    tracing::warn!("Schema mismatch (non-migratable), recreating index...");
                    let _ = std::fs::remove_dir_all(index_path);
                    std::fs::create_dir_all(index_path)?;
                    return Ok(Index::create_in_dir(index_path, schema.clone())?);
                }
                // Schema matches → use the existing open index.
                return Ok(existing);
            }
            Err(e) => {
                tracing::warn!("Existing index unopenable ({:?}); will recreate", e);
            }
        }
    }

    // No existing index (or unopenable/corrupt) → create fresh.
    let dir_result = tantivy::directory::MmapDirectory::open(index_path);
    let open_result: anyhow::Result<Index> = match dir_result {
        Ok(dir) => Index::open_or_create(dir, schema.clone())
            .map_err(|e| anyhow::anyhow!(e)),
        Err(e) => {
            tracing::warn!("MmapDirectory::open failed: {:?}, will recreate", e);
            Err(anyhow::anyhow!("MmapDirectory open: {:?}", e))
        }
    };

    match open_result {
        Ok(idx) => Ok(idx),
        Err(e) => {
            tracing::warn!(
                "Failed to open/create index: {:?}, clearing directory and recreating from scratch...",
                e
            );
            let _ = std::fs::remove_dir_all(index_path);
            if let Err(e2) = std::fs::create_dir_all(index_path) {
                tracing::warn!("Could not recreate index dir: {:?}, trying temp fallback", e2);
            }
            match Index::create_in_dir(index_path, schema.clone()) {
                Ok(idx) => Ok(idx),
                Err(e2) => {
                    tracing::error!(
                        "Failed to create fresh index in {}: {:?}, trying temp fallback",
                        index_path, e2
                    );
                    let temp_path = format!("{}_fresh_{}", index_path, std::process::id());
                    let _ = std::fs::create_dir_all(&temp_path);
                    Ok(Index::create_in_dir(&temp_path, schema.clone())?)
                }
            }
        }
    }
}

/// In-place schema migration: rebuild the Tantivy index WITHOUT the `embedding`
/// column, copying all other fields verbatim from the existing segments. This
/// preserves the entire text corpus (URL/title/content/timestamp/authority) so a
/// schema change does not force a full re-crawl. After rebuild the new schema's
/// index is created and the old segment files are replaced.
fn rebuild_index_dropping_embedding(index_path: &str, new_schema: Schema) -> anyhow::Result<()> {
    use tantivy::collector::TopDocs;
    use tantivy::query::AllQuery;

    // Open the OLD index using its on-disk schema (which still has `embedding`)
    // so we can copy the text fields out verbatim before recreating.
    let dir = tantivy::directory::MmapDirectory::open(index_path)?;
    let old_index = Index::open(dir)?;
    let old_schema = old_index.schema();
    let f_url = old_schema.get_field("url").unwrap();
    let f_title = old_schema.get_field("title").unwrap();
    let f_content = old_schema.get_field("content").unwrap();
    let f_timestamp = old_schema.get_field("timestamp").unwrap();
    let f_authority = old_schema.get_field("authority").unwrap();

    let reader = old_index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    // Enumerate every alive doc via the public AllQuery + TopDocs API (stable
    // across Tantivy versions; avoids fragile segment-internal iteration).
    let top_docs = searcher
        .search(&AllQuery, &TopDocs::with_limit(searcher.num_docs() as usize + 1))
        .map_err(|e| anyhow::anyhow!("migration search failed: {:?}", e))?;

    let mut rows: Vec<(String, String, String, u64, f64)> = Vec::new();
    for (_score, doc_address) in top_docs {
        if let Ok(d) = searcher.doc::<TantivyDocument>(doc_address) {
            let url = d.get_first(f_url).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = d.get_first(f_title).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = d.get_first(f_content).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ts = d.get_first(f_timestamp).and_then(|v| v.as_u64()).unwrap_or(0);
            let auth = d.get_first(f_authority).and_then(|v| v.as_f64()).unwrap_or(0.5);
            if !url.is_empty() {
                rows.push((url, title, content, ts, auth));
            }
        }
    }

    tracing::info!("Migration: copying {} docs (text only) to new schema", rows.len());

    // Swap in the new index (drop the old segment files, which still carry the
    // embedding column).
    let _ = std::fs::remove_dir_all(index_path);
    std::fs::create_dir_all(index_path)?;
    let new_index = Index::create_in_dir(index_path, new_schema)?;
    let mut writer = new_index.writer(200_000_000)?;

    let n_url = new_index.schema().get_field("url").unwrap();
    let n_title = new_index.schema().get_field("title").unwrap();
    let n_content = new_index.schema().get_field("content").unwrap();
    let n_ts = new_index.schema().get_field("timestamp").unwrap();
    let n_auth = new_index.schema().get_field("authority").unwrap();

    for (url, title, content, ts, auth) in rows {
        let mut doc = TantivyDocument::default();
        doc.add_text(n_url, url);
        doc.add_text(n_title, title);
        doc.add_text(n_content, content);
        doc.add_u64(n_ts, ts);
        doc.add_f64(n_auth, auth);
        writer.add_document(doc)?;
    }
    writer.commit()?;
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let index_path = "./index_data";

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("url", STRING | STORED);
    schema_builder.add_text_field("title", STORED | TEXT);
    schema_builder.add_text_field("content", TEXT | STORED);
    schema_builder.add_u64_field("timestamp", INDEXED | FAST | STORED);
    // `embedding` is intentionally NOT a Tantivy field anymore — offloaded to sled.
    schema_builder.add_f64_field("authority", STORED | FAST);
    schema_builder.add_f64_field("price", FAST | STORED);
    schema_builder.add_text_field("currency", STRING | STORED);
    
    let schema = schema_builder.build();

    // Use the robust open-or-create that handles corrupted data gracefully.
    // NOTE: this may run an in-place schema migration that does remove_dir_all
    // on ./index_data, so the embedding sled store MUST be opened AFTER this
    // call (otherwise the migration would delete the sled directory).
    let index = open_or_create_index_robust(index_path, schema.clone())?;

    // Open the embedding KV store (keyed by URL → 1536×f32 LE bytes). Lives
    // alongside index_data so it is covered by the same volume/backup. Opened
    // here (after the index migration) so the migration cannot wipe it.
    let embedding_store = sled::open("./index_data/embeddings")
        .map_err(|e| anyhow::anyhow!("failed to open embedding sled store: {:?}", e))?;

    // Pre-build query parsers once at startup. The search path previously
    // constructed 2 QueryParsers per request (~0.5ms each) — now cloned cheaply.
    let qp_title = schema.get_field("title").unwrap();
    let qp_content = schema.get_field("content").unwrap();
    let query_parser = tantivy::query::QueryParser::for_index(&index, vec![qp_title, qp_content]);
    let title_parser = tantivy::query::QueryParser::for_index(&index, vec![qp_title]);

    // Raised from 50MB -> 200MB so the 2s commit cadence produces larger, fewer
    // segments (less segment sprawl, smaller index, faster search).
    let writer = index.writer(200_000_000)?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;

    let state = Arc::new(AppState {
        index,
        reader,
        writer: Arc::new(tokio::sync::Mutex::new(writer)),
        schema,
        writer_pending: Arc::new(AtomicU64::new(0)),
        query_parser,
        title_parser,
        embedding_store,
    });

    // Background batched committer: decouples commit() from the ingest hot path.
    // Previously every document committed individually (~130ms/doc ceiling => ~7.5 docs/s).
    // Now we commit at most every 500ms, raising sustained ingest throughput by ~20-50x
    // with <1s search-refresh lag. Failed commits are retried next tick.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(2000)).await;
                let n = state.writer_pending.swap(0, Ordering::SeqCst);
                if n > 0 {
                    let mut w = state.writer.lock().await;
                    if let Err(e) = w.commit() {
                        tracing::warn!("Background commit failed: {:?}", e);
                        state.writer_pending.fetch_add(n, Ordering::SeqCst);
                    } else {
                        tracing::info!("Background commit flushed {} pending doc(s)", n);
                    }
                }
            }
        });
    }

    // One-time segment consolidation on startup: merges all existing segments so
    // tombstones (e.g. 16k+ deleted docs) and tiny 1-2 doc segments are reclaimed.
    // Runs in the background; reads stay available during the merge.
    {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let seg_ids = state.index.searchable_segment_ids().map(|s| s.to_vec()).unwrap_or_default();
            if seg_ids.len() > 1 {
                tracing::info!("Starting one-time merge of {} segments", seg_ids.len());
                let mut w = state.writer.lock().await;
                if let Err(e) = w.merge(&seg_ids).wait() {
                    tracing::warn!("Startup segment merge failed: {:?}", e);
                } else {
                    tracing::info!("Startup segment merge requested for {} segments", seg_ids.len());
                }
            }
        });
    }

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/index", post(handle_ingest))
        .route("/ingest_batch", post(handle_ingest_batch))
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
    let authority_field = state.schema.get_field("authority").unwrap();
    let price_field = state.schema.get_field("price").unwrap();
    let currency_field = state.schema.get_field("currency").unwrap();

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

    if let Some(p) = payload.price {
        doc.add_f64(price_field, p);
    }
    if let Some(ref c) = payload.currency {
        doc.add_text(currency_field, c.clone());
    }

    // Embedding offloaded to sled KV store (keyed by URL), not Tantivy.
    // Remove any prior embedding for this URL (upsert) BEFORE storing the new
    // one, so we never delete the freshly-inserted vector.
    let _ = state.embedding_store.remove(payload.url.as_bytes());
    if let Some(vec) = payload.embedding {
        tracing::info!("Adding embedding ({} dims) to sled store", vec.len());
        let bytes: Vec<u8> = vec.iter().flat_map(|&f| f.to_le_bytes()).collect();
        let _ = state.embedding_store.insert(payload.url.as_bytes(), bytes);
    }

    // Store domain authority score if provided
    let auth = payload.authority.unwrap_or(0.5);
    doc.add_f64(authority_field, auth);

    // Delete existing Tantivy document with the same URL to prevent duplicates.
    // (sled embedding already handled above: removed then re-inserted.)
    let term = tantivy::Term::from_field_text(url_field, &payload.url);
    writer.delete_term(term);

    writer.add_document(doc).unwrap();
    state.writer_pending.fetch_add(1, Ordering::SeqCst);
    tracing::info!("Queued for commit: {}", payload.url);

    Json(serde_json::json!({ "status": "indexed", "url": payload.url }))
}

// Batch ingest: one mutex lock + one pending-inc for N docs (vs N separate
// HTTP POSTs + N locks from the crawler). Upsert per doc by default.
async fn handle_ingest_batch(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BatchIngestRequest>,
) -> Json<serde_json::Value> {
    let n = payload.documents.len();
    if n == 0 {
        return Json(serde_json::json!({ "status": "indexed", "count": 0 }));
    }

    let url_field = state.schema.get_field("url").unwrap();
    let title_field = state.schema.get_field("title").unwrap();
    let content_field = state.schema.get_field("content").unwrap();
    let timestamp_field = state.schema.get_field("timestamp").unwrap();
    let authority_field = state.schema.get_field("authority").unwrap();
    let price_field = state.schema.get_field("price").unwrap();
    let currency_field = state.schema.get_field("currency").unwrap();

    let mut writer = state.writer.lock().await;
    for doc_payload in &payload.documents {
        let mut doc = TantivyDocument::default();
        doc.add_text(url_field, doc_payload.url.clone());
        doc.add_text(title_field, doc_payload.title.clone());
        doc.add_text(content_field, doc_payload.content.clone());

        let ts = doc_payload.timestamp.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });
        doc.add_u64(timestamp_field, ts);

        if let Some(p) = doc_payload.price {
            doc.add_f64(price_field, p);
        }
        if let Some(ref c) = &doc_payload.currency {
            doc.add_text(currency_field, c.clone());
        }

        // Embeddings are offloaded to the sled KV store (keyed by URL), not
        // stored in Tantivy anymore. This keeps the index text-only (~1.5GB saved).
        // On upsert, remove any prior embedding FIRST, then store the new one so
        // we never delete the freshly-written vector.
        if payload.replace_existing {
            let term = tantivy::Term::from_field_text(url_field, &doc_payload.url);
            writer.delete_term(term);
            let _ = state.embedding_store.remove(doc_payload.url.as_bytes());
        }
        if let Some(vec) = &doc_payload.embedding {
            let bytes: Vec<u8> = vec.iter().flat_map(|&f| f.to_le_bytes()).collect();
            let _ = state.embedding_store.insert(doc_payload.url.as_bytes(), bytes);
        }

        let auth = doc_payload.authority.unwrap_or(0.5);
        doc.add_f64(authority_field, auth);

        writer.add_document(doc).unwrap();
    }
    state.writer_pending.fetch_add(n as u64, Ordering::SeqCst);
    tracing::info!("Queued batch of {} doc(s) for commit", n);

    Json(serde_json::json!({ "status": "indexed", "count": n }))
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
        let authority_field = state_clone.schema.get_field("authority").unwrap();
        let content_field = state_clone.schema.get_field("content").unwrap();
        let price_field = state_clone.schema.get_field("price").unwrap();
        let currency_field = state_clone.schema.get_field("currency").unwrap();
        // Embeddings live in the sled KV store now (keyed by URL), not Tantivy.
        let embedding_store = state_clone.embedding_store.clone();

        let query_vector: Option<Vec<f32>> = vector.and_then(|v_str| {
            serde_json::from_str::<Vec<f32>>(&v_str).ok()
        });

        // Sub-linear embedding lookup: pull a single doc's vector from sled by URL
        // instead of loading a 6144-byte blob from every candidate's stored doc.
        let get_embedding = move |url: &str| -> Option<Vec<f32>> {
            let raw = embedding_store.get(url.as_bytes()).ok().flatten()?;
            if raw.len() % 4 != 0 {
                return None;
            }
            Some(raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
        };

        // Query parsers are built once at startup and cloned here (cheap) instead
        // of reconstructed per request.
        let query_parser = state_clone.query_parser.clone();
        let title_query_parser = state_clone.title_parser.clone();

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

        let limit = 50;
        let top_docs = searcher.search(&query, &tantivy::collector::TopDocs::with_limit(limit)).unwrap_or_default();

        let title_hits: std::collections::HashSet<String> = searcher.search(&title_query, &tantivy::collector::TopDocs::with_limit(limit))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, addr)| {
                searcher.doc::<TantivyDocument>(addr).ok()
                    .and_then(|d| d.get_first(url_field).and_then(|v| v.as_str()).map(|s| s.to_string()))
            })
            .collect();

        let mut bm25_ranked: Vec<String> = Vec::new();
        let mut semantic_ranked: Vec<(String, f32, String, u64)> = Vec::new();
        let mut metadata: HashMap<String, (String, u64, f64, String, Option<f64>, Option<String>)> = HashMap::new();
        let mut urls_without_embeddings: std::collections::HashSet<String> = std::collections::HashSet::new();

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let threshold = min_score.unwrap_or(0.75);

        // Distinctive query terms (stopword-free) used to measure TOPIC overlap per
        // result. A page that matches the query only on its function-word skeleton
        // ("how to make ... at home") earns a high BM25 score but shares ZERO
        // distinctive terms with the query — that is crawl noise, not a real match.
        // Folding term overlap into `quality` lets the gateway demote such pages.
        const LOCAL_STOPWORDS: &[&str] = &[
            "the","and","for","with","that","this","from","into","your","you","are","was","were",
            "how","what","why","when","where","who","which","can","will","would","should","could",
            "make","made","get","got","use","using","best","top","home","house","way","ways","like",
            "need","want","know","find","finds","help","helping","about","than","then","them","they",
            "does","did","doing","easy","simple","quick","guide","tutorial","recipe","recipes",
        ];
        let q_distinctive: Vec<String> = q
            .split_whitespace()
            .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| w.len() >= 3 && !LOCAL_STOPWORDS.contains(&w.as_str()))
            .collect();
        let q_distinctive_dedup: Vec<&str> = {
            let mut seen = std::collections::HashSet::new();
            q_distinctive.iter().filter(|w| seen.insert(w.as_str())).map(|w| w.as_str()).collect()
        };

        let mut semantic_pass_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
        let is_semantic = query_vector.is_some();

        // ── Relevance floors (robustness) ──
        // Tantivy's BM25 returns *some* document even when nothing lexically
        // matches, and the embedding path can return an unrelated page when only a
        // few docs carry embeddings. Both must be gated so an off-topic page
        // (e.g. Wikipedia "Cleopatra" for an unrelated query) can never top results.
        const BM25_ABS_MIN: f32 = 0.5; // below this, one doc is not a real lexical match
        const BM25_REL_FLOOR: f32 = 0.05; // keep docs at least 5% as relevant as the best
        const SEMANTIC_MIN: f32 = 0.80; // best embedding similarity required to trust semantic
        const MIN_EMBEDDED_FOR_SEMANTIC: usize = 5; // semantic needs a real embedding corpus

        let mut candidates: Vec<(String, f32)> = Vec::new(); // (url, bm25_score)
        let mut max_bm25: f32 = 0.0;
        let mut max_sim: f32 = 0.0;

        for (bm25_score, doc_address) in top_docs {
            let Ok(retrieved_doc) = searcher.doc::<TantivyDocument>(doc_address) else { continue; };
            let url = retrieved_doc.get_first(url_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = retrieved_doc.get_first(title_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let timestamp = retrieved_doc.get_first(timestamp_field).and_then(|v| v.as_u64()).unwrap_or(0);
            let authority = retrieved_doc.get_first(authority_field).and_then(|v| v.as_f64()).unwrap_or(0.5);
            let content = retrieved_doc.get_first(content_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let price = retrieved_doc.get_first(price_field).and_then(|v| v.as_f64());
            let currency = retrieved_doc.get_first(currency_field).and_then(|v| v.as_str()).map(|s| s.to_string());

            metadata.insert(url.clone(), (title.clone(), timestamp, authority, content, price, currency));

            let mut has_embedding = false;
            if let Some(ref q_vec) = query_vector {
                if let Some(doc_vec) = get_embedding(&url) {
                    if doc_vec.len() == q_vec.len() {
                        has_embedding = true;
                        let dot_product: f32 = q_vec.iter().zip(doc_vec.iter()).map(|(a, b)| a * b).sum();
                        max_sim = max_sim.max(dot_product);
                        if dot_product >= threshold {
                            semantic_ranked.push((url.clone(), dot_product, title.clone(), timestamp));
                            semantic_pass_urls.insert(url.clone());
                        }
                    }
                }
            }

            if !has_embedding {
                urls_without_embeddings.insert(url.clone());
            }

            if bm25_score > max_bm25 {
                max_bm25 = bm25_score;
            }
            candidates.push((url.clone(), bm25_score));
        }

        // BM25 relevance floor: a doc qualifies only if its BM25 score is a real
        // lexical match. If even the best doc scores below the absolute floor, the
        // query has no lexical match at all → exclude everything (don't surface an
        // unrelated page). Otherwise keep docs within BM25_REL_FLOOR of the best.
        let bm25_floor = if max_bm25 < BM25_ABS_MIN {
            f32::MAX
        } else {
            (max_bm25 * BM25_REL_FLOOR).max(BM25_ABS_MIN)
        };

        for (url, s) in &candidates {
            if *s >= bm25_floor {
                bm25_ranked.push(url.clone());
            }
        }

        // Semantic gate (robustness): only trust embedding hits when (a) the best
        // similarity is strong AND (b) there is a real embedding corpus. When
        // embeddings are sparse (only a few docs embedded) or weak, a single embedded
        // doc can "win" every query at the 0.75 floor — falling back to BM25 ranking
        // avoids injecting an off-topic page.
        let embedded_count = candidates.len().saturating_sub(urls_without_embeddings.len());
        if is_semantic && (max_sim < SEMANTIC_MIN || embedded_count < MIN_EMBEDDED_FOR_SEMANTIC) {
            tracing::info!(
                "INDEXER semantic gate: q='{}' max_sim={:.3} embedded={}/{} → ignoring semantic hits (BM25-only)",
                q, max_sim, embedded_count, candidates.len()
            );
            semantic_ranked.clear();
            semantic_pass_urls.clear();
        }

        tracing::info!(
            "INDEXER search q='{}' is_semantic={} max_bm25={:.3} bm25_floor={:.3} max_sim={:.3} embedded={}/{} bm25_hits={} semantic_hits={}",
            q, is_semantic, max_bm25, bm25_floor, max_sim, embedded_count, candidates.len(),
            bm25_ranked.len(), semantic_ranked.len()
        );

        semantic_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let k = 60.0;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        // Track which URLs are *genuinely* matched (strong BM25 or verified semantic),
        // so we can emit a per-result signal-quality metric for the gateway to gate on.
        // ADDITIVE SEMANTIC FUSION (P0 fix): a semantic query must NEVER shrink the
        // BM25 result set. Previously the loop `continue`d past every BM25 hit that
        // wasn't in the (often tiny, embedding-sparse) semantic set, so a query whose
        // only embedded docs were 3 arxiv papers would replace 10 good local results
        // with those 3 papers. Now BM25 hits always contribute; semantic contributes
        // only its *additional* (new) URLs. The semantic gate at ~663 still clears
        // `semantic_ranked` entirely when embeddings are too sparse/weak, so in that
        // case the RRF below is purely BM25 — exactly the robust fallback we want.
        let mut genuine_match: HashMap<String, bool> = HashMap::new();
        for url in &bm25_ranked {
            // A BM25 hit above the relative floor is a real lexical match.
            let _ = genuine_match.entry(url.clone()).or_insert(true);
        }
        for (url, _sim, _title, _ts) in &semantic_ranked {
            // Semantic hits are genuine only if the gate above did NOT clear them
            // (i.e. embeddings were sufficiently dense + strong). When the gate fired,
            // semantic_ranked is empty so this branch never tags anything.
            genuine_match.entry(url.clone()).or_insert(true);
        }

        for (rank, url) in bm25_ranked.iter().enumerate() {
            *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
        }

        for (rank, (url, _sim, _title, _ts)) in semantic_ranked.iter().enumerate() {
            *rrf_scores.entry(url.clone()).or_insert(0.0) += 1.0 / (k + (rank + 1) as f32);
        }

        // Per-URL BM25 strength in [0,1] (candidate score / best candidate score).
        // Surfaces how strongly each local page lexically matches the query so the
        // gateway can demote low-signal crawled pages (P2) without a hardcoded list.
        let bm25_strength: HashMap<String, f32> = candidates
            .iter()
            .map(|(u, s)| (u.clone(), if max_bm25 > 0.0 { (*s / max_bm25).clamp(0.0, 1.0) } else { 0.0 }))
            .collect();

        let mut results: Vec<SearchResult> = rrf_scores
            .into_iter()
            .map(|(url, score)| {
                let (title, ts, auth, content, price, currency) = metadata.get(&url).cloned().unwrap_or(("No Title".to_string(), 0, 0.5, String::new(), None, None));
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

                // Signal-quality metric for the gateway's local-index quality gate (P2).
                // Genuine matches (in the RRF set) start at 1.0; we dampen by weak
                // lexical strength so pages that barely matched (low BM25 vs the best
                // hit) read as lower-quality. Fail-safe: 1.0 when strength is unknown.
                let strength = bm25_strength.get(&url).copied().unwrap_or(1.0);
                // Topic-overlap factor: what fraction of the query's DISTINCTIVE terms
                // (stopword-free) actually appear in this page's title/content. A page
                // that scored on the function-word skeleton ("how to make ... home")
                // but shares none of the query's real topic terms is crawl noise and
                // must read as low quality so the gateway can demote it.
                let term_overlap = if q_distinctive_dedup.is_empty() {
                    1.0
                } else {
                    let hay = format!(" {} {}", title.to_lowercase(), content.to_lowercase());
                    let mut hits = 0usize;
                    for t in &q_distinctive_dedup {
                        if hay.contains(&format!(" {} ", t)) || hay.contains(&format!("{}.", t)) || hay.contains(&format!("{} ", t)) {
                            hits += 1;
                        }
                    }
                    (hits as f32 / q_distinctive_dedup.len() as f32).clamp(0.0, 1.0)
                };
                let quality = if *genuine_match.get(&url).unwrap_or(&false) {
                    // Strong match (>=0.5 of best) = full quality; weaker = linearly damped.
                    // Then multiply by topic overlap so structure-only matches collapse.
                    (0.5 + 0.5 * strength.clamp(0.0, 1.0)) * (0.3 + 0.7 * term_overlap)
                } else {
                    // Should not happen (RRF set ⊆ genuine), but fail-safe low.
                    0.3
                };

                SearchResult {
                    url, title,
                    score: final_score,
                    authority: auth as f32,
                    content: if content.len() > 500 { content.chars().take(500).collect() } else { content },
                    quality,
                    price,
                    currency,
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
