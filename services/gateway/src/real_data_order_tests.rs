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

/// A query the SHIPPED model shape must accept. It is ASSERTED at runtime (not
/// assumed) so this file fails loudly if the data policy drifts, rather than
/// silently becoming vacuous.
const EXACT_MODEL_QUERY: &str = "iphone 16 pro max price";

/// A SECOND exact-model family the shipped shape must accept — one whose
/// designator is a hyphenated alphanumeric compound rather than `brand + number`.
/// This is the case the old fitted regex MISSED while its own synthetic-pattern
/// test passed; asserting it against the REAL loaded shape is what stops the
/// data file and the Rust tests from drifting apart again.
const EXACT_MODEL_COMPOUND_QUERY: &str = "sony wh-1000xm5";

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
        ctx.model_shape.min_designator_len > 0 && ctx.model_shape.max_number_len > 0,
        "no exact-model shape loaded — the monetization policy is data-driven and \
         must ship in the data file"
    );
    assert!(
        !ctx.model_shape.variant_suffixes.is_empty()
            && !ctx.model_shape.commerce_words.is_empty()
            && !ctx.model_shape.quantity_markers.is_empty(),
        "shipped model_shape is missing a structural vocabulary list — the gate \
         would degenerate into a bare number-mention test"
    );
    for q in [EXACT_MODEL_QUERY, EXACT_MODEL_COMPOUND_QUERY] {
        assert!(
            ctx.is_exact_model_query(q),
            "shipped model shape must accept {q:?} — otherwise the invariance \
             comparisons below would never exercise decoration"
        );
    }
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

fn ctx_with(networks: Vec<AffiliateNetwork>, shape: &ModelShape) -> AffiliateCtx {
    AffiliateCtx { networks, model_shape: shape.clone() }
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
        let keyed_ctx = ctx_with(keyed, &ctx.model_shape);
        let mut with_key = ranked_fixture();
        decorate_affiliate_for_query(&mut with_key, &keyed_ctx, EXACT_MODEL_QUERY, true);
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
        let unkeyed_ctx = ctx_with(unkeyed, &ctx.model_shape);
        let mut no_key = ranked_fixture();
        decorate_affiliate_for_query(&mut no_key, &unkeyed_ctx, EXACT_MODEL_QUERY, true);
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
        &ctx.model_shape,
    );
    let mut absent = ranked_fixture();
    decorate_affiliate_for_query(&mut absent, &unkeyed_ctx, EXACT_MODEL_QUERY, true);
    let absent_ranked = ranked_urls(&absent);
    assert!(
        affiliate_urls(&absent).is_empty(),
        "keys unresolvable => nothing may be decorated"
    );

    let keyed_ctx = ctx_with(
        remap_keys(&ctx.networks, "WHOLE_PRESENT", true),
        &ctx.model_shape,
    );
    let mut present = ranked_fixture();
    decorate_affiliate_for_query(&mut present, &keyed_ctx, EXACT_MODEL_QUERY, true);
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
        &ctx.model_shape,
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
        decorate_affiliate_for_query(&mut arr, &keyed_ctx, q, true);
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

/// THE ELIGIBILITY TABLE, driven by the REAL shipped `model_shape`.
///
/// The previous policy was a regex fitted to four test literals, and its own
/// Rust test injected a SYNTHETIC pattern instead of loading the shipped data
/// file — so `sony wh-1000xm5` (which the test claimed was monetized) was in
/// fact rejected by production, and `1984 Orwell book` was in fact monetized.
/// This test closes both directions against the real file.
///
/// The sets are SHAPES, not literals the shape was fitted to: the accepted set
/// spans three designator forms (alphanumeric compound `wh-1000xm5`, multi-token
/// variant `note 13 pro`, trailing bare number `3310`, release-suffixed `20 LTS`),
/// and the rejected set spans four non-model numeric shapes (year, year-decade,
/// quantity, count) plus the broad/price-fragment shapes already locked above.
#[test]
fn real_data_model_shape_accepts_models_and_rejects_non_model_numbers() {
    let ctx = real_ctx();

    // Exact product models, across designator families and regions.
    let accept = [
        "sony wh-1000xm5",
        "sony wh-1000xm5 price",
        "sony wh-1000xm5 price in india",
        "redmi note 13 pro",
        "nokia 3310",
        "node 20 LTS",
        "pixel 9 pro max",
        EXACT_MODEL_QUERY,
        "buy iphone 15 pro",
    ];
    for q in accept {
        assert!(
            ctx.is_exact_model_query(q),
            "exact-model query {q:?} must be eligible — the shape must admit every \
             product family, not just an enumerated one"
        );
        // …and the full gate still requires a commercial signal.
        assert!(
            !ctx.is_monetizable(q, false),
            "{q:?} carries a model designator but the request is NOT commercial; \
             the gate must still refuse to monetize it"
        );
        assert!(
            ctx.is_monetizable(q, true),
            "{q:?} is a commercial exact-model query and must be monetizable"
        );
    }

    // Numbers that are years, decades, quantities, or counts — never a product.
    let reject = [
        "1984 Orwell book",
        "f1 2024 standings",
        "1920s jazz playlist",
        "2 minute chess",
        "buy 2 pairs of socks",
    ];
    for q in reject {
        assert!(
            !ctx.is_exact_model_query(q),
            "non-model numeric query {q:?} must NOT be eligible — a bare number \
             that is a year/quantity, not a product designator"
        );
        assert!(
            !ctx.is_monetizable(q, true),
            "non-model query {q:?} was monetized even with a commercial signal"
        );
    }
}
