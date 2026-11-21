-- IntentForge Phase 1 — anonymous goals with hosted Supabase persistence.
-- Apply in Supabase SQL Editor (or `supabase db push`). Idempotent where possible.
-- Phase 2 auth/consent objects are created dormant; do not wire them up yet.

-- External goal_id contract: goal_<zero-padded digits>.
CREATE SEQUENCE IF NOT EXISTS goal_id_seq;
CREATE OR REPLACE FUNCTION goal_id_next() RETURNS text LANGUAGE sql VOLATILE AS $$
  SELECT 'goal_' || lpad(nextval('goal_id_seq')::text, 6, '0');
$$;

CREATE TABLE IF NOT EXISTS goals (
  goal_id           text PRIMARY KEY DEFAULT goal_id_next(),
  owner_ref         uuid NOT NULL,                 -- browser capability (UUID v4)
  owner_id          uuid,                          -- Phase 2 only; NULL in Phase 1
  goal              text NOT NULL,
  intent            text,
  resources         jsonb NOT NULL DEFAULT '[]'::jsonb,
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
CREATE INDEX IF NOT EXISTS idx_goals_anon_expiry ON goals (updated_at) WHERE owner_id IS NULL;

-- DORMANT: Phase 2 accounts/consent. Create now, do not wire up.
CREATE TABLE IF NOT EXISTS profiles (
  id           uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
  display_name text NOT NULL CHECK (char_length(display_name) BETWEEN 1 AND 40),
  skills       text[] NOT NULL DEFAULT '{}',
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS consents (
  owner_id       uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  purpose        text NOT NULL CHECK (purpose IN ('leaderboard','hiring_notices')),
  enabled        boolean NOT NULL DEFAULT false,
  granted_at     timestamptz,
  revoked_at     timestamptz,
  policy_version text NOT NULL DEFAULT '1',
  updated_at     timestamptz NOT NULL DEFAULT now(),
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

-- DORMANT: Phase 2 hiring profile. Never expose goal text or email.
CREATE OR REPLACE VIEW hiring_profiles WITH (security_invoker = true) AS
SELECT p.id, p.display_name, p.skills,
       count(*) FILTER (WHERE g.status = 'completed') AS completed_goals,
       max(c.granted_at) AS consented_at
FROM profiles p
JOIN consents c ON c.owner_id = p.id AND c.purpose = 'hiring_notices'
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

-- Called on every anonymous goal request to implement the sliding 6h window.
CREATE OR REPLACE FUNCTION touch_anonymous_goals(p_ref uuid)
RETURNS void LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
  UPDATE goals SET updated_at = now()
  WHERE owner_ref = p_ref AND owner_id IS NULL;
$$;
REVOKE ALL ON FUNCTION touch_anonymous_goals(uuid) FROM anon;
GRANT EXECUTE ON FUNCTION touch_anonymous_goals(uuid) TO service_role;

-- RLS. Phase 1 traffic uses the secret key (bypasses RLS), so the gateway is
-- the enforcement point. These policies are for Phase 2 JWT users.
ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE goals    ENABLE ROW LEVEL SECURITY;
ALTER TABLE consents ENABLE ROW LEVEL SECURITY;

CREATE POLICY profiles_self ON profiles FOR ALL TO authenticated
  USING (id = auth.uid()) WITH CHECK (id = auth.uid());
CREATE POLICY goals_owner_read ON goals FOR SELECT TO authenticated
  USING (owner_id = auth.uid());
CREATE POLICY goals_owner_write ON goals FOR ALL TO authenticated
  USING (owner_id = auth.uid()) WITH CHECK (owner_id = auth.uid());
CREATE POLICY consents_owner_all ON consents FOR ALL TO authenticated
  USING (owner_id = auth.uid()) WITH CHECK (owner_id = auth.uid());
GRANT SELECT ON leaderboard TO anon, authenticated;

-- Hourly purge. Enable the pg_cron extension first (Dashboard → Extensions).
-- SELECT cron.schedule('purge-anon-goals', '0 * * * *',
--   $$SELECT purge_expired_anonymous_goals(6)$$);
