# IntentForge API Endpoint Expansion Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Add `/images`, `/videos`, `/news` endpoints to the IntentForge gateway, and make the existing `/search` endpoint more intelligent by routing video/image/news-intent queries to the appropriate specialized sources.

**Architecture:** All three new endpoints are thin wrappers over existing services — SearXNG for images+news (via `categories=images`/`categories=news`), Invidious for videos. The `/search` endpoint's intent analysis already classifies queries; we add routing logic so video-intent queries pull from Invidious, news-intent from SearXNG news, etc. No new services or API keys needed — SearXNG already has bing_news, google_news, bing_images, google_images engines built-in, they just need to be enabled in settings.yml.

**Tech Stack:** Rust (Axum + Tokio + reqwest), SearXNG categories API, Invidious API, existing circuit breaker + cache infrastructure.

**Files to modify:**
- `services/meta-search-engines/searxng/searxng/settings.yml` — enable image+news engines
- `services/gateway/src/main.rs` — add 3 new handlers + new data types + route registration

---

## Task 1: Enable SearXNG Image & News Engines

**Objective:** Add bing_images, bing_news, google_news engines to SearXNG's `keep_only` list so `categories=images` and `categories=news` queries return real results.

**Files:**
- Modify: `services/meta-search-engines/searxng/searxng/settings.yml`

**Changes:**

Add these engines to the `keep_only` list (after `marginalia`):
```yaml
      - bing_images
      - bing_news
      - google_news
```

Also add per-engine config entries (at the end of the `engines:` section):
```yaml
  - name: bing_images
    disabled: false
    timeout: 3.0

  - name: bing_news
    disabled: false
    timeout: 3.0

  - name: google_news
    disabled: false
    timeout: 3.0
```

**Verify:**
```bash
# Restart SearXNG
docker compose -f services/docker-compose.yml restart searxng
sleep 5

# Test image search
curl -s "http://127.0.0.1:8080/search?q=cats&format=json&categories=images&pageno=1" | python -m json.tool | head -30

# Test news search
curl -s "http://127.0.0.1:8080/search?q=AI&format=json&categories=news&pageno=1" | python -m json.tool | head -30
```

Both should return results with `img_src`/`thumbnail` (images) or `publishedDate` (news) fields.

**Commit:**
```bash
git add services/meta-search-engines/searxng/searxng/settings.yml
git commit -m "feat(searxng): enable image and news engine categories"
```

---

## Task 2: Add New Data Types for Images, Videos, News

**Objective:** Define response structs for the three new endpoints.

**Files:**
- Modify: `services/gateway/src/main.rs` (after `InvidiousResult` struct, ~line 82)

**Add these structs:**

```rust
// ─── Image Result (from SearXNG categories=images) ────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxImageResult {
    title: String,
    url: String,
    #[serde(default)]
    img_src: String,        // full-size image URL
    #[serde(default)]
    thumbnail: String,      // thumbnail URL
    #[serde(default)]
    content: String,        // alt text / description
    engine: String,
    #[serde(default)]
    source: String,         // original site (e.g. "unsplash.com")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxImageResponse {
    results: Vec<SearxImageResult>,
}

// ─── News Result (from SearXNG categories=news) ───────────────────
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxNewsResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    engine: String,
    #[serde(default)]
    publishedDate: String,  // ISO date string from SearXNG
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SearxNewsResponse {
    results: Vec<SearxNewsResult>,
}

// ─── Video Result (from Invidious) ────────────────────────────────
#[derive(Serialize)]
struct VideoResult {
    title: String,
    url: String,            // youtube.com/watch?v=ID
    #[serde(default)]
    description: String,
    #[serde(default)]
    video_id: String,
    #[serde(default)]
    thumbnail: String,      // i.ytimg.com/vi/ID/hqdefault.jpg
    #[serde(default)]
    source: String,         // always "invidious"
}

// ─── Unified API Response Shapes ──────────────────────────────────
#[derive(Serialize)]
struct ImageResponse {
    results: Vec<ImageResult>,
    count: usize,
    query: String,
}

#[derive(Serialize)]
struct ImageResult {
    title: String,
    url: String,
    image_url: String,
    thumbnail_url: String,
    #[serde(default)]
    description: String,
    source: String,         // engine name that returned it
}

#[derive(Serialize)]
struct VideoResponse {
    results: Vec<VideoResult>,
    count: usize,
    query: String,
}

#[derive(Serialize)]
struct NewsResponse {
    results: Vec<NewsResult>,
    count: usize,
    query: String,
}

#[derive(Serialize)]
struct NewsResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    published_at: String,
    source: String,         // engine name
}
```

**Verify:** Code compiles:
```bash
cd services/gateway && cargo check
```

**Commit:**
```bash
git add services/gateway/src/main.rs
git commit -m "feat(gateway): add data types for image/video/news endpoints"
```

---

## Task 3: Implement `/images` Endpoint

**Objective:** New handler that calls SearXNG with `categories=images` and returns normalized image results.

**Files:**
- Modify: `services/gateway/src/main.rs`

**Add handler function (before `main`):**

```rust
async fn handle_images(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // Cache check
    let cache_key = format!("images:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Call SearXNG with categories=images
    let searx_url = format!(
        "http://127.0.0.1:8080/search?q={}&format=json&categories=images&pageno=1",
        q_encoded
    );

    let results: Vec<ImageResult> = match state.http_client.get(&searx_url).send().await {
        Ok(resp) => match resp.json::<SearxImageResponse>().await {
            Ok(data) => data.results.into_iter().map(|r| ImageResult {
                title: r.title,
                url: r.url,
                image_url: if r.img_src.is_empty() { r.thumbnail.clone() } else { r.img_src },
                thumbnail_url: r.thumbnail,
                description: r.content,
                source: r.engine,
            }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG image parse error: {}", e);
                vec![]
            }
        },
        Err(e) => {
            tracing::warn!("SearXNG image request error: {}", e);
            vec![]
        }
    };

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    Json(response)
}
```

**Register route** (in `main`, after the `/search/fast` route):
```rust
        .route("/images", get(handle_images))
```

**Verify:**
```bash
cd services/gateway && cargo check
# After building and deploying:
curl -s "http://127.0.0.1:4000/images?q=cats" | python -m json.tool | head -30
```

**Commit:**
```bash
git add services/gateway/src/main.rs
git commit -m "feat(gateway): add /images endpoint via SearXNG categories=images"
```

---

## Task 4: Implement `/videos` Endpoint

**Objective:** New handler that calls Invidious and returns normalized video results with thumbnails.

**Files:**
- Modify: `services/gateway/src/main.rs`

**Add handler function:**

```rust
async fn handle_videos(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // Cache check
    let cache_key = format!("videos:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Call Invidious
    let invidious_url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);

    let results: Vec<VideoResult> = match state.http_client.get(&invidious_url).send().await {
        Ok(resp) => match resp.json::<Vec<InvidiousResult>>().await {
            Ok(data) => data.into_iter()
                .filter(|r| r.result_type.as_deref() == Some("video"))
                .filter_map(|r| {
                    let vid = r.video_id?;
                    let title = r.title.unwrap_or_default();
                    let description = r.description.unwrap_or_default();
                    Some(VideoResult {
                        title,
                        url: format!("https://www.youtube.com/watch?v={}", vid),
                        description,
                        thumbnail: format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", vid),
                        video_id: vid,
                        source: "invidious".to_string(),
                    })
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Invidious parse error: {}", e);
                vec![]
            }
        },
        Err(e) => {
            tracing::warn!("Invidious request error: {}", e);
            vec![]
        }
    };

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    Json(response)
}
```

**Register route:**
```rust
        .route("/videos", get(handle_videos))
```

**Verify:**
```bash
cd services/gateway && cargo check
# After building:
curl -s "http://127.0.0.1:4000/videos?q=rust+programming" | python -m json.tool | head -30
```

**Commit:**
```bash
git add services/gateway/src/main.rs
git commit -m "feat(gateway): add /videos endpoint via Invidious"
```

---

## Task 5: Implement `/news` Endpoint

**Objective:** New handler that calls SearXNG with `categories=news` and returns normalized news results with published dates.

**Files:**
- Modify: `services/gateway/src/main.rs`

**Add handler function:**

```rust
async fn handle_news(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let q = params.q.clone();
    let q_encoded = urlencoding::encode(&q);

    // Cache check
    let cache_key = format!("news:{}", q.to_lowercase().trim());
    if let Some(cached) = state.cache.get(&cache_key) {
        let value: serde_json::Value = serde_json::from_str(&cached).unwrap_or(serde_json::json!({}));
        return Json(value);
    }

    // Call SearXNG with categories=news
    let searx_url = format!(
        "http://127.0.0.1:8080/search?q={}&format=json&categories=news&pageno=1",
        q_encoded
    );

    let results: Vec<NewsResult> = match state.http_client.get(&searx_url).send().await {
        Ok(resp) => match resp.json::<SearxNewsResponse>().await {
            Ok(data) => data.results.into_iter().map(|r| NewsResult {
                title: r.title,
                url: r.url,
                description: r.content,
                published_at: r.publishedDate,
                source: r.engine,
            }).collect(),
            Err(e) => {
                tracing::warn!("SearXNG news parse error: {}", e);
                vec![]
            }
        },
        Err(e) => {
            tracing::warn!("SearXNG news request error: {}", e);
            vec![]
        }
    };

    let response = serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": q,
    });

    // Cache for 5 minutes
    let response_json = serde_json::to_string(&response).unwrap_or_default();
    if !results.is_empty() {
        state.cache.put(cache_key, response_json, Duration::from_secs(300));
    }

    Json(response)
}
```

**Register route:**
```rust
        .route("/news", get(handle_news))
```

**Verify:**
```bash
cd services/gateway && cargo check
# After building:
curl -s "http://127.0.0.1:4000/news?q=artificial+intelligence" | python -m json.tool | head -30
```

**Commit:**
```bash
git add services/gateway/src/main.rs
git commit -m "feat(gateway): add /news endpoint via SearXNG categories=news"
```

---

## Task 6: Enhance `/search` with Media-Aware Intent Routing

**Objective:** Make the main `/search` endpoint smarter — when intent detects video/image/news queries, include results from the specialized sources alongside regular web results.

**Files:**
- Modify: `services/gateway/src/main.rs` (in `handle_search`, around the fan-out section ~line 1864)

**Logic:**

The existing intent engine already classifies queries. We add conditional fan-out:

1. If intent contains "video" or query contains video-related terms → add Invidious results to the unified response
2. If intent contains "news" or "fresh" → add SearXNG `categories=news` results
3. If intent contains "image" or "visual" → add SearXNG `categories=images` results

**In the parallel fan-out section, add conditional futures:**

```rust
    // Conditional media fan-out based on intent
    let q_lower = params.q.to_lowercase();
    let is_video_intent = intent_resp.intent.contains("video")
        || q_lower.contains("video")
        || q_lower.contains("watch")
        || q_lower.contains("tutorial");
    let is_news_intent = intent_resp.intent.contains("news")
        || intent_resp.intent.contains("fresh")
        || q_lower.contains("news")
        || q_lower.contains("latest");
    let is_image_intent = intent_resp.intent.contains("image")
        || intent_resp.intent.contains("visual")
        || q_lower.contains("image")
        || q_lower.contains("photo")
        || q_lower.contains("picture");

    // Video fan-out (only if video intent and invidious not circuit-broken)
    let video_fut = async {
        if !is_video_intent || invidious_open {
            return Ok(vec![]) as Result<Vec<InvidiousResult>, reqwest::Error>;
        }
        let url = format!("http://127.0.0.1:3000/api/v1/search?q={}", q_encoded);
        match client_ref.get(&url).send().await {
            Ok(resp) => resp.json::<Vec<InvidiousResult>>().await,
            Err(e) => Err(e),
        }
    };

    // News fan-out
    let news_fut = async {
        if !is_news_intent || searx_open {
            return Ok(SearxNewsResponse { results: vec![] }) as Result<SearxNewsResponse, reqwest::Error>;
        }
        let url = format!(
            "http://127.0.0.1:8080/search?q={}&format=json&categories=news&pageno=1",
            q_encoded
        );
        match client_ref.get(&url).send().await {
            Ok(resp) => resp.json::<SearxNewsResponse>().await,
            Err(e) => Err(e),
        }
    };
```

Then join the extra futures alongside the existing ones:
```rust
    let (indexer_res, searx_results, whoogle_res, invidious_res, video_res, news_res) = tokio::join!(
        indexer_fut,
        searx_fut,
        whoogle_fut,
        invidious_fut,
        video_fut,
        news_fut,
    );
```

And merge video/news results into the unified result set with appropriate source tags and RRF contributions.

**Verify:**
```bash
cd services/gateway && cargo check
# Test video-intent query
curl -s "http://127.0.0.1:4000/search?q=how+to+cook+pasta+video" | python -m json.tool | grep -c "invidious"
# Test news-intent query
curl -s "http://127.0.0.1:4000/search?q=latest+AI+news" | python -m json.tool | grep -c "news"
```

**Commit:**
```bash
git add services/gateway/src/main.rs
git commit -m "feat(gateway): add media-aware intent routing to /search"
```

---

## Task 7: Rebuild, Deploy, and Full Integration Test

**Objective:** Build the gateway, deploy, and verify all endpoints work end-to-end.

**Steps:**

```bash
# 1. Rebuild gateway
docker compose -f services/docker-compose.yml build gateway

# 2. Deploy (stop/rm/create/start pattern for MSYS)
docker stop intentforge-gateway && docker rm intentforge-gateway
docker compose -f services/docker-compose.yml create gateway
docker start intentforge-gateway

# 3. Wait for startup
sleep 3

# 4. Test all endpoints
echo "=== /health ==="
curl -s http://127.0.0.1:4000/health

echo -e "\n=== /search ==="
curl -s "http://127.0.0.1:4000/search?q=rust+programming" | python -m json.tool | head -20

echo -e "\n=== /images ==="
curl -s "http://127.0.0.1:4000/images?q=cats" | python -m json.tool | head -20

echo -e "\n=== /videos ==="
curl -s "http://127.0.0.1:4000/videos?q=rust+async" | python -m json.tool | head -20

echo -e "\n=== /news ==="
curl -s "http://127.0.0.1:4000/news?q=AI" | python -m json.tool | head -20

echo -e "\n=== /search (video intent) ==="
curl -s "http://127.0.0.1:4000/search?q=how+to+bake+bread+video" | python -m json.tool | head -20

echo -e "\n=== /search (news intent) ==="
curl -s "http://127.0.0.1:4000/search?q=latest+tech+news" | python -m json.tool | head -20
```

**Commit:**
```bash
git add -A
git commit -m "feat: complete API endpoint expansion — images, videos, news, smart routing"
```

---

## Summary of Endpoints

| Endpoint       | Source              | Response Fields                              |
|----------------|---------------------|----------------------------------------------|
| `GET /search`  | SearXNG + Whoogle + Invidious + Indexer | intent, constraints, results with scores |
| `GET /images`  | SearXNG (categories=images) | image_url, thumbnail_url, source engine  |
| `GET /videos`  | Invidious           | video_id, youtube URL, thumbnail, description |
| `GET /news`    | SearXNG (categories=news) | published_at, description, source engine  |
| `GET /search/fast` | Local index only | fast local results                          |

All endpoints accept `?q=<query>` and return JSON. Results are cached for 5 minutes. Circuit breaker protects against downstream failures.
