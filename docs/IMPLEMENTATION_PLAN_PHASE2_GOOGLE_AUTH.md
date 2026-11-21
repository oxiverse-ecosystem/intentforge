# Phase 2 Handoff — Google Auth, Claim Flow, Leaderboard, Consents

**Prompt to paste into goal mode:** `Execute docs/IMPLEMENTATION_PLAN_PHASE2_GOOGLE_AUTH.md in /c/Users/Likhith/Documents/projects/intentforge. It is self-contained and locked. Do not ask questions; work through P0–P8 and report verified results.`

**Status:** Ready for autonomous execution. Every decision below is locked; do not re-ask them.
**Prerequisite (already true):** Phase 1 is complete — 518 Rust tests and 54 API tests pass; `goals`, `profiles`, `consents`, `leaderboard`, `hiring_profiles` and the `claim_anonymous_goals` / `purge_expired_anonymous_goals` / `touch_anonymous_goals` functions exist in Supabase; dev gateway runs `IF_GOAL_STORE=in_memory`, prod compose runs `supabase`.
**Dashboards (already true):** Google Cloud project `intentforge-515414` has Google Auth Platform configured, Web OAuth client `IntentForge Web`, redirect `https://omnrbfguunidctyrbpoh.supabase.co/auth/v1/callback`, test user `semalalikithsai@gmail.com`. Supabase project `omnrbfguunidctyrbpoh` has Google provider ENABLED, nonce checks OFF, email-less users OFF, Site URL `http://localhost:3000`, redirect allow-list `http://localhost:3000/**` and `http://localhost:4000/**`. Client ID/secret exist only in Supabase; never read, print or commit them.
**Non-goals:** hiring delivery/company endpoints, email/SMS notifications, payments, self-hosted Supabase, changes to search/commerce ranking, changes to Phase 1 anonymous behavior.

---

## 0. Locked decisions (do not revisit)

| # | Decision |
|---|---|
| D12 | Phase 2 adds a third credential: `Authorization: Bearer *** Supabase access JWT>` → `Caller::User(uuid from `sub`). API key and `X-Anonymous-Owner` remain valid. Precedence on normal goal routes: API key → valid JWT → anonymous ref. Invalid/malformed JWT is ignored (falls through), not a 401, so an expired session degrades to anonymous rather than breaking the app. |
| D13 | **JWT verification via Supabase introspection + cache (provider-contract correction, 2026-11-21):** the live JWKS at `https://omnrbfguunidctyrbpoh.supabase.co/auth/v1/.well-known/jwks.json` is **EC / ES256** (one key, `kty:EC`), not RS256, and `jsonwebtoken` v9 exposes no clean JWK→`DecodingKey` conversion. Per the allowed fallback in this decision, validate by `GET {SUPABASE_URL}/auth/v1/user` with `apikey: publishable` and `Authorization: Bearer <token>`; a 200 with an `id` field proves signature and expiry server-side. Cache validated `sub` by a non-cryptographic hash of the token for 5 minutes in memory. HS256/JWKS code is deliberately NOT added. |
| D14 | `Owner` gains `Claimed(Uuid)`. Anonymous behavior is unchanged. Every Supabase query for a claimed owner filters `owner_id=eq.<uuid>`; anonymous still filters `owner_ref=eq.<uuid>`. Service keeps the all-zeros ref and unchanged global behavior. |
| D15 | `POST /goals/claim` accepts **both** the user JWT and a valid `X-Anonymous-Owner` header. It calls the existing `claim_anonymous_goals(p_ref)` RPC (publishable key + user JWT so `auth.uid()` is set), returns the moved goals as a bare array, and is idempotent (empty array when nothing to claim). No new SQL function needed. On success the UI clears `if_owner_ref`. |
| D16 | Claimed goals never expire; anonymous goals keep the 6h sliding TTL. The `expires_at` field is omitted for claimed owners (additive field only). |
| D17 | Leaderboard becomes live: the existing `leaderboard` view already enforces claimed-only + opt-in + `roadmap IS NOT NULL`, score-desc, ≤50. No new view. Response stays a bare JSON array with the existing fields. Anonymous/service goals never appear. |
| D18 | Consents are user-scoped and RLS-protected by the existing `consents` table. Expose `PUT /consents/:purpose` (`leaderboard` or `hiring_notices`) with `{"enabled": bool}` using the user's JWT via PostgREST; `enabled=false` sets `revoked_at`. Default off. No company-facing endpoint. |
| D19 | UI prompt gains Google sign-in, claim-on-login, an opt-in leaderboard toggle and a **separate** default-off hiring-notices toggle with plain-language purpose text and revoke. Never bundle the two consents. No external scripts/CDNs (protects the anonymous capability and the session token). |
| D20 | If production JWTs are RS256 (signing keys), `SUPABASE_JWT_SECRET` stays empty; otherwise set it. Both env names become active in prod compose and `.env.example`. The secret key remains server-only and `apikey`-header-only. |
| D21 | In-memory dev mode stays the default and keeps all Phase 1 tests green. JWT verification and claim are exercised only when `IF_GOAL_STORE=supabase`. No unit test may require live Supabase credentials. |
| D22 | Throwaway test users are created via Supabase Admin API with the secret key and `email_confirm=true`, then signed in via `/auth/v1/token?grant_type=password` to obtain a real JWT. Credentials are generated randomly per test run and never committed. |

---

## 1. Current code map (grounded)

| Concern | File / location |
|---|---|
| `Owner`, `valid_owner_ref`, `parse_owner`, `ttl_hours`, `add_expires` | `services/gateway/src/goals.rs` ~996–1085 |
| `GoalsBackend` trait, `InMemoryBackend`, `GoalRow`, `SupabaseStore`, `backend_from_env` | `services/gateway/src/goals.rs` ~1092–1470 |
| Goal handlers (`create`, `answers`, `get`, `list`, `leaderboard`, `quick`, `complete`, `progress`) | `services/gateway/src/goals.rs` ~1090+ (each takes `HeaderMap` and calls `parse_owner`) |
| `api_key_valid`, `extract_api_key`, `auth_middleware` | `services/gateway/src/main.rs` ~19–65 |
| Goal routes (leaderboard in open router; rest in keyed router) | `services/gateway/src/main.rs` ~14422–14433 |
| `AppState.goals: Arc<dyn GoalsBackend>` | `services/gateway/src/main.rs` ~13580, init ~14298 |
| Dormant SQL objects (profiles, consents, leaderboard, hiring_profiles, claim RPC) | `services/database/migrations/0001_goals.sql` lines 34–89 |
| Deferred env vars (commented) | `.env.example` lines 30–39 |
| Phase 1 UI auth contract (must be rewritten) | `UI_PROMPT_GOOGLE_STITCH.md` §5 + §3.2 + §4.8 + §6.4–6.5 |
| Phase 1 tests | `tests/test_goals_api_full.py`, `tests/test_goals_anonymous.py`, `tests/test_goals_api_schema.py` |
| Phase 1 verification record | `docs/_generated/goals_supabase_verification.md` |

---

## 2. Gateway changes

### 2.1 Identity + verification

- In `goals.rs`, add `Owner::Claimed(String)` and extend `supabase_ref()` usage so claimed owners pass `owner_id`, not `owner_ref` (add a `filter()` helper returning the PostgREST pair, e.g. `("owner_ref", uuid)` vs `("owner_id", uuid)`; service keeps the all-zeros ref).
- Add `parse_owner` branch: after API key, inspect `Authorization: Bearer *** if present. Call `verify_supabase_jwt(token) -> Option<Uuid>`; on success return `Owner::Claimed(uid)`. On failure ignore and continue to `X-Anonymous-Owner`. Never log the token.
- Implement `verify_supabase_jwt` in a new `services/gateway/src/auth.rs` module (add `mod auth;` in `main.rs`). Use `jsonwebtoken` v9 + JWKS cache (D13). The JWKS HTTP call uses the existing `reqwest` client pattern with a 10s timeout; cache by `kid` in a `parking_lot::RwLock<HashMap<String, DecodingKey>>`.
- `auth_middleware` in `main.rs`: allow a valid Supabase JWT on `/goals*` by calling `goals::jwt_claims(headers).is_some()` (cheap local verify). Keep `/shopping` and `/commerce/extract` API-key-only. Keep `/goals/leaderboard` open.

### 2.2 Backend trait

Extend `GoalsBackend` with:
```rust
async fn claim(&self, anon_ref: &str, uid: &str) -> Vec<StoredGoal>;
```
- `InMemoryBackend::claim`: move the `anon:<ref>` store's goals into a new `claimed:<uid>` store, return them; no-op if absent.
- `SupabaseStore::claim`: `POST {SUPABASE_URL}/rest/v1/rpc/claim_anonymous_goals` with `apikey: publishable`, `Authorization: Bearer *** user JWT`, body `{"p_ref":"<uuid>"}`. Map returned rows via `GoalRow`.
- Every existing read/write in `SupabaseStore` must use the correct filter for `Owner::Claimed` (`owner_id=eq.<uid>`) and must never match on `owner_ref` for claimed rows.
- `add_expires` must skip `Owner::Claimed`.

### 2.3 New endpoints

- `POST /goals/claim` — keyed router; parse JWT and `X-Anonymous-Owner` **explicitly** (do not rely on `parse_owner` precedence); 401 if either is missing/invalid; call backend `claim`; return bare array.
- `PUT /consents/:purpose` — keyed router; require `Caller::User`; validate purpose ∈ {`leaderboard`,`hiring_notices`}; body `{"enabled": bool}`; call PostgREST `UPSERT /rest/v1/consents` with publishable key + user JWT and `Prefer: resolution=merge-duplicates,return=representation`; on `enabled=false` include `revoked_at: now()`, else `granted_at: now(), revoked_at: null`. Return the consent row. No service-role key on this path.
- Keep all existing goal response shapes unchanged.

### 2.4 Leaderboard

`SupabaseStore::leaderboard()` already reads the dormant view; nothing to change server-side. Verify with an opted-in claimed goal that an entry appears with all seven fields, and that opting out removes it immediately. In-memory leaderboard keeps Phase 1 behavior (service namespace) so existing tests stay green.

---

## 3. Compose + env

No new env vars are needed: D13's introspection path reuses the already-configured `SUPABASE_URL` and `SUPABASE_PUBLISHABLE_KEY`. `SUPABASE_JWKS_URL` / `SUPABASE_JWT_SECRET` stay commented in `.env.example` and absent from prod compose (they belong to the abandoned JWKS approach). The secret key remains server-only and `apikey`-header-only.

**Additional DDL applied during this run** (both via the Supabase SQL Editor; files kept for reproducibility):
- `services/database/migrations/0002_phase2_claim.sql` — drops `NOT NULL` from `goals.owner_ref` and adds `goals_exactly_one_owner CHECK (num_nonnulls(owner_ref, owner_id) = 1)`; without this the dormant claim RPC fails with `23502`.
- `services/database/migrations/0003_leaderboard_anon.sql` — `ALTER VIEW leaderboard SET (security_invoker = false)`; with `security_invoker=true` anon callers had no SELECT on the base tables and the view was always empty even for opted-in claimed rows.

## 4. UI prompt (external frontend, `UI_PROMPT_GOOGLE_STITCH.md`)

Rewrite §5 from "TWO CREDENTIAL CLASSES ONLY (Phase 1)" to three classes: service API key, anonymous owner capability, Supabase user JWT. Add:
- Google sign-in via `@supabase/supabase-js` `signInWithOAuth({provider:'google'})`; same-origin relative API paths; session access token sent as `Authorization: Bearer` on goal requests.
- On first successful sign-in, call `POST /goals/claim` with both the JWT and `X-Anonymous-Owner`; on success clear `if_owner_ref` and reload goals. Show a one-time "Your roadmaps are now saved to your account" state.
- Account menu with sign-out; after sign-out, generate a fresh `if_owner_ref` and continue anonymously.
- Settings/Privacy screen: separate toggles for leaderboard opt-in and hiring notices, both default off, plain-language purpose text, revoke buttons. Hiring copy must say no goal text or email is shared with companies.
- Leaderboard renders real entries from the bare array; empty state remains for anonymous visitors.
- Keep "no external scripts/fonts/CDNs", self-hosted assets, defensive optional-field access, and all Phase 1 provenance/latency rules.

---

## 5. Execution phases (each ends with a verifiable artifact)

**P0 — Ground & lock.** Read the code map (§1) and confirm current behavior. Done when no code changes yet and a short status note is written to `docs/_generated/phase2_start.md`.

**P1 — JWT module.** Add `auth.rs`, `jsonwebtoken` dep, JWKS cache, HS256 fallback, `verify_supabase_jwt`, unit tests with locally generated RSA/HS256 tokens (no network). Done when `cargo test` passes with the new tests and no live Supabase calls occur.

**P2 — Owner/backend extension.** Add `Owner::Claimed`, filter helper, `claim()` on both backends, `expires_at` skip. Done when `cargo test` passes and all 54 Phase 1 API tests still pass in in-memory mode.

**P3 — Endpoints.** `POST /goals/claim`, `PUT /consents/:purpose`, middleware JWT branch. Done when dev stack with `IF_GOAL_STORE=supabase` returns 401 for claim without JWT/ref and 200 with both.

**P4 — Live claim test.** Create a throwaway Supabase user via Admin API (D22), obtain a JWT, create two anonymous goals with a fresh ref, claim them, assert: user sees both, old ref sees none, a second user cannot read/mutate them, claiming twice returns `[]`. Save redacted output to `docs/_generated/phase2_claim.md`.

**P5 — Leaderboard + consents.** Opt in via `PUT /consents/leaderboard`, assert entry appears with seven fields and correct score; opt out, assert removal. Hiring consent defaults off and revokes. Done when the live checks pass and are recorded.

**P6 — UI prompt.** Apply §4. Done when `grep -nE 'Google|claim|consent|JWT'` shows the new flows and no "TWO CREDENTIAL CLASSES ONLY" text remains.

**P7 — Compose/env + regression.** Apply §3. Done when prod compose has only placeholders, `.env.example` matches, `git diff --check` is clean, and the full suite (Phase 1 + Phase 2 tests) passes in both store modes.

**P8 — Handoff note.** Write `docs/_generated/phase2_verification.md` with commands, results, and remaining manual UI work.

---

## 6. Verification commands

```bash
cd /c/Users/Likhith/Documents/projects/intentforge/services
docker compose -f docker-compose.dev.yml build gateway
docker compose --env-file ../.env -f docker-compose.dev.yml up -d --no-deps gateway
curl -fsS http://localhost:4000/health
cd ../..
IF_API_KEY=intentforge-dev-key INTENTFORGE_REQUIRE_GATEWAY=1 \
  uv run --with pytest --with requests python -m pytest \
  tests/test_goals_api_full.py tests/test_goals_anonymous.py tests/test_goals_api_schema.py tests/test_goals_phase2.py -q
# Rust: no local cargo — use the container
docker run --rm -v "C:/Users/Likhith/Documents/projects/intentforge/services/gateway":/src -w /src \
  rust:1.89-slim-bookworm bash -c 'apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq libssl-dev pkg-config >/dev/null 2>&1 && export PATH=$PATH:/usr/local/cargo/bin && cargo test --message-format=short'
```

Add `tests/test_goals_phase2.py` with env-configurable `IF_API_KEY`, `IF_TEST_EMAIL_SUFFIX`, and JWT obtained at runtime; no committed tokens.

---

## 7. Risks / rollback

- **JWT verification fragility:** JWKS key conversion may vary by crate version; the D13 fallback (Introspect via `/auth/v1/user` + 5-minute cache) is allowed and must be documented if used.
- **Expired sessions:** invalid JWT degrades to anonymous (D12), so a user may briefly lose claimed goals until re-auth; UI must show a sign-in prompt rather than a hard error.
- **Claim race:** two concurrent claims with the same ref are safe because the RPC updates rows in one statement and a second claim returns `[]`.
- **Consent leakage:** consents are RLS-scoped to `auth.uid()`; never use the secret key on the consent endpoint.
- **Rollback:** revert `IF_GOAL_STORE=in_memory` and remove the new routes; dormant SQL objects can stay. Claimed rows keep `owner_id` and are ignored by Phase 1 code.
