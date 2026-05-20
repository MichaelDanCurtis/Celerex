use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
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
