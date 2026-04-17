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
