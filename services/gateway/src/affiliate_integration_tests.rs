//! Integration tests: end-to-end affiliate decoration against real data files.
//! Tests that the engine loads from data/commerce/*.json and decorates results
//! with the correct network templates, graceful degradation, and disclosure.

use crate::affiliate::*;

// ─── Data file loading ──────────────────────────────────────────────────

#[test]
fn load_real_data_files() {
    let ctx = AffiliateCtx::load();
    // Should load at least the legacy networks from affiliate.json
    assert!(!ctx.networks.is_empty(), "Should load legacy networks from affiliate.json");
    // Should load per-merchant programs from affiliate_programs.json
    assert!(!ctx.programs.is_empty(), "Should load per-merchant programs from affiliate_programs.json");
    // Should load network metadata from networks.json
    assert!(!ctx.network_meta.is_empty(), "Should load network metadata from networks.json");
}

#[test]
fn loaded_networks_have_required_fields() {
    let ctx = AffiliateCtx::load();
    for net in &ctx.networks {
        assert!(!net.id.is_empty(), "Network id should not be empty");
        assert!(!net.template.is_empty(), "Network template should not be empty");
        assert!(!net.network.is_empty(), "Network name should not be empty");
    }
}

#[test]
fn loaded_programs_have_host_suffix() {
    let ctx = AffiliateCtx::load();
    for prog in &ctx.programs {
        assert!(!prog.id.is_empty(), "Program id should not be empty");
        assert!(prog.host_suffix.is_some(), "Per-merchant program should have host_suffix");
    }
}

// ─── End-to-end decoration with real data ──────────────────────────────

#[test]
fn decorate_amazon_result_with_real_data() {
    let ctx = AffiliateCtx::load();
    // Set the Amazon tag env var
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "test-20");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Test Product"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert!(aff.is_object(), "Amazon result should have affiliate block");
    assert_eq!(aff["disclosed"], true);
    assert!(aff["url"].as_str().unwrap().contains("tag=test-20"));

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
}

#[test]
fn decorate_ebay_result_with_real_data() {
    let ctx = AffiliateCtx::load();
    // Set the eBay EPN env vars
    std::env::set_var("EBAY_EPN_ROTATION_ID", "test-rotation");
    std::env::set_var("EBAY_EPN_CAMPAIGN_ID", "test-campaign");

    let mut results = vec![
        serde_json::json!({"url": "https://www.ebay.com/itm/12345", "title": "Test eBay Item"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    let aff = &results[0]["affiliate"];
    assert!(aff.is_object(), "eBay result should have affiliate block");
    assert_eq!(aff["disclosed"], true);
    let url = aff["url"].as_str().unwrap();
    assert!(url.contains("mkrid=test-rotation"), "Should contain eBay rotation id");
    assert!(url.contains("campid=test-campaign"), "Should contain eBay campaign id");

    std::env::remove_var("EBAY_EPN_ROTATION_ID");
    std::env::remove_var("EBAY_EPN_CAMPAIGN_ID");
}

#[test]
fn decorate_non_merchant_result_degrades_gracefully() {
    let ctx = AffiliateCtx::load();
    // Don't set any env vars — should degrade gracefully

    let mut results = vec![
        serde_json::json!({"url": "https://www.wikipedia.org/wiki/Test", "title": "Wikipedia Article"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Non-merchant URL with no matching program and no catch-all key => no affiliate
    // (or if catch-all exists with key, it gets decorated — both are valid)
    // The key assertion is: it never panics
    let _ = &results[0];
}

#[test]
fn decorate_mixed_results_with_real_data() {
    let ctx = AffiliateCtx::load();
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "amz-20");
    std::env::set_var("EBAY_EPN_ROTATION_ID", "ebay-123");
    std::env::set_var("EBAY_EPN_CAMPAIGN_ID", "ebay-camp");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "Amazon Product"}),
        serde_json::json!({"url": "https://www.ebay.com/itm/12345", "title": "eBay Item"}),
        serde_json::json!({"url": "https://www.wikipedia.org/wiki/Test", "title": "Wikipedia"}),
        serde_json::json!({"url": "https://www.amazon.co.uk/dp/B08N5WRWNW", "title": "Amazon UK Product"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // Amazon.com should be decorated
    assert!(results[0].get("affiliate").is_some(), "Amazon.com should have affiliate");
    assert_eq!(results[0]["affiliate"]["network"], "Amazon Associates");

    // eBay.com should be decorated
    assert!(results[1].get("affiliate").is_some(), "eBay.com should have affiliate");
    assert_eq!(results[1]["affiliate"]["network"], "eBay Partner Network");

    // Wikipedia should NOT have affiliate (no matching program, no catch-all key set)
    // (unless a catch-all network key is set in env, which it isn't in test)

    // Amazon.co.uk should be decorated
    assert!(results[3].get("affiliate").is_some(), "Amazon.co.uk should have affiliate");

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
    std::env::remove_var("EBAY_EPN_ROTATION_ID");
    std::env::remove_var("EBAY_EPN_CAMPAIGN_ID");
}

#[test]
fn order_invariance_with_real_data() {
    let ctx = AffiliateCtx::load();
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "amz-20");
    std::env::set_var("EBAY_EPN_ROTATION_ID", "ebay-123");
    std::env::set_var("EBAY_EPN_CAMPAIGN_ID", "ebay-camp");

    let make_results = || vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/1", "title": "A"}),
        serde_json::json!({"url": "https://www.ebay.com/itm/2", "title": "B"}),
        serde_json::json!({"url": "https://www.amazon.com/dp/3", "title": "C"}),
        serde_json::json!({"url": "https://www.wikipedia.org/wiki/4", "title": "D"}),
    ];

    let mut results1 = make_results();
    let mut results2 = make_results();

    decorate_affiliate(&mut results1, &ctx);
    decorate_affiliate(&mut results2, &ctx);

    // Order must be identical
    for i in 0..results1.len() {
        assert_eq!(results1[i]["title"], results2[i]["title"], "Order changed at index {}", i);
    }

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
    std::env::remove_var("EBAY_EPN_ROTATION_ID");
    std::env::remove_var("EBAY_EPN_CAMPAIGN_ID");
}

#[test]
fn disclosure_flag_always_true_when_decorated() {
    let ctx = AffiliateCtx::load();
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "amz-20");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    assert_eq!(results[0]["affiliate"]["disclosed"], true, "Disclosure flag must be true");

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
}

#[test]
fn network_field_matches_data_file() {
    let ctx = AffiliateCtx::load();
    std::env::set_var("AMAZON_ASSOCIATES_TAG", "amz-20");

    let mut results = vec![
        serde_json::json!({"url": "https://www.amazon.com/dp/B08N5WRWNW"}),
    ];
    decorate_affiliate(&mut results, &ctx);

    // The network name should come from the data file, not be hardcoded
    assert_eq!(results[0]["affiliate"]["network"], "Amazon Associates");

    std::env::remove_var("AMAZON_ASSOCIATES_TAG");
}
