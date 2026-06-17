CREATE TABLE copilot_logins (
    engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE,
    device_code TEXT NOT NULL,
    user_code TEXT NOT NULL,
    verification_uri TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
