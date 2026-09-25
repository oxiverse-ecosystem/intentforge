// ─────────────────────────────────────────────────────────────────────────────
// ROADMAP item 6 — ORDER INVARIANCE under the REAL runtime data.
//
// The order-invariance tests in commerce_contract_tests.rs build synthetic
// networks. This file closes the remaining gap: the production data file
// `data/commerce/affiliate.json` ships FIVE networks spanning both template
// kinds (`wrap` and `append_params`) with different `params` / `bid_floor` /
// `fallback_url` shapes. A rendering bug in any of those shapes could in
// principle alter a result. These tests load the ACTUAL file the gateway loads
// at startup (`AffiliateCtx::load()` — the same function production uses) and
// assert that the ranked URL list is byte-identical whether the network's key
// is PRESENT or ABSENT.
//
// ── WHY THESE TESTS NEVER DELETE AN ENV VAR ──────────────────────────────────
// `first_usable()` reads the process environment, so "key present" normally
// means `set_var` and "key absent" means `remove_var`. That is UNSAFE in a test
// binary: `cargo test` runs test functions on parallel threads sharing one
// process environment, so a `remove_var` here deletes a variable another test
// just set and makes an unrelated test fail non-deterministically. (An earlier
// draft of this file did exactly that and broke four unrelated tests.)
//
// Instead we vary key PRESENCE WITHOUT MUTATION: each network clone is given a
// `key_env` name that is either (a) freshly set to a dummy value, or (b) a name
// that is never set anywhere in the suite. Both paths run the identical real
// production code (`decorate_affiliate_for_query` -> `first_usable` ->
// `render_affiliate_url`); only the resolvability of the declared key differs.
// That is exactly the variable under test, with zero cross-test interference.
//
// The live two-process counterpart is scripts/verify_affiliate_order_invariance.py
// (real gateways, one with keys, two without). This file is the offline CI lock.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
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
        .map(|r| {
            r.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn affiliate_urls(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|r| {
            r.get("affiliate")
                .and_then(|a| a.get("url"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string())
        .collect()
}

/// A query the shipped eligibility patterns must accept. It is ASSERTED at
/// runtime (not assumed) so this file fails loudly if the data policy drifts,
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
    assert!(
        !ctx.exact_model_patterns.is_empty(),
        "no exact-model eligibility patterns loaded — the monetization policy is \
         data-driven and must ship in the data file"
    );
    assert!(
        ctx.is_exact_model_query(EXACT_MODEL_QUERY),
        "shipped eligibility patterns must accept {EXACT_MODEL_QUERY:?} — otherwise \
         the invariance comparisons below would never exercise decoration"
    );
    ctx
}

/// Clone the real networks with every `key_env` remapped to names that are
/// guaranteed either SET (dummy value) or UNSET. Nothing is ever removed from
/// the process environment, so these tests cannot disturb any other test.
fn remap_keys(networks: &[AffiliateNetwork], suffix: &str, set_them: bool) -> Vec<AffiliateNetwork> {
    let mut cloned: Vec<AffiliateNetwork> = networks.to_vec();
    for n in cloned.iter_mut() {
        // Sanitize the id so the env-var name is always a legal identifier.
        let id: String = n
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let var = format!("ORDERINV_{}_{}", id.to_uppercase(), suffix);
        if set_them {
            std::env::set_var(&var, "DUMMY_KEY_FOR_ORDER_INVARIANCE");
        }
        n.key_env = Some(var);
    }
    cloned
}

fn ctx_with(networks: Vec<AffiliateNetwork>, patterns: &[String]) -> AffiliateCtx {
    AffiliateCtx {
        networks,
        exact_model_patterns: patterns.to_vec(),
    }
}

/// THE ORDER-INVARIANCE LOCK, driven by the REAL data file.
///
/// For each shipped network in isolation: decorate the same already-ranked input
/// once with that network's key resolvable and once with it unresolvable, and
/// require the ranked URL list to be byte-identical. Non-vacuity is enforced in
/// both directions — the keyed run MUST have decorated, the keyless run MUST NOT
/// — so this can never "pass" by comparing two no-op runs.
#[test]
fn real_data_order_is_invariant_for_every_network_key_on_and_off() {
    let ctx = real_ctx();

    for net in &ctx.networks {
        // KEYS PRESENT: only this network, its remapped key set to a dummy value.
        let keyed = remap_keys(std::slice::from_ref(net), "PRESENT", true);
        let keyed_ctx = ctx_with(keyed, &ctx.exact_model_patterns);
        let mut with_key = ranked_fixture();
        decorate_affiliate_for_query(&mut with_key, &keyed_ctx, EXACT_MODEL_QUERY);
        let with_key_ranked = ranked_urls(&with_key);
        let with_key_aff = affiliate_urls(&with_key);
        assert!(
            !with_key_aff.is_empty(),
            "network {:?} ({:?}): the keys-present run must actually decorate \
             (affiliate urls present); an empty decoration makes the comparison vacuous",
            net.id,
            net.kind
        );

        // KEYS ABSENT: same network, key_env remapped to a name nothing ever sets.
        let unkeyed = remap_keys(std::slice::from_ref(net), "ABSENT", false);
        let unkeyed_ctx = ctx_with(unkeyed, &ctx.exact_model_patterns);
        let mut no_key = ranked_fixture();
        decorate_affiliate_for_query(&mut no_key, &unkeyed_ctx, EXACT_MODEL_QUERY);
        let no_key_ranked = ranked_urls(&no_key);
        let no_key_aff = affiliate_urls(&no_key);
        assert!(
            no_key_aff.is_empty(),
            "network {:?} ({:?}): the keys-absent run must NOT decorate \
             (affiliate urls absent)",
            net.id,
            net.kind
        );

        assert_eq!(
            with_key_ranked, no_key_ranked,
            "network {:?} ({:?}): affiliate decoration changed the ranked URL order \
             (count, URLs, or sequence) between keys-present and keys-absent runs",
            net.id, net.kind
        );
    }
}

/// The WHOLE production config, keys resolvable vs keys unresolvable — the
/// closest offline analogue of the live probe: same `AffiliateCtx::load()`
/// production uses, only the resolvability of the declared keys differs.
#[test]
fn real_data_whole_config_order_invariant_keys_present_vs_absent() {
    let ctx = real_ctx();

    let unkeyed_ctx = ctx_with(
        remap_keys(&ctx.networks, "WHOLE_ABSENT", false),
        &ctx.exact_model_patterns,
    );
    let mut absent = ranked_fixture();
    decorate_affiliate_for_query(&mut absent, &unkeyed_ctx, EXACT_MODEL_QUERY);
    let absent_ranked = ranked_urls(&absent);
    assert!(
        affiliate_urls(&absent).is_empty(),
        "keys unresolvable => nothing may be decorated"
    );

    let keyed_ctx = ctx_with(
        remap_keys(&ctx.networks, "WHOLE_PRESENT", true),
        &ctx.exact_model_patterns,
    );
    let mut present = ranked_fixture();
    decorate_affiliate_for_query(&mut present, &keyed_ctx, EXACT_MODEL_QUERY);
    let present_ranked = ranked_urls(&present);
    assert!(
        !affiliate_urls(&present).is_empty(),
        "keys resolvable => decoration must happen"
    );

    assert_eq!(
        present_ranked, absent_ranked,
        "with the full shipped network set, affiliate keys changed the ranked URL order"
    );
}

/// Broad category queries must stay free AND must not move anything either. This
/// is the business-policy half of the same invariant: no monetization on
/// non-model queries, and still no reordering.
///
/// The queries are SHAPES, not one string tuned to the regex: a category phrase,
/// a bare product phrase, a generic object, and a price fragment. If the shipped
/// policy legitimately grows to monetize one of them, these assertions fail and
/// the change is made deliberately instead of drifting silently.
#[test]
fn real_data_broad_queries_never_decorate_and_never_reorder() {
    let ctx = real_ctx();
    let keyed_ctx = ctx_with(
        remap_keys(&ctx.networks, "BROAD_PRESENT", true),
        &ctx.exact_model_patterns,
    );

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
        let baseline_ranked = ranked_urls(&ranked_fixture());
        let mut arr = ranked_fixture();
        decorate_affiliate_for_query(&mut arr, &keyed_ctx, q);
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
}
