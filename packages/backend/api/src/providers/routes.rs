//! HTTP handlers for per-engagement provider API keys.
//!
//!   - `GET    /api/providers/keys`               — list configured keys
//!   - `PUT    /api/providers/keys/{provider_id}` — set/replace a key
//!   - `DELETE /api/providers/keys/{provider_id}` — remove a key
//!
//! All routes are engagement-scoped via the JWT claims; no cross-engagement
//! lookup is possible, so a 403 path isn't needed.
//!
//! Plaintext keys are accepted only on `PUT`. Responses always return masked
//! `ProviderKeyMeta` — never the plaintext.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    AppState,
    auth::extractors::AuthClaims,
    providers::store::{ProviderKeyMeta, SUPPORTED_PROVIDERS},
};

#[derive(Deserialize)]
pub struct SetKeyRequest {
    pub key: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({ "error": msg.into() })))
}

fn validate_provider_id(provider_id: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if SUPPORTED_PROVIDERS.contains(&provider_id) {
        Ok(())
    } else {
        Err(err(
            StatusCode::BAD_REQUEST,
            format!(
                "unknown provider_id '{provider_id}'; expected one of {:?}",
                SUPPORTED_PROVIDERS
            ),
        ))
    }
}

pub async fn list(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state
        .provider_keys
        .list_for_engagement(claims.engagement_id)
        .await
    {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn set_key(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(body): Json<SetKeyRequest>,
) -> impl IntoResponse {
    if let Err(e) = validate_provider_id(&provider_id) {
        return e.into_response();
    }
    let trimmed = body.key.trim();
    if trimmed.is_empty() {
        return err(StatusCode::BAD_REQUEST, "key must be non-empty").into_response();
    }
    // NOTE: we deliberately do NOT log `trimmed` or even its preview here.
    // The plaintext key never appears in logs. The store recomputes the
    // preview from the plaintext and only returns it via the response body.
    match state
        .provider_keys
        .set(claims.engagement_id, &provider_id, trimmed)
        .await
    {
        Ok(meta) => (StatusCode::OK, Json::<ProviderKeyMeta>(meta)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

pub async fn delete_key(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = validate_provider_id(&provider_id) {
        return e.into_response();
    }
    match state
        .provider_keys
        .delete(claims.engagement_id, &provider_id)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
