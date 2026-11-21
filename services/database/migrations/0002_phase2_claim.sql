-- Phase 2 — allow claimed goals to drop the anonymous capability.
-- Phase 1 created goals.owner_ref NOT NULL. The claim RPC sets owner_ref = NULL
-- once owner_id is populated, so relax the column and add an explicit invariant.
ALTER TABLE goals ALTER COLUMN owner_ref DROP NOT NULL;

ALTER TABLE goals DROP CONSTRAINT IF EXISTS goals_exactly_one_owner;
ALTER TABLE goals ADD CONSTRAINT goals_exactly_one_owner
  CHECK (num_nonnulls(owner_ref, owner_id) = 1);

-- Claimed rows are found by owner_id; anonymous/service rows by owner_ref.
CREATE INDEX IF NOT EXISTS idx_goals_owner_id ON goals (owner_id) WHERE owner_id IS NOT NULL;
