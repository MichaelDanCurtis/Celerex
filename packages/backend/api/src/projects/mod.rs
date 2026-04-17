use axum::{response::IntoResponse, Json};
use serde_json::json;
use crate::auth::AuthClaims;

pub async fn list(_auth: AuthClaims) -> impl IntoResponse {
    Json(json!([]))
}
