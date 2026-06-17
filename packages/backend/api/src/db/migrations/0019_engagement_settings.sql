CREATE TABLE engagement_settings (
    engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE,
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
