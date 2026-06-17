use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::auth::extractors::AuthClaims;
use crate::{db, AppState};

pub async fn get_settings(
    a: AuthClaims,
    State(s): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let settings = db::get_settings(&s.pool, a.0.engagement_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "settings": settings })))
}

#[derive(Deserialize)]
pub struct PutSettings {
    pub settings: Value,
}

pub async fn put_settings(
    a: AuthClaims,
    State(s): State<AppState>,
    Json(body): Json<PutSettings>,
) -> Result<Json<Value>, (StatusCode, String)> {
    db::upsert_settings(&s.pool, a.0.engagement_id, &body.settings)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "settings": body.settings })))
}
