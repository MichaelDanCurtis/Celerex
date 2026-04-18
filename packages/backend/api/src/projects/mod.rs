use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, auth::AuthClaims, db};

#[derive(Deserialize)]
pub struct CreateBody { pub name: String }

pub async fn list(auth: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    match db::list_projects(&s.pool, auth.0.engagement_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create(auth: AuthClaims, State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_project(&s.pool, auth.0.engagement_id, &body.name).await {
        Ok(p) => (StatusCode::CREATED, Json(p)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
