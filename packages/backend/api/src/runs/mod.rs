pub mod bus;
pub mod sse;

use std::sync::Arc;

use celerex_orchestrator::executor::RunStore;
use celerex_orchestrator::job::{RunStatus, StepRecord};
use sqlx::PgPool;
use uuid::Uuid;

use crate::runs::bus::{RunBus, RunEvent};

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
}

impl RunStore for PgRunStore {
    fn record(&self, rec: StepRecord) {
        let pool = self.pool.clone();
        let bus = self.bus.clone();
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
            bus.publish(rec.run_id, ev);
        });
    }

    fn records_for(&self, _run_id: Uuid) -> Vec<StepRecord> {
        Vec::new()
    }
}
