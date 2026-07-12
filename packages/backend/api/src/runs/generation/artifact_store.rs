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
