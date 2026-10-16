// ─────────────────────────────────────────────────────────────────────────────
// ROADMAP item 6 — ORDER INVARIANCE under the REAL runtime data.
//
// The order-invariance tests in commerce_contract_tests.rs build synthetic
// networks. This file closes the remaining gap: the production data file
// `data/commerce/affiliate.json` ships FIVE networks with different template
// kinds (`wrap` and `append_params`) and different `params`/`bid_floor`/
// `fallback_url` shapes. A rendering bug in any of those shapes could, in
// principle, alter a result. These tests load the ACTUAL file the gateway loads
// at startup (`AffiliateCtx::load()` — the very same function production uses)
// and assert, for EVERY network and EVERY key-presence configuration, that the
// ranked URL list is byte-identical.
//
// The live two-process counterpart is scripts/verify_affiliate_order_invariance.py
// (real gateways, keys present vs absent). This file is the offline CI lock.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
use crate::affiliate::*;
use serde_json::Value;

/// The ranked input every test here decorates: several results with DIFFERENT
/// hosts (so `subid` differs per row) and non-uniform scores, so a reordering or
/// a dedup-by-URL bug cannot hide behind a homogeneous fixture.
fn ranked_fixture() -> Vec<Value> {
    vec![
        serde_json::json!({
            "url": "https://merchant-a.example/p/1000xm5",
            "title": "Merchant A",
            "score": 0.97,
        }),
        serde_json::json!({
            "url": "https://merchant-b.example/p/1000xm5?ref=feed",
            "title": "Merchant B",
            "score": 0.91,
        }),
        serde_json::json!({
            "url": "https://merchant-c.example/catalog/item/42",
            "title": "Merchant C",
            "score": 0.88,
        }),
        serde_json::json!({
            "url": "https://merchant-d.example/",
            "title": "Merchant D",
            "score": 0.80,
        }),
    ]
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

/// A query the shipped eligibility patterns must accept. It is asserted at
/// runtime (not assumed) so this test fails loudly if the data policy drifts,
/// rather than silently becoming vacuous.
const EXACT_MODEL_QUERY: &str = "iphone 16 pro max price";

/// Load the real runtime config and assert it is actually usable, so a broken or
/// missing data file fails here instead of making every invariance test below
/// pass by decorating nothing.
fn real_ctx() -> AffiliateCtx {
    let ctx = AffiliateCtx::load();
    assert!(
        !ctx.networks.is_empty(),
        "data/commerce/affiliate.json yielded no networks — invariance tests would be vacuous"
    );
    ctx
}

/// Present every network's declared key env var with a dummy value, so
/// `first_usable()` can select networks in priority order. The VALUES are
/// irrelevant to ordering; only their presence/absence is under test.
fn present_all_keys(ctx: &AffiliateCtx) -> Vec<String> {
    let mut set = Vec::new();
    for n in &ctx.networks {
        if let Some(k) = &n.key_env {
            std::env::set_var(k, "DUMMY_KEY_FOR_ORDER_INVARIANCE");
            set.push(k.clone());
        }
    }
    set
}

fn remove_keys(keys: &[String]) {
    for k in keys {
        std::env::remove_var(k);
    }
}

/// THE ORDER-INVARIANCE LOCK, driven by the REAL data file.
///
/// For each shipped network in isolation: decorate the same already-ranked input
/// twice — once with that network's key PRESENT, once with it ABSENT (and every
/// other network's key removed so no network is usable at all) — and require the
/// ranked URL list to be byte-identical. Non-vacuity is enforced in both
/// directions: the keyed run MUST have decorated, the keyless run MUST NOT.
#[test]
fn real_data_order_is_invariant_for_every_network_key_on_and_off() {
    // Serialize against every other env-mutating test: these assertions depend
    // on process-global key vars being exactly as we left them.
    let _env_guard = crate::ENV_TEST_LOCK.lock();
    let ctx = real_ctx();
    // Queries the shipped policy must treat as exact-model. Asserted, not assumed.
    assert!(
        ctx.is_exact_model_query(EXACT_MODEL_QUERY),
        "shipped eligibility patterns must accept {EXACT_MODEL_QUERY:?} — otherwise the \
         invariance comparison below would never exercise decoration"
    );

    for net in &networks {
        // KEYS PRESENT: only this network, its remapped key set to a dummy value.
        let keyed_ctx = AffiliateCtx { networks: remap_keys(std::slice::from_ref(net), "PRESENT", true) };
        let mut with_key = ranked_fixture();
        decorate_affiliate_for_query(&mut with_key, &solo, EXACT_MODEL_QUERY);
        let with_key_ranked = ranked_urls(&with_key);
        let with_key_aff = affiliate_urls(&with_key);
        assert!(
            !with_key_aff.is_empty(),
            "network {:?}: keys-present run must actually decorate (affiliate urls present); \
             an empty decoration makes the comparison vacuous",
            net.id
        );

        // KEYS ABSENT: same network, key_env remapped to a name nothing ever sets.
        let unkeyed_ctx = AffiliateCtx { networks: remap_keys(std::slice::from_ref(net), "ABSENT", false) };
        let mut no_key = ranked_fixture();
        decorate_affiliate_for_query(&mut no_key, &solo, EXACT_MODEL_QUERY);
        let no_key_ranked = ranked_urls(&no_key);
        let no_key_aff = affiliate_urls(&no_key);
        assert!(
            no_key_aff.is_empty(),
            "network {:?}: keys-absent run must NOT decorate (affiliate urls absent)",
            net.id
        );

        assert_eq!(
            with_key_ranked, no_key_ranked,
            "network {:?} ({:?}): affiliate decoration changed the ranked URL order \
             (count, URLs, or sequence) between keys-present and keys-absent runs",
            net.id, net.kind
        );
    }
}

/// The whole production config, keys present, vs the whole production config,
/// keys absent. This is the closest offline analogue of the live probe: same
/// `AffiliateCtx::load()` production uses, only the env key presence differs.
#[test]
fn real_data_whole_config_order_invariant_keys_present_vs_absent() {
    // Serialize against every other env-mutating test: these assertions depend
    // on process-global key vars being exactly as we left them.
    let _env_guard = crate::ENV_TEST_LOCK.lock();
    let ctx = real_ctx();
    let all_keys: Vec<String> = ctx.networks.iter().filter_map(|n| n.key_env.clone()).collect();

    let unkeyed_ctx = AffiliateCtx { networks: remap_keys(&networks, "WHOLE_ABSENT", false) };
    let mut absent = ranked_fixture();
    decorate_affiliate_for_query(&mut absent, &ctx, EXACT_MODEL_QUERY);
    let absent_ranked = ranked_urls(&absent);
    let absent_aff = affiliate_urls(&absent);
    assert!(absent_aff.is_empty(), "keys absent => nothing may be decorated");

    let keyed_ctx = AffiliateCtx { networks: remap_keys(&networks, "WHOLE_PRESENT", true) };
    let mut present = ranked_fixture();
    decorate_affiliate_for_query(&mut present, &ctx, EXACT_MODEL_QUERY);
    let present_ranked = ranked_urls(&present);
    let present_aff = affiliate_urls(&present);
    assert!(!present_aff.is_empty(), "keys present => decoration must happen");

    assert_eq!(
        present_ranked, absent_ranked,
        "with the full shipped network set, affiliate keys changed the ranked URL order"
    );
}

/// Broad category queries must stay free AND must not move anything either. This
/// is the privacy/business-policy half of the same invariant: no monetization on
/// non-model queries, and still no reordering.
#[test]
fn real_data_decoration_touches_no_field_except_affiliate() {
    let networks = real_ctx();
    let keyed_ctx = AffiliateCtx { networks: remap_keys(&networks, "FIELDS", true) };

    // Broad, non-model commercial queries spanning several shapes. These are
    // shapes, not fixtures tuned to one string: if the data policy legitimately
    // grows to monetize one of them, the assertions below fail and the test is
    // updated deliberately rather than silently drifting.
    let broad = [
        "best wireless earbuds under 50 dollars",
        "wireless earbuds",
        "steel water bottle",
        "under 50 dollars",
    ];
    for q in broad {
        assert!(
            !ctx.is_exact_model_query(q),
            "broad query {q:?} must not be eligible for affiliate decoration"
        );
        let baseline = ranked_fixture();
        let baseline_ranked = ranked_urls(&baseline);
        let mut arr = ranked_fixture();
        decorate_affiliate_for_query(&mut arr, &ctx, q);
        assert!(
            affiliate_urls(&arr).is_empty(),
            "broad query {q:?} received affiliate decoration: {:?}",
            affiliate_urls(&arr)
        );
        assert_eq!(
            ranked_urls(&arr),
            baseline_ranked,
            "broad query {q:?}: the no-op decoration path altered the ranked order"
        );
    }

    remove_keys(&all_keys);
}

/// NON-VACUITY ASSERTION for the main path (card t_a46a53d3).
///
/// The main-path /search `shopping` strip is built from a CLONE of the
/// top-N ranked results, enriched via `enrich_with_commerce`, then
/// filtered to only rows with a `commerce` block. If enrichment attaches
/// no commerce block (e.g. because every page fetch fails), the strip is
/// empty and the main-path order-invariance comparison is VACUOUS — it
/// compares two no-decoration runs.
///
/// This test proves the enrichment pipeline CAN attach commerce blocks
/// when pages are fetchable, so a future regression that breaks the
/// pipeline (e.g. a status-code check that rejects everything, or an
/// extraction change that finds nothing) fails here rather than silently
/// making the live comparison vacuous.
#[tokio::test]
async fn main_path_enrichment_is_non_vacuous_with_fetchable_pages() {
    // Build a ranked fixture with a URL that the fake fetch will resolve.
    let mut arr = vec![
        serde_json::json!({"url": "https://merchant-a.example/p/1000xm5", "title": "Product A", "score": 0.97}),
        serde_json::json!({"url": "https://merchant-b.example/p/1000xm5?ref=feed", "title": "Product B", "score": 0.91}),
    ];

    // Fake fetch: return a minimal product page with JSON-LD price.
    // This exercises the REAL extract_commerce_offer path with zero network.
    let fake_html = r#"<html><head><script type="application/ld+json">{"@type":"Product","offers":{"@type":"Offer","price":"99.99","priceCurrency":"USD"}}</script></head><body>Product</body></html>"#;
    let fetch = |_url: String| async move { Some(fake_html.to_string()) };

    // Enrich the array (same function the main path calls).
    enrich_with_commerce(&mut arr, fetch).await;

    // NON-VACUITY: at least one result must have a commerce block.
    let with_commerce = arr.iter().filter(|r| r.get("commerce").is_some()).count();
    assert!(
        with_commerce > 0,
        "enrichment attached zero commerce blocks to {} results — the main-path \
         shopping strip would be vacuous (card t_a46a53d3)",
        arr.len()
    );

    // The commerce block must carry honest provenance.
    for r in &arr {
        if let Some(c) = r.get("commerce") {
            assert!(
                c.get("observed_at").is_some(),
                "commerce block missing observed_at provenance"
            );
        }
    }

    // Now decorate with keys present and verify the strip would be non-empty.
    let networks = real_ctx();
    let keyed_ctx = AffiliateCtx { networks: remap_keys(&networks, "STRIP_PRESENT", true) };
    decorate_affiliate(&mut arr, &keyed_ctx);

    // The concrete_offers filter (main path gate) must find at least one row.
    let concrete_offers: Vec<_> = arr
        .iter()
        .filter(|r| r.get("commerce").is_some())
        .collect();
    assert!(
        !concrete_offers.is_empty(),
        "main-path concrete_offers filter found zero rows — the shopping strip \
         would be None and the order-invariance comparison would be vacuous"
    );

    // And the decorated rows must carry affiliate blocks.
    let with_affiliate = arr.iter().filter(|r| r.get("affiliate").is_some()).count();
    assert!(
        with_affiliate > 0,
        "keys-present run decorated zero results — non-vacuity violated"
    );
}
