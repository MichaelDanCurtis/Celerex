//! Human-in-the-loop approvals.
//!
//! A flow's `Review` node pauses the run (`runs.status = 'awaiting_review'`) and
//! persists an `awaiting_review` row in `run_steps` carrying the draft. These two
//! endpoints let a person see what's pending and approve/reject it, which resumes
//! the run via `Executor::resume_from`.

use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::project_members::{graph, run_job};
use crate::{AppState, auth::extractors::AuthClaims};
use celerex_orchestrator::job::JobRequest;
use celerex_orchestrator::templating::StepContext;

/// One pending review surfaced in the approvals inbox.
#[derive(Debug, Serialize)]
pub struct ApprovalItem {
    pub run_id: Uuid,
    pub step_id: String,
    pub project_member_id: Option<Uuid>,
    pub flow_name: Option<String>,
    pub prompt: String,
    pub draft: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ApprovalRow {
    run_id: Uuid,
    step_id: String,
    project_member_id: Option<Uuid>,
    flow_name: Option<String>,
    inputs: Value,
    created_at: DateTime<Utc>,
}

/// `GET /api/approvals` — runs paused awaiting this engagement's review.
///
/// Returns the latest pending review step per run (robust to multi-stage
/// approvals where earlier reviews are already resolved).
pub async fn list_approvals(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Vec<ApprovalItem>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, ApprovalRow>(
        "SELECT latest.run_id, latest.step_id, latest.project_member_id,
                latest.flow_name, latest.inputs, latest.created_at
         FROM (
           SELECT DISTINCT ON (rs.run_id, rs.step_id)
                  rs.run_id, rs.step_id, rs.status, rs.inputs,
                  r.created_at, r.project_member_id, pm.name AS flow_name
           FROM run_steps rs
           JOIN runs r ON r.id = rs.run_id
           LEFT JOIN project_members pm ON pm.id = r.project_member_id
           WHERE r.engagement_id = $1 AND r.status = 'awaiting_review'
           ORDER BY rs.run_id, rs.step_id, rs.completed_at DESC
         ) latest
         WHERE latest.status = 'awaiting_review'
         ORDER BY latest.created_at DESC",
    )
    .bind(claims.engagement_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items = rows
        .into_iter()
        .map(|r| ApprovalItem {
            run_id: r.run_id,
            step_id: r.step_id,
            project_member_id: r.project_member_id,
            flow_name: r.flow_name,
            prompt: r
                .inputs
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("Review and approve this draft")
                .to_string(),
            draft: r.inputs.get("draft").cloned().unwrap_or(Value::Null),
            created_at: r.created_at,
        })
        .collect();
    Ok(Json(items))
}

#[derive(Deserialize)]
pub struct ReviewDecision {
    /// "approve" opens the gate (downstream runs); anything else rejects it.
    pub decision: String,
    /// Optional edited draft. When omitted, the original draft flows downstream.
    #[serde(default)]
    pub content: Option<Value>,
}

#[derive(Serialize)]
pub struct ReviewAck {
    pub run_id: Uuid,
    pub approved: bool,
    pub resumed: bool,
}

/// `POST /api/runs/{run_id}/review` — record a human decision and resume the run.
///
/// Rebuilds the executor context from the run's persisted `run_steps` (succeeded
/// upstream outputs) plus the human decision keyed by the review step, then
/// re-runs the DAG from where it paused. Approve = gate open; reject = the review
/// step's downstream is skipped.
pub async fn submit_review(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(body): Json<ReviewDecision>,
) -> Result<(StatusCode, Json<ReviewAck>), (StatusCode, String)> {
    // 1. Authorize + verify the run is actually paused.
    let run: Option<(Uuid, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT engagement_id, project_member_id, status FROM runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (engagement_id, member_id, status) =
        run.ok_or((StatusCode::NOT_FOUND, "run not found".into()))?;
    if engagement_id != claims.engagement_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }
    if status != "awaiting_review" {
        return Err((
            StatusCode::CONFLICT,
            format!("run is '{status}', not awaiting_review"),
        ));
    }
    let member_id = member_id.ok_or((
        StatusCode::CONFLICT,
        "run has no project member to resume".into(),
    ))?;

    // 2. Find the pending review step (latest awaiting_review step for this run).
    let pending: Option<(String, Value)> = sqlx::query_as(
        "SELECT step_id, inputs FROM run_steps
         WHERE run_id = $1 AND status = 'awaiting_review'
         ORDER BY completed_at DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (review_step_id, review_inputs) =
        pending.ok_or((StatusCode::CONFLICT, "no pending review step".into()))?;

    let approved = body.decision.eq_ignore_ascii_case("approve");
    // Edited content wins; otherwise the original draft flows downstream.
    let content = body
        .content
        .unwrap_or_else(|| review_inputs.get("draft").cloned().unwrap_or(Value::Null));
    let decision = json!({ "approved": approved, "content": content });

    // 3. Recompile the member's graph into steps (same pipeline as enqueue_run).
    let member = state
        .members
        .get(member_id, engagement_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::NOT_FOUND, "member not found".into()))?;
    let mut graph_json = member.graph_json.clone();
    run_job::apply_agent_type_defaults(&mut graph_json, &member.agent_type);
    run_job::apply_doc_engagement(&mut graph_json, engagement_id);
    let steps = graph::compile(&graph_json).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // 4. Rebuild context from persisted succeeded steps (latest row per step).
    let prior: Vec<(String, String, Option<Value>)> = sqlx::query_as(
        "SELECT DISTINCT ON (step_id) step_id, status, outputs
         FROM run_steps WHERE run_id = $1
         ORDER BY step_id, completed_at DESC",
    )
    .bind(run_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut ctx = StepContext::new();
    let mut already_done: HashSet<String> = HashSet::new();
    for (step_id, st, outputs) in prior {
        if st == "succeeded" {
            if let Some(out) = outputs {
                ctx.set(&step_id, out);
            }
            already_done.insert(step_id);
        }
    }
    // Seed the human decision so the review step resolves its gate on resume.
    ctx.set(&review_step_id, decision);

    // 5. Flip the run back to running and resume the DAG in the background.
    let _ = sqlx::query("UPDATE runs SET status = 'running' WHERE id = $1")
        .bind(run_id)
        .execute(&state.pool)
        .await;

    let req = JobRequest {
        run_id,
        engagement_id,
        project_id: Some(member.project_id),
        steps,
    };
    let exec = state.executor.clone();
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let status = exec.resume_from(req, ctx, already_done).await;
        // Terminal statuses are stamped by the run store's `completed()` hook; a
        // resume that hits ANOTHER review pauses again and must be re-stamped.
        if matches!(
            status,
            Ok(celerex_orchestrator::job::RunStatus::AwaitingReview)
        ) {
            let _ = sqlx::query("UPDATE runs SET status = 'awaiting_review' WHERE id = $1")
                .bind(run_id)
                .execute(&pool)
                .await;
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ReviewAck {
            run_id,
            approved,
            resumed: true,
        }),
    ))
}
