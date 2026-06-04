#!/usr/bin/env python3
"""Apply remaining critical fixes - P0 real fix and P2 optimization.
Run this from the services/ directory."""

import re

with open('gateway/src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# ─── FIX P0 (REAL): Add hard negative filter AFTER merge_local_and_web() ───
# The current code has the hard negative filter on web_results (pre-merge).
# We need to ADD a new hard negative filter on the merged results.
# Find the merge_local_and_web call and add filter right after.

old_merge_call = (
    '    // 8. Unified Merge: Local + Web → Single Ranked List\n'
    '    // Cross-source dedup, consensus boosting, unified ranking\n'
    '    // Pass intent distribution for distribution-aware ranking (intent as hint, not gate)\n'
    '    let mut results = merge_local_and_web(\n'
    '        local_results,\n'
    '        web_results,\n'
    '        &q,\n'
    '        &intent.intent,\n'
    '        &intent.structured_constraints,\n'
    '        Some(&intent.distribution),\n'
    '    );\n'
    '\n'
    '    // Sanitize content for safe JSON serialization'
)

new_merge_call = (
    '    // 8. Unified Merge: Local + Web → Single Ranked List\n'
    '    // Cross-source dedup, consensus boosting, unified ranking\n'
    '    // Pass intent distribution for distribution-aware ranking (intent as hint, not gate)\n'
    '    let mut results = merge_local_and_web(\n'
    '        local_results,\n'
    '        web_results,\n'
    '        &q,\n'
    '        &intent.intent,\n'
    '        &intent.structured_constraints,\n'
    '        Some(&intent.distribution),\n'
    '    );\n'
    '\n'
    '    // 8b. Post-merge hard negative filter: apply negative constraints to ALL results\n'
    '    // (local + web). The pre-merge filter only catches web results; local index\n'
    '    // results that match negative terms must also be removed here.\n'
    '    if !intent.structured_constraints.negative.is_empty() {\n'
    '        let before_count = results.len();\n'
    '        let negative_norm: Vec<String> = intent\n'
    '            .structured_constraints\n'
    '            .negative\n'
    '            .iter()\n'
    '            .map(|n| n.to_lowercase())\n'
    '            .collect();\n'
    '\n'
    '        results.retain(|r| {\n'
    '            let text = format!("{} {} {}", r.title, r.content, r.url);\n'
    '            let text_lower = text.to_lowercase();\n'
    '            let text_normalized = {\n'
    '                let chars: Vec<char> = text_lower.chars().collect();\n'
    '                let mut out = String::with_capacity(chars.len());\n'
    '                for (i, &c) in chars.iter().enumerate() {\n'
    '                    if c == \'.\' || c == \'-\' || c == \'_\' {\n'
    '                        if i > 0\n'
    '                            && i + 1 < chars.len()\n'
    '                            && chars[i-1].is_alphanumeric()\n'
    '                            && chars[i+1].is_alphanumeric()\n'
    '                        {\n'
    '                        } else {\n'
    '                            out.push(c);\n'
    '                        }\n'
    '                    } else {\n'
    '                        out.push(c);\n'
    '                    }\n'
    '                }\n'
    '                out\n'
    '            };\n'
    '\n'
    '            let should_keep = negative_norm.iter().all(|neg| {\n'
    '                let neg_lower = neg.to_lowercase();\n'
    '                let words: Vec<&str> = neg_lower.split_whitespace().collect();\n'
    '                if words.len() == 1 {\n'
    '                    let neg_clean: String = neg_lower.chars().filter(|c| c.is_alphanumeric()).collect();\n'
    '                    !(text_lower.split_whitespace().any(|w| {\n'
    '                        let w_clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();\n'
    '                        w_clean.starts_with(&neg_clean) || w_clean.contains(&neg_clean)\n'
    '                    }) || text_normalized.contains(&neg_clean))\n'
    '                } else {\n'
    '                    let joined = words.join(" ");\n'
    '                    !(text_lower.contains(&joined) || text_normalized.contains(&joined))\n'
    '                }\n'
    '            });\n'
    '\n'
    '            if !should_keep {\n'
    '                tracing::info!("HARD NEGATIVE DROP (post-merge): result \\"{}\\" (local={}) removed because negative constraint matched", &r.title[..r.title.len().min(50)], r.is_local);\n'
    '            }\n'
    '            should_keep\n'
    '        });\n'
    '\n'
    '        let removed = before_count.saturating_sub(results.len());\n'
    '        if removed > 0 {\n'
    '            tracing::info!(\n'
    '                "Negative constraint hard filter: removed {}/{} merged results (hard gate, post-merge)",\n'
    '                removed, before_count\n'
    '            );\n'
    '        }\n'
    '    }\n'
    '\n'
    '    // Sanitize content for safe JSON serialization'
)

if old_merge_call in content:
    content = content.replace(old_merge_call, new_merge_call)
    print("P0 (REAL): Added post-merge hard negative filter after merge_local_and_web()")
else:
    print("P0 (REAL): WARN - merge call pattern not found")

# ─── FIX P2 optimization: Cache semantic scores in thin-result gate ───
# Replace the thin-result block with one that reuses cached scores
old_thin_new = (
    '    if merged.len() < 15 && merged.len() > 0 {\n'
    '        let max_score = merged.iter().map(|r| r.score).fold(0.0f32, f32::max);\n'
    '        // Semantic relevance gate: only apply thin-result boost if at least one result\n'
    '        // has minimum semantic relevance to the query. This prevents garbage results\n'
    '        // (local index misses with negative constraint hits) from being amplified.\n'
    '        let max_semantic = merged.iter()\n'
    '            .map(|r| semantic_relevance_score(query, &r.title, &r.content))\n'
    '            .fold(0.0f32, f32::max);\n'
    '        if max_score < 0.30 && max_semantic > 0.05 {\n'
    '            let boost_factor = (0.30 / max_score.max(0.01)).min(2.5);\n'
    '            tracing::info!(\n'
    '                "THIN RESULTS: merged.len={} max_score={:.3} max_sem={:.3} boost={:.2}x",\n'
    '                merged.len(), max_score, max_semantic, boost_factor\n'
    '            );\n'
    '            for r in merged.iter_mut() {\n'
    '                r.score *= boost_factor;\n'
    '            }\n'
    '        } else if max_score < 0.30 {\n'
    '            tracing::info!(\n'
    '                "THIN RESULTS SKIPPED (garbage gate): merged.len={} max_score={:.3} max_sem={:.3}",\n'
    '                merged.len(), max_score, max_semantic\n'
    '            );\n'
    '        }\n'
    '    }'
)

# We need to update the thin-result block to use a cached semantic score
# Rather than re-computing. The main scoring loop already computed scores
# into the 'semantic' variable for each result. We can store max_semantic there.
# But the thin-result block is inside merge_local_and_web which is a separate function.
# The scores are already computed per-result above at line ~1893.
# Let me look at this more carefully...

# Actually the thin-result block is INSIDE merge_local_and_web function.
# The semantic scores are computed at line ~1864 inside the for r in merged.iter_mut() loop.
# The thin-result block is at line ~1915 after that loop.
# So we need to store the max_semantic during the scoring loop.

# Let me find and update the scoring loop to also track max_semantic
old_scoring_loop_start = (
    '    for r in merged.iter_mut() {\n'
    '        let semantic = semantic_relevance_score(query, &r.title, &r.content);\n'
    '        let intent_boost = calculate_intent_boost(&r.url, &r.title, query, intent);\n'
    '        let freshness = freshness_score(&r.url, intent);\n'
    '        let quality = content_quality_score(&r.content);\n'
    '        let c_score = constraint_score(&r.title, &r.content, &r.url, constraints);\n'
    '        let consensus = consensus_score(&r.sources);'
)

new_scoring_loop_start = (
    '    let mut _max_semantic: f32 = 0.0; // tracked for thin-result gate\n'
    '    for r in merged.iter_mut() {\n'
    '        let semantic = semantic_relevance_score(query, &r.title, &r.content);\n'
    '        if semantic > _max_semantic { _max_semantic = semantic; }\n'
    '        let intent_boost = calculate_intent_boost(&r.url, &r.title, query, intent);\n'
    '        let freshness = freshness_score(&r.url, intent);\n'
    '        let quality = content_quality_score(&r.content);\n'
    '        let c_score = constraint_score(&r.title, &r.content, &r.url, constraints);\n'
    '        let consensus = consensus_score(&r.sources);'
)

if old_scoring_loop_start in content:
    content = content.replace(old_scoring_loop_start, new_scoring_loop_start)
    print("P2 opt: Added max_semantic tracking in scoring loop, reusing computed values")
else:
    print("P2 opt: WARN - scoring loop start pattern not found")

# Now update the thin-result block to use cached _max_semantic instead of recomputing
old_thin_opt = (
    '        let max_semantic = merged.iter()\n'
    '            .map(|r| semantic_relevance_score(query, &r.title, &r.content))\n'
    '            .fold(0.0f32, f32::max);\n'
    '        if max_score < 0.30 && max_semantic > 0.05 {'
)

new_thin_opt = (
    '        // Use cached max_semantic from scoring loop (avoids recomputing all scores)\n'
    '        let max_semantic = _max_semantic;\n'
    '        if max_score < 0.30 && max_semantic > 0.05 {'
)

if old_thin_opt in content:
    content = content.replace(old_thin_opt, new_thin_opt)
    print("P2 opt: Replaced recomputed semantic scores with cached _max_semantic")
else:
    print("P2 opt: WARN - thin-result compute pattern not found")

# Write back gateway
with open('gateway/src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("\nGateway file updated successfully with all remaining fixes!")

# Quick verification
verify_checks = [
    ("post-merge hard negative", "results.retain(|r| {" in content and "HARD NEGATIVE DROP (post-merge)" in content),
    ("max_semantic tracking", "_max_semantic" in content),
    ("cached semantic in thin gate", "let max_semantic = _max_semantic;" in content),
]

for name, result in verify_checks:
    status = "✓" if result else "✗"
    print(f"  {status} {name}")

print("\nDone!")
