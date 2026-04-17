use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
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
