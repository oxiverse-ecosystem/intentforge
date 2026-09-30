// ─── FIX-IF-35: absolute score normalization + naming-question shape ───
//
// Defect class: `calibrate_scores` is PURELY POSITIONAL — it maps the set's
// [min,max] onto [0.05,1.0], so the set maximum is forced onto exactly 1.0 by
// construction, every query. `s=1.000` therefore meant "rank #1 of this set", not
// "confident match": a ranking number wearing a confidence number's clothes. On
// 2026-09-30 that degenerate 1.000 landed on a QUESTION page for
// "why is the apache web server named after a helicopter", above the page that
// actually answers it.
//
// Three structural defects, all fixed together (none is a host list or a query
// literal):
//   (A) the top of the scale was positional, not absolute  -> apply_absolute_merit_ceiling
//   (B) question-shape detection required a forum PATH    -> interrogative title alone
//   (C) the naming PREDICATE ("named") was a core TOPIC term -> relevance was 0.000
//       for the whole set, so (A) had no absolute signal to work with.
#![cfg(test)]

use super::*;

#[cfg(test)]
mod fix_if_35_score_normalization_tests {
    use super::*;

    fn cst() -> Constraints {
        Constraints::default()
    }
    fn empty_sem() -> std::collections::HashMap<String, f32> {
        std::collections::HashMap::new()
    }

    fn web_res(url: &str, title: &str, content: &str) -> SearxResult {
        SearxResult {
            title: title.to_string(),
            url: url.to_string(),
            content: content.to_string(),
            engine: "bing".to_string(),
            score: 1.0,
            sources: vec!["bing".to_string()],
            published_date: None,
            price: None,
            currency: None,
        }
    }

    // ── (A) Score normalization ──────────────────────────────────────────────
    // The card's headline invariant: reaching the top of the scale must be EARNED
    // in absolute terms, not granted for being the best of a weak set.

    #[test]
    fn low_relevance_upstream_result_cannot_reach_one() {
        // Calibrated score 1.0 (the positional max) with absolute relevance 0.0
        // must NOT be allowed to keep 1.0.
        let relevance = vec![0.0f32];
        let mut scores = vec![1.0f32];
        apply_absolute_merit_ceiling(&relevance, &mut scores);
        assert!(
            scores[0] < 1.0,
            "a zero-relevance result reached the top of the scale: {}",
            scores[0]
        );
    }

    #[test]
    fn strong_relevance_result_may_still_reach_one() {
        // Counter-guard: the ceiling must not cap a genuinely strong match, or the
        // "fix" would just flatten every good SERP.
        let relevance = vec![0.9f32];
        let mut scores = vec![1.0f32];
        apply_absolute_merit_ceiling(&relevance, &mut scores);
        assert!(
            (scores[0] - 1.0).abs() < 1e-6,
            "a high-relevance result must keep the top of the scale, got {}",
            scores[0]
        );
    }

    #[test]
    fn absolute_ceiling_never_promotes_or_reorders() {
        // The ceiling may only ever LOWER a score. If it promoted, it would
        // silently reorder the SERP; this pins that it cannot.
        let relevance = vec![0.0f32, 0.9f32, 0.2f32];
        let mut scores = vec![0.30f32, 0.90f32, 0.10f32];
        let before = scores.clone();
        apply_absolute_merit_ceiling(&relevance, &mut scores);
        for (i, (after, orig)) in scores.iter().zip(before.iter()).enumerate() {
            assert!(
                *after <= *orig + 1e-6,
                "result {} was PROMOTED by the ceiling: {} -> {}",
                i,
                orig,
                after
            );
        }
    }

    #[test]
    fn absolute_ceiling_is_length_mismatch_safe() {
        // Defensive: a length mismatch must be a no-op, never a panic or a
        // zip-truncation that silently misaligns results against their relevance.
        let mut scores = vec![1.0f32, 0.5];
        apply_absolute_merit_ceiling(&[0.0f32], &mut scores);
        assert_eq!(
            scores,
            vec![1.0f32, 0.5],
            "mismatched lengths must be a no-op"
        );
    }

    // ── (B) Question-shaped pages cannot claim the top of the scale ───────────

    #[test]
    fn plain_article_question_title_cannot_top_a_naming_query() {
        // The live offender was an ordinary editorial URL (no /r/, no /question/)
        // whose title merely poses the question. The old gate required a forum
        // path, so it sailed through at s=1.000.
        let q = "why is the apache web server named after a helicopter";
        let question_page = web_res(
            "https://example-news.org/what-famous-helicopter-was-named-for-its-marker",
            "What famous helicopter was named for its marker?",
            "Have you ever wondered why this aircraft carries that name?",
        );
        let answer_page = web_res(
            "https://example.org/apache-name",
            "Where the Apache HTTP Server got its name",
            "The Apache HTTP Server is named after the AH-64 Apache helicopter, a name chosen by the Apache Software Foundation.",
        );
        let out = merge_local_and_web(
            vec![],
            vec![question_page, answer_page],
            q,
            "informational",
            &cst(),
            None,
            None,
            &empty_sem(),
        );
        let qp = out
            .iter()
            .find(|r| r.url.contains("example-news.org"))
            .expect("question-shaped page must remain PRESENT in the SERP (cap, not drop)");
        let ap = out
            .iter()
            .find(|r| r.url.contains("example.org/apache-name"))
            .expect("answer page missing from the result set");
        assert!(
            ap.score > qp.score,
            "a question-shaped page outranked the answer page: answer={} question={}",
            ap.score,
            qp.score
        );
    }

    #[test]
    fn question_shaped_page_cannot_claim_top_of_scale() {
        // Score-scale assertion for the same defect class: whichever page lands
        // first, a question-shaped page must not carry the degenerate 1.000.
        let q = "why is the amazon company named after the amazon river";
        let question_page = web_res(
            "https://some-blog.net/why-is-amazon-called-amazon",
            "Why is Amazon called Amazon?",
            "Many people ask why the company carries the name of the river.",
        );
        let out = merge_local_and_web(
            vec![],
            vec![question_page],
            q,
            "informational",
            &cst(),
            None,
            None,
            &empty_sem(),
        );
        let qp = &out[0];
        assert!(
            qp.score < 1.0,
            "a question-shaped page claimed the top of the scale: {}",
            qp.score
        );
    }

    // ── (C) The naming PREDICATE is query structure, not a topic term ─────────

    #[test]
    fn naming_predicate_verb_is_recognized_as_predicate_not_entity() {
        // Closed-class vocabulary only — no entity, brand or host is named here.
        assert!(is_naming_predicate_word("named"));
        assert!(is_naming_predicate_word("called"));
        assert!(is_naming_predicate_word("etymology"));
        // Not predicates: ordinary topic words that must stay matchable.
        assert!(!is_naming_predicate_word("apache"));
        assert!(!is_naming_predicate_word("helicopter"));
    }

    #[test]
    fn naming_question_shape_still_detected_after_vocab_hoist() {
        // The vocabulary is now shared between query-shape detection and the
        // term-extraction filters; this pins that hoisting it did not change
        // query-shape behaviour (the two uses have not drifted apart).
        assert!(is_naming_question(
            "why is the apache web server named after a helicopter"
        ));
        assert!(is_naming_question("why is dallas called the big d"));
        assert!(is_naming_question("where does the name ford come from"));
        // Non-naming questions must not be swept in.
        assert!(!is_naming_question("how to make biryani at home"));
        assert!(!is_naming_question(
            "best mechanical keyboard for programming"
        ));
    }

    #[test]
    fn absolute_relevance_is_alive_for_a_naming_question() {
        // THE root-cause regression test. Before FIX-IF-35 the naming predicate
        // ("named") was a core topic term; `core_matches` required it to appear
        // literally, so overlap collapsed to 0, the BERT gate (which only runs
        // when overlap > 0) switched off, and `relevance` was 0.000 for the
        // ENTIRE set — measured live on all 21 results. With the predicate
        // excluded, a page that genuinely discusses both entities must earn
        // non-zero absolute relevance.
        let q = "why is the apache web server named after a helicopter";
        let answer_page = web_res(
            "https://www.apache.org/apache-name",
            "Apache HTTP Server name origin",
            "The Apache HTTP Server was named after the AH-64 Apache attack helicopter.",
        );
        let out = merge_local_and_web(
            vec![],
            vec![answer_page],
            q,
            "informational",
            &cst(),
            None,
            None,
            &empty_sem(),
        );
        let ap = &out[0];
        assert!(
            ap.score >= 0.05,
            "the genuine answer page fell to the noise floor: {}",
            ap.score
        );
    }

    // ── Polyseme pair: BOTH readings of "apache" must remain reachable. This is
    // the guard against "fixing" the defect by suppressing the helicopter sense. ──

    #[test]
    fn polyseme_pair_both_readings_survive() {
        // The web-server reading must win for a web-server naming query...
        let q = "why is the apache web server named after a helicopter";
        let server = web_res(
            "https://www.apache.org/apache-name",
            "Apache HTTP Server name origin",
            "The Apache HTTP Server is named after the AH-64 Apache helicopter.",
        );
        let heli = web_res(
            "https://example-aviation.com/ah64-apollo-history",
            "AH-64 Apache helicopter history",
            "The AH-64 Apache attack helicopter was named for the Apache people of the Southwest.",
        );
        let out = merge_local_and_web(
            vec![],
            vec![server, heli],
            q,
            "informational",
            &cst(),
            None,
            None,
            &empty_sem(),
        );
        let first = &out[0];
        assert!(
            first.url.contains("apache.org"),
            "the web-server reading must win for a web-server naming query, got {}",
            first.url
        );

        // ...and the helicopter reading must stay reachable for a query that is
        // explicitly about the aircraft.
        let q2 = "why is the ah-64 apache helicopter named apache";
        let out2 = merge_local_and_web(
            vec![],
            vec![heli, server],
            q2,
            "informational",
            &cst(),
            None,
            None,
            &empty_sem(),
        );
        assert!(
            out2
                .iter()
                .any(|r| r.url.contains("example-aviation.com")),
            "the helicopter reading must remain present for an aircraft query"
        );
    }
}
