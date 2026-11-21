// ─── Supabase Auth JWT verification (Phase 2) ─────────────────────────
// Provider contract correction D13: the live JWKS is EC/ES256, and the crate
// available here has no clean JWK→DecodingKey conversion. Validate access
// tokens server-side via Supabase Auth's /auth/v1/user endpoint and cache the
// validated subject for 5 minutes. A 200 with an `id` proves signature + exp.

use axum::http::HeaderMap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

struct Cache {
    entries: HashMap<u64, (String, Instant)>,
}

static CACHE: OnceLock<RwLock<Cache>> = OnceLock::new();
static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
const CACHE_TTL: Duration = Duration::from_secs(300);

fn cache() -> &'static RwLock<Cache> {
    CACHE.get_or_init(|| RwLock::new(Cache { entries: HashMap::new() }))
}

fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client")
    })
}

fn token_key(token: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut h);
    h.finish()
}

/// Extract a `Bearer *** token from request headers. Never logged.
pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = v.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Validate a Supabase access token and return the user's UUID (`sub`).
pub async fn validate_jwt(token: &str) -> Option<String> {
    let key = token_key(token);
    {
        let cache = cache().read();
        if let Some((uid, at)) = cache.entries.get(&key) {
            if at.elapsed() < CACHE_TTL {
                return Some(uid.clone());
            }
        }
    }

    let base = std::env::var("SUPABASE_URL").ok()?;
    let publishable = std::env::var("SUPABASE_PUBLISHABLE_KEY")
        .or_else(|_| std::env::var("SUPABASE_ANON_KEY"))
        .ok()?;
    let url = format!("{}/auth/v1/user", base.trim_end_matches('/'));
    let resp = client()
        .get(url)
        .header("apikey", publishable.trim())
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    let uid = body.get("id")?.as_str()?.to_string();
    if uid.is_empty() {
        return None;
    }
    cache().write().entries.insert(key, (uid.clone(), Instant::now()));
    Some(uid)
}

/// Convenience for the auth middleware and handlers.
pub async fn jwt_uid(headers: &HeaderMap) -> Option<String> {
    let token = bearer_token(headers)?;
    validate_jwt(&token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_token_extraction() {
        let mut h = HeaderMap::new();
        assert!(bearer_token(&h).is_none());
        h.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer abc.def.ghi".parse().unwrap(),
        );
        assert_eq!(bearer_token(&h).as_deref(), Some("abc.def.ghi"));
    }

    #[test]
    fn token_key_is_stable() {
        assert_eq!(token_key("same-token"), token_key("same-token"));
        assert_ne!(token_key("a"), token_key("b"));
    }
}
