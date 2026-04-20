use std::sync::Arc;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::notifiers::{NotificationEvent, Notifier};
use crate::project_members::graph;
use crate::project_members::store::MembersStore;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::job::{JobRequest, RunStatus};

/// Trigger source recorded in `runs.triggered_by`.
/// Keep in sync with the CHECK constraint in migration 0009_amend_runs.sql.
pub const TRIGGERED_BY_MANUAL: &str = "manual";
pub const TRIGGERED_BY_SCHEDULED: &str = "scheduled";

/// Shared pipeline for starting a run: lookup member, snapshot, compile graph,
/// INSERT `runs` row, spawn the executor + notifier task.
///
/// Used by both the manual `POST /run` route and the scheduler dispatch closure.
pub async fn enqueue_run(
    pool: PgPool,
    members: Arc<dyn MembersStore>,
    executor: Arc<Executor>,
    notifiers: Arc<Vec<Arc<dyn Notifier>>>,
    member_id: Uuid,
    engagement_id: Uuid,
    triggered_by: &'static str,
) -> Result<Uuid, String> {
    let member = members
        .get(member_id)
        .await?
        .ok_or_else(|| format!("member {member_id} not found"))?;
    let snap_id = members.snapshot_id_for_current(member_id).await?;
    let steps = graph::compile(&member.graph_json)?;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO runs (id, engagement_id, project_id, trigger, status, snapshot_id, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', $5, $6)",
    )
    .bind(run_id)
    .bind(engagement_id)
    .bind(Some(member.project_id))
    .bind(json!({"source": triggered_by}).to_string())
    .bind(snap_id)
    .bind(triggered_by)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let req = JobRequest {
        run_id,
        engagement_id,
        project_id: Some(member.project_id),
        steps,
    };
    let exec = executor.clone();
    let pool_for_task = pool.clone();
    let notifiers_for_task = notifiers.clone();
    tokio::spawn(async move {
        let status = exec.run(req.clone()).await;
        let event = match status {
            Ok(RunStatus::Failed) => Some(NotificationEvent::RunFailed {
                run_id: req.run_id,
                project_member_id: member_id,
                step_id: "unknown".into(),
                error: "run failed; see run_steps for details".into(),
            }),
            Ok(RunStatus::Succeeded) => Some(NotificationEvent::RunSucceeded {
                run_id: req.run_id,
                project_member_id: member_id,
            }),
            _ => None,
        };
        if let Some(ev) = event {
            for n in notifiers_for_task.iter() {
                let _ = n.dispatch(&ev).await;
            }
            let _ = sqlx::query(
                "INSERT INTO notifications (project_member_id, run_id, event, payload_json)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(ev.project_member_id())
            .bind(ev.run_id())
            .bind(ev.tag())
            .bind(ev.payload())
            .execute(&pool_for_task)
            .await;
            let status_str = match ev {
                NotificationEvent::RunFailed { .. } => "failed",
                NotificationEvent::RunSucceeded { .. } => "succeeded",
            };
            let _ = sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
                .bind(req.run_id)
                .bind(status_str)
                .execute(&pool_for_task)
                .await;
        }
    });

    Ok(run_id)
}
