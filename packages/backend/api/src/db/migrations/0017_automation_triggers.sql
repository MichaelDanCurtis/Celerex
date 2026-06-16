CREATE TABLE automation_triggers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    connector_id TEXT NOT NULL,
    source_config JSONB NOT NULL,
    filter JSONB,
    poll_interval_secs INT NOT NULL DEFAULT 300,
    cursor TEXT,
    last_polled_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_automation_triggers_due ON automation_triggers(enabled, last_polled_at);

ALTER TABLE runs DROP CONSTRAINT runs_triggered_by_chk;
ALTER TABLE runs ADD CONSTRAINT runs_triggered_by_chk
    CHECK (triggered_by IN ('scheduled', 'manual', 'webhook', 'connector'));
