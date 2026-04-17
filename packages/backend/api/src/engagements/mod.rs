use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, db};

#[derive(Deserialize)]
pub struct CreateBody { pub name: String }

pub async fn list(State(s): State<AppState>) -> impl IntoResponse {
    match db::list_engagements(&s.pool).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create(State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_engagement(&s.pool, &body.name).await {
        Ok(row) => (StatusCode::CREATED, Json(row)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
