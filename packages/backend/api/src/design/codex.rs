use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::extractors::DeviceAuth;

#[derive(Debug, Deserialize)]
pub struct CodexCredentialBody {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
    #[serde(default)]
    pub expires_at: i64,
}

/// `POST /api/design/codex/credential` — store the captured Codex tokens in the
/// vault under provider id `codex`, scoped to the caller's engagement. The
/// daemon uploads here using its `design.device` pairing token (same auth model
/// as `design/deploy.rs`), so `DeviceAuth` — not `AuthClaims` — guards it.
pub async fn put_credential(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<CodexCredentialBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if body.access_token.is_empty() || body.refresh_token.is_empty() || body.account_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing token fields".into()));
    }
    let blob = serde_json::to_string(&json!({
        "access_token": body.access_token,
        "refresh_token": body.refresh_token,
        "account_id": body.account_id,
        "expires_at": body.expires_at
    }))
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state
        .provider_keys
        .set(claims.engagement_id, "codex", &blob)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /api/design/codex/status` — report whether a `codex` blob is vaulted for
/// this engagement (never returns the credential itself).
pub async fn status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let connected = state
        .provider_keys
        .get(claims.engagement_id, "codex")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "connected": connected })))
}
