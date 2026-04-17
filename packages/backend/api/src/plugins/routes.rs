use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, auth::AuthClaims, db, plugins::manifest::PluginManifest};

#[derive(Deserialize)]
pub struct InstallBody {
    pub manifest: PluginManifest,
}

pub async fn install(
    auth: AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<InstallBody>,
) -> impl IntoResponse {
    let manifest_json = serde_json::to_value(&b.manifest).unwrap();
    match db::install_plugin(&s.pool, auth.0.engagement_id, &b.manifest.id, &manifest_json).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list(
    auth: AuthClaims,
    State(s): State<AppState>,
) -> impl IntoResponse {
    match db::list_plugins(&s.pool, auth.0.engagement_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
