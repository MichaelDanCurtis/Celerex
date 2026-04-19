use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::project_members::graph;
use crate::{AppState, auth::extractors::AuthClaims};
use celerex_orchestrator::job::JobRequest;

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
    let member = match s.members.get(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e})),
            )
                .into_response();
        }
    };
    let snap_id = match s.members.snapshot_id_for_current(id).await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e})),
            )
                .into_response();
        }
    };

    let steps = match graph::compile(&member.graph_json) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
        }
    };

    let run_id = Uuid::new_v4();
    // NOTE: `snapshot_id` + `triggered_by` columns are added in Task 7.
    // Until then, this INSERT will fail at runtime; the covering integration
    // test is #[ignore]'d until Task 7 amends the runs schema.
    if let Err(e) = sqlx::query(
        "INSERT INTO runs (id, engagement_id, project_id, trigger, status, snapshot_id, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', $5, 'manual')",
    )
    .bind(run_id)
    .bind(a.0.engagement_id)
    .bind(Some(member.project_id))
    .bind(json!({"source":"manual"}).to_string())
    .bind(snap_id)
    .execute(&s.pool)
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    let req = JobRequest {
        run_id,
        engagement_id: a.0.engagement_id,
        project_id: Some(member.project_id),
        steps,
    };
    let exec = s.executor.clone();
    tokio::spawn(async move {
        let _ = exec.run(req).await;
    });

    (StatusCode::ACCEPTED, Json(json!({"run_id": run_id}))).into_response()
}
