-- Celerex Design handoff: the "Design Library". An ingested design template is
-- the unzipped CDB bundle (manifest + renderable HTML/MJML + slot schema). In
-- `live` mode the template is re-rendered every flow run by the Render node
-- (filling {{slot:name}} with fresh data). See
-- docs/plans/2026-06-12-design-handoff-contract.md.
CREATE TABLE design_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    kind TEXT NOT NULL,          -- newsletter | slides | web_prototype | image | video | document
    mode TEXT NOT NULL,          -- static | live
    render_engine TEXT NOT NULL DEFAULT 'html',  -- html | mjml
    template_html TEXT NOT NULL,
    tokens_css TEXT NOT NULL DEFAULT '',
    slots JSONB NOT NULL DEFAULT '[]'::jsonb,
    manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
    source_project_ref TEXT,     -- the desktop app's own project id, for re-publish
    version INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_design_templates_engagement ON design_templates(engagement_id, created_at DESC);

-- A re-publish from the same desktop project updates the same row (bumps version)
-- rather than creating a duplicate.
CREATE UNIQUE INDEX design_templates_source_uq
    ON design_templates(engagement_id, source_project_ref)
    WHERE source_project_ref IS NOT NULL;

-- Bundle assets (images/fonts) stored as blobs for v0.1 (object store later).
CREATE TABLE design_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES design_templates(id) ON DELETE CASCADE,
    path TEXT NOT NULL,          -- bundle-relative path, e.g. assets/hero.png
    content_type TEXT NOT NULL,
    bytes BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_design_assets_template ON design_assets(template_id);
