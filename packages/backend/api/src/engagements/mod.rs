use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, db};
use crate::auth::extractors::AuthClaims;

#[derive(Deserialize)]
pub struct CreateBody { pub name: String }

pub async fn list(a: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    match db::get_engagement(&s.pool, a.0.engagement_id).await {
        Ok(Some(e)) => (StatusCode::OK, Json(vec![e])).into_response(),
        Ok(None) => (StatusCode::OK, Json(Vec::<db::Engagement>::new())).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create(_a: AuthClaims, State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_engagement(&s.pool, &body.name).await {
        Ok(row) => (StatusCode::CREATED, Json(row)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
