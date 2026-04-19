CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivery_status TEXT NOT NULL DEFAULT 'sent'
);

CREATE INDEX notifications_member_sent_idx
    ON notifications (project_member_id, sent_at DESC);
