use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: Uuid,
    pub project_member_id: Uuid,
    pub cron_expr: String,
    pub timezone: String,
    pub next_fire_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[async_trait]
pub trait SchedulesStore: Send + Sync {
    async fn create(&self, s: Schedule) -> Result<Uuid, String>;
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Schedule>, String>;
    async fn mark_fired(
        &self,
        id: Uuid,
        fired_at: DateTime<Utc>,
        next_fire_at: DateTime<Utc>,
    ) -> Result<(), String>;
}

pub struct PgSchedulesStore {
    pool: PgPool,
}

impl PgSchedulesStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SchedulesStore for PgSchedulesStore {
    async fn create(&self, s: Schedule) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO schedules (id, project_member_id, cron_expr, timezone, next_fire_at, enabled)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(s.id)
        .bind(s.project_member_id)
        .bind(&s.cron_expr)
        .bind(&s.timezone)
        .bind(s.next_fire_at)
        .bind(s.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())
    }

    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Schedule>, String> {
        let rows = sqlx::query_as::<
            _,
            (Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool),
        >(
            "SELECT id, project_member_id, cron_expr, timezone, next_fire_at, last_fired_at, enabled
             FROM schedules WHERE enabled = TRUE AND next_fire_at <= $1",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|r| Schedule {
                id: r.0,
                project_member_id: r.1,
                cron_expr: r.2,
                timezone: r.3,
                next_fire_at: r.4,
                last_fired_at: r.5,
                enabled: r.6,
            })
            .collect())
    }

    async fn mark_fired(
        &self,
        id: Uuid,
        fired_at: DateTime<Utc>,
        next_fire_at: DateTime<Utc>,
    ) -> Result<(), String> {
        sqlx::query("UPDATE schedules SET last_fired_at = $2, next_fire_at = $3 WHERE id = $1")
            .bind(id)
            .bind(fired_at)
            .bind(next_fire_at)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
