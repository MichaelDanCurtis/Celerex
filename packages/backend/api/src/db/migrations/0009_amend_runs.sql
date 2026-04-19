ALTER TABLE runs
    ADD COLUMN snapshot_id UUID REFERENCES project_member_snapshots(id),
    ADD COLUMN triggered_by TEXT NOT NULL DEFAULT 'scheduled';

ALTER TABLE runs
    ADD CONSTRAINT runs_triggered_by_chk
    CHECK (triggered_by IN ('scheduled', 'manual', 'webhook'));
