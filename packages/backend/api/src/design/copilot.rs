use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::auth::extractors::DeviceAuth;
use crate::llm::copilot::{CopilotOAuthClient, PollOutcome};
use crate::{AppState, db};

fn oauth_client() -> CopilotOAuthClient {
    match (
        std::env::var("GITHUB_OAUTH_BASE"),
        std::env::var("GITHUB_API_BASE"),
    ) {
        (Ok(g), Ok(a)) => CopilotOAuthClient::with_bases(g, a),
        _ => CopilotOAuthClient::new(),
    }
}

/// `POST /api/design/copilot/login/start` — kick off the GitHub device flow and
/// spawn a background poller that vaults the resulting OAuth token (`ghu_…`)
/// under the `copilot` provider id for this engagement.
///
/// Device-paired credential management (same family as `design/codex.rs`,
/// rituals, and automations), so `DeviceAuth` — not `AuthClaims` — guards it.
/// This lets the local daemon drive the flow with its engagement-scoped
/// `design.device` pairing token; a full session token still works too.
pub async fn start(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let client = oauth_client();
    let dc = client
        .start_device_flow()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(dc.expires_in as i64);
    db::upsert_copilot_login(
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
                let _ = db::set_copilot_login_status(&pool, eng, "expired").await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            match client.poll_access_token(&device_code).await {
                PollOutcome::Token(ghu) => {
                    if keys.set(eng, "copilot", &ghu).await.is_ok() {
                        let _ = db::set_copilot_login_status(&pool, eng, "connected").await;
                    } else {
                        let _ = db::set_copilot_login_status(&pool, eng, "error").await;
                    }
                    break;
                }
                PollOutcome::SlowDown => wait += 5,
                PollOutcome::Pending => {}
                PollOutcome::Denied => {
                    let _ = db::set_copilot_login_status(&pool, eng, "denied").await;
                    break;
                }
                PollOutcome::Expired => {
                    let _ = db::set_copilot_login_status(&pool, eng, "expired").await;
                    break;
                }
                PollOutcome::Err(_) => {
                    let _ = db::set_copilot_login_status(&pool, eng, "error").await;
                    break;
                }
            }
        }
    });

    Ok(Json(json!({
        "user_code": dc.user_code,
        "verification_uri": dc.verification_uri,
        "interval": dc.interval,
    })))
}

/// `GET /api/design/copilot/login/status` — report the device-flow status row plus
/// whether a `copilot` token is actually vaulted for this engagement.
pub async fn status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row = db::get_copilot_login(&state.pool, claims.engagement_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let status = row.map(|(_, _, s)| s).unwrap_or_else(|| "none".to_string());
    let connected = state
        .provider_keys
        .get(claims.engagement_id, "copilot")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "status": status, "connected": connected })))
}
