use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, auth::AuthClaims, db};

#[derive(Deserialize)]
pub struct CreateBody {
    pub title: String,
}

pub async fn create(
    auth: AuthClaims,
    State(s): State<AppState>,
    Json(body): Json<CreateBody>,
) -> impl IntoResponse {
    match db::create_document(&s.pool, auth.0.engagement_id, auth.0.project_id, &body.title).await {
        Ok(d) => (StatusCode::CREATED, Json(d)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn append_block(
    _auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match db::append_block(&s.pool, doc_id, &payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
