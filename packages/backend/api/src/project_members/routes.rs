use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::project_members::graph;
use crate::project_members::run_job::{self, TRIGGERED_BY_MANUAL};
use crate::{AppState, auth::extractors::AuthClaims};

#[derive(Deserialize)]
pub struct CreateBody {
    pub project_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub graph_json: Value,
    #[serde(default)]
    pub provider_config_json: Value,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub graph_json: Value,
    #[serde(default)]
    pub provider_config_json: Value,
}

pub async fn create(
    _a: AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<CreateBody>,
) -> impl IntoResponse {
    if let Err(e) = graph::validate(&b.graph_json) {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }
    match s
        .members
        .create(
            b.project_id,
            &b.name,
            &b.description,
            &b.graph_json,
            &b.provider_config_json,
        )
        .await
    {
        Ok(m) => (StatusCode::CREATED, Json(m)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn update(
    _a: AuthClaims,
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<UpdateBody>,
) -> impl IntoResponse {
    if let Err(e) = graph::validate(&b.graph_json) {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }
    match s
        .members
        .update(
            id,
            &b.name,
            &b.description,
            &b.graph_json,
            &b.provider_config_json,
        )
        .await
    {
        Ok(m) => (StatusCode::OK, Json(m)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn get(
    _a: AuthClaims,
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match s.members.get(id).await {
        Ok(Some(m)) => (StatusCode::OK, Json(m)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn list(
    _a: AuthClaims,
    State(s): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> impl IntoResponse {
    match s.members.list_for_project(project_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn run_manual(
    a: AuthClaims,
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    // Pre-validate the graph before the helper tries to compile it, so callers
    // get 400 on a malformed graph instead of 500.
    match s.members.get(id).await {
        Ok(Some(m)) => {
            if let Err(e) = graph::compile(&m.graph_json) {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
            }
        }
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e})),
            )
                .into_response();
        }
    }

    match run_job::enqueue_run(
        s.pool.clone(),
        s.members.clone(),
        s.executor.clone(),
        s.notifiers.clone(),
        id,
        a.0.engagement_id,
        TRIGGERED_BY_MANUAL,
    )
    .await
    {
        Ok(run_id) => (StatusCode::ACCEPTED, Json(json!({"run_id": run_id}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
pub struct FleetRow {
    pub id: Uuid,
    pub name: String,
    pub project_id: Uuid,
    pub last_status: Option<String>,
    pub next_fire_at: Option<chrono::DateTime<chrono::Utc>>,
    pub running: bool,
}

pub async fn fleet(a: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    let rows: Result<
        Vec<(
            Uuid,
            String,
            Uuid,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<String>,
        )>,
        _,
    > = sqlx::query_as(
        "SELECT m.id, m.name, m.project_id,
                (SELECT status FROM runs WHERE runs.project_id = m.project_id ORDER BY created_at DESC LIMIT 1) AS last_status,
                (SELECT next_fire_at FROM schedules WHERE schedules.project_member_id = m.id ORDER BY next_fire_at LIMIT 1) AS next_fire_at,
                (SELECT status FROM runs WHERE runs.project_id = m.project_id ORDER BY created_at DESC LIMIT 1) AS running_marker
         FROM project_members m
         JOIN projects p ON p.id = m.project_id
         WHERE p.engagement_id = $1",
    )
    .bind(a.0.engagement_id)
    .fetch_all(&s.pool)
    .await;
    match rows {
        Ok(rows) => {
            let out: Vec<FleetRow> = rows
                .into_iter()
                .map(|r| FleetRow {
                    id: r.0,
                    name: r.1,
                    project_id: r.2,
                    last_status: r.3,
                    next_fire_at: r.4,
                    running: matches!(r.5.as_deref(), Some("running")),
                })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
