ALTER TABLE runs
    ADD COLUMN project_member_id UUID REFERENCES project_members(id) ON DELETE CASCADE;

-- Backfill from existing rows if trigger JSONB contains the id. The current
-- `enqueue_run` writes only `{"source": ...}` into trigger, so this UPDATE is a
-- no-op on production data today and survives a future schema where the id
-- gets recorded in trigger JSONB too.
UPDATE runs
SET project_member_id = (trigger->>'project_member_id')::uuid
WHERE project_member_id IS NULL
  AND trigger ? 'project_member_id';

CREATE INDEX idx_runs_project_member ON runs(project_member_id, created_at DESC);
