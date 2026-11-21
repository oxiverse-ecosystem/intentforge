# Implementation Plan — Anonymous Goals with Supabase Persistence (Phase 1: no login)

**Status:** Ready for autonomous execution. All design decisions below are locked; do not re-ask them.
**Scope:** Gateway (`services/gateway`), both compose files, Supabase project schema, CI, docs, and the external UI prompt.
**Phase 1 delivers:** expiring anonymous roadmaps that survive gateway restarts, plus the 27-test API suite and the bundled review fixes.
**Deferred to a later phase (do NOT implement now):** Google OAuth, JWT verification, login/claim, leaderboard entries, hiring notices and their consent UI. See §9.
**Non-goals:** company-facing hiring API, payments, email/SMS notifications, self-hosted Supabase, migrations of the existing WebGraph/commerce tables (spec `06` stays unapproved except the §1.6 amendment in P0).

---

## 0. Locked decisions (do not revisit)

| # | Decision |
|---|---|
| D1 | Anonymous goals are the only supported mode. A browser-generated `owner_ref` (UUID v4, localStorage `if_owner_ref`) is the sole identity. Sent as `X-Anonymous-Owner` header only — never in a query string (URLs get logged). |
| D2 | Goals and their `owner_ref` **expire on a sliding 6-hour TTL** of last goal activity. Env knob `IF_ANON_GOAL_TTL_HOURS=6`. On expiry rows are **deleted**, not anonymised. |
| D3 | **No login in Phase 1.** No Google OAuth, no JWT verification, no `POST /goals/claim`. All auth scaffolding in the schema is dormant and must not be wired into handlers or UI. |
| D4 | `GET /goals/leaderboard` keeps its 200/bare-array contract but returns `[]` in Phase 1 — there are no claimable accounts yet. The view and consents table are created now so Phase 2 needs no data migration. UI shows a "leaderboard opens when accounts launch" placeholder. |
| D5 | Hiring notices are a **separate, default-off, revocable consent**. Schema only in Phase 1; no UI, no delivery, no company access. |
| D6 | Supabase is **hosted**, not self-hosted in Compose. No new containers; only env vars on the gateway. Project ref: `omnrbfguunidctyrbpoh`. |
| D7 | Gateway talks to Supabase over **HTTPS per request** (PostgREST + RPC). **No `sqlx`/PgPool** — the gateway shares gluetun's rotating VPN namespace, so long-lived TCP dies on every IP rotation. `reqwest` is already a dependency. |
| D8 | In-memory store stays the default (`IF_GOAL_STORE=in_memory`). All 27 existing tests must still pass without Supabase. |
| D9 | Wire contract is unchanged: `goal_id` format `goal_<zero-padded digits>`, 1-indexed phases, bare JSON arrays for `/goals` and `/goals/leaderboard` (max 50), score = `completed_phases*100` + 500 all-complete bonus. Additive field only: `expires_at` on anonymous goal responses. |
| D10 | No JWT is expected on any goal request in Phase 1. A `Bearer` value is treated as the legacy service API key, exactly as today. |
| D11 | Use the **new publishable/secret API keys** (`sb_publishable_*` / `sb_secret_*`); legacy `anon`/`service_role` are deprecated by end of 2026. Legacy env names are accepted **only as fallback**. Non-JWT keys go on the `apikey` header **only** — sending one as `Authorization: Bearer` gets `Invalid JWT`. |

Key migration reference (checked 2026-11-20): https://supabase.com/docs/guides/getting-started/migrating-to-new-api-keys

---

## 1. Target architecture

```
Browser                    Gateway (in gluetun netns)              Supabase (hosted)
───────                    ──────────────────────────              ─────────────────
localStorage               two credential paths:                   PostgREST REST + RPC
  if_owner_ref  ──header──▶  API key  (service/dev)                 goals + dormant auth tables
                             owner_ref capability (anon, scoped)   pg_cron  (TTL purge)
                                    │
                     GoalStore trait◀────▶ InMemoryGoalStore (default, tests)
                                    └────▶ SupabaseGoalStore (reqwest)
```

Two credential classes on goal routes in Phase 1:
1. `x-api-key` / `Bearer <service key>` — existing behavior, kept for dev + service-to-service.
2. `X-Anonymous-Owner: <uuid>` — capability; the gateway enforces that every Supabase query is scoped to it.

`/shopping` and `/commerce/extract` stay API-key-only. `/goals/leaderboard` stays open (it returns `[]`).

---

## 2. Supabase schema (migration `0001_goals.sql`)

Run via Supabase SQL editor or `supabase db push`. All statements idempotent where possible.
The `profiles`, `consents`, `leaderboard`, `hiring_profiles` and `claim_anonymous_goals` objects are created **dormant** — they are future-proofing, not Phase 1 features. Do not add endpoints or UI for them.

```sql
-- Sequence preserves the external goal_id contract (goal_000001 style).
CREATE SEQUENCE IF NOT EXISTS goal_id_seq;
CREATE OR REPLACE FUNCTION goal_id_next() RETURNS text LANGUAGE sql VOLATILE AS $$
  SELECT 'goal_' || lpad(nextval('goal_id_seq')::text, 6, '0');
$$;

CREATE TABLE IF NOT EXISTS goals (
  goal_id           text PRIMARY KEY DEFAULT goal_id_next(),
  owner_ref         uuid NOT NULL,                 -- Phase 1: always set
  owner_id          uuid,                          -- Phase 2 only; stays NULL in Phase 1
  goal              text NOT NULL,
  intent            text,
  answers           jsonb NOT NULL DEFAULT '[]'::jsonb,
  roadmap           jsonb,
  completed_phases  integer NOT NULL DEFAULT 0,
  total_phases      integer NOT NULL DEFAULT 0,
  score             integer NOT NULL DEFAULT 0,
  status            text NOT NULL DEFAULT 'pending_answers',
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_goals_owner_goal
  ON goals (owner_ref, md5(lower(trim(goal))));
CREATE INDEX IF NOT EXISTS idx_goals_owner_ref ON goals (owner_ref);
CREATE INDEX IF NOT EXISTS idx_goals_anon_expiry ON goals (updated_at);

-- ── DORMANT: Phase 2 accounts/consent. Create now, do not wire up. ──
CREATE TABLE IF NOT EXISTS profiles (
  id           uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
  display_name text NOT NULL CHECK (char_length(display_name) BETWEEN 1 AND 40),
  skills       text[] NOT NULL DEFAULT '{}',
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS consents (
  owner_id      uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  purpose       text NOT NULL CHECK (purpose IN ('leaderboard','hiring_notices')),
  enabled       boolean NOT NULL DEFAULT false,
  granted_at    timestamptz,
  revoked_at    timestamptz,
  policy_version text NOT NULL DEFAULT '1',
  updated_at    timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (owner_id, purpose)
);

-- Returns zero rows in Phase 1 (no owner_id, no profiles, no consents).
CREATE OR REPLACE VIEW leaderboard WITH (security_invoker = true) AS
SELECT g.goal_id, g.goal, p.display_name AS user_name, g.score,
       g.completed_phases, g.total_phases, g.created_at
FROM goals g
JOIN profiles p ON p.id = g.owner_id
JOIN consents c ON c.owner_id = g.owner_id
              AND c.purpose = 'leaderboard' AND c.enabled AND c.revoked_at IS NULL
WHERE g.owner_id IS NOT NULL AND g.roadmap IS NOT NULL
ORDER BY g.score DESC, g.created_at ASC
LIMIT 50;

-- Hiring: user-chosen fields only. No goal text, no email. Dormant in Phase 1.
CREATE OR REPLACE VIEW hiring_profiles WITH (security_invoker = true) AS
SELECT p.id, p.display_name, p.skills,
       count(*) FILTER (WHERE g.status = 'completed') AS completed_goals,
       max(c.granted_at) AS consented_at
FROM profiles p
JOIN consents c ON c.owner_id = p.id AND c.purpose='hiring_notices'
                AND c.enabled AND c.revoked_at IS NULL
LEFT JOIN goals g ON g.owner_id = p.id
GROUP BY p.id, p.display_name, p.skills;

-- DORMANT: Phase 2 claim RPC. Do not call from the gateway in Phase 1.
CREATE OR REPLACE FUNCTION claim_anonymous_goals(p_ref uuid)
RETURNS SETOF goals LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_uid uuid := auth.uid();
BEGIN
  IF v_uid IS NULL THEN RAISE EXCEPTION 'not authenticated'; END IF;
  INSERT INTO profiles (id, display_name)
  VALUES (v_uid, 'Learner') ON CONFLICT (id) DO NOTHING;
  RETURN QUERY
    UPDATE goals g SET owner_id = v_uid, owner_ref = NULL, updated_at = now()
    WHERE g.owner_ref = p_ref AND g.owner_id IS NULL
    RETURNING g.*;
END $$;
REVOKE ALL ON FUNCTION claim_anonymous_goals(uuid) FROM anon;
GRANT EXECUTE ON FUNCTION claim_anonymous_goals(uuid) TO authenticated;

-- TTL purge — ACTIVE in Phase 1.
CREATE OR REPLACE FUNCTION purge_expired_anonymous_goals(p_hours integer DEFAULT 6)
RETURNS integer LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
  WITH deleted AS (
    DELETE FROM goals
    WHERE owner_id IS NULL AND updated_at < now() - make_interval(hours => p_hours)
    RETURNING 1
  ) SELECT count(*)::integer FROM deleted;
$$;

-- RLS. Phase 1 traffic uses the secret key (bypasses RLS), so the gateway is
-- the enforcement point. These policies are for Phase 2 JWT users.
ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE goals    ENABLE ROW LEVEL SECURITY;
ALTER TABLE consents ENABLE ROW LEVEL SECURITY;

CREATE POLICY profiles_self ON profiles FOR ALL TO authenticated USING (id = auth.uid()) WITH CHECK (id = auth.uid());
CREATE POLICY goals_owner_read   ON goals FOR SELECT TO authenticated USING (owner_id = auth.uid());
CREATE POLICY goals_owner_write  ON goals FOR ALL TO authenticated USING (owner_id = auth.uid()) WITH CHECK (owner_id = auth.uid());
CREATE POLICY consents_owner_all ON consents FOR ALL TO authenticated USING (owner_id = auth.uid()) WITH CHECK (owner_id = auth.uid());
GRANT SELECT ON leaderboard TO anon, authenticated;
```

**pg_cron** (enable the extension in Supabase Dashboard → Extensions, then SQL):
```sql
SELECT cron.schedule('purge-anon-goals', '0 * * * *',
  $$SELECT purge_expired_anonymous_goals(6)$$);
```
Fallback if `pg_cron` is unavailable: a gateway background task calling the same RPC once an hour. Must not block startup.

---

## 3. Gateway changes

### 3.1 `GoalStore` trait (`services/gateway/src/goals.rs`)

Extract the current `Mutex<HashMap<...>>` behind a trait. Async methods (Supabase impl is I/O-bound):

```rust
#[async_trait]
pub trait GoalStore: Send + Sync {
    async fn create(&self, owner: &Owner, goal: &str, intent: &str, resources: Vec<Resource>) -> StoredGoal;
    async fn get(&self, owner: &Owner, goal_id: &str) -> Option<StoredGoal>;
    async fn list(&self, owner: &Owner) -> Vec<StoredGoal>;
    async fn set_roadmap(&self, owner: &Owner, goal_id: &str, roadmap: Roadmap) -> bool;
    async fn set_phase(&self, owner: &Owner, goal_id: &str, phase_id: usize, completed: bool) -> Result<StoredGoal, &'static str>;
    async fn leaderboard(&self) -> Vec<LeaderboardEntry>;
    async fn touch(&self, owner: &Owner);   // refresh anon TTL
}
```

- `Owner` enum in Phase 1: `Anonymous(Uuid)` | `Service` (API-key path keeps current global behavior). Do **not** add a `Claimed` variant yet.
- `InMemoryGoalStore`: current `HashMap` logic verbatim, `touch()` a no-op. **No response shape changes.**
- `SupabaseGoalStore`: `reqwest::Client`, base URL + keys from env. Read `SUPABASE_PUBLISHABLE_KEY` (fallback `SUPABASE_ANON_KEY`) and `SUPABASE_SECRET_KEY` (fallback `SUPABASE_SERVICE_ROLE_KEY`) at startup; fail fast if neither pair is set.
- **Header rules (non-JWT API keys are rejected if also sent as an `Authorization: Bearer` header):**

| Call (Phase 1) | `apikey` | `Authorization` |
|---|---|---|
| All anonymous goal rows (secret key, bypasses RLS) | `sb_secret_...` | **absent** |
| Leaderboard read (returns `[]`) | `sb_publishable_...` | absent |

- The secret key is used for **all** goal rows; the gateway is the enforcement point (secret keys bypass RLS) — therefore validate `X-Anonymous-Owner` is a UUID v4 before use, scope every query with `owner_ref=eq.<uuid>`, and never echo it back in a response.
- Secret keys return HTTP 401 from browser User-Agents; still never put one in the UI prompt or any response.
- Select via `IF_GOAL_STORE=in_memory|supabase` at startup; invalid value fails fast with a clear log.

PostgREST call examples:
```
GET  {SUPABASE_URL}/rest/v1/goals?goal_id=eq.goal_000001&owner_ref=eq.<uuid>&select=*
POST {SUPABASE_URL}/rest/v1/goals            (Prefer: return=representation)
PATCH{SUPABASE_URL}/rest/v1/goals?goal_id=eq.X&owner_ref=eq.<uuid>
GET  {SUPABASE_URL}/rest/v1/leaderboard?select=*
POST {SUPABASE_URL}/rest/v1/rpc/purge_expired_anonymous_goals   {"p_hours":6}
```

### 3.2 Identity helper (`main.rs` + `goals.rs`)

Replace the boolean `api_key_valid()` with `parse_caller(headers) -> Caller`:
- API key present and valid → `Caller::Service`.
- Else if `X-Anonymous-Owner` present and parses as UUID v4 → `Caller::Anonymous(Uuid)`.
- Else `401 {"error":"unauthorized"}`.

**No JWT branch. No `Caller::User`. No `SUPABASE_JWT_SECRET` / JWKS code.** A `Bearer` value is handled by the existing `extract_api_key()` logic as a service key only.

Keep `extract_api_key()` for the keyed `/shopping` + `/commerce/extract` routes. Move `GET /goals/leaderboard` into the open router (it returns `[]`). Pass `Caller` into handlers via `axum::Extension` or an explicit helper — handlers already take `HeaderMap`, so a `caller_from_headers()` helper is acceptable.

### 3.3 TTL response field

For `Caller::Anonymous`, include `"expires_at": <RFC3339>` on goal responses, computed `updated_at + IF_ANON_GOAL_TTL_HOURS`. Additive field only. UI shows a countdown and "These roadmaps expire in a few hours."

### 3.4 Bug fixes bundled here

- `truncate_at_word_boundary` (`goals.rs:186`): byte-slicing `&text[..max_len]` panics on multibyte UTF-8. Replace with `char_indices()` boundary logic; add tests with `"日本語..."` and emoji at offsets 85/180.
- `search_resources` (`goals.rs:757`): hardcodes `http://localhost:4000/search`. Use the same configured URL source as the other engine URLs; add a test asserting no literal `localhost` in the non-dev config path.
- CI gate (`ci.yml:173`): `pytest tests/goals_api_schema.py -v` → `tests/test_goals_api_schema.py`; pass `IF_GATEWAY_URL`/`IF_API_KEY` from secrets; skip only when no gateway is reachable, not always.
- Docs drift: endpoint table lists 5/9 routes → all 9; error table add `invalid_goal`, `invalid_answers`, `internal_error`; fix the unrelated `price_lt`/`price_max` assertion.
- Land `test_goals_api_full.py` (27 tests, exists at `C:\Users\Likhith\AppData\Local\hermes\profiles\devops-link\cache\scratch\test_goals_api_full.py`) into `tests/` with env-configurable gateway URL and key. No key literals.
- Document that `/phases/:id/complete` sets `true` only while `/progress` toggles; add a contract test for each rather than silently changing behavior.

---

## 4. Compose and env

`services/docker-compose.prod.yml` — add to the `gateway.environment` block:
```yaml
      - IF_GOAL_STORE=supabase
      - SUPABASE_URL=${SUPABASE_URL}
      - SUPABASE_PUBLISHABLE_KEY=${SUPABASE_PUBLISHABLE_KEY}
      - SUPABASE_SECRET_KEY=${SUPABASE_SECRET_KEY}
      - IF_ANON_GOAL_TTL_HOURS=6
```
All values come from `.env` at runtime; **never** commit literals. `SUPABASE_SECRET_KEY` is server-only — it must not appear in the browser, the UI prompt, or any response. Legacy `SUPABASE_ANON_KEY` / `SUPABASE_SERVICE_ROLE_KEY` are fallback-only while the project still has them; deactivate them in the Dashboard only after P7 confirms no caller uses them.

`services/docker-compose.dev.yml` — keep `IF_GOAL_STORE=in_memory` (default). Add a commented Supabase block for opt-in integration testing. Do not remove `INTENTFORGE_DEV_MODE=1`.

`.env.example` — placeholders for `IF_GOAL_STORE`, `SUPABASE_URL`, `SUPABASE_PUBLISHABLE_KEY`, `SUPABASE_SECRET_KEY`, `IF_ANON_GOAL_TTL_HOURS`, plus commented legacy fallbacks and a clearly-marked "Phase 2 / deferred" block for `SUPABASE_JWKS_URL` and `SUPABASE_JWT_SECRET`. Mark secrets clearly. Project ref `omnrbfguunidctyrbpoh` → `SUPABASE_URL=https://omnrbfguunidctyrbpoh.supabase.co`.

---

## 5. Docs + UI prompt (external frontend)

- `docs/research-2026-11/06_DATABASE_AND_API_SPEC.md §1.6`: amend to
  > "No mandatory identity and no behavioral tracking. Accounts, if introduced, are user-initiated attachment for achievement portability; all other analytics remain anonymized and short-lived. Anonymous goal data carries a short, fixed expiry."
  Add a footnote pointing to `10_OPEN_DECISIONS_AND_RISKS.md` and this plan.
- `docs/research-2026-11/10_OPEN_DECISIONS_AND_RISKS.md`: record D1–D11 as accepted, with D3/D4/D5 marked "deferred — Phase 2", and the acceptance date.
- `UI_PROMPT_GOOGLE_STITCH.md`: update §3.2 (no "in-memory only" — now expiring anonymous roadmaps; no leaderboard entries yet), §4.8 (expiry rule), §5 (two auth classes only — API key and `X-Anonymous-Owner`; **no Google sign-in, no claim call, no consent toggles**), §6.4 (add `expires_at`; leaderboard renders an empty-state placeholder). Require `if_owner_ref` UUID v4 in localStorage, sent as `X-Anonymous-Owner`. No external scripts/CDNs (protects the capability from XSS).

---

## 6. Execution phases (each ends with a verifiable artifact)

**P0 — Decisions & docs.** Amend §1.6, add decision record. Done when both files changed and `git diff --check` clean.

**P1 — Store refactor, no behavior change.** Add trait + `InMemoryGoalStore`, route handlers through it. Done when `cd services/gateway && cargo test` passes and the 27-test API suite still passes against the in-memory stack.

**P2 — Supabase schema.** Apply §2. Done when: an anonymous row can be inserted/read/updated via PostgREST with the secret key; two different `owner_ref` values cannot see each other's rows; `purge_expired_anonymous_goals(0)` deletes all anonymous rows and `leaderboard` returns zero rows. Save verification SQL + output to `docs/_generated/`.

**P3 — SupabaseGoalStore + caller parsing.** Implement reqwest store and `Caller` with only Service/Anonymous. Done when a dev stack with `IF_GOAL_STORE=supabase` serves create → answers → progress → list across a gateway restart with data intact.

**P4 — TTL.** `touch()` on every goal call, `expires_at` field, purge job verified. Done when a row with backdated `updated_at` disappears within one cron tick and no other row is affected.

**P5 — Leaderboard empty contract + UI prompt.** Public `/goals/leaderboard` returns `[]` (no auth wiring); apply the §5 prompt edits. Done when the UI empty state renders and the prompt contains no Google/claim/consent instructions.

**P6 — Compose, CI, bug fixes.** §3.4 + §4. Done when CI step names are correct, UTF-8 tests pass, prod compose contains only env placeholders (no secrets), and no legacy `anon`/`service_role` env name appears in the gateway's primary path.

**P7 — End-to-end.** Script `scripts/verify_goals_supabase.sh` exercising: anon create/list/progress, cross-`owner_ref` negative test, expiry purge, leaderboard `[]`, API-key path still works. Done when the script exits 0 against a live dev stack with Supabase enabled.

---

## 7. Verification commands

```bash
cd /c/Users/Likhith/Documents/projects/intentforge/services
docker compose -f docker-compose.dev.yml up -d --build
curl -fsS http://localhost:4000/health
cd ../.. && pytest tests/test_goals_api_full.py -v          # 27 tests
cd services/gateway && cargo test
```

Supabase-negative checks use the publishable key and throwaway `owner_ref` UUIDs; no production credentials and no Google accounts needed.

---

## 8. Risks / rollback

- **Capability theft:** anyone with `owner_ref` owns those goals. Mitigated by UUID v4 randomness, header-only transport, no echo in responses, and CSP/no-third-party-scripts in the UI. Acceptable for 6-hour anonymous data.
- **Data loss on expiry:** goals vanish after 6 hours of inactivity. Phase 1 explicitly accepts this; the UI must warn, and Phase 2 accounts are the fix.
- **VPN rotation:** HTTPS per request only. If someone later adds a PgPool, put it outside gluetun's netns or use Supavisor transaction mode.
- **Supabase outage:** goal routes degrade to `503`; in-memory fallback stays available via `IF_GOAL_STORE=in_memory` restart.
- **Rollback:** flip `IF_GOAL_STORE` back to `in_memory`; Supabase tables can be dropped without touching search/commerce. All Phase 1 data is anonymous and disposable, so no export is needed.

---

## 9. Deferred — Phase 2 (do not implement in this run)

Kept here so nothing is lost. Requires a separate handoff/goal.

1. **Google OAuth — dashboard setup COMPLETE (2026-11-21); code still deferred.** Google Auth Platform project `intentforge-515414`: External audience, test user `semalalikithsai@gmail.com`, Web OAuth client `IntentForge Web` with redirect URI `https://omnrbfguunidctyrbpoh.supabase.co/auth/v1/callback`. Credentials were entered directly into Supabase → Authentication → Providers → Google (do NOT commit them) and the provider reads **ENABLED** after reload. *Skip nonce checks* and *Allow users without an email* are OFF. Supabase → Authentication → URL Configuration: Site URL `http://localhost:3000`, Redirect URLs `http://localhost:3000/**` and `http://localhost:4000/**`. Remaining: update those URLs when the production app origin is known.
2. **JWT verification** — JWKS/RS256 cached by `kid`, HS256 fallback via `SUPABASE_JWT_SECRET`; validate `exp`, `aud=authenticated`, `iss`, `role=authenticated`; `sub` → `owner_id`. Add `SUPABASE_JWKS_URL` / `SUPABASE_JWT_SECRET` back to prod compose and `.env.example` at that point.
3. **Claim flow** — `Caller::User`, `POST /goals/claim`, call the `claim_anonymous_goals` RPC with publishable key + user JWT, clear `if_owner_ref` in the UI. Add a `claim()` method to `GoalStore`.
4. **Leaderboard** — wire `consents.leaderboard`, opt-in toggle in UI, claimed-only entries; public read already open.
5. **Hiring notices** — separate revocable consent, `hiring_profiles` view, user-chosen profile only, no raw goal text/email, no company-facing endpoint in the first cut. Legal review of consent wording before shipping.
