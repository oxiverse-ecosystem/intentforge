OXIVERSE DOCS & COVERAGE CYCLE — v2 FINAL REPORT
(reporter t_7b6218d1, following independent audit t_5348df31)

== IntentForge (repo: intentforge) ==
v2 round commits (confirmed in `git log`):
- b9c33b8  docs(api-ref): verified live media shapes, Goals error codes + intent reconciliation
- 04dea49  docs(readme): complete Goals section, softened /goals question-count claim
Base audited: 3cc8249 -> 04dea49 (ahead 2 of origin/master, unpushed)

ADDED/FIXED:
- Full Goals section in README (7-route table, 1-indexed phase IDs, +100/phase, in-memory notes).
- API_REFERENCE Goals error-code table: 400 empty_goal / 400 invalid_phase / 404 not_found / 422 invalid_payload — verified live.
- Earlier false claim that /search/fast lacks top-level `source` (t_fdab0124) HELD: re-verified live `/search/fast?q=rust` -> top keys ['count','results','source'].

COVERAGE: 14/14 routes have a doc section. Media shapes re-verified live:
- /images -> image_url + thumbnail_url (NOT thumbnail)
- /videos -> thumbnail (NOT thumbnail_url), video_id observed ''
- /news -> published_at (observed '' on bing-news items)

AUDIT VERDICT: 1 VIOLATION (F1, doc-structural). The v2 diff had dropped the `### POST /goals`
heading, orphaning the create-goal docs. FIX card t_7eb143a3 spawned AND ALREADY remediated in
the working tree — reporter verified via `git diff`: the `### POST /goals` heading is re-inserted
at API_REFERENCE.md:1015, the error-code block is intact, and the change is structural only (no
content drift). This fix is currently UNCOMMITTED WIP (FIX card still running), so F1 is RESOLVED
in the tree but not yet banked as a commit.

== RAVANA (repo: ravana) ==
v2 round commits (confirmed in `git log`):
- 6aa79e9  docs(readme): add 'What RAVANA does' derived-capability section (+33 lines)
- 3c586d2  docs(index): cross-link README from docs orientation
- 31ee109  chore: stop tracking generated .test_durations cache
Base audited: 1618ed2 -> 31ee109 (ahead 3 of origin/main, unpushed)

ADDED: a real "What RAVANA does" user-facing section (README.md:30) documenting 6 behaviors
(chat/identity, fact learning, stances, self-correction, recall, abstention). Each reply traces to
a value-interpolated code template, NOT authored prose. Cited store classes (IdentityEngine,
UserStanceStore, PersonalFactStore, BeliefStore, ConceptGraph) all exist in source.

REMOVED: .test_durations de-cluttered from index + gitignored — verified inert (no code/CI refs;
local copy retained). No coverage loss.

AUDIT VERDICT: SUBSTANTIALLY HONEST, no violations. All 6 capability replies verified against the
generating code. No missing user-facing coverage found.

== BROADENED MANDATE — gap closed? ==
- IntentForge Goals section: YES, complete in README + API_REFERENCE (heading fix pending commit).
- RAVANA derived capabilities: YES, documented from the live self-model, engine-derived.
- No undocumented user-facing route/feature found in either repo.

== HONEST LIMITATIONS ==
- IntentForge live checks re-run against the gateway as it is now; the same contracts/shapes hold.
  Full 30+ request transcript NOT re-executed — targeted the drift-prone claims.
- RAVANA capability replies verified by reading generating code, not re-running a live chat probe.
- F1 fix is uncommitted working-tree WIP by the FIX card; it has not been committed/pushed yet.
  The audit report file still reads "VIOLATION" because it predates the fix landing in the tree.
- All claims above trace to: `git log` SHAs (intentforge 3cc8249..04dea49, ravana 1618ed2..31ee109),
  the independent audit report docs/_generated/audit-report-v2.md, and live grep/git-diff evidence.
