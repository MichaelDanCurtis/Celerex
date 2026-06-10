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

/// Case-insensitive lookup by name. Returns `Ok(None)` if no match.
pub async fn find_engagement_by_name(
    pool: &PgPool,
    name: &str,
) -> sqlx::Result<Option<Engagement>> {
    sqlx::query_as::<_, Engagement>(
        "SELECT id, name, created_at FROM engagements WHERE LOWER(name) = LOWER($1) LIMIT 1"
    ).bind(name).fetch_optional(pool).await
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

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct DocumentSummary {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub block_count: i64,
}

pub async fn list_documents_for_engagement(
    pool: &PgPool,
    eng: Uuid,
) -> sqlx::Result<Vec<DocumentSummary>> {
    sqlx::query_as::<_, DocumentSummary>(
        "SELECT d.id, d.engagement_id, d.project_id, d.title, d.created_at,
                COALESCE((SELECT COUNT(*) FROM doc_blocks b WHERE b.document_id = d.id), 0) AS block_count
         FROM documents d
         WHERE d.engagement_id = $1
         ORDER BY d.created_at DESC"
    ).bind(eng).fetch_all(pool).await
}

pub async fn get_document(pool: &PgPool, doc: Uuid) -> sqlx::Result<Option<Document>> {
    sqlx::query_as::<_, Document>(
        "SELECT id, engagement_id, project_id, title, created_at FROM documents WHERE id = $1"
    ).bind(doc).fetch_optional(pool).await
}

pub async fn delete_document(pool: &PgPool, doc: Uuid) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(doc).execute(pool).await?;
    Ok(r.rows_affected())
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct DocBlock {
    pub id: Uuid,
    pub document_id: Uuid,
    pub position: i32,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub async fn list_blocks(pool: &PgPool, doc: Uuid) -> sqlx::Result<Vec<DocBlock>> {
    sqlx::query_as::<_, DocBlock>(
        "SELECT id, document_id, position, payload, created_at
         FROM doc_blocks
         WHERE document_id = $1
         ORDER BY position ASC"
    ).bind(doc).fetch_all(pool).await
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

// ---- Users (real auth) ---------------------------------------------------

/// Find an engagement by name (case-insensitive) or create it. Used by signup
/// so a new account also provisions its workspace.
pub async fn find_or_create_engagement(pool: &PgPool, name: &str) -> sqlx::Result<Engagement> {
    if let Some(e) = find_engagement_by_name(pool, name).await? {
        return Ok(e);
    }
    create_engagement(pool, name).await
}

/// Create a user. Returns the new user id. Surfaces the unique-email violation
/// to the caller (mapped to 409 at the route layer).
pub async fn create_user(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    engagement_id: Uuid,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO users (email, password_hash, engagement_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(password_hash)
    .bind(engagement_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Row needed to authenticate a login: the stored hash + the user's engagement
/// (id + name) for the JWT and the response. Case-insensitive email match.
pub struct UserLogin {
    pub password_hash: String,
    pub engagement_id: Uuid,
    pub engagement_name: String,
}

pub async fn find_user_for_login(pool: &PgPool, email: &str) -> sqlx::Result<Option<UserLogin>> {
    let row: Option<(String, Uuid, String)> = sqlx::query_as(
        "SELECT u.password_hash, u.engagement_id, e.name
         FROM users u JOIN engagements e ON e.id = u.engagement_id
         WHERE LOWER(u.email) = LOWER($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(password_hash, engagement_id, engagement_name)| UserLogin {
        password_hash,
        engagement_id,
        engagement_name,
    }))
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
