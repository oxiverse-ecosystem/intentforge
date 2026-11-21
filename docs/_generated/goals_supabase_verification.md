# Goal schema verification — Phase 1 (2026-11-21)

Project: `omnrbfguunidctyrbpoh` (hosted Supabase). Keys sourced from `.env` at runtime; no values recorded here.

## Schema objects present (PostgREST, secret key on `apikey` only)

| Object | Result |
|---|---|
| `GET /rest/v1/goals?select=goal_id&limit=1` | 200 `[]` |
| `GET /rest/v1/profiles?select=id&limit=1` | 200 `[]` |
| `GET /rest/v1/consents?select=owner_id&limit=1` | 200 `[]` |
| `GET /rest/v1/leaderboard?select=goal_id&limit=1` | 200 `[]` (dormant: no claimed rows) |
| `POST /rest/v1/rpc/purge_expired_anonymous_goals` `{"p_hours":999999}` | 200 `0` |
| `POST /rest/v1/rpc/touch_anonymous_goals` `{"p_ref":<uuid>}` | 204 |

`pg_cron` was not enabled during verification; the hourly purge schedule is documented in `services/database/migrations/0001_goals.sql` and was exercised by calling the RPC directly.

## Cross-restart persistence

- Created `goal_000057` with a fresh `X-Anonymous-Owner` UUID v4.
- Recreated/restarted the gateway; `GET /goals/goal_000057` with the same owner → **200** with `expires_at`.

## TTL purge end-to-end

- `PATCH /rest/v1/goals?goal_id=eq.goal_000057` `{"updated_at":"2020-01-01T00:00:00Z"}` → 204.
- `POST /rest/v1/rpc/purge_expired_anonymous_goals` `{"p_hours":6}` → `1`.
- `GET /rest/v1/goals?goal_id=eq.goal_000057&select=goal_id` → `[]`.
- `GET /goals/goal_000057` via gateway with a non-matching owner → **404**.

## Test results (final code)

| Suite | Result |
|---|---|
| Rust unit (`cargo test`, Rust 1.89 container) | 517 passed, 0 failed (later 518 after configurability test) |
| `tests/test_goals_api_full.py` + `tests/test_goals_anonymous.py` + `tests/test_goals_api_schema.py`, in-memory dev gateway | 53 passed |
| Same three suites with `IF_GOAL_STORE=supabase` | 53 passed |
| `scripts/verify_goals_supabase.sh` | all checks passed |

Notes: Supabase-mode run exposed a duplicate-create 409 panic; fixed by gateway-side dedupe before insert. The two leaderboard assertions that assumed visible entries were relaxed per decision D4 (view is empty until accounts launch).
