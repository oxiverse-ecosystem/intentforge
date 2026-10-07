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
// PORT NOTE (this file was ported onto the round branch from
// `commerce/fbu-guard-t_caef42c9`, whose HEAD-of-file variant also asserted an
// `exact_model_patterns` / `is_exact_model_query` eligibility gate). That gate
// does not exist on this lineage, so the broad-query eligibility test could not
// be ported as written; inventing a gate here would be a FEATURE, not a port.
// Only the order-invariance invariant is asserted below, which is the mandate.
// See the eligibility gap noted on the originating card.
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
// production code (`decorate_affiliate` -> `first_usable` ->
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

/// Load the real runtime config and assert it is actually usable, so a broken or
/// missing data file FAILS here instead of making every invariance test below
/// pass by decorating nothing.
///
/// This is the guard the card asked for: without it an empty/mis-mounted
/// `data/commerce/affiliate.json` turns both comparison runs into no-ops and the
/// order-invariance guarantee reports success while testing nothing.
fn real_ctx() -> Vec<AffiliateNetwork> {
    let ctx = AffiliateCtx::load();
    assert!(
        !ctx.networks.is_empty(),
        "data/commerce/affiliate.json yielded no networks — the order-invariance \
         tests would be VACUOUS (both runs would decorate nothing and compare \
         equal). Check that the test image COPYs data/ into the working dir."
    );
    // The fixture spans both renderer kinds, so asserting both are present
    // catches a config that silently lost a whole template kind.
    assert!(
        ctx.networks.iter().any(|n| n.kind == "wrap"),
        "shipped config has no `wrap` network — the fixture would not exercise the \
         network-prefix renderer"
    );
    assert!(
        ctx.networks.iter().any(|n| n.kind == "append_params"),
        "shipped config has no `append_params` network — the fixture would not \
         exercise the param-append renderer"
    );
    ctx.networks
}

/// Clone the real networks with every `key_env` remapped to names that are
/// guaranteed either SET (dummy value) or UNSET. Nothing is ever removed from
/// the process environment, so these tests cannot disturb any other test.
fn remap_keys(
    networks: &[AffiliateNetwork],
    suffix: &str,
    set_them: bool,
) -> Vec<AffiliateNetwork> {
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

/// THE ORDER-INVARIANCE LOCK, driven by the REAL data file.
///
/// For each shipped network in isolation: decorate the same already-ranked input
/// once with that network's key resolvable and once with it unresolvable, and
/// require the ranked URL list to be byte-identical. Non-vacuity is enforced in
/// both directions — the keyed run MUST have decorated, the keyless run MUST NOT
/// — so this can never "pass" by comparing two no-op runs.
#[test]
fn real_data_order_is_invariant_for_every_network_key_on_and_off() {
    let networks = real_ctx();

    for net in &networks {
        // KEYS PRESENT: only this network, its remapped key set to a dummy value.
        let keyed_ctx = AffiliateCtx {
            networks: remap_keys(std::slice::from_ref(net), "PRESENT", true),
        };
        let mut with_key = ranked_fixture();
        decorate_affiliate(&mut with_key, &keyed_ctx);
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
        let unkeyed_ctx = AffiliateCtx {
            networks: remap_keys(std::slice::from_ref(net), "ABSENT", false),
        };
        let mut no_key = ranked_fixture();
        decorate_affiliate(&mut no_key, &unkeyed_ctx);
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
    let networks = real_ctx();

    let unkeyed_ctx = AffiliateCtx {
        networks: remap_keys(&networks, "WHOLE_ABSENT", false),
    };
    let mut absent = ranked_fixture();
    decorate_affiliate(&mut absent, &unkeyed_ctx);
    let absent_ranked = ranked_urls(&absent);
    assert!(
        affiliate_urls(&absent).is_empty(),
        "keys unresolvable => nothing may be decorated"
    );

    let keyed_ctx = AffiliateCtx {
        networks: remap_keys(&networks, "WHOLE_PRESENT", true),
    };
    let mut present = ranked_fixture();
    decorate_affiliate(&mut present, &keyed_ctx);
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

/// Decoration must be a PURE post-ranking pass over the real config: the
/// complete serialized result array (not just the URL column) is byte-identical
/// once the `affiliate` block is removed. This catches a decoration path that
/// mutates a field nobody diffs by eye — score, title, metadata — which a
/// URL-only comparison would miss.
#[test]
fn real_data_decoration_touches_no_field_except_affiliate() {
    let networks = real_ctx();
    let keyed_ctx = AffiliateCtx {
        networks: remap_keys(&networks, "FIELDS", true),
    };

    let mut decorated = ranked_fixture();
    decorate_affiliate(&mut decorated, &keyed_ctx);
    assert!(
        !affiliate_urls(&decorated).is_empty(),
        "the keyed run must actually decorate, otherwise this test is vacuous"
    );

    // Strip the affiliate block and compare against the untouched fixture.
    let mut stripped = decorated.clone();
    for r in stripped.iter_mut() {
        if let Some(obj) = r.as_object_mut() {
            obj.remove("affiliate");
        }
    }
    assert_eq!(
        ranked_fixture(),
        stripped,
        "decoration mutated a field other than `affiliate` — it is a strict \
         post-ranking pass and must touch nothing else"
    );

    // And the rank ORDER itself must survive, unchanged and complete.
    assert_eq!(
        ranked_urls(&ranked_fixture()),
        ranked_urls(&decorated),
        "decoration changed the ranked URL order"
    );
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
    let keyed_ctx = AffiliateCtx {
        networks: remap_keys(&networks, "STRIP_PRESENT", true),
    };
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
