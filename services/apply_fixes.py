#!/usr/bin/env python3
"""Apply all 5 bottleneck fixes to the source files.
Run this from the services/ directory."""

import re

# ─── FIX P0: Add distinguishing log label for the pre-merge hard negative filter ───
with open('gateway/src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Change the log message in the pre-merge hard negative filter
old_log = 'tracing::info!("HARD NEGATIVE DROP: result removed because negative constraint matched");'
new_log = 'tracing::info!("HARD NEGATIVE DROP (pre-merge WEB ONLY): result removed because negative constraint matched");'
if old_log in content:
    content = content.replace(old_log, new_log)
    print("P0: Added (pre-merge WEB ONLY) label to existing hard negative filter")
else:
    print("P0: WARN - old log message not found")

# ─── FIX P2: Add semantic relevance gate to thin-result boost ───
old_thin = (
    '    if merged.len() < 15 && merged.len() > 0 {\n'
    '        let max_score = merged.iter().map(|r| r.score).fold(0.0f32, f32::max);\n'
    '        if max_score < 0.30 {\n'
    '            let boost_factor = (0.30 / max_score.max(0.01)).min(2.5);\n'
    '            tracing::info!(\n'
    '                "THIN RESULTS: merged.len={} max_score={:.3} boost={:.2}x",\n'
    '                merged.len(), max_score, boost_factor\n'
    '            );\n'
    '            for r in merged.iter_mut() {\n'
    '                r.score *= boost_factor;\n'
    '            }\n'
    '        }\n'
    '    }'
)

new_thin = (
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

if old_thin in content:
    content = content.replace(old_thin, new_thin)
    print("P2: Applied semantic gate to thin-result boost")
else:
    print("P2: WARN - old thin-result pattern not found")

# ─── FIX P3: Early-exit in semantic_relevance_score for empty/short content ───
old_sem_start = (
    'fn semantic_relevance_score(query: &str, title: &str, content: &str) -> f32 {\n'
    '    let q_lower = query.to_lowercase();\n'
    '    let t_lower = title.to_lowercase();\n'
    '    let c_lower = content.to_lowercase();'
)

new_sem_start = (
    'fn semantic_relevance_score(query: &str, title: &str, content: &str) -> f32 {\n'
    '    // Early exit: if both title and content are empty/too short, return 0.01\n'
    '    let title_trimmed = title.trim();\n'
    '    let content_trimmed = content.trim();\n'
    '    if title_trimmed.is_empty() && content_trimmed.len() < 10 {\n'
    '        return 0.01;\n'
    '    }\n'
    '    // Early exit: if title is meaningful but content is empty, score based on title only\n'
    '    // (skip full TF-IDF scoring that would return 0 anyway)\n'
    '    if content_trimmed.len() < 10 {\n'
    '        let q_lower = query.to_lowercase();\n'
    '        let t_lower = title_trimmed.to_lowercase();\n'
    '        let q_words: Vec<&str> = q_lower.split_whitespace().collect();\n'
    '        let matched = q_words.iter().filter(|w| w.len() > 2 && t_lower.contains(**w)).count();\n'
    '        if matched > 0 {\n'
    '            return (matched as f32 / q_words.iter().filter(|w| w.len() > 2).count().max(1) as f32).clamp(0.01, 0.5);\n'
    '        }\n'
    '        return 0.01;\n'
    '    }\n'
    '\n'
    '    let q_lower = query.to_lowercase();\n'
    '    let t_lower = title_trimmed.to_lowercase();\n'
    '    let c_lower = content_trimmed.to_lowercase();'
)

if old_sem_start in content:
    content = content.replace(old_sem_start, new_sem_start)
    print("P3: Added early-exit for empty/short content in semantic_relevance_score")
else:
    print("P3: WARN - old semantic_relevance_score start pattern not found")

# Write back gateway
with open('gateway/src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("Gateway file updated successfully!")


# ─── FIX P4: Fix Swift false-positive in intent-engine language detection ───
with open('intent-engine/src/main.rs', 'r', encoding='utf-8') as f:
    ie_content = f.read()

# Add storage context disambiguation for "swift"
old_swift = '        ("swift", &["swift"]),'

new_swift_fn = (
    '        ("swift", &["swift"]),\n'
    '        // Disambiguation for "swift" when it appears in storage context:\n'
    '        // OpenStack Swift (object storage) is NOT the Swift programming language.\n'
    '        // Storage context clues: ring, container, object storage, cluster, proxy, account\n'
    '        // These are checked in detect_query_language via additional context logic below.'
)

if old_swift in ie_content:
    ie_content = ie_content.replace(old_swift, new_swift_fn)
    print("P4: Added Swift disambiguation comment")
else:
    print("P4: WARN - old swift pattern not found")

# Find where the context check section and add storage disambiguation for swift
old_context_check_swift = (
    '            // Long names (>= 4 chars): exact word match is sufficient\n'
    '            if alias.len() >= 4 {\n'
    '                if words.iter().any(|w| *w == *alias) {\n'
    '                    return Some(canonical.to_string());\n'
    '                }\n'
    '                continue;\n'
    '            }'
)

new_context_check_swift = (
    '            // Long names (>= 4 chars): exact word match is sufficient\n'
    '            if alias.len() >= 4 {\n'
    '                if words.iter().any(|w| *w == *alias) {\n'
    '                    // Disambiguation: "swift" in OpenStack storage context is NOT a programming language.\n'
    '                    // Check for storage-related terms that indicate OpenStack Swift object storage.\n'
    '                    if *alias == "swift" {\n'
    '                        let storage_context = ["ring", "container", "object", "storage", "cluster",\n'
    '                            "proxy", "account", "tenant", "replication", "consistency",\n'
    '                            "openstack", "keystone", "glance", "nova", "cinder", "horizon",\n'
    '                            "swiftstack", "mid-range", "midrange", "block", "backup",\n'
    '                            "availability zone", "storage policy", "object-store"];\n'
    '                        if storage_context.iter().any(|sc| q_lower.contains(sc)) {\n'
    '                            return None; // This is OpenStack Swift, not the programming language\n'
    '                        }\n'
    '                    }\n'
    '                    return Some(canonical.to_string());\n'
    '                }\n'
    '                continue;\n'
    '            }'
)

if old_context_check_swift in ie_content:
    ie_content = ie_content.replace(old_context_check_swift, new_context_check_swift)
    print("P4: Added storage-context disambiguation for swift language detection")
else:
    print("P4: WARN - old context check pattern not found")

# Write back intent-engine
with open('intent-engine/src/main.rs', 'w', encoding='utf-8') as f:
    f.write(ie_content)

print("Intent-engine file updated successfully!")
print("\nAll fixes applied!")
