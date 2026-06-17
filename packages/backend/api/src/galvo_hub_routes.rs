//! HTTP surface for Galvo Hub federation: register Celerex with a Hub, check
//! status, and invoke a remote agent over the encrypted mTLS A2A relay.
//!
//! Federation identity is workspace-global (one keystore per Celerex instance),
//! so these are auth-gated but not engagement-scoped.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::connectors::galvo_hub::{self, GalvoHubClient, RegisterRequest};
use crate::{auth::extractors::AuthClaims, AppState};

fn read_ca(ca_path: &Option<String>, ca_cert_pem: &Option<String>) -> Result<String, String> {
    if let Some(pem) = ca_cert_pem {
        if !pem.is_empty() {
            return Ok(pem.clone());
        }
    }
    let path = ca_path
        .clone()
        .or_else(|| std::env::var("GALVO_HUB_CA_PATH").ok())
        .ok_or_else(|| "provide ca_cert_pem, ca_path, or set GALVO_HUB_CA_PATH".to_string())?;
    std::fs::read_to_string(&path).map_err(|e| format!("read CA at {path}: {e}"))
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub hub_url: String,
    pub bootstrap_token: String,
    #[serde(default)]
    pub ca_path: Option<String>,
    #[serde(default)]
    pub ca_cert_pem: Option<String>,
    pub org_id: String,
    #[serde(default)]
    pub org_name: Option<String>,
    pub agent_slug: String,
    #[serde(default)]
    pub agent_description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Optional: elevate this org's trust tier right after registering
    /// (bootstrap-gated) so it can clear targets' default Verified policy.
    #[serde(default)]
    pub elevate_tier: Option<String>,
}

/// `POST /api/galvo/hub/register` — register Celerex as an org/agent + persist creds.
pub async fn register(
    AuthClaims(claims): AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<RegisterBody>,
) -> impl IntoResponse {
    if !s.hub_admin_engagements.contains(&claims.engagement_id) {
        return (StatusCode::FORBIDDEN, "hub admin access required").into_response();
    }
    let ca = match read_ca(&b.ca_path, &b.ca_cert_pem) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };
    let org_id = b.org_id.clone();
    let hub_url = b.hub_url.clone();
    let ks = match galvo_hub::register(RegisterRequest {
        hub_url: b.hub_url,
        bootstrap_token: b.bootstrap_token.clone(),
        ca_cert_pem: ca.clone(),
        org_id: b.org_id,
        org_name: b.org_name.unwrap_or_else(|| org_id.clone()),
        agent_slug: b.agent_slug,
        agent_description: b.agent_description.unwrap_or_default(),
        capabilities: b.capabilities,
    })
    .await
    {
        Ok(ks) => ks,
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": e.to_string() }))).into_response()
        }
    };
    if let Some(tier) = b.elevate_tier {
        if let Err(e) =
            galvo_hub::set_org_tier(&hub_url, &ca, &b.bootstrap_token, &org_id, &tier).await
        {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("registered, but set_org_tier failed: {e}") }))).into_response();
        }
    }
    let path = galvo_hub::keystore_path();
    if let Err(e) = ks.save(&path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response();
    }
    (
        StatusCode::CREATED,
        Json(json!({ "handle": ks.handle, "hub_url": ks.hub_url, "keystore": path.display().to_string() })),
    )
        .into_response()
}

/// `GET /api/galvo/hub/status` — is this Celerex registered, and as whom?
pub async fn status(AuthClaims(claims): AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    if !s.hub_admin_engagements.contains(&claims.engagement_id) {
        return (StatusCode::FORBIDDEN, "hub admin access required").into_response();
    }
    match galvo_hub::HubKeystore::load(&galvo_hub::keystore_path()) {
        Ok(ks) => Json(json!({
            "registered": true,
            "handle": ks.handle,
            "org_id": ks.org_id,
            "hub_url": ks.hub_url,
            "x25519_pub_b64": ks.x25519_public_b64,
        }))
        .into_response(),
        Err(_) => Json(json!({ "registered": false })).into_response(),
    }
}

#[derive(Deserialize)]
pub struct InvokeBody {
    pub to_handle: String,
    /// Arbitrary JSON payload (the A2A message) to seal + relay.
    pub payload: serde_json::Value,
}

/// `POST /api/galvo/hub/invoke` — invoke a remote agent over the Hub relay.
pub async fn invoke(
    AuthClaims(claims): AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<InvokeBody>,
) -> impl IntoResponse {
    if !s.hub_admin_engagements.contains(&claims.engagement_id) {
        return (StatusCode::FORBIDDEN, "hub admin access required").into_response();
    }
    let ks = match galvo_hub::HubKeystore::load(&galvo_hub::keystore_path()) {
        Ok(ks) => ks,
        Err(_) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "not registered — POST /api/galvo/hub/register first" })),
            )
                .into_response()
        }
    };
    let client = GalvoHubClient::new(ks);
    let payload = serde_json::to_vec(&b.payload).unwrap_or_default();
    match client.invoke(&b.to_handle, &payload).await {
        Ok(env) => (
            StatusCode::OK,
            Json(json!({
                "relayed": true,
                "from": env.header.from,
                "to": env.header.to,
                "task_id": env.header.task_id,
                "msg_id": env.header.msg_id,
            })),
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}
