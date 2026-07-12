-- Server-side design-generation artifacts. One row per validated artifact a
-- design-chat run produced. Keyed by (engagement_id, project_id, artifact_id)
-- for engagement-isolated serving via GET /api/live-artifacts/{artifact_id}.
CREATE TABLE design_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    title TEXT NOT NULL,
    template_html TEXT NOT NULL,
    rendered_html TEXT NOT NULL,
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (engagement_id, project_id, artifact_id)
);

CREATE INDEX idx_design_artifacts_lookup
    ON design_artifacts (engagement_id, project_id, artifact_id);
