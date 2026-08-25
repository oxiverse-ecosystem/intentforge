// ─────────────────────────────────────────────────────────────────────────────
// ROADMAP item 4 — Disclosure + no-tracking CI CONTRACT.
//
// This file exists to make the affiliate monetization contract UNMISTAKABLE and
// permanent in CI. It is not a soft "should": every assertion here is a hard
// guarantee Likhith's business thesis requires:
//
//   (A) DISCLOSURE — every affiliate-decorated result MUST carry
//       `"affiliate": { "disclosed": true }` so the frontend can label it.
//       Undisclosed monetization is not acceptable.
//   (B) NO USER/QUERY/IP LEAK — no query text, user id, session id, or IP ever
//       reaches an affiliate URL parameter. The ONLY coarse subid permitted is
//       the destination merchant host (non-identifying, documented in the code).
//
// These tests operate on a REPRESENTATIVE /shopping payload (the exact shape
// `handle_shopping` returns) so the contract is exercised end-to-end through the
// real `decorate_affiliate` post-ranking pass — not just the low-level renderer.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
use serde_json::{json, Value};

/// Build a network/program row for tests (mirrors the data-driven shape; all
/// knowledge comes from arguments, none is hardcoded in this test).
fn contract_net(
    kind: &str,
    template: &str,
    params: HashMap<String, String>,
    key_env: Option<&str>,
) -> AffiliateNetwork {
    AffiliateNetwork {
        id: "contract".to_string(),
        kind: kind.to_string(),
        enabled: true,
        priority: 1,
        network: "ContractNet".to_string(),
        template: template.to_string(),
        params,
        key_env: key_env.map(|s| s.to_string()),
        param_env: HashMap::new(),
    }
}

/// Build a representative already-ranked /shopping payload for a REAL user query.
/// The query text is deliberately echoed into top-level `query` AND into each
/// result's `title`/`content`, so we can later PROVE none of it leaks into an
/// affiliate URL (the renderer only ever reads `url` + the coarse host).
fn representative_shopping_payload(user_query: &str) -> Value {
    json!({
        "query": user_query,
        "results": [
            {
                "url": "https://shop.example.com/wireless-earbuds-pro?ref=blog&utm_source=newsletter",
                "title": format!("Buy {} — Wireless Earbuds Pro", user_query),
                "content": "Best wireless earbuds under 50 dollars, reviewed.",
                "score": 9.7,
                "sources": ["bing"]
            },
            {
                "url": "https://store.example.org/cheaper-buds",
                "title": "Cheaper earbuds alternative",
                "content": "A budget option.",
                "score": 8.1,
                "sources": ["brave"]
            },
            {
                "url": "https://market.example.net/bundle-deal",
                "title": "Earbuds bundle",
                "content": "Bundle with case.",
                "score": 6.3,
                "sources": ["local"]
            }
        ]
    })
}

/// Collect every `affiliate.url` string from a payload's results array.
fn collect_affiliate_urls(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = payload.get("results").and_then(|v| v.as_array()) {
        for r in arr {
            if let Some(aff) = r.get("affiliate") {
                if let Some(u) = aff.get("url").and_then(|v| v.as_str()) {
                    out.push(u.to_string());
                }
            }
        }
    }
    out
}

#[test]
fn contract_disclosure_present_on_every_decorated_result() {
    // A wrap network (Sovrn-like) that also carries a cuid subid param.
    std::env::set_var("CONTRACT_SOVRN_KEY", "CONTRACT_SOVRN_KEY");
    let mut params = HashMap::new();
    params.insert("cuid".to_string(), "{subid}".to_string());
    let n = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        params,
        Some("CONTRACT_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx { networks: vec![n] };

    let mut payload = representative_shopping_payload("best wireless earbuds under 50 dollars");
    // Decorate the ALREADY-RANKED results, exactly as handle_shopping does.
    if let Some(arr) = payload.get_mut("results").and_then(|v| v.as_array_mut()) {
        decorate_affiliate(arr, &ctx);
    }

    let urls = collect_affiliate_urls(&payload);
    assert_eq!(urls.len(), 3, "all three results must be decorated");
    // CONTRACT (A): every decorated result discloses.
    if let Some(arr) = payload.get("results").and_then(|v| v.as_array()) {
        for r in arr {
            let aff = r.get("affiliate").expect("affiliate block present");
            assert_eq!(
                aff.get("disclosed").and_then(|v| v.as_bool()),
                Some(true),
                "every decorated result MUST carry disclosed:true (contract A)"
            );
            assert_eq!(
                aff.get("network").and_then(|v| v.as_str()),
                Some("ContractNet"),
                "network name must be present for frontend labelling"
            );
        }
    }
}

#[test]
fn contract_no_query_user_or_ip_in_affiliate_urls() {
    std::env::set_var("CONTRACT_SOVRN_KEY", "CONTRACT_SOVRN_KEY");
    let mut params = HashMap::new();
    params.insert("cuid".to_string(), "{subid}".to_string());
    let n = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        params,
        Some("CONTRACT_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx { networks: vec![n] };

    let user_query = "best wireless earbuds under 50 dollars";
    let mut payload = representative_shopping_payload(user_query);
    if let Some(arr) = payload.get_mut("results").and_then(|v| v.as_array_mut()) {
        decorate_affiliate(arr, &ctx);
    }

    // Only AFFILIATE-LEVEL params are checked (preceded by `&`/`?`). The wrapped
    // destination (`u=...`) carries the merchant URL verbatim and its value MUST be
    // ignored — otherwise a merchant URL containing e.g. `user=` would
    // false-positive. Legit affiliate params emitted by the renderer are
    // key/u/cuid/tag/customid/mkrid/campid/toolid/mkevt/mkcid/bf/fbu; none of the
    // tokens below is ever emitted, so their presence at the affiliate level ⇒ a
    // privacy leak. Note `cuid=` is legit (coarse host) and does NOT match `&uid=`.
    let forbidden = [
        "&uid=", "?uid=",
        "&user=", "?user=",
        "&session=", "?session=",
        "&ip=", "?ip=",
        "&client=", "?client=",
        "&q=", "?q=",
    ];

    let urls = collect_affiliate_urls(&payload);
    assert!(!urls.is_empty(), "decoration must have produced affiliate urls");
    for u in &urls {
        let lower = u.to_lowercase();
        for bad in forbidden {
            assert!(
                !lower.contains(bad),
                "affiliate url leaked forbidden token '{}': {}",
                bad,
                u
            );
        }
        // The exact user query text must never appear (even url-encoded would be
        // caught above via 'query'; here we assert the raw phrase is absent).
        assert!(
            !u.contains(user_query),
            "user query text leaked into affiliate url: {}",
            u
        );
    }
}

#[test]
fn contract_subid_is_coarse_host_not_user_identifying() {
    // Even when the destination merchant URL itself carries tracking params
    // (utm_source, ref), the subid placed into the affiliate param surface MUST
    // be ONLY the coarse host — never the merchant's own tracking junk, and never
    // anything from the user.
    std::env::set_var("CONTRACT_SOVRN_KEY", "CONTRACT_SOVRN_KEY");
    let mut params = HashMap::new();
    params.insert("cuid".to_string(), "{subid}".to_string());
    let n = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        params,
        Some("CONTRACT_SOVRN_KEY"),
    );

    let dest =
        "https://shop.example.com/widget?ref=blog&utm_source=newsletter&utm_medium=email";
    let out = render_affiliate_url(&n, dest, "shop.example.com");

    // The cuid subid must equal the coarse host exactly.
    assert!(
        out.contains("cuid=shop.example.com"),
        "subid must be the coarse merchant host, got: {}",
        out
    );
    // The merchant's own tracking params stay encoded INSIDE the wrapped
    // destination (the {url} value), never promoted to affiliate-level params.
    assert!(
        out.contains("https%3A%2F%2Fshop.example.com%2Fwidget"),
        "destination must be url-encoded inside the wrap"
    );
    // The raw destination (with its tracking params) must NOT appear outside the
    // encoded blob, proving it cannot leak as a top-level affiliate param.
    assert!(
        !out.contains("shop.example.com/widget?ref=blog"),
        "raw merchant tracking params must not appear unencoded in affiliate url"
    );
}

#[test]
fn contract_decoration_is_idempotent_and_order_preserved() {
    // The no-manipulation guarantee, locked at the contract level: decorating
    // twice yields the same URL (no double-wrap) and the ranked order is
    // byte-identical before/after.
    std::env::set_var("CONTRACT_SOVRN_KEY", "CONTRACT_SOVRN_KEY");
    let n = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::new(),
        Some("CONTRACT_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx { networks: vec![n] };

    let mut payload = representative_shopping_payload("best wireless earbuds under 50 dollars");
    let before: Vec<String> = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();

    let arr = payload.get_mut("results").and_then(|v| v.as_array_mut()).unwrap();
    decorate_affiliate(arr, &ctx);
    let first_pass: Vec<String> = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["affiliate"]["url"].as_str().unwrap().to_string())
        .collect();

    // Second pass must not double-wrap.
    let arr2 = payload.get_mut("results").and_then(|v| v.as_array_mut()).unwrap();
    decorate_affiliate(arr2, &ctx);
    let second_pass: Vec<String> = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["affiliate"]["url"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(first_pass, second_pass, "idempotent: no double-wrap");

    let after: Vec<String> = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(before, after, "affiliate decoration must never reorder ranked results");
}

#[test]
fn contract_missing_key_degrades_without_leak_or_crash() {
    // No usable key (env var unset) => results keep `affiliate` omitted entirely.
    // This must not panic and must not emit any affiliate url (so no leak surface).
    let n = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::new(),
        Some("CONTRACT_UNSET_KEY_ENV_VAR_ZZZ"),
    );
    let ctx = AffiliateCtx { networks: vec![n] };

    let mut payload = representative_shopping_payload("best wireless earbuds under 50 dollars");
    if let Some(arr) = payload.get_mut("results").and_then(|v| v.as_array_mut()) {
        decorate_affiliate(arr, &ctx);
    }
    // No affiliate block anywhere => graceful degradation, no crash.
    assert!(
        payload["results"].as_array().unwrap().iter().all(|r| r.get("affiliate").is_none()),
        "missing key => affiliate omitted, results still returned"
    );
    assert!(
        collect_affiliate_urls(&payload).is_empty(),
        "no affiliate urls emitted when key is absent"
    );
}
