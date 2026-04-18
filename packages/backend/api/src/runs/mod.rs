use celerex_orchestrator::executor::RunStore;
use celerex_orchestrator::job::StepRecord;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgRunStore { pool: PgPool }

impl PgRunStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

impl RunStore for PgRunStore {
    fn record(&self, rec: StepRecord) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let status = format!("{:?}", rec.status).to_lowercase();
            let _ = sqlx::query(
                "INSERT INTO run_steps (run_id, step_id, status, inputs, outputs, error, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())"
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
}
