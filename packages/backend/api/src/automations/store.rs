use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrigger {
    pub id: Uuid,
    pub project_member_id: Uuid,
    pub connector_id: String,
    pub source_config: Value,
    pub filter: Option<Value>,
    pub poll_interval_secs: i32,
    pub cursor: Option<String>,
    pub last_polled_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[async_trait]
pub trait AutomationTriggersStore: Send + Sync {
    async fn create(&self, t: AutomationTrigger) -> Result<Uuid, String>;
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<AutomationTrigger>, String>;
    async fn mark_polled(
        &self,
        id: Uuid,
        at: DateTime<Utc>,
        cursor: Option<String>,
    ) -> Result<(), String>;
    async fn get(&self, id: Uuid) -> Result<Option<AutomationTrigger>, String>;
    async fn list_for_engagement(
        &self,
        engagement_id: Uuid,
    ) -> Result<Vec<AutomationTrigger>, String>;
    async fn update(
        &self,
        id: Uuid,
        source_config: &Value,
        filter: Option<&Value>,
        poll_interval_secs: i32,
        enabled: bool,
    ) -> Result<(), String>;
    async fn delete(&self, id: Uuid) -> Result<(), String>;
}

pub struct PgAutomationTriggersStore {
    pool: PgPool,
}

impl PgAutomationTriggersStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type Row = (
    Uuid,
    Uuid,
    String,
    Value,
    Option<Value>,
    i32,
    Option<String>,
    Option<DateTime<Utc>>,
    bool,
);

fn row_to_trigger(r: Row) -> AutomationTrigger {
    AutomationTrigger {
        id: r.0,
        project_member_id: r.1,
        connector_id: r.2,
        source_config: r.3,
        filter: r.4,
        poll_interval_secs: r.5,
        cursor: r.6,
        last_polled_at: r.7,
        enabled: r.8,
    }
}

#[async_trait]
impl AutomationTriggersStore for PgAutomationTriggersStore {
    async fn create(&self, t: AutomationTrigger) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO automation_triggers
             (id, project_member_id, connector_id, source_config, filter, poll_interval_secs, cursor, enabled)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(t.id)
        .bind(t.project_member_id)
        .bind(&t.connector_id)
        .bind(&t.source_config)
        .bind(&t.filter)
        .bind(t.poll_interval_secs)
        .bind(&t.cursor)
        .bind(t.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())
    }

    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<AutomationTrigger>, String> {
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, project_member_id, connector_id, source_config, filter,
                    poll_interval_secs, cursor, last_polled_at, enabled
             FROM automation_triggers
             WHERE enabled = TRUE
               AND (last_polled_at IS NULL
                    OR last_polled_at + (poll_interval_secs || ' seconds')::interval <= $1)",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(row_to_trigger).collect())
    }

    async fn mark_polled(
        &self,
        id: Uuid,
        at: DateTime<Utc>,
        cursor: Option<String>,
    ) -> Result<(), String> {
        sqlx::query(
            "UPDATE automation_triggers
             SET last_polled_at = $2, cursor = COALESCE($3, cursor)
             WHERE id = $1",
        )
        .bind(id)
        .bind(at)
        .bind(cursor)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    async fn get(&self, id: Uuid) -> Result<Option<AutomationTrigger>, String> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id, project_member_id, connector_id, source_config, filter,
                    poll_interval_secs, cursor, last_polled_at, enabled
             FROM automation_triggers WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(row.map(row_to_trigger))
    }

    async fn list_for_engagement(
        &self,
        engagement_id: Uuid,
    ) -> Result<Vec<AutomationTrigger>, String> {
        let rows = sqlx::query_as::<_, Row>(
            "SELECT a.id, a.project_member_id, a.connector_id, a.source_config, a.filter,
                    a.poll_interval_secs, a.cursor, a.last_polled_at, a.enabled
             FROM automation_triggers a
             JOIN project_members m ON m.id = a.project_member_id
             JOIN projects p ON p.id = m.project_id
             WHERE p.engagement_id = $1
             ORDER BY a.created_at DESC",
        )
        .bind(engagement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(row_to_trigger).collect())
    }

    async fn update(
        &self,
        id: Uuid,
        source_config: &Value,
        filter: Option<&Value>,
        poll_interval_secs: i32,
        enabled: bool,
    ) -> Result<(), String> {
        sqlx::query(
            "UPDATE automation_triggers
             SET source_config = $2, filter = $3, poll_interval_secs = $4, enabled = $5
             WHERE id = $1",
        )
        .bind(id)
        .bind(source_config)
        .bind(filter)
        .bind(poll_interval_secs)
        .bind(enabled)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    async fn delete(&self, id: Uuid) -> Result<(), String> {
        sqlx::query("DELETE FROM automation_triggers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
