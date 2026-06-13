-- Celerex Design handoff, piece C: how the desktop app gets a credential, and
-- where an ingested live template's delivery flow is recorded.
-- See docs/plans/2026-06-12-design-handoff-contract.md.

-- Device-pairing handshake (OAuth device-flow shape). The desktop app, which
-- holds no credential yet, calls /pair/start to get a `device_code` (secret, it
-- polls with this) and a short `user_code` the human types into Celerex. An
-- authenticated user calls /pair/approve with the user_code to bind it to their
-- engagement and mint the scoped `design.publish` token, which the desktop then
-- collects via /pair/poll.
CREATE TABLE design_pairings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_code TEXT NOT NULL UNIQUE,   -- secret; desktop polls with this
    user_code TEXT NOT NULL UNIQUE,     -- short human code, typed into Celerex
    status TEXT NOT NULL DEFAULT 'pending',  -- pending | approved
    engagement_id UUID REFERENCES engagements(id) ON DELETE CASCADE,  -- null until approved
    token TEXT,                         -- minted scoped token, set on approval
    label TEXT,                         -- optional device label the desktop sends
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ
);

CREATE INDEX idx_design_pairings_expires ON design_pairings(expires_at);

-- A live template's auto-scaffolded delivery flow (project_member). Lets a
-- re-publish refresh the same flow's Render node instead of spawning a new one.
ALTER TABLE design_templates
    ADD COLUMN flow_member_id UUID REFERENCES project_members(id) ON DELETE SET NULL;
