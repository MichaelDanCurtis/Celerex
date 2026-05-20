CREATE TABLE engagement_provider_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,  -- "anthropic" | "openai" | "kimi" | "gemini"
    encrypted_key BYTEA NOT NULL,
    nonce BYTEA NOT NULL,  -- AEAD nonce
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (engagement_id, provider_id)
);

CREATE INDEX idx_engagement_provider_keys_engagement
    ON engagement_provider_keys (engagement_id);
