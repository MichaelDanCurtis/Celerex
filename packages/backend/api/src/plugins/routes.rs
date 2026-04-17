use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, auth::AuthClaims, db, plugins::manifest::PluginManifest};
#[allow(unused_imports)]
use crate::plugins::credentials::CredentialStore;

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

#[derive(Deserialize)]
pub struct CredBody {
    pub direction: String,
    pub payload: serde_json::Value,
}

pub async fn upsert_credentials(
    _a: AuthClaims,
    State(s): State<AppState>,
    Path(install): Path<Uuid>,
    Json(b): Json<CredBody>,
) -> impl IntoResponse {
    let pt = serde_json::to_vec(&b.payload).unwrap();
    let blob = match s.credentials.seal(&pt).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    match db::upsert_credentials(&s.pool, install, &b.direction, &blob.ciphertext, &blob.nonce).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
