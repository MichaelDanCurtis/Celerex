-- Living Documents v1: per-slot agent-fill primitive. Each row binds a
-- design_templates slot to a server-held instruction (the "AI spot") plus the
-- last filled value. A Fill runs the engagement's assistant provider one-shot
-- and stores the result in last_value; Render substitutes it into the
-- template's {{slot:name}} markers. Scoped to the template's engagement via the
-- FK (ownership is verified in the handler before any read/write).
CREATE TABLE template_slot_fills (
    template_id UUID NOT NULL REFERENCES design_templates(id) ON DELETE CASCADE,
    slot_name TEXT NOT NULL,
    instruction TEXT NOT NULL,
    last_value TEXT,
    provider_id TEXT,
    model TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (template_id, slot_name)
);
