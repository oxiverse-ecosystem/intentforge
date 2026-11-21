-- Phase 2 — leaderboard must be readable by anonymous visitors.
-- security_invoker=true made RLS on goals/profiles/consents apply to the anon
-- caller, which has no SELECT grants on those tables, so the view was empty.
-- The view's own WHERE clause already restricts rows to claimed + opted-in,
-- so definer rights expose only that filtered set.
ALTER VIEW leaderboard SET (security_invoker = false);
GRANT SELECT ON leaderboard TO anon, authenticated;
