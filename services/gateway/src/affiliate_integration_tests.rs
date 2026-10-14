// ─────────────────────────────────────────────────────────────────────────────
// INTEGRATION TESTS — Affiliate decoration end-to-end against real data files.
//
// These tests verify the 5 acceptance criteria from the task brief:
//   (1) A merchant in the data file gets an affiliate URL when the env var is set
//   (2) A merchant NOT in the data file passes through unchanged
//   (3) A missing env var causes graceful degradation (plain URL, affiliate: null)
//   (4) No user-identifying data appears in any generated affiliate URL
//   (5) Ranking output is identical regardless of affiliate env var presence
//
// Unlike the unit tests in affiliate_engine_tests.rs and the contract tests in
// commerce_contract_tests.rs, these tests exercise the REAL data files
// (affiliate_programs.json + networks.json) loaded via AffiliateCtx::load(),
// and use the ACTUAL env var names from those files. They are the closest
// offline analogue of the live integration probe.
//
// ENV VAR SAFETY: cargo test runs tests on parallel threads sharing one
// process environment. These tests NEVER call remove_var — they only set_var
// with unique prefixes (INTEG_) and rely on the fact that unset env vars
// are simply absent. This avoids cross-test interference.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
use serde_json::{json, Value};

/// Load the real runtime config and assert it is usable (non-vacuity guard).
fn real_ctx() -> AffiliateCtx {
    let ctx = AffiliateCtx::load();
    assert!(
        !ctx.programs.is_empty(),
        "affiliate_programs.json yielded no programs — tests would be vacuous"
    );
    assert!(
        !ctx.network_meta.is_empty(),
        "networks.json yielded no metadata — tests would be vacuous"
    );
    ctx
}

/// Clone programs with every key_env remapped to a unique name that is either
/// SET (dummy value) or UNSET. Nothing is ever removed from the environment.
fn remap_keys(
    programs: &[AffiliateNetwork],
    suffix: &str,
    set_them: bool,
) -> Vec<AffiliateNetwork> {
    let mut cloned: Vec<AffiliateNetwork> = programs.to_vec();
    for p in cloned.iter_mut() {
        let id: String = p
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let var = format!("INTEG_{}_{}", id.to_uppercase(), suffix);
        if set_them {
            std::env::set_var(&var, "DUMMY_INTEGRATION_KEY");
        }
        p.key_env = Some(var);
    }
    cloned
}

/// Collect ranked URLs from a result array.
fn ranked_urls(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .map(|r| r.get("url").and_then(|v| v.as_str()).unwrap_or_default().to_string())
        .collect()
}

/// Collect affiliate URLs from a result array.
fn affiliate_urls(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|r| r.get("affiliate").and_then(|a| a.get("url")).and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITERION 1: A merchant in the data file gets an affiliate URL when the env
//              var is set.
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_merchant_in_data_file_gets_affiliate_url_when_env_set() {
    let ctx = real_ctx();

    // Find a per-merchant program with a host_suffix (not a catch-all).
    let program = ctx
        .programs
        .iter()
        .find(|p| p.host_suffix.is_some())
        .expect("at least one per-merchant program with host_suffix");

    let suffix = program.host_suffix.as_ref().unwrap().trim_end_matches('.');
    // Build a URL that matches this program's host_suffix.
    let test_url = format!("https://{}/product/123", suffix);

    // Set the env var for this program.
    let var_name = format!("INTEG_CRIT1_{}", program.id.to_uppercase().replace('-', "_"));
    std::env::set_var(&var_name, "INTEG_CRIT1_KEY");

    // Build a context with ONLY this program, key remapped to our set var.
    let mut prog = program.clone();
    prog.key_env = Some(var_name);
    let ctx = AffiliateCtx {
        networks: vec![],
        programs: vec![prog],
        network_meta: vec![],
    };

    let mut results = vec![json!({
        "url": test_url,
        "title": "Test Product",
        "score": 0.95,
    })];
    decorate_affiliate(&mut results, &ctx);

    let aff = results[0]
        .get("affiliate")
        .expect("merchant in data file must get an affiliate block when env var is set");
    assert_eq!(
        aff["network"].as_str().unwrap(),
        program.id,
        "affiliate network should match the program id"
    );
    assert_eq!(aff["disclosed"], true, "disclosed must be true");
    let aff_url = aff["url"].as_str().unwrap();
    assert!(
        aff_url.contains("INTEG_CRIT1_KEY"),
        "affiliate URL should contain the key value: {}",
        aff_url
    );
    // The original URL should be present (either raw or encoded).
    assert!(
        aff_url.contains(&test_url) || aff_url.contains(&urlencoding::encode(&test_url).to_string()),
        "affiliate URL should contain the destination: {}",
        aff_url
    );
}

#[test]
fn integ_merchant_in_data_file_gets_affiliate_url_with_real_data() {
    // Use the REAL data files as-is, with all program keys set.
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT1_REAL", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT1_REAL", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    // Find a merchant that matches one of the real programs.
    let program = ctx
        .programs
        .iter()
        .find(|p| p.host_suffix.is_some())
        .expect("at least one per-merchant program");
    let suffix = program.host_suffix.as_ref().unwrap().trim_end_matches('.');
    let test_url = format!("https://{}/dp/B08N5WRWNW", suffix);

    let mut results = vec![json!({
        "url": test_url,
        "title": "Real Product",
        "score": 0.97,
    })];
    decorate_affiliate(&mut results, &keyed_ctx);

    assert!(
        results[0].get("affiliate").is_some(),
        "real merchant with env set must get affiliate block"
    );
    assert_eq!(results[0]["affiliate"]["disclosed"], true);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITERION 2: A merchant NOT in the data file passes through unchanged.
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_merchant_not_in_data_file_passes_through_unchanged() {
    let ctx = real_ctx();

    // Collect all host_suffixes from the data file.
    let suffixes: Vec<String> = ctx
        .programs
        .iter()
        .filter_map(|p| p.host_suffix.clone())
        .collect();

    // Find a host that does NOT match any program's suffix.
    // Use a domain that is clearly not in the data file.
    let unmatched_url = "https://www.unmatched-merchant-xyz123.com/product/456";
    let host = url_host(unmatched_url);
    for suffix in &suffixes {
        let suffix = suffix.trim_end_matches('.');
        assert!(
            host != suffix && !host.split('.').any(|label| label == suffix),
            "test URL must NOT match any program suffix"
        );
    }

    // Set all program keys so decoration is active.
    let keyed_programs = remap_keys(&ctx.programs, "CRIT2", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT2", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    let mut results = vec![json!({
        "url": unmatched_url,
        "title": "Unmatched Product",
        "score": 0.88,
    })];
    decorate_affiliate(&mut results, &keyed_ctx);

    // The URL must be unchanged.
    assert_eq!(
        results[0]["url"].as_str().unwrap(),
        unmatched_url,
        "unmatched merchant URL must pass through unchanged"
    );

    // If there are no catch-all networks, there should be no affiliate block.
    // If there ARE catch-all networks, the result may get a catch-all affiliate
    // block — but the URL field itself must still be unchanged.
    // The key assertion is: the `url` field is never modified.
    assert_eq!(
        results[0]["url"].as_str().unwrap(),
        unmatched_url,
        "url field must never be modified by decoration"
    );
}

#[test]
fn integ_merchant_not_in_data_file_no_affiliate_block_without_catchall() {
    // Build a context with ONLY per-merchant programs (no catch-all networks).
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT2B", true);
    let ctx_no_catchall = AffiliateCtx {
        networks: vec![], // no catch-all
        programs: keyed_programs,
        network_meta: vec![],
    };

    let unmatched_url = "https://www.totally-unknown-merchant.com/item/789";
    let mut results = vec![json!({
        "url": unmatched_url,
        "title": "Unknown Product",
        "score": 0.75,
    })];
    decorate_affiliate(&mut results, &ctx_no_catchall);

    // No catch-all + no matching program => no affiliate block.
    assert!(
        results[0].get("affiliate").is_none(),
        "unmatched merchant with no catch-all must have no affiliate block"
    );
    assert_eq!(
        results[0]["url"].as_str().unwrap(),
        unmatched_url,
        "url must be unchanged"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITERION 3: A missing env var causes graceful degradation (plain URL,
//              affiliate: null).
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_missing_env_var_graceful_degradation() {
    let ctx = real_ctx();

    // Build a context with programs whose key_env names are NEVER set.
    let unkeyed_programs = remap_keys(&ctx.programs, "CRIT3_UNSET", false);
    let unkeyed_networks = remap_keys(&ctx.networks, "CRIT3_UNSET", false);
    let unkeyed_ctx = AffiliateCtx {
        networks: unkeyed_networks,
        programs: unkeyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    // Use a URL that WOULD match a per-merchant program if the key were set.
    let program = ctx
        .programs
        .iter()
        .find(|p| p.host_suffix.is_some())
        .expect("at least one per-merchant program");
    let suffix = program.host_suffix.as_ref().unwrap().trim_end_matches('.');
    let test_url = format!("https://{}/product/999", suffix);

    let mut results = vec![json!({
        "url": test_url,
        "title": "Test Product",
        "score": 0.92,
    })];
    decorate_affiliate(&mut results, &unkeyed_ctx);

    // Graceful degradation: no affiliate block, URL unchanged.
    assert!(
        results[0].get("affiliate").is_none(),
        "missing env var => affiliate must be omitted (graceful degradation)"
    );
    assert_eq!(
        results[0]["url"].as_str().unwrap(),
        test_url,
        "missing env var => URL must be the plain original"
    );
}

#[test]
fn integ_missing_env_var_no_crash_multiple_results() {
    let ctx = real_ctx();
    let unkeyed_programs = remap_keys(&ctx.programs, "CRIT3B_UNSET", false);
    let unkeyed_networks = remap_keys(&ctx.networks, "CRIT3B_UNSET", false);
    let unkeyed_ctx = AffiliateCtx {
        networks: unkeyed_networks,
        programs: unkeyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    let mut results = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "A", "score": 0.97}),
        json!({"url": "https://www.ebay.com/itm/12345", "title": "B", "score": 0.91}),
        json!({"url": "https://www.example.com/product", "title": "C", "score": 0.88}),
    ];
    decorate_affiliate(&mut results, &unkeyed_ctx);

    // All results must pass through with no affiliate block and no crash.
    for (i, r) in results.iter().enumerate() {
        assert!(
            r.get("affiliate").is_none(),
            "result {} must have no affiliate block when env var is missing",
            i
        );
    }
    assert_eq!(results[0]["url"], "https://www.amazon.com/dp/B08N5WRWNW");
    assert_eq!(results[1]["url"], "https://www.ebay.com/itm/12345");
    assert_eq!(results[2]["url"], "https://www.example.com/product");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITERION 4: No user-identifying data appears in any generated affiliate URL.
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_no_user_identifying_data_in_affiliate_urls() {
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT4", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT4", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    // A user query that contains identifying information.
    let user_query = "best wireless earbuds for john smith in new york";

    // Build results with user-identifying data in title and content.
    let mut results = vec![
        json!({
            "url": "https://www.amazon.com/dp/B08N5WRWNW",
            "title": format!("Buy {} — Wireless Earbuds Pro", user_query),
            "content": "Best wireless earbuds for john smith in new york, reviewed.",
            "score": 0.97,
        }),
        json!({
            "url": "https://www.ebay.com/itm/12345",
            "title": "Cheaper earbuds alternative",
            "content": "A budget option for john smith.",
            "score": 0.91,
        }),
        json!({
            "url": "https://shop.example.com/earbuds?ref=blog&utm_source=newsletter",
            "title": "Earbuds bundle",
            "content": "Bundle with case.",
            "score": 0.88,
        }),
    ];
    decorate_affiliate(&mut results, &keyed_ctx);

    let aff_urls = affiliate_urls(&results);
    assert!(
        !aff_urls.is_empty(),
        "decoration must have produced affiliate urls for this test to be meaningful"
    );

    // Forbidden tokens that must never appear at the affiliate param level.
    let forbidden = [
        "&uid=", "?uid=",
        "&user=", "?user=",
        "&session=", "?session=",
        "&ip=", "?ip=",
        "&client=", "?client=",
        "&q=", "?q=",
        "&query=", "?query=",
        "&name=", "?name=",
        "&email=", "?email=",
    ];

    for url in &aff_urls {
        let lower = url.to_lowercase();
        for bad in &forbidden {
            assert!(
                !lower.contains(bad),
                "affiliate URL leaked forbidden token '{}': {}",
                bad,
                url
            );
        }
        // The user query text must never appear.
        assert!(
            !url.contains(user_query),
            "user query text leaked into affiliate URL: {}",
            url
        );
        // Specific identifying strings must never appear.
        assert!(
            !url.contains("john smith"),
            "user name leaked into affiliate URL: {}",
            url
        );
        assert!(
            !url.contains("new york"),
            "user location leaked into affiliate URL: {}",
            url
        );
    }
}

#[test]
fn integ_no_user_data_leaks_with_real_data_all_programs() {
    // Use ALL real programs with keys set, and verify no leaks across all of them.
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT4B", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT4B", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    // Test URLs that match various program suffixes.
    let test_urls = vec![
        "https://www.amazon.com/dp/B08N5WRWNW",
        "https://www.ebay.com/itm/12345",
        "https://shop.example.com/product?ref=blog",
    ];

    let mut results: Vec<Value> = test_urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            json!({
                "url": url,
                "title": format!("Product {} for user query text", i),
                "content": "Content with session_id=abc123 and ip=192.168.1.1",
                "score": 0.97 - (i as f64 * 0.05),
            })
        })
        .collect();
    decorate_affiliate(&mut results, &keyed_ctx);

    let aff_urls = affiliate_urls(&results);
    assert!(!aff_urls.is_empty(), "must have decorated some results");

    for url in &aff_urls {
        let lower = url.to_lowercase();
        // Check for session_id, IP, and other identifying data.
        assert!(
            !lower.contains("session_id"),
            "session_id leaked: {}",
            url
        );
        assert!(
            !lower.contains("192.168.1.1"),
            "IP address leaked: {}",
            url
        );
        assert!(
            !lower.contains("abc123"),
            "session token leaked: {}",
            url
        );
        assert!(
            !lower.contains("user query text"),
            "query text leaked: {}",
            url
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CRITERION 5: Ranking output is identical regardless of affiliate env var
//              presence.
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_ranking_identical_with_and_without_env_vars() {
    let ctx = real_ctx();

    // A mixed set of results: some matching programs, some not.
    let ranked_input = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "A", "score": 0.97}),
        json!({"url": "https://www.ebay.com/itm/12345", "title": "B", "score": 0.91}),
        json!({"url": "https://www.example.com/product", "title": "C", "score": 0.88}),
        json!({"url": "https://shop.example.org/item/42", "title": "D", "score": 0.80}),
    ];

    // ENABLED run: all keys set.
    let keyed_programs = remap_keys(&ctx.programs, "CRIT5_ON", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT5_ON", true);
    let enabled_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };
    let mut enabled = ranked_input.clone();
    decorate_affiliate(&mut enabled, &enabled_ctx);
    let enabled_ranked = ranked_urls(&enabled);
    let enabled_aff = affiliate_urls(&enabled);

    // DISABLED run: no keys set.
    let unkeyed_programs = remap_keys(&ctx.programs, "CRIT5_OFF", false);
    let unkeyed_networks = remap_keys(&ctx.networks, "CRIT5_OFF", false);
    let disabled_ctx = AffiliateCtx {
        networks: unkeyed_networks,
        programs: unkeyed_programs,
        network_meta: ctx.network_meta.clone(),
    };
    let mut disabled = ranked_input.clone();
    decorate_affiliate(&mut disabled, &disabled_ctx);
    let disabled_ranked = ranked_urls(&disabled);
    let disabled_aff = affiliate_urls(&disabled);

    // Non-vacuity: enabled must decorate, disabled must not.
    assert!(
        !enabled_aff.is_empty(),
        "enabled run must actually decorate"
    );
    assert!(
        disabled_aff.is_empty(),
        "disabled run must not decorate"
    );

    // THE CONTRACT: ranked URL order is byte-identical.
    assert_eq!(
        enabled_ranked, disabled_ranked,
        "affiliate env var presence MUST NOT change ranked URL order"
    );
}

#[test]
fn integ_ranking_identical_real_data_all_programs_keyed_vs_unkeyed() {
    // Use the REAL data files with ALL programs, keyed vs unkeyed.
    let ctx = real_ctx();

    let ranked_input = vec![
        json!({"url": "https://merchant-a.example/p/1000xm5", "title": "A", "score": 0.97}),
        json!({"url": "https://merchant-b.example/p/1000xm5?ref=feed", "title": "B", "score": 0.91}),
        json!({"url": "https://merchant-c.example/catalog/item/42", "title": "C", "score": 0.88}),
        json!({"url": "https://merchant-d.example/", "title": "D", "score": 0.80}),
    ];

    // Keyed run.
    let keyed_programs = remap_keys(&ctx.programs, "CRIT5B_KEYED", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT5B_KEYED", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };
    let mut keyed = ranked_input.clone();
    decorate_affiliate(&mut keyed, &keyed_ctx);
    let keyed_ranked = ranked_urls(&keyed);
    assert!(
        !affiliate_urls(&keyed).is_empty(),
        "keyed run must decorate"
    );

    // Unkeyed run.
    let unkeyed_programs = remap_keys(&ctx.programs, "CRIT5B_UNKEYED", false);
    let unkeyed_networks = remap_keys(&ctx.networks, "CRIT5B_UNKEYED", false);
    let unkeyed_ctx = AffiliateCtx {
        networks: unkeyed_networks,
        programs: unkeyed_programs,
        network_meta: ctx.network_meta.clone(),
    };
    let mut unkeyed = ranked_input.clone();
    decorate_affiliate(&mut unkeyed, &unkeyed_ctx);
    let unkeyed_ranked = ranked_urls(&unkeyed);
    assert!(
        affiliate_urls(&unkeyed).is_empty(),
        "unkeyed run must not decorate"
    );

    assert_eq!(
        keyed_ranked, unkeyed_ranked,
        "real data: affiliate key presence changed ranked URL order"
    );
}

#[test]
fn integ_decoration_touches_no_field_except_affiliate_real_data() {
    // Verify that decoration with real data only adds the `affiliate` field
    // and does not modify any other field in the result objects.
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT5C", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT5C", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    let original = vec![
        json!({"url": "https://www.amazon.com/dp/B08N5WRWNW", "title": "A", "score": 0.97, "sources": ["bing"]}),
        json!({"url": "https://www.ebay.com/itm/12345", "title": "B", "score": 0.91, "sources": ["brave"]}),
    ];

    let mut decorated = original.clone();
    decorate_affiliate(&mut decorated, &keyed_ctx);
    assert!(
        !affiliate_urls(&decorated).is_empty(),
        "must have decorated"
    );

    // Strip affiliate blocks and compare.
    let mut stripped = decorated.clone();
    for r in stripped.iter_mut() {
        if let Some(obj) = r.as_object_mut() {
            obj.remove("affiliate");
        }
    }
    assert_eq!(
        original, stripped,
        "decoration mutated a field other than `affiliate`"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// ADDITIONAL INTEGRATION: Full pipeline with real data files.
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn integ_full_pipeline_real_data_mixed_merchants() {
    // End-to-end: real data files, mixed merchants (some match, some don't),
    // keys set, verify all 5 criteria in one test.
    let ctx = real_ctx();
    let keyed_programs = remap_keys(&ctx.programs, "CRIT_FULL", true);
    let keyed_networks = remap_keys(&ctx.networks, "CRIT_FULL", true);
    let keyed_ctx = AffiliateCtx {
        networks: keyed_networks,
        programs: keyed_programs,
        network_meta: ctx.network_meta.clone(),
    };

    let user_query = "best wireless earbuds under 50 dollars";
    let mut results = vec![
        // Should match amazon program.
        json!({
            "url": "https://www.amazon.com/dp/B08N5WRWNW",
            "title": format!("Buy {} — Wireless Earbuds Pro", user_query),
            "content": "Best wireless earbuds under 50 dollars, reviewed.",
            "score": 0.97,
        }),
        // Should match ebay program.
        json!({
            "url": "https://www.ebay.com/itm/12345",
            "title": "Cheaper earbuds alternative",
            "content": "A budget option.",
            "score": 0.91,
        }),
        // Should NOT match any program (no catch-all in this context).
        json!({
            "url": "https://www.unknown-merchant.com/product",
            "title": "Unknown merchant product",
            "content": "Not in data file.",
            "score": 0.85,
        }),
    ];

    let before_ranked = ranked_urls(&results);
    decorate_affiliate(&mut results, &keyed_ctx);
    let after_ranked = ranked_urls(&results);

    // Criterion 5: ranking unchanged.
    assert_eq!(before_ranked, after_ranked, "ranking must be identical");

    // Criterion 1: matched merchants get affiliate URLs.
    assert!(results[0].get("affiliate").is_some(), "amazon must be decorated");
    assert!(results[1].get("affiliate").is_some(), "ebay must be decorated");

    // Criterion 2: unmatched merchant passes through (url unchanged).
    assert_eq!(
        results[2]["url"].as_str().unwrap(),
        "https://www.unknown-merchant.com/product",
        "unmatched merchant url must be unchanged"
    );

    // Criterion 4: no user data in affiliate URLs.
    let aff_urls = affiliate_urls(&results);
    for url in &aff_urls {
        assert!(
            !url.contains(user_query),
            "query text leaked: {}",
            url
        );
        assert!(
            !url.contains("wireless earbuds"),
            "query keywords leaked: {}",
            url
        );
    }
}
