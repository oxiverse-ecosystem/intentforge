// ─────────────────────────────────────────────────────────────────────────────
// D5 — the enrichment wall must be DERIVED from the request budget, not chosen.
//
// DEFECT (measured live on this lineage): `/shopping` returned one cold sample
// in 29.48s against a 30s `TimeoutLayer`. The wall was a FLAT 22s at both
// commerce call sites, but the two budgets are independent:
//
//   handle_search  : upstream fan-out (3-10s) + main-path enrichment (22s)
//   handle_shopping: handle_search, THEN a SECOND enrichment pass (22s)
//
// Worst case 44s+ of optional decoration against a 30s transport ceiling. An
// optional commerce pass must never be able to cost the user the entire result
// set with an HTTP 408.
//
// These tests assert the INVARIANT ("elapsed + wall <= transport budget"), not
// a magic number, so they hold for any elapsed time rather than passing only on
// the sample that was measured.
// ─────────────────────────────────────────────────────────────────────────────
#![cfg(test)]

use super::*;
use serde_json::{json, Value};

/// The flat wall a fresh request gets. This is the CEILING, never the budget.
fn fresh_wall() -> std::time::Duration {
    commerce_wall_for_elapsed(std::time::Duration::ZERO)
}

/// A fresh request may not claim more than the documented ceiling, and must not
/// claim the whole transport budget — the headroom is what keeps serialisation
/// and the transport hop inside the budget.
#[test]
fn d5_a_fresh_request_gets_the_ceiling_minus_headroom() {
    assert_eq!(
        fresh_wall(),
        std::time::Duration::from_secs(MAINPATH_ENRICHMENT_WALL_SECS),
        "a fresh request should get the full documented enrichment ceiling"
    );
    assert!(
        fresh_wall() < std::time::Duration::from_secs(REQUEST_BUDGET_SECS),
        "enrichment must never be allowed to claim the entire transport budget; \
         headroom is what keeps the response inside the TimeoutLayer"
    );
}

/// THE INVARIANT: enrichment must never ADD time past the transport budget,
/// i.e. `wall <= budget - elapsed` (saturating). Checked across the whole
/// plausible range, not one sample.
///
/// Note the saturating form: once `elapsed` alone already exceeds the budget the
/// request is late no matter what we do, and the correct wall is ZERO — not a
/// panic and not a negative Duration. Asserting `elapsed + wall <= budget` here
/// would be asserting something impossible, and would "fail" for elapsed=31s
/// regardless of how correct the implementation is.
#[test]
fn d5_elapsed_plus_wall_never_exceeds_the_transport_budget() {
    let budget = std::time::Duration::from_secs(REQUEST_BUDGET_SECS);
    for secs in 0u64..=(REQUEST_BUDGET_SECS * 2) {
        let elapsed = std::time::Duration::from_secs(secs);
        let wall = commerce_wall_for_elapsed(elapsed);
        let remaining = budget.saturating_sub(elapsed);
        assert!(
            wall <= remaining,
            "at elapsed={secs}s the wall was {:?}, but only {:?} of budget remained — \
             decoration would push the request past the {:?}s transport ceiling \
             (the 408 outage class)",
            wall,
            remaining,
            budget
        );
    }
}

/// The real end-to-end statement of the invariant: decoration never pushes the
/// total past the budget. This is the shape of the measured defect (a 30.014s
/// HTTP 408).
///
/// The `max(budget, elapsed)` bound is deliberate and correct: if the upstream
/// fan-out ALONE blew the 30s budget, the request is late no matter what
/// enrichment does — the only correct wall is zero, and claiming we could pull
/// that back would be false. What this asserts is that decoration contributes
/// NOTHING once the budget is spent, which is exactly what the old flat 22s
/// wall violated.
#[test]
fn d5_total_request_time_is_bounded_by_the_transport_budget() {
    let budget = std::time::Duration::from_secs(REQUEST_BUDGET_SECS);
    for upstream_secs in 0u64..=(REQUEST_BUDGET_SECS * 2) {
        let elapsed = std::time::Duration::from_secs(upstream_secs);
        let total = elapsed + commerce_wall_for_elapsed(elapsed);
        assert!(
            total <= std::cmp::max(budget, elapsed),
            "upstream took {upstream_secs}s and decoration pushed the total to {total:?} \
             — past the {:?}s budget the TimeoutLayer cuts the response and the user \
             gets NO results at all",
            budget
        );
    }
    // And across the whole range where the budget is not already blown, the
    // total really does stay inside it.
    for upstream_secs in 0u64..REQUEST_BUDGET_SECS {
        let elapsed = std::time::Duration::from_secs(upstream_secs);
        assert!(
            elapsed + commerce_wall_for_elapsed(elapsed) <= budget,
            "within the budget the total must never exceed {budget:?}"
        );
    }
}

/// A slow upstream must shorten the enrichment window. The ceiling
/// (`MAINPATH_ENRICHMENT_WALL_SECS`) is an upper bound for recall, but the
/// budget-derived term is TIGHTER for any elapsed time at all — 30 - 10 - 2 =
/// 18s, not 22s — so the wall falls one-for-one with elapsed time from the very
/// first second, and reaches zero when the budget is gone.
#[test]
fn d5_a_slow_upstream_shrinks_the_enrichment_window() {
    let ceiling = std::time::Duration::from_secs(MAINPATH_ENRICHMENT_WALL_SECS);
    let fresh = fresh_wall();
    assert_eq!(
        fresh, ceiling,
        "only a zero-elapsed request gets the full ceiling"
    );

    // The ceiling holds while the remaining budget still exceeds it — that is
    // the `30 - 6 - 2 == 22` boundary — and the wall falls away from it after.
    let last_secs_at_ceiling = (REQUEST_BUDGET_SECS - ENRICHMENT_HEADROOM_SECS
        - MAINPATH_ENRICHMENT_WALL_SECS) as u64;
    assert_eq!(
        last_secs_at_ceiling, 6,
        "with a 30s budget, 2s headroom and a 22s ceiling, the wall sits at the \
         ceiling until 6s of the request is spent"
    );
    assert_eq!(
        commerce_wall_for_elapsed(std::time::Duration::from_secs(last_secs_at_ceiling)),
        ceiling,
        "at the boundary the wall is exactly the ceiling"
    );
    assert!(
        commerce_wall_for_elapsed(std::time::Duration::from_secs(
            last_secs_at_ceiling + 1
        )) < fresh,
        "one second past the boundary the wall must fall below the ceiling"
    );

    for secs in (last_secs_at_ceiling + 1)..REQUEST_BUDGET_SECS {
        let wall = commerce_wall_for_elapsed(std::time::Duration::from_secs(secs));
        let expected = std::time::Duration::from_secs(
            REQUEST_BUDGET_SECS
                .saturating_sub(secs)
                .saturating_sub(ENRICHMENT_HEADROOM_SECS)
                .min(MAINPATH_ENRICHMENT_WALL_SECS),
        );
        assert!(
            wall < fresh,
            "at {secs}s elapsed the wall must be below the fresh-request ceiling"
        );
        assert_eq!(
            wall, expected,
            "at {secs}s elapsed the wall must be the remaining budget, minus headroom"
        );
    }
}

/// Once the budget is spent the wall is ZERO — never negative (which would
/// underflow a Duration) and never still-full (which is the bug).
#[test]
fn d5_an_exhausted_budget_yields_a_zero_wall_never_a_negative_one() {
    for secs in [REQUEST_BUDGET_SECS, REQUEST_BUDGET_SECS + 1, REQUEST_BUDGET_SECS * 3] {
        let wall = commerce_wall_for_elapsed(std::time::Duration::from_secs(secs));
        assert_eq!(
            wall,
            std::time::Duration::ZERO,
            "an exhausted or over-budget request must get a zero wall, got {:?} at {secs}s",
            wall
        );
    }
}

/// A zero wall needs no special-casing downstream: the user still gets their
/// ranked results, just without the optional commerce cards. This is the
/// must-not-over-reject direction — the fix must degrade, never break search.
#[test]
fn d5_a_zero_wall_still_returns_results_and_never_fabricates_facts() {
    let mut results = vec![
        json!({"url": "https://shop.acme-electronics.com/p/1", "title": "Widget"}),
        json!({"url": "https://shop.acme-electronics.com/p/2", "title": "Gadget"}),
    ];
    let urls_before: Vec<String> = results
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();

    // No fetch closure is ever invoked (the wall is zero), so nothing can be
    // learned about any page — exactly the situation this test models.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(enrich_with_commerce_par(
        &mut results,
        |url: String| async move {
            panic!("a zero wall must issue no fetch, but was asked for {url}")
        },
        std::time::Duration::ZERO,
    ));

    // Results survive, in the SAME order — enrichment is still a strict
    // post-rank pass.
    let urls_after: Vec<String> = results
        .iter()
        .map(|r| r["url"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(urls_after, urls_before, "a zero wall must not reorder or drop results");

    // Nothing is fabricated: no commerce block, and provenance admits we never
    // looked rather than claiming an observation.
    for r in &results {
        assert!(
            r.get("commerce").is_none(),
            "no page was fetched, so no commerce facts may be attached"
        );
        let prov = &r["commerce_provenance"];
        assert_eq!(
            prov["fetched"], json!(false),
            "provenance must admit the page was never fetched"
        );
        assert_eq!(
            prov["observed_at"], Value::Null,
            "a page we never looked at must not carry an observation time"
        );
    }

    // And the main-path gate therefore suppresses the strip entirely.
    assert!(
        !has_any_commerce_block(&results),
        "with no facts observed the shopping strip must stay suppressed"
    );
}

/// THE REGRESSION LOCK for the actual defect: the /shopping shape is
/// `handle_search` (upstream + its own enrichment) FOLLOWED BY a second
/// enrichment pass. Both passes must draw from ONE budget, so their combined
/// decoration can never exceed the transport ceiling. Before the fix each pass
/// claimed a flat 22s and the pair reached 44s against 30s.
#[test]
fn d5_shopping_and_search_enrichment_share_one_budget_not_two() {
    // Simulate the full /shopping request shape at several upstream costs.
    for upstream_secs in [0u64, 3, 8, 15, 25] {
        // Pass 1 (inside handle_search).
        let pass1 = commerce_wall_for_elapsed(std::time::Duration::from_secs(upstream_secs));
        // Pass 2 (inside handle_shopping) — measured from the SAME request
        // origin, so it accounts for the upstream time AND pass 1.
        let after_pass1 = upstream_secs + pass1.as_secs();
        let pass2 = commerce_wall_for_elapsed(std::time::Duration::from_secs(after_pass1));

        let total = std::time::Duration::from_secs(upstream_secs + pass1.as_secs() + pass2.as_secs());
        assert!(
            total <= std::time::Duration::from_secs(REQUEST_BUDGET_SECS),
            "upstream={upstream_secs}s: the two passes together reached {total:?}, \
             which exceeds the {:?}s transport budget",
            REQUEST_BUDGET_SECS
        );
    }
}

/// The ceiling constant is a CEILING, not the budget: even a zero-elapsed
/// request must leave headroom. This pins the relationship so a future edit
/// that "simplifies" the formula back to `budget - elapsed` (dropping headroom)
/// is caught.
#[test]
fn d5_the_ceiling_is_strictly_below_the_transport_budget() {
    assert!(
        MAINPATH_ENRICHMENT_WALL_SECS < REQUEST_BUDGET_SECS,
        "MAINPATH_ENRICHMENT_WALL_SECS ({MAINPATH_ENRICHMENT_WALL_SECS}) must stay \
         below REQUEST_BUDGET_SECS ({REQUEST_BUDGET_SECS})"
    );
    assert!(
        ENRICHMENT_HEADROOM_SECS > 0,
        "headroom must be non-zero or the response can land exactly on the timeout"
    );
}