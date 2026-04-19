CREATE TABLE project_member_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    graph_json JSONB NOT NULL,
    provider_config_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_member_id, version)
);
