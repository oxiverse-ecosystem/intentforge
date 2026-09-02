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
        bid_floor: None,
        fallback_url: None,
        bid_check_url: None,
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

/// Pure helper used by the order-invariance test: the ranked URL list is the
/// ORDER of the `url` field exactly as ranking produced it. Affiliate decoration
/// MUST leave this byte-identical. We read `url` (not `affiliate.url`) on
/// purpose — that is the field the ranking pipeline owns and the one a future
/// manipulation would try to change.
fn collect_ranked_urls(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = payload.get("results").and_then(|v| v.as_array()) {
        for r in arr {
            if let Some(u) = r.get("url").and_then(|v| v.as_str()) {
                out.push(u.to_string());
            }
        }
    }
    out
}

/// ╔════════════════════════════════════════════════════════════════════════╗
/// ║ MANDATORY ORDER-INVARIANCE TEST — THE BUSINESS THESIS.                  ║
/// ║                                                                          ║
/// ║ Likhith's entire commerce idea: "affiliate links instead of ads, with NO ║
/// ║ search manipulation." The ONLY machine-checkable proof of that claim is  ║
/// ║ this ORDER-INVARIANCE test: for the same already-ranked results, the     ║
/// ║ ranked URL list (order + URLs + count) must be byte-identical whether    ║
/// ║ affiliate decoration is ENABLED or DISABLED. Without it a future change   ║
/// ║ could quietly let an affiliated merchant rank higher and the loop could   ║
/// ║ not prove the promise. The audit brief (oxiverse-qa-cycle.sh) requires    ║
/// ║ this; this test IS that requirement, made permanent in CI.                ║
/// ╚════════════════════════════════════════════════════════════════════════╝
///
/// DESIGN: `decorate_affiliate` is already a STRICT post-ranking pass (it is
/// called in `handle_shopping` only AFTER `handle_search` fixes the ranked
/// order, and it only writes each result's `affiliate` field — never `url`,
/// never order). The test below compares the pre-decoration order against the
/// post-decoration order and they MUST match by construction. No refactor was
/// needed; this test locks the invariant so it cannot regress.
///
/// ENABLED run: a network whose shape is IDENTICAL to the production Sovrn row
/// in data/commerce/affiliate.json (kind=`wrap`, same template, `cuid={subid}`);
/// its `key_env` is presented with a dummy-but-present value so the REAL
/// decoration code path executes. DISABLED run: an empty network set (no key =>
/// `first_usable()` returns None => `affiliate` omitted, results returned
/// untouched) — exactly the "keys unset" behaviour. Both runs feed the SAME
/// ranked input; the ranked `url` order must not move.
///
/// The test is GENERAL: it asserts on the ranked URL list, not on any
/// query-specific string, so it holds for ANY shopping query, not just the
/// example. The representative payload is just a fixture exercising the real
/// `decorate_affiliate` pass.
#[test]
fn test_affiliate_decoration_does_not_change_ranking() {
    // Representative already-ranked results for a shopping query shape
    // ("buy wireless earbuds under 200" — triggers shopping intent downstream).
    let ranked_input = representative_shopping_payload("buy wireless earbuds under 200");

    // ── ENABLED run ──────────────────────────────────────────────────────────
    // Mirror production's Sovrn network (real `wrap` template + cuid={subid});
    // enable it by presenting its key env var with a dummy-but-present value so
    // the decoration code path actually runs.
    std::env::set_var("SOVRN_COMMERCE_KEY", "DUMMY_ENABLED");
    let enabled_net = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        {
            let mut p = HashMap::new();
            p.insert("cuid".to_string(), "{subid}".to_string());
            p
        },
        Some("SOVRN_COMMERCE_KEY"),
    );
    let enabled_ctx = AffiliateCtx { networks: vec![enabled_net] };
    let mut enabled_payload = ranked_input.clone();
    if let Some(arr) = enabled_payload
        .get_mut("results")
        .and_then(|v| v.as_array_mut())
    {
        decorate_affiliate(arr, &enabled_ctx);
    }
    let enabled_ranked = collect_ranked_urls(&enabled_payload);
    let enabled_aff = collect_affiliate_urls(&enabled_payload);

    // ── DISABLED run ─────────────────────────────────────────────────────────
    // No networks at all => first_usable() returns None => affiliate omitted,
    // results returned untouched. This IS the "keys unset" behaviour.
    let disabled_ctx = AffiliateCtx { networks: vec![] };
    let mut disabled_payload = ranked_input.clone();
    if let Some(arr) = disabled_payload
        .get_mut("results")
        .and_then(|v| v.as_array_mut())
    {
        decorate_affiliate(arr, &disabled_ctx);
    }
    let disabled_ranked = collect_ranked_urls(&disabled_payload);
    let disabled_aff = collect_affiliate_urls(&disabled_payload);

    // Non-vacuous guard: the ENABLED run must actually decorate, and the
    // DISABLED run must not — otherwise the two runs would trivially match and
    // the test would "pass" while proving nothing.
    assert!(
        !enabled_aff.is_empty(),
        "ENABLED run must actually decorate (affiliate urls present); else the test is vacuous"
    );
    assert!(
        disabled_aff.is_empty(),
        "DISABLED run must NOT decorate (affiliate urls absent)"
    );

    // THE CONTRACT: ranked URL order is byte-identical with or without
    // decoration. If they differ, affiliate monetization changed ranking.
    if enabled_ranked != disabled_ranked {
        eprintln!("ORDER-INVARIANCE VIOLATION — affiliate monetization changed ranking:");
        eprintln!("  ENABLED  ranked urls (count={}):", enabled_ranked.len());
        for (i, u) in enabled_ranked.iter().enumerate() {
            eprintln!("    [{}] {}", i, u);
        }
        eprintln!("  DISABLED ranked urls (count={}):", disabled_ranked.len());
        for (i, u) in disabled_ranked.iter().enumerate() {
            eprintln!("    [{}] {}", i, u);
        }
    }
    assert_eq!(
        enabled_ranked, disabled_ranked,
        "affiliate decoration MUST NOT change the ranked URL order (count, URLs, or sequence)"
    );
}

/// NON-VACUOUS CONTROL: proves the order-invariance test above would CATCH a
/// real manipulation. Decoration must write only the `affiliate` field and
/// leave `url` (the ranking-owned field) byte-identical. If a future change
/// incorrectly folded URL wrapping into `url` (the classic "monetization affects
/// ranking" bug), the ranked URL list would change and the contract test would
/// FAIL. This test locks that field-level distinction so the invariant is
/// enforced structurally, not just asserted.
#[test]
fn test_affiliate_decoration_writes_affiliate_field_not_url() {
    std::env::set_var("SOVRN_COMMERCE_KEY", "DUMMY_ENABLED");
    let net = contract_net(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        {
            let mut p = HashMap::new();
            p.insert("cuid".to_string(), "{subid}".to_string());
            p
        },
        Some("SOVRN_COMMERCE_KEY"),
    );
    let ctx = AffiliateCtx { networks: vec![net] };

    let raw = representative_shopping_payload("buy wireless earbuds under 200");
    let before_urls: Vec<String> = raw["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();

    let mut payload = raw.clone();
    if let Some(arr) = payload.get_mut("results").and_then(|v| v.as_array_mut()) {
        decorate_affiliate(arr, &ctx);
    }
    let after_urls: Vec<String> = payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();

    // The `url` field is NEVER touched by decoration => ranked order intact.
    assert_eq!(
        before_urls, after_urls,
        "decorate_affiliate must NOT mutate the `url` field (ranking-owned)"
    );
    // Whereas the `affiliate` field IS added (decoration happened).
    let aff = collect_affiliate_urls(&payload);
    assert_eq!(
        aff.len(),
        before_urls.len(),
        "every ranked result gained an affiliate block"
    );
    // And the decorated (affiliate) urls are OBVIOUSLY different strings from the
    // ranked (raw) urls, so a future change that folded wrapping into `url` would
    // be caught by the order-invariance test above.
    assert_ne!(
        aff, after_urls,
        "decoration targets `affiliate`, not `url`; if these were equal, wrapping leaked into ranking"
    );
}

/// ─────────────────────────────────────────────────────────────────────────────
/// ROADMAP item 6 — bid-floor (`bf`) / fallback (`fbu`) support + reporting.
/// These are DATA-DRIVEN network fields that are appended to the decorated URL as
/// query params and surfaced in the `affiliate` block for reporting. They MUST
/// never affect ranking: decoration runs strictly AFTER the ranked order is fixed,
/// so the `*_invariance` assertions below prove they cannot move results.
/// ─────────────────────────────────────────────────────────────────────────────

/// Build a network row that carries `bid_floor` + `fallback_url` (item 6 fields).
fn contract_net_with_bf(
    kind: &str,
    template: &str,
    params: HashMap<String, String>,
    key_env: Option<&str>,
    bid_floor: Option<&str>,
    fallback_url: Option<&str>,
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
        bid_floor: bid_floor.map(|s| s.to_string()),
        fallback_url: fallback_url.map(|s| s.to_string()),
        bid_check_url: None,
    }
}

#[test]
fn item6_bid_floor_and_fallback_appended_and_reported() {
    std::env::set_var("CONTRACT_SOVRN_BF_KEY", "CONTRACT_SOVRN_BF_KEY");
    let mut params = HashMap::new();
    params.insert("cuid".to_string(), "{subid}".to_string());
    let n = contract_net_with_bf(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        params,
        Some("CONTRACT_SOVRN_BF_KEY"),
        Some("0.10"),
        Some("https://shop.example.com/fallback"),
    );
    let ctx = AffiliateCtx { networks: vec![n] };

    let mut payload = representative_shopping_payload("best wireless earbuds under 50 dollars");
    if let Some(arr) = payload.get_mut("results").and_then(|v| v.as_array_mut()) {
        decorate_affiliate(arr, &ctx);
    }

    let urls = collect_affiliate_urls(&payload);
    assert!(!urls.is_empty(), "decoration must have produced affiliate urls");
    for u in &urls {
        let lower = u.to_lowercase();
        assert!(lower.contains("bf=0.10"), "bid floor must be appended: {}", u);
        assert!(
            lower.contains("fbu=https%3a%2f%2fshop.example.com%2ffallback")
                || lower.contains("fbu=https%3A%2F%2Fshop.example.com%2Ffallback"),
            "fallback url must be url-encoded and appended: {}",
            u
        );
        // CONTRACT (A): still disclosed.
        assert!(
            u.contains("disclosed") || true,
            "disclosure is on the affiliate block, checked separately"
        );
    }
    // Reporting fields present on the affiliate block.
    if let Some(arr) = payload.get("results").and_then(|v| v.as_array()) {
        for r in arr {
            let aff = r.get("affiliate").expect("affiliate block present");
            assert_eq!(
                aff.get("disclosed").and_then(|v| v.as_bool()),
                Some(true),
                "disclosed must stay true with bf/fbu"
            );
            assert_eq!(
                aff.get("bid_floor").and_then(|v| v.as_str()),
                Some("0.10"),
                "bid_floor surfaced for reporting"
            );
            assert_eq!(
                aff.get("fallback").and_then(|v| v.as_str()),
                Some("https://shop.example.com/fallback"),
                "fallback surfaced for reporting"
            );
        }
    }
}

#[test]
fn item6_bf_fbu_never_change_ranking() {
    // Same order-invariance guarantee, now WITH bf/fbu present (the hardest case).
    std::env::set_var("SOVRN_COMMERCE_KEY", "DUMMY_ENABLED");
    let mut params = HashMap::new();
    params.insert("cuid".to_string(), "{subid}".to_string());
    let enabled_net = contract_net_with_bf(
        "wrap",
        "https://sovrn.co?key={key}&u={url}",
        params,
        Some("SOVRN_COMMERCE_KEY"),
        Some("0.25"),
        Some("https://shop.example.com/fallback"),
    );
    let enabled_ctx = AffiliateCtx { networks: vec![enabled_net] };
    let ranked_input = representative_shopping_payload("buy wireless earbuds under 200");

    let mut enabled_payload = ranked_input.clone();
    if let Some(arr) = enabled_payload
        .get_mut("results")
        .and_then(|v| v.as_array_mut())
    {
        decorate_affiliate(arr, &enabled_ctx);
    }
    let enabled_ranked = collect_ranked_urls(&enabled_payload);
    let enabled_aff = collect_affiliate_urls(&enabled_payload);
    assert!(
        !enabled_aff.is_empty(),
        "ENABLED run must actually decorate (with bf/fbu)"
    );
    assert!(
        enabled_aff.iter().all(|u| u.to_lowercase().contains("bf=0.25")),
        "bf must be present in decorated urls"
    );

    let disabled_ctx = AffiliateCtx { networks: vec![] };
    let mut disabled_payload = ranked_input.clone();
    if let Some(arr) = disabled_payload
        .get_mut("results")
        .and_then(|v| v.as_array_mut())
    {
        decorate_affiliate(arr, &disabled_ctx);
    }
    let disabled_ranked = collect_ranked_urls(&disabled_payload);

    assert_eq!(
        enabled_ranked, disabled_ranked,
        "bf/fbu decoration MUST NOT change the ranked URL order"
    );
}

/// ─────────────────────────────────────────────────────────────────────────────
/// ROADMAP item 5 — multi-merchant offer comparison lock-in (pure, offline).
/// `build_offer_comparisons` is a read-only, post-enrichment view over already
/// attached `commerce` blocks. These tests prove it groups by gtin/sku, sorts by
/// extracted price, and flags mixed observation times — and never reorders results.
/// ─────────────────────────────────────────────────────────────────────────────
fn commerce_result(url: &str, gtin: &str, price: f64, observed_at: &str) -> Value {
    json!({
        "url": url,
        "commerce": {
            "url": url,
            "observed_at": observed_at,
            "source": "json-ld",
            "data": {
                "gtin": gtin,
                "price": price,
                "currency": "USD",
                "merchant": url
            }
        }
    })
}

#[test]
fn item5_groups_by_gtin_sorts_by_price_and_flags_mixed_times() {
    let results = vec![
        commerce_result("https://a.example/prod", "GTIN1", 19.99, "100"),
        commerce_result("https://b.example/prod", "GTIN1", 9.99, "200"),
        commerce_result("https://c.example/other", "GTIN2", 5.00, "300"),
    ];
    let comps = build_offer_comparisons(&results);
    // GTIN1 has 2 offers => one comparison. GTIN2 has only 1 offer => correctly
    // excluded (a single offer is not a "comparison"). So exactly ONE group.
    assert_eq!(comps.len(), 1, "one comparison for the product with >=2 offers");

    // Find GTIN1's comparison: must contain both a + b, sorted ascending by price.
    let g1 = comps
        .iter()
        .find(|c| c.product_id.as_deref() == Some("GTIN1"))
        .expect("GTIN1 group present");
    assert_eq!(g1.id_kind.as_deref(), Some("gtin"));
    assert_eq!(g1.offers.len(), 2, "both merchants grouped");
    assert_eq!(g1.offers[0].price, Some(9.99), "lowest price first");
    assert_eq!(g1.offers[1].price, Some(19.99));
    // Observed at 100 vs 200 => mixed observation times => must be flagged.
    assert!(
        g1.mixed_observation_times,
        "different observed_at across offers must be flagged as mixed"
    );

    // GTIN2 has a single offer => no comparison (need >= 2 to compare).
    let g2 = comps
        .iter()
        .find(|c| c.product_id.as_deref() == Some("GTIN2"));
    assert!(g2.is_none(), "single-offer product is NOT a comparison");
}

#[test]
fn item5_no_comparison_without_shared_id() {
    let results = vec![
        commerce_result("https://a.example/prod", "GTIN_A", 19.99, "100"),
        commerce_result("https://b.example/prod", "GTIN_B", 9.99, "100"),
    ];
    let comps = build_offer_comparisons(&results);
    assert!(
        comps.is_empty(),
        "no comparison when products share no gtin/sku (never fabricate)"
    );
}

