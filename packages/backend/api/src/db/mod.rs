use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct Engagement {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn init_pool(database_url: &str) -> sqlx::Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(8).connect(database_url).await?;
    sqlx::migrate!("./src/db/migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn create_engagement(pool: &PgPool, name: &str) -> sqlx::Result<Engagement> {
    sqlx::query_as::<_, Engagement>(
        "INSERT INTO engagements (name) VALUES ($1) RETURNING id, name, created_at"
    ).bind(name).fetch_one(pool).await
}

pub async fn list_engagements(pool: &PgPool) -> sqlx::Result<Vec<Engagement>> {
    sqlx::query_as::<_, Engagement>(
        "SELECT id, name, created_at FROM engagements ORDER BY created_at DESC"
    ).fetch_all(pool).await
}

#[derive(sqlx::FromRow, serde::Serialize, Clone)]
pub struct Project {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_project(pool: &PgPool, eng: Uuid, name: &str) -> sqlx::Result<Project> {
    sqlx::query_as::<_, Project>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, $2)
         RETURNING id, engagement_id, name, created_at"
    ).bind(eng).bind(name).fetch_one(pool).await
}

pub async fn list_projects(pool: &PgPool, eng: Uuid) -> sqlx::Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        "SELECT id, engagement_id, name, created_at FROM projects
         WHERE engagement_id = $1 ORDER BY created_at DESC"
    ).bind(eng).fetch_all(pool).await
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct Document {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_document(
    pool: &PgPool,
    eng: Uuid,
    proj: Option<Uuid>,
    title: &str,
) -> sqlx::Result<Document> {
    sqlx::query_as::<_, Document>(
        "INSERT INTO documents (engagement_id, project_id, title) VALUES ($1, $2, $3)
         RETURNING id, engagement_id, project_id, title, created_at"
    ).bind(eng).bind(proj).bind(title).fetch_one(pool).await
}

pub async fn append_block(
    pool: &PgPool,
    doc: Uuid,
    payload: &serde_json::Value,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO doc_blocks (document_id, position, payload)
         VALUES ($1, COALESCE((SELECT MAX(position) + 1 FROM doc_blocks WHERE document_id = $1), 0), $2)
         RETURNING id"
    ).bind(doc).bind(payload).fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn replace_block(
    pool: &PgPool,
    block_id: Uuid,
    payload: &serde_json::Value,
) -> sqlx::Result<u64> {
    let r = sqlx::query("UPDATE doc_blocks SET payload = $2 WHERE id = $1")
        .bind(block_id).bind(payload).execute(pool).await?;
    Ok(r.rows_affected())
}

pub async fn install_plugin(
    pool: &PgPool,
    eng: Uuid,
    plugin_id: &str,
    manifest: &serde_json::Value,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, $2, $3) RETURNING id"
    ).bind(eng).bind(plugin_id).bind(manifest).fetch_one(pool).await?;
    Ok(row.0)
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct PluginInstall {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub plugin_id: String,
    pub manifest: serde_json::Value,
    pub installed_at: DateTime<Utc>,
}

pub async fn list_plugins(pool: &PgPool, eng: Uuid) -> sqlx::Result<Vec<PluginInstall>> {
    sqlx::query_as::<_, PluginInstall>(
        "SELECT id, engagement_id, plugin_id, manifest, installed_at FROM plugin_installs
         WHERE engagement_id = $1 ORDER BY installed_at DESC"
    ).bind(eng).fetch_all(pool).await
}

pub async fn upsert_credentials(
    pool: &PgPool,
    install: Uuid,
    direction: &str,
    ct: &[u8],
    nonce: &[u8],
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO plugin_credentials (install_id, direction, encrypted_payload, nonce)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (install_id) DO UPDATE SET
            direction = EXCLUDED.direction,
            encrypted_payload = EXCLUDED.encrypted_payload,
            nonce = EXCLUDED.nonce"
    ).bind(install).bind(direction).bind(ct).bind(nonce).execute(pool).await?;
    Ok(())
}
