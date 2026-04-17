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
