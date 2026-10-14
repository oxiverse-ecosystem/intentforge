//! Unit tests for the affiliate template engine.
//! Tests URL rendering, host matching, graceful degradation, and privacy.

use crate::affiliate::*;

// ─── url_host ───────────────────────────────────────────────────────────

#[test]
fn url_host_extracts_simple_domain() {
    assert_eq!(url_host("https://www.amazon.com/path"), "www.amazon.com");
}

#[test]
fn url_host_strips_port() {
    assert_eq!(url_host("https://example.com:8080/path"), "example.com");
}

#[test]
fn url_host_strips_userinfo() {
    assert_eq!(url_host("https://user:pass@example.com/path"), "example.com");
}

#[test]
fn url_host_strips_trailing_dot() {
    assert_eq!(url_host("https://example.com./path"), "example.com");
}

#[test]
fn url_host_lowercases() {
    assert_eq!(url_host("https://WWW.AMAZON.COM"), "www.amazon.com");
}

#[test]
fn url_host_no_scheme() {
    assert_eq!(url_host("example.com/path"), "example.com");
}

#[test]
fn url_host_empty() {
    assert_eq!(url_host(""), "");
}

// ─── is_reserved_documentation_host ─────────────────────────────────────

#[test]
fn reserved_host_rejects_example_com() {
    assert!(is_reserved_documentation_host("example.com"));
}

#[test]
fn reserved_host_rejects_test_tld() {
    assert!(is_reserved_documentation_host("foo.test"));
}

#[test]
fn reserved_host_rejects_localhost() {
    assert!(is_reserved_documentation_host("localhost"));
}

#[test]
fn reserved_host_rejects_example_prefix() {
    assert!(is_reserved_documentation_host("example-merchant.com"));
    assert!(is_reserved_documentation_host("example_shop.co.uk"));
}

#[test]
fn reserved_host_accepts_real_shops() {
    assert!(!is_reserved_documentation_host("amazon.com"));
    assert!(!is_reserved_documentation_host("testosterone-shop.com"));
    assert!(!is_reserved_documentation_host("invalid-syntax.co.uk"));
    assert!(!is_reserved_documentation_host("notexample.com"));
}

// ─── render_affiliate_url ──────────────────────────────────────────────

#[test]
fn render_wrap_template_encodes_destination() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: [("cuid".to_string(), "{subid}".to_string())].into_iter().collect(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key_123");
    let url = render_affiliate_url(&net, "https://www.amazon.com/dp/B08N5WRWNW", "www.amazon.com");
    assert!(url.starts_with("https://sovrn.co?key=test_key_123&u=https%3A%2F%2F"));
    assert!(url.contains("&cuid=www.amazon.com"));
    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn render_append_params_template_adds_query_params() {
    let net = AffiliateNetwork {
        id: "amazon".to_string(),
        kind: "append_params".to_string(),
        enabled: true,
        priority: 70,
        network: "Amazon Associates".to_string(),
        template: "{url}".to_string(),
        params: [("tag".to_string(), "{key}".to_string())].into_iter().collect(),
        key_env: Some("AMAZON_ASSOCIATES_TAG".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "mytag-20");
    let url = render_affiliate_url(&net, "https://www.amazon.com/dp/B08N5WRWNW", "www.amazon.com");
    assert!(url.starts_with("https://www.amazon.com/dp/B08N5WRWNW?tag=mytag-20"));
    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
}

#[test]
fn render_missing_key_degrades_gracefully() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY_MISSING".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    // Ensure the env var is NOT set
    std::env::remove_var("SOVRN_COMMERCE_KEY_MISSING");
    let url = render_affiliate_url(&net, "https://www.amazon.com/dp/B08N5WRWNW", "www.amazon.com");
    // Key resolves to empty string, but URL is still constructed
    assert!(url.starts_with("https://sovrn.co?key=&u=https%3A%2F%2F"));
}

#[test]
fn render_param_env_resolution() {
    let net = AffiliateNetwork {
        id: "ebay".to_string(),
        kind: "append_params".to_string(),
        enabled: true,
        priority: 80,
        network: "eBay Partner Network".to_string(),
        template: "{url}".to_string(),
        params: [
            ("mkrid".to_string(), "{key}".to_string()),
            ("campid".to_string(), "{param:campaign_id}".to_string()),
        ].into_iter().collect(),
        key_env: Some("EBAY_EPN_ROTATION_ID".to_string()),
        param_env: [("campaign_id".to_string(), "EBAY_EPN_CAMPAIGN_ID".to_string())].into_iter().collect(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    std::env::set_var("EBAY_EPN_ROTATION_ID", "rot123");
    std::env::set_var("EBAY_EPN_CAMPAIGN_ID", "camp456");
    let url = render_affiliate_url(&net, "https://www.ebay.com/itm/12345", "www.ebay.com");
    assert!(url.contains("mkrid=rot123"));
    assert!(url.contains("campid=camp456"));
    std::env::remove_var("EBAY_EPN_ROTATION_ID");
    std::env::remove_var("EBAY_EPN_CAMPAIGN_ID");
}

#[test]
fn render_missing_param_env_strips_placeholder() {
    let net = AffiliateNetwork {
        id: "ebay".to_string(),
        kind: "append_params".to_string(),
        enabled: true,
        priority: 80,
        network: "eBay Partner Network".to_string(),
        template: "{url}".to_string(),
        params: [
            ("mkrid".to_string(), "{key}".to_string()),
            ("campid".to_string(), "{param:campaign_id}".to_string()),
        ].into_iter().collect(),
        key_env: Some("EBAY_EPN_ROTATION_ID".to_string()),
        param_env: [("campaign_id".to_string(), "EBAY_EPN_CAMPAIGN_ID_MISSING".to_string())].into_iter().collect(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    std::env::set_var("EBAY_EPN_ROTATION_ID", "rot123");
    std::env::remove_var("EBAY_EPN_CAMPAIGN_ID_MISSING");
    let url = render_affiliate_url(&net, "https://www.ebay.com/itm/12345", "www.ebay.com");
    assert!(url.contains("mkrid=rot123"));
    // Missing param should be stripped entirely
    assert!(!url.contains("campid="));
    assert!(!url.contains("{param:campaign_id}"));
    std::env::remove_var("EBAY_EPN_ROTATION_ID");
}

#[test]
fn render_bid_floor_appended() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: Some("0.10".to_string()),
        fallback_url: None,
        host_suffix: None,
    };
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");
    let url = render_affiliate_url(&net, "https://www.amazon.com/dp/B08N5WRWNW", "www.amazon.com");
    assert!(url.contains("&bf=0.10"));
    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

// ─── decorate_affiliate ────────────────────────────────────────────────

#[test]
fn decorate_adds_affiliate_block_with_disclosure() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product 1"}),
        serde_json::json!({"url": "https://www.ebay.com/itm/12345", "title": "Product 2"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    assert_eq!(results[0]["affiliate"]["network"], "Sovrn Commerce");
    assert_eq!(results[0]["affiliate"]["disclosed"], true);
    assert!(results[0]["affiliate"]["url"].as_str().unwrap().starts_with("https://sovrn.co?key=test_key&u=https%3A%2F%2F"));

    assert_eq!(results[1]["affiliate"]["network"], "Sovrn Commerce");
    assert_eq!(results[1]["affiliate"]["disclosed"], true);

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn decorate_missing_key_leaves_affiliate_null() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY_MISSING".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::remove_var("SOVRN_COMMERCE_KEY_MISSING");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product 1"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // No key => no affiliate block added (graceful degradation)
    assert!(results[0].get("affiliate").is_none());
}

#[test]
fn decorate_preserves_order() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/1", "title": "A"}),
        serde_json::json!({"url": "https://www.amazon.com/dp/2", "title": "B"}),
        serde_json::json!({"url": "https://www.amazon.com/dp/3", "title": "C"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    assert_eq!(results[0]["title"], "A");
    assert_eq!(results[1]["title"], "B");
    assert_eq!(results[2]["title"], "C");

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn decorate_idempotent() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product 1"}),
    ];
    decorate_affiliate(&mut results, &ctx);
    let first_url = results[0]["affiliate"]["url"].clone();

    // Second pass should not modify
    decorate_affiliate(&mut results, &ctx);
    assert_eq!(results[0]["affiliate"]["url"], first_url);

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn decorate_per_merchant_matching() {
    let amazon_net = AffiliateNetwork {
        id: "amazon".to_string(),
        kind: "append_params".to_string(),
        enabled: true,
        priority: 70,
        network: "Amazon Associates".to_string(),
        template: "{url}".to_string(),
        params: [("tag".to_string(), "{key}".to_string())].into_iter().collect(),
        key_env: Some("AMAZON_ASSOCIATES_TAG".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: Some("amazon.".to_string()),
    };
    let ebay_net = AffiliateNetwork {
        id: "ebay".to_string(),
        kind: "append_params".to_string(),
        enabled: true,
        priority: 80,
        network: "eBay Partner Network".to_string(),
        template: "{url}".to_string(),
        params: [("mkrid".to_string(), "{key}".to_string())].into_iter().collect(),
        key_env: Some("EBAY_EPN_ROTATION_ID".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: Some("ebay.".to_string()),
    };
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![amazon_net, ebay_net],
        network_meta: vec![],
    };
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "amz-20");
    std::env::set_var("EBAY_EPN_ROTATION_ID", "ebay123");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW"}),
        serde_json::json!({"url": "https://www.ebay.com/itm/12345"}),
        serde_json::json!({"url": "https://www.amazon.co.uk/dp/B08N5WRWNW"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    assert_eq!(results[0]["affiliate"]["network"], "Amazon Associates");
    assert!(results[0]["affiliate"]["url"].as_str().unwrap().contains("tag=amz-20"));

    assert_eq!(results[1]["affiliate"]["network"], "eBay Partner Network");
    assert!(results[1]["affiliate"]["url"].as_str().unwrap().contains("mkrid=ebay123"));

    assert_eq!(results[2]["affiliate"]["network"], "Amazon Associates");

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
    std::env::remove_var("EBAY_EPN_ROTATION_ID");
}

#[test]
fn decorate_privacy_no_user_data_in_subid() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: [("cuid".to_string(), "{subid}".to_string())].into_iter().collect(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Product 1"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff_url = results[0]["affiliate"]["url"].as_str().unwrap();
    // Subid should be the merchant host, not user data
    assert!(aff_url.contains("cuid=www.amazon.com"));
    // Should NOT contain user ID, session ID, IP, or query text
    assert!(!aff_url.contains("user_id"));
    assert!(!aff_url.contains("session"));
    assert!(!aff_url.contains("ip="));
    assert!(!aff_url.contains("query="));

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn decorate_skips_non_object_results() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!("not an object"),
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Non-object should be skipped without panic
    assert_eq!(results[0], serde_json::json!("not an object"));
    // Object should be decorated
    assert!(results[1].get("affiliate").is_some());

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}

#[test]
fn decorate_skips_results_without_url() {
    let net = AffiliateNetwork {
        id: "sovrn".to_string(),
        kind: "wrap".to_string(),
        enabled: true,
        priority: 100,
        network: "Sovrn Commerce".to_string(),
        template: "https://sovrn.co?key={key}&u={url}".to_string(),
        params: Default::default(),
        key_env: Some("SOVRN_COMMERCE_KEY".to_string()),
        param_env: Default::default(),
        bid_floor: None,
        fallback_url: None,
        host_suffix: None,
    };
    let ctx = AffiliateCtx::from_networks(vec![net]);
    std::env::set_var("SOVRN_COMMERCE_KEY", "test_key");

    let mut results = vec![
        serde_json::json!({"title": "No URL here"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    assert!(results[0].get("affiliate").is_none());

    std::env::remove_var("SOVRN_COMMERCE_KEY");
}
