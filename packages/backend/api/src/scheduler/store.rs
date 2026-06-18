use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
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
    // intentional: system-wide sweep, resolves engagement per-row
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Schedule>, String>;
    // intentional: system-wide sweep, resolves engagement per-row
    async fn mark_fired(
        &self,
        id: Uuid,
        fired_at: DateTime<Utc>,
        next_fire_at: DateTime<Utc>,
    ) -> Result<(), String>;
    async fn list_for_project_member(
        &self,
        project_member_id: Uuid,
        engagement_id: Uuid,
    ) -> Result<Vec<Schedule>, String>;
    async fn get(&self, id: Uuid, engagement_id: Uuid) -> Result<Option<Schedule>, String>;
    async fn update(
        &self,
        id: Uuid,
        engagement_id: Uuid,
        cron_expr: &str,
        timezone: &str,
        enabled: bool,
        next_fire_at: DateTime<Utc>,
    ) -> Result<(), String>;
    async fn delete(&self, id: Uuid, engagement_id: Uuid) -> Result<(), String>;
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

    // intentional: system-wide sweep, resolves engagement per-row
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

    // intentional: system-wide sweep, resolves engagement per-row
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

    async fn list_for_project_member(
        &self,
        project_member_id: Uuid,
        engagement_id: Uuid,
    ) -> Result<Vec<Schedule>, String> {
        let rows = sqlx::query_as::<
            _,
            (Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool),
        >(
            "SELECT id, project_member_id, cron_expr, timezone, next_fire_at, last_fired_at, enabled
             FROM schedules WHERE project_member_id = $1
               AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = $1 AND p.engagement_id = $2)
             ORDER BY next_fire_at",
        )
        .bind(project_member_id)
        .bind(engagement_id)
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

    async fn get(&self, id: Uuid, engagement_id: Uuid) -> Result<Option<Schedule>, String> {
        let row = sqlx::query_as::<
            _,
            (Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool),
        >(
            "SELECT id, project_member_id, cron_expr, timezone, next_fire_at, last_fired_at, enabled
             FROM schedules WHERE id = $1
               AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = schedules.project_member_id AND p.engagement_id = $2)",
        )
        .bind(id)
        .bind(engagement_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.map(|r| Schedule {
            id: r.0,
            project_member_id: r.1,
            cron_expr: r.2,
            timezone: r.3,
            next_fire_at: r.4,
            last_fired_at: r.5,
            enabled: r.6,
        }))
    }

    async fn update(
        &self,
        id: Uuid,
        engagement_id: Uuid,
        cron_expr: &str,
        timezone: &str,
        enabled: bool,
        next_fire_at: DateTime<Utc>,
    ) -> Result<(), String> {
        // Scope the mutation to the caller's engagement. If RETURNING yields no
        // row (cross-tenant id / no match), surface the not-found error instead
        // of silently succeeding.
        let matched: Option<Uuid> = sqlx::query_scalar(
            "UPDATE schedules SET cron_expr = $3, timezone = $4, enabled = $5, next_fire_at = $6
             WHERE id = $1
               AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = schedules.project_member_id AND p.engagement_id = $2)
             RETURNING id",
        )
        .bind(id)
        .bind(engagement_id)
        .bind(cron_expr)
        .bind(timezone)
        .bind(enabled)
        .bind(next_fire_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        matched.map(|_| ()).ok_or_else(|| "not found".to_string())
    }

    async fn delete(&self, id: Uuid, engagement_id: Uuid) -> Result<(), String> {
        let res = sqlx::query(
            "DELETE FROM schedules
             WHERE id = $1
               AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = schedules.project_member_id AND p.engagement_id = $2)",
        )
        .bind(id)
        .bind(engagement_id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        if res.rows_affected() == 0 {
            return Err("not found".to_string());
        }
        Ok(())
    }
}
