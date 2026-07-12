use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::auth::extractors::AuthClaims;
use crate::llm::xai::{PollOutcome, XaiCredential, XaiOAuthClient};
use crate::{AppState, db};

fn oauth_client() -> XaiOAuthClient {
    match std::env::var("XAI_OAUTH_BASE") {
        Ok(b) => XaiOAuthClient::with_base(b),
        _ => XaiOAuthClient::new(),
    }
}

/// `POST /api/design/xai/login/start` — kick off the xAI device flow and spawn a
/// background poller that seals the resulting token blob (access + refresh +
/// expiry) under the `xai` provider id for this engagement.
pub async fn start(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let client = oauth_client();
    let dc = client
        .start_device_flow()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(dc.expires_in as i64);
    db::upsert_xai_login(
        &state.pool,
        claims.engagement_id,
        &dc.device_code,
        &dc.user_code,
        &dc.verification_uri,
        expires_at,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pool = state.pool.clone();
    let keys = state.provider_keys.clone();
    let eng = claims.engagement_id;
    let device_code = dc.device_code.clone();
    let interval = dc.interval.max(1);
    let expires_in = dc.expires_in;
    tokio::spawn(async move {
        let client = oauth_client();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_in);
        let mut wait = interval;
        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = db::set_xai_login_status(&pool, eng, "expired").await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            match client.poll_token(&device_code).await {
                PollOutcome::Token(access, refresh, ttl) => {
                    let cred = XaiCredential {
                        access_token: access,
                        refresh_token: refresh,
                        expires_at: chrono::Utc::now().timestamp() + ttl,
                    };
                    match serde_json::to_string(&cred) {
                        Ok(blob) if keys.set(eng, "xai", &blob).await.is_ok() => {
                            let _ = db::set_xai_login_status(&pool, eng, "connected").await;
                        }
                        _ => {
                            let _ = db::set_xai_login_status(&pool, eng, "error").await;
                        }
                    }
                    break;
                }
                PollOutcome::SlowDown => wait += 5,
                PollOutcome::Pending => {}
                PollOutcome::Denied => {
                    let _ = db::set_xai_login_status(&pool, eng, "denied").await;
                    break;
                }
                PollOutcome::Expired => {
                    let _ = db::set_xai_login_status(&pool, eng, "expired").await;
                    break;
                }
                PollOutcome::Err(_) => {
                    let _ = db::set_xai_login_status(&pool, eng, "error").await;
                    break;
                }
            }
        }
    });

    Ok(Json(json!({
        "user_code": dc.user_code,
        "verification_uri": dc.verification_uri,
        "verification_uri_complete": dc.verification_uri_complete,
        "interval": dc.interval,
    })))
}

/// `GET /api/design/xai/login/status` — report the device-flow status row plus
/// whether an `xai` token blob is actually vaulted for this engagement.
pub async fn status(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row = db::get_xai_login(&state.pool, claims.engagement_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let status = row.map(|(_, _, s)| s).unwrap_or_else(|| "none".to_string());
    let connected = state
        .provider_keys
        .get(claims.engagement_id, "xai")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "status": status, "connected": connected })))
}
