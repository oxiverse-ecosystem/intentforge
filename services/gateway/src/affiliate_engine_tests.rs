// ─────────────────────────────────────────────────────────────────────────────
// Generic affiliate template engine — per-merchant matching tests.
//
// These tests prove the engine:
//   1. Loads affiliate_programs.json + networks.json at runtime
//   2. Matches a result URL's merchant domain against program host_suffix entries
//   3. Substitutes URL + credential env-vars into the affiliate template
//   4. Degrades gracefully when a credential is missing (affiliate: null)
//   5. Never adds user-identifying info to subid (only coarse merchant host)
//   6. Falls back to catch-all networks when no per-merchant program matches
//   7. Adding a new merchant requires editing DATA ONLY (no code changes)
//
// NO hardcoded merchant/network knowledge lives in the engine — everything
// comes from data/commerce/affiliate_programs.json + data/commerce/networks.json.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
use serde_json::{json, Value};

/// Build a per-merchant program for tests (mirrors the data-driven shape).
fn program(
    id: &str,
    host_suffix: Option<&str>,
    kind: &str,
    template: &str,
    params: HashMap<String, String>,
    key_env: Option<&str>,
) -> AffiliateNetwork {
    AffiliateNetwork {
        id: id.to_string(),
        kind: kind.to_string(),
        enabled: true,
        priority: 1,
        network: format!("Net_{}", id),
        template: template.to_string(),
        params,
        key_env: key_env.map(|s| s.to_string()),
        param_env: HashMap::new(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: host_suffix.map(|s| s.to_string()),
    }
}

/// Build a catch-all network (no host_suffix) for tests.
fn catch_all(
    id: &str,
    kind: &str,
    template: &str,
    params: HashMap<String, String>,
    key_env: Option<&str>,
) -> AffiliateNetwork {
    program(id, None, kind, template, params, key_env)
}

fn ranked_urls(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .map(|r| r.get("url").and_then(|v| v.as_str()).unwrap_or_default().to_string())
        .collect()
}

fn affiliate_urls(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|r| r.get("affiliate").and_then(|a| a.get("url")).and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect()
}

// ── 1. Per-merchant matching by host_suffix ─────────────────────────────────

#[test]
fn per_merchant_program_matches_by_host_suffix() {
    // Amazon program with host_suffix "amazon." should match amazon.com URLs
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ctx = AffiliateCtx::from_networks(vec![]);
    // We need to test match_program directly since decorate_affiliate uses both programs and networks
    // But match_program is on AffiliateCtx which has programs field...
    // Let's test via a full ctx with programs
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let url = "https://www.amazon.com/dp/B08N5WRWNW";
    let matched = ctx.match_program(url);
    assert!(matched.is_some(), "amazon program should match amazon.com URL");
    assert_eq!(matched.unwrap().id, "amazon");

    // Should NOT match a non-amazon URL
    let url2 = "https://www.ebay.com/itm/12345";
    assert!(ctx.match_program(url2).is_none(), "amazon program should NOT match ebay URL");
}

#[test]
fn per_merchant_program_matches_international_hosts() {
    // "amazon." should match amazon.co.uk, amazon.de, etc.
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    for host in ["amazon.com", "amazon.co.uk", "amazon.de", "amazon.co.jp", "www.amazon.com"] {
        let url = format!("https://{}/dp/B08N5WRWNW", host);
        assert!(
            ctx.match_program(&url).is_some(),
            "amazon program should match {}",
            host
        );
    }

    // Should NOT match hosts that merely contain "amazon" as a substring
    for host in ["notamazon.com", "amazonian-shop.com"] {
        let url = format!("https://{}/product", host);
        assert!(
            ctx.match_program(&url).is_none(),
            "amazon program should NOT match {}",
            host
        );
    }
}

#[test]
fn per_merchant_program_respects_label_boundary() {
    // "ebay." should match ebay.com but not notebay.com
    std::env::set_var("TEST_EBAY_KEY", "ebay-key");
    let ebay = program(
        "ebay",
        Some("ebay."),
        "append_params",
        "{url}",
        HashMap::from([("mkrid".to_string(), "{key}".to_string())]),
        Some("TEST_EBAY_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![ebay],
        network_meta: vec![],
    };

    assert!(ctx.match_program("https://www.ebay.com/itm/123").is_some());
    assert!(ctx.match_program("https://ebay.co.uk/itm/456").is_some());
    assert!(ctx.match_program("https://notebay.com/itm/789").is_none());
}

// ── 2. Template substitution ─────────────────────────────────────────────────

#[test]
fn per_merchant_template_substitutes_key_and_url() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert_eq!(aff["network"], "Net_amazon");
    assert_eq!(aff["disclosed"], true);
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("tag=amzn-20"), "key should be substituted: {}", url);
    assert!(url.contains("https://www.amazon.com/dp/B08N5WRWNW"), "url should be present: {}", url);
}

#[test]
fn per_merchant_wrap_template_encodes_destination() {
    std::env::set_var("TEST_SOVRN_KEY", "sovrn-key");
    let sovrn = program(
        "sovrn",
        Some("shop."),
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::from([("cuid".to_string(), "{subid}".to_string())]),
        Some("TEST_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![sovrn],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://shop.example.com/product?ref=blog", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("key=sovrn-key"), "key should be substituted: {}", url);
    assert!(url.contains("cuid=shop.example.com"), "subid should be merchant host: {}", url);
    // The destination URL should be encoded inside the wrap
    assert!(url.contains("https%3A%2F%2Fshop.example.com"), "destination should be url-encoded: {}", url);
}

// ── 3. Graceful degradation when credential is missing ───────────────────────

#[test]
fn missing_credential_degrades_gracefully() {
    // Program with a key_env that is NOT set
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG_UNSET"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // No affiliate block should be added (graceful degradation)
    assert!(
        results[0].get("affiliate").is_none(),
        "missing key => affiliate should be omitted, results still returned"
    );
    // URL should be untouched
    assert_eq!(results[0]["url"], "https://www.amazon.com/dp/B08N5WRWNW");
}

#[test]
fn missing_credential_falls_back_to_catch_all() {
    // Per-merchant program exists but key is missing; catch-all network has key
    std::env::set_var("TEST_CATCHALL_KEY", "catchall-key");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG_UNSET"),
    );
    let catchall = catch_all(
        "sovrn",
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::new(),
        Some("TEST_CATCHALL_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![catchall],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Should fall back to the catch-all network
    let aff = &results[0]["affiliate"];
    assert_eq!(aff["network"], "Net_sovrn");
    assert_eq!(aff["disclosed"], true);
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("key=catchall-key"), "catch-all key should be used: {}", url);
}

// ── 4. Subid is non-identifying (coarse merchant host only) ─────────────────

#[test]
fn subid_is_coarse_host_not_user_identifying() {
    std::env::set_var("TEST_SOVRN_KEY", "sovrn-key");
    let sovrn = program(
        "sovrn",
        Some("shop."),
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::from([("cuid".to_string(), "{subid}".to_string())]),
        Some("TEST_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![sovrn],
        network_meta: vec![],
    };

    let dest = "https://shop.example.com/widget?ref=blog&utm_source=newsletter";
    let mut results = vec![
        json!({"url": dest, "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    let url = aff["url"].as_str().unwrap();
    // The cuid subid must be the coarse merchant host
    assert!(url.contains("cuid=shop.example.com"), "subid must be merchant host: {}", url);
    // No user-identifying info should appear
    assert!(!url.contains("uid="), "no user id in subid");
    assert!(!url.contains("session="), "no session id in subid");
    assert!(!url.contains("ip="), "no IP in subid");
    assert!(!url.contains("q="), "no query text in subid");
}

// ── 5. Fallback to catch-all when no per-merchant program matches ───────────

#[test]
fn falls_back_to_catch_all_when_no_program_matches() {
    std::env::set_var("TEST_SOVRN_KEY", "sovrn-key");
    // Program only matches "amazon." URLs
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    // Catch-all network
    let sovrn = catch_all(
        "sovrn",
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        HashMap::new(),
        Some("TEST_SOVRN_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![sovrn],
        programs: vec![amazon],
        network_meta: vec![],
    };

    // A non-amazon URL should fall back to the catch-all
    let mut results = vec![
        json!({"url": "https://www.ebay.com/itm/12345", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert_eq!(aff["network"], "Net_sovrn");
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("key=sovrn-key"), "catch-all key should be used: {}", url);
}

// ── 6. Order invariance with per-merchant matching ───────────────────────────

#[test]
fn per_merchant_decoration_does_not_change_ranking() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    std::env::set_var("TEST_EBAY_KEY", "ebay-key");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ebay = program(
        "ebay",
        Some("ebay."),
        "append_params",
        "{url}",
        HashMap::from([("mkrid".to_string(), "{key}".to_string())]),
        Some("TEST_EBAY_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon, ebay],
        network_meta: vec![],
    };

    let ranked_input = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "A", "score": 0.97}),
        json!({"url": "https://www.ebay.com/itm/12345", "title": "B", "score": 0.91}),
        json!({"url": "https://www.example.com/product", "title": "C", "score": 0.88}),
    ];

    // ENABLED run
    let mut enabled = ranked_input.clone();
    decorate_affiliate(&mut enabled, &ctx);
    let enabled_ranked = ranked_urls(&enabled);
    let enabled_aff = affiliate_urls(&enabled);

    // DISABLED run (no programs, no networks)
    let disabled_ctx = AffiliateCtx::from_networks(vec![]);
    let mut disabled = ranked_input.clone();
    decorate_affiliate(&mut disabled, &disabled_ctx);
    let disabled_ranked = ranked_urls(&disabled);
    let disabled_aff = affiliate_urls(&disabled);

    // Non-vacuity
    assert!(!enabled_aff.is_empty(), "enabled run must decorate");
    assert!(disabled_aff.is_empty(), "disabled run must not decorate");

    // Order invariance
    assert_eq!(
        enabled_ranked, disabled_ranked,
        "per-merchant decoration MUST NOT change ranked URL order"
    );
}

// ── 7. Disabled program is skipped ──────────────────────────────────────────

#[test]
fn disabled_program_is_skipped() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    let mut amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    amazon.enabled = false;
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Disabled program should not match
    assert!(results[0].get("affiliate").is_none(), "disabled program should not decorate");
}

// ── 8. Priority ordering among per-merchant programs ─────────────────────────

#[test]
fn higher_priority_program_wins() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    std::env::set_var("TEST_AMAZON_TAG2", "amzn-21");
    let mut amazon_low = program(
        "amazon_low",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    amazon_low.priority = 1;
    let mut amazon_high = program(
        "amazon_high",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG2"),
    );
    amazon_high.priority = 100;
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon_low, amazon_high],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert_eq!(aff["network"], "Net_amazon_high", "higher priority program should win");
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("tag=amzn-21"), "higher priority key should be used: {}", url);
}

// ── 9. Runtime data file loading ─────────────────────────────────────────────

#[test]
fn loads_affiliate_programs_and_networks_at_runtime() {
    let ctx = AffiliateCtx::load();
    // The shipped data files should load successfully
    assert!(
        !ctx.networks.is_empty() || !ctx.programs.is_empty(),
        "at least one of affiliate.json or affiliate_programs.json should yield networks"
    );
    // networks.json should load metadata
    assert!(
        !ctx.network_meta.is_empty(),
        "networks.json should yield network metadata"
    );
}

#[test]
fn shipped_affiliate_programs_have_valid_host_suffixes() {
    let ctx = AffiliateCtx::load();
    for prog in &ctx.programs {
        if let Some(ref suffix) = prog.host_suffix {
            assert!(
                !suffix.is_empty(),
                "program {:?} has empty host_suffix",
                prog.id
            );
            // Should end with "." for label-boundary matching
            assert!(
                suffix.ends_with('.'),
                "program {:?} host_suffix should end with '.' for label-boundary matching, got: {}",
                prog.id,
                suffix
            );
        }
    }
}

// ── 10. Adding a new merchant requires DATA ONLY (no code changes) ───────────

#[test]
fn new_merchant_can_be_added_by_data_only() {
    // This test proves the engine is fully data-driven: we simulate adding a
    // new merchant by constructing a program in code (as if it came from the
    // data file) and verify it works without any code changes.
    std::env::set_var("TEST_NEW_MERCHANT_KEY", "new-key");
    let new_merchant = program(
        "new_merchant",
        Some("newshop."),
        "append_params",
        "{url}",
        HashMap::from([("ref".to_string(), "{key}".to_string())]),
        Some("TEST_NEW_MERCHANT_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![new_merchant],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.newshop.com/product/123", "title": "New Product", "score": 0.9}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert_eq!(aff["network"], "Net_new_merchant");
    assert_eq!(aff["disclosed"], true);
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("ref=new-key"), "new merchant key should be substituted: {}", url);
    assert!(url.contains("https://www.newshop.com/product/123"), "url should be present: {}", url);
}

// ── 11. Idempotency with per-merchant matching ───────────────────────────────

#[test]
fn per_merchant_decoration_is_idempotent() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product", "score": 0.9}),
    ];

    decorate_affiliate(&mut results, &ctx);
    let first_pass = results[0]["affiliate"]["url"].as_str().unwrap().to_string();

    decorate_affiliate(&mut results, &ctx);
    let second_pass = results[0]["affiliate"]["url"].as_str().unwrap().to_string();

    assert_eq!(first_pass, second_pass, "idempotent: no double-wrap");
}

// ── 12. Multiple merchants in one result set ─────────────────────────────────

#[test]
fn multiple_merchants_decorated_with_different_programs() {
    std::env::set_var("TEST_AMAZON_TAG", "amzn-20");
    std::env::set_var("TEST_EBAY_KEY", "ebay-key");
    let amazon = program(
        "amazon",
        Some("amazon."),
        "append_params",
        "{url}",
        HashMap::from([("tag".to_string(), "{key}".to_string())]),
        Some("TEST_AMAZON_TAG"),
    );
    let ebay = program(
        "ebay",
        Some("ebay."),
        "append_params",
        "{url}",
        HashMap::from([("mkrid".to_string(), "{key}".to_string())]),
        Some("TEST_EBAY_KEY"),
    );
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon, ebay],
        network_meta: vec![],
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "A", "score": 0.97}),
        json!({"url": "https://www.ebay.com/itm/12345", "title": "B", "score": 0.91}),
        json!({"url": "https://www.example.com/product", "title": "C", "score": 0.88}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Amazon result should use amazon program
    assert_eq!(results[0]["affiliate"]["network"], "Net_amazon");
    assert!(results[0]["affiliate"]["url"].as_str().unwrap().contains("tag=amzn-20"));

    // eBay result should use ebay program
    assert_eq!(results[1]["affiliate"]["network"], "Net_ebay");
    assert!(results[1]["affiliate"]["url"].as_str().unwrap().contains("mkrid=ebay-key"));

    // Example.com result should have no affiliate block (no matching program, no catch-all)
    assert!(results[2].get("affiliate").is_none());
}
