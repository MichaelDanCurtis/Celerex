pub mod bus;
pub mod routes;
pub mod sse;

use std::sync::Arc;

use celerex_orchestrator::executor::RunStore;
use celerex_orchestrator::job::{RunStatus, StepRecord};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::runs::bus::{RunBus, RunEvent};

/// One row in the `/api/runs` list response.
///
/// Shape kept narrow on purpose — the monitor view fetches the full per-run
/// detail via SSE; this struct only carries enough to render the history table.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RunSummary {
    pub id: Uuid,
    pub project_member_id: Option<Uuid>,
    pub engagement_id: Uuid,
    pub status: String,
    pub triggered_by: String,
    pub created_at: DateTime<Utc>,
}

pub struct PgRunStore {
    pool: PgPool,
    bus: Arc<RunBus>,
}

impl PgRunStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, bus: Arc::new(RunBus::new()) }
    }

    pub fn with_bus(pool: PgPool, bus: Arc<RunBus>) -> Self {
        Self { pool, bus }
    }

    pub fn bus(&self) -> Arc<RunBus> {
        self.bus.clone()
    }

    /// Return the most recent `limit` runs for a given project member, newest
    /// first. Uses the `idx_runs_project_member` index defined in migration
    /// 0012.
    pub async fn list_for_member(
        &self,
        project_member_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RunSummary>, String> {
        sqlx::query_as::<_, RunSummary>(
            "SELECT id, project_member_id, engagement_id, status, triggered_by, created_at
             FROM runs WHERE project_member_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(project_member_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())
    }
}

impl RunStore for PgRunStore {
    fn record(&self, rec: StepRecord) {
        // Publish to the live SSE bus SYNCHRONOUSLY (before spawning the DB
        // write). This guarantees event ordering on the bus matches execution
        // order — in particular `RunCompleted`, which the executor publishes
        // via `completed()` immediately after the final `record()`, can never
        // race ahead of the last step event and prematurely close the stream.
        let ev = match rec.status {
            RunStatus::Succeeded => RunEvent::RunStepSucceeded {
                run_id: rec.run_id,
                step_id: rec.step_id.clone(),
                outputs: rec.outputs.clone().unwrap_or(serde_json::json!({})),
            },
            RunStatus::Failed => RunEvent::RunStepFailed {
                run_id: rec.run_id,
                step_id: rec.step_id.clone(),
                error: rec.error.clone().unwrap_or_default(),
            },
            RunStatus::Running | RunStatus::Pending => RunEvent::RunStepStarted {
                run_id: rec.run_id,
                step_id: rec.step_id.clone(),
            },
        };
        self.bus.publish(rec.run_id, ev);

        // Persist asynchronously — the durable history doesn't gate the live UI.
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let status = format!("{:?}", rec.status).to_lowercase();
            let _ = sqlx::query(
                "INSERT INTO run_steps (run_id, step_id, status, inputs, outputs, error, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())",
            )
            .bind(rec.run_id)
            .bind(&rec.step_id)
            .bind(&status)
            .bind(&rec.inputs)
            .bind(&rec.outputs)
            .bind(&rec.error)
            .execute(&pool)
            .await;
        });
    }

    fn records_for(&self, _run_id: Uuid) -> Vec<StepRecord> {
        Vec::new()
    }

    /// Emit a `RunStepStarted` so the live canvas can light the node "running"
    /// the moment dispatch begins (the per-step `record()` only fires on a
    /// terminal status).
    fn started(&self, run_id: Uuid, step_id: &str) {
        self.bus.publish(
            run_id,
            RunEvent::RunStepStarted {
                run_id,
                step_id: step_id.to_string(),
            },
        );
    }

    /// Emit the terminal `RunCompleted` (which the frontend uses as its
    /// end-of-stream signal) and durably stamp the final run status.
    fn completed(&self, run_id: Uuid, status: RunStatus) {
        let status_str = match status {
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
            RunStatus::Running | RunStatus::Pending => "running",
        };
        self.bus.publish(
            run_id,
            RunEvent::RunCompleted {
                run_id,
                status: status_str.to_string(),
            },
        );
        let pool = self.pool.clone();
        let status_owned = status_str.to_string();
        tokio::spawn(async move {
            let _ = sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
                .bind(run_id)
                .bind(&status_owned)
                .execute(&pool)
                .await;
        });
    }
}
