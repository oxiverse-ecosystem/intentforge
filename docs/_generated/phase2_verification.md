# Phase 2 verification — 2026-11-21

## Corrections found during execution
- **D13:** live Supabase JWKS is EC/ES256, not RS256. Implemented the plan's documented fallback: validate via `GET /auth/v1/user` with publishable key + bearer, cache validated `sub` by token hash for 5 min (`services/gateway/src/auth.rs`). No `jsonwebtoken` dependency added.
- **DDL:** Phase 1 made `goals.owner_ref NOT NULL`, so `claim_anonymous_goals` failed with `23502` (null owner_ref). Applied `0002_phase2_claim.sql` (nullable owner_ref + `goals_exactly_one_owner` check).
- **DDL:** `leaderboard` had `security_invoker=true`; anon callers had no SELECT on base tables, so the view was always empty. Applied `0003_leaderboard_anon.sql` (`security_invoker=false`; view's own WHERE already restricts to claimed + opted-in).
- **Resilience:** a transient VPN network error made `SupabaseStore::create` panic and drop the client connection. Added 3-attempt retry with backoff and a 503 `degraded` response instead of a panic.

## Applied changes
- Gateway: `Owner::Claimed`, per-owner PostgREST filter (`owner_id` vs `owner_ref`), async `parse_owner` with JWT branch, `POST /goals/claim`, `PUT /consents/:purpose`, auth middleware JWT allowance for `/goals*` and `/consents*`, `expires_at` omitted for claimed goals.
- Tests: `tests/test_goals_phase2.py` (throwaway Supabase users via Admin API, claim, isolation, idempotency, consent default-off/opt-in/revoke, leaderboard shape).
- UI prompt: three credential classes, Google sign-in, claim-on-login, sign-out, separate default-off leaderboard/hiring consent toggles, live leaderboard entries.
- No new env vars; prod compose unchanged from Phase 1. `.env.example` JWKS/JWT vars remain commented.

## Real results
| Check | Result |
|---|---|
| `cargo test` (Rust 1.89 container) | **520 passed, 0 failed** |
| 4 API suites, `IF_GOAL_STORE=supabase` | **56 passed** (366.74s) |
| 3 Phase 1 API suites, `IF_GOAL_STORE=in_memory` | **54 passed** (163.26s) |
| `scripts/verify_goals_supabase.sh` (in-memory) | all checks passed |
| `git diff --check` | clean (pre-existing CRLF warnings only) |
| Tracked-file OAuth credential scan | no matches |

Dev gateway is back on the documented `in_memory` default; prod compose remains `supabase`.
