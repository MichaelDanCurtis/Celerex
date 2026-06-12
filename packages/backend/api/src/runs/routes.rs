use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::runs::{PgRunStore, RunSummary};
use crate::{AppState, auth::extractors::AuthClaims};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Deserialize)]
pub struct ListRunsQuery {
    pub project_member_id: Uuid,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /api/runs?project_member_id=<uuid>&limit=<n>` — return the most
/// recent runs (newest first) for the given project member.
///
/// Auth: resolves the engagement that owns the member and 403s on mismatch.
pub async fn list(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Query(q): Query<ListRunsQuery>,
) -> Result<Json<Vec<RunSummary>>, (StatusCode, String)> {
    let engagement_id = state
        .members
        .engagement_id_for(q.project_member_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;
    if engagement_id != claims.engagement_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }
    let limit = q
        .limit
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let store = PgRunStore::new(state.pool.clone());
    let rows = store
        .list_for_member(q.project_member_id, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(rows))
}

/// One row in the engagement-wide activity feed.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RunActivityRow {
    pub id: Uuid,
    pub project_member_id: Option<Uuid>,
    pub flow_name: Option<String>,
    pub status: String,
    pub triggered_by: String,
    pub created_at: DateTime<Utc>,
}

/// `GET /api/runs/recent?limit=<n>` — the engagement's most recent runs across
/// ALL flows (newest first), joined with the flow name. Powers the Activity
/// dashboard; auth scopes to the caller's engagement.
pub async fn recent(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Query(q): Query<RecentQuery>,
) -> Result<Json<Vec<RunActivityRow>>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let rows = sqlx::query_as::<_, RunActivityRow>(
        "SELECT r.id, r.project_member_id, pm.name AS flow_name,
                r.status, r.triggered_by, r.created_at
         FROM runs r
         LEFT JOIN project_members pm ON pm.id = r.project_member_id
         WHERE r.engagement_id = $1
         ORDER BY r.created_at DESC
         LIMIT $2",
    )
    .bind(claims.engagement_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct RecentQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One persisted step record for a (possibly historical) run.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RunStepRow {
    pub step_id: String,
    pub status: String,
    pub inputs: serde_json::Value,
    pub outputs: Option<serde_json::Value>,
    pub error: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// `GET /api/runs/{run_id}/steps` — the durable per-step history of a run.
///
/// This is what makes the Monitor useful for PAST runs: SSE only covers the
/// live stream, while this reads the `run_steps` rows the executor persisted.
/// Auth: 403 unless the run belongs to the caller's engagement.
pub async fn steps(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Vec<RunStepRow>>, (StatusCode, String)> {
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT engagement_id FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match owner {
        None => return Err((StatusCode::NOT_FOUND, "run not found".into())),
        Some(eng) if eng != claims.engagement_id => {
            return Err((StatusCode::FORBIDDEN, "forbidden".into()));
        }
        Some(_) => {}
    }
    let rows = sqlx::query_as::<_, RunStepRow>(
        "SELECT step_id, status, inputs, outputs, error, completed_at
         FROM run_steps WHERE run_id = $1
         ORDER BY completed_at ASC NULLS LAST",
    )
    .bind(run_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}
