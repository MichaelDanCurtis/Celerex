use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DesignArtifactRow {
    pub artifact_id: String,
    pub title: String,
    pub template_html: String,
    pub rendered_html: String,
}

/// `la-<slug>-<12 hex>`. Port of the daemon's `generateLiveArtifactId` shape
/// (prefix `la`, slug from title, hex suffix). Uses a random suffix for
/// uniqueness within (engagement, project).
pub fn generate_artifact_id(title: &str) -> String {
    let slug = slugify(title);
    let suffix: String = {
        let mut bytes = [0u8; 6];
        // ring is already a dependency (see providers::store); reuse its RNG.
        use ring::rand::{SecureRandom, SystemRandom};
        SystemRandom::new().fill(&mut bytes).expect("rng");
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    };
    format!("la-{slug}-{suffix}")
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in input.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "live-artifact".to_string()
    } else {
        trimmed
    }
}

pub async fn insert_artifact(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: &str,
    artifact_id: &str,
    title: &str,
    template_html: &str,
    rendered_html: &str,
    created_by_run_id: Uuid,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO design_artifacts
            (engagement_id, project_id, artifact_id, title, template_html, rendered_html, created_by_run_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (engagement_id, project_id, artifact_id) DO UPDATE SET
            title = EXCLUDED.title,
            template_html = EXCLUDED.template_html,
            rendered_html = EXCLUDED.rendered_html,
            created_by_run_id = EXCLUDED.created_by_run_id",
    )
    .bind(engagement_id)
    .bind(project_id)
    .bind(artifact_id)
    .bind(title)
    .bind(template_html)
    .bind(rendered_html)
    .bind(created_by_run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_artifact(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: &str,
    artifact_id: &str,
) -> Result<Option<DesignArtifactRow>, String> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT artifact_id, title, template_html, rendered_html
         FROM design_artifacts
         WHERE engagement_id = $1 AND project_id = $2 AND artifact_id = $3",
    )
    .bind(engagement_id)
    .bind(project_id)
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(
        |(artifact_id, title, template_html, rendered_html)| DesignArtifactRow {
            artifact_id,
            title,
            template_html,
            rendered_html,
        },
    ))
}

/// Sibling to `get_artifact`, scoped by `(project_id, artifact_id)` ONLY —
/// deliberately WITHOUT `engagement_id`. Exists solely for
/// `artifact_routes::get_artifact_preview`'s anonymous same-origin-iframe
/// path, which by construction has no claims to scope by (see that
/// handler's doc comment for the full security rationale). Do not call this
/// from any Bearer-authenticated call site — use `get_artifact` there.
pub async fn get_artifact_by_project(
    pool: &PgPool,
    project_id: &str,
    artifact_id: &str,
) -> Result<Option<DesignArtifactRow>, String> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT artifact_id, title, template_html, rendered_html
         FROM design_artifacts
         WHERE project_id = $1 AND artifact_id = $2",
    )
    .bind(project_id)
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(
        |(artifact_id, title, template_html, rendered_html)| DesignArtifactRow {
            artifact_id,
            title,
            template_html,
            rendered_html,
        },
    ))
}

/// Row shape for the LIST route (`GET /api/live-artifacts`) — deliberately
/// narrower than `DesignArtifactRow`: a workspace-tab listing has no need for
/// the (potentially large) `template_html`/`rendered_html` blobs, only enough
/// to build the SPA's `LiveArtifactSummary` shape (`@celerex/contracts`'s
/// `api/live-artifacts.ts`). `slug` is derived here (via the same `slugify`
/// used to build the artifact id itself) so `artifact_routes.rs` stays purely
/// about HTTP/JSON shaping.
#[derive(Debug, Clone)]
pub struct DesignArtifactSummaryRow {
    pub artifact_id: String,
    pub title: String,
    pub slug: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `GET /api/live-artifacts?projectId=...` — every artifact for one
/// `(engagement_id, project_id)`, most-recently-created first. Engagement-
/// scoped like `get_artifact`: this path stays Bearer-gated
/// (`artifact_routes::list_live_artifacts` keeps `AuthClaims` mandatory), so
/// there is no anonymous-access reason to loosen the scoping the way
/// `get_artifact_by_project` deliberately does for `/preview`.
pub async fn list_artifacts(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: &str,
) -> Result<Vec<DesignArtifactSummaryRow>, String> {
    let rows: Vec<(String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT artifact_id, title, created_at
         FROM design_artifacts
         WHERE engagement_id = $1 AND project_id = $2
         ORDER BY created_at DESC",
    )
    .bind(engagement_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(artifact_id, title, created_at)| {
            let slug = slugify(&title);
            DesignArtifactSummaryRow {
                artifact_id,
                title,
                slug,
                created_at,
            }
        })
        .collect())
}
