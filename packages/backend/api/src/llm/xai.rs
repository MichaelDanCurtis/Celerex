//! xAI (Grok) OAuth device-code flow + a vaulted-blob provider.
//!
//! Device flow mirrors `crate::llm::copilot::CopilotOAuthClient`; the blob-backed
//! `XaiProvider` (added in the provider half of this file) mirrors
//! `crate::llm::codex::CodexProvider` — refresh-on-expiry — but issues an
//! OpenAI-compatible chat call via `crate::llm::openai_compat::OpenAiCompatProvider`.
//!
//! The `client_id` below is PUBLIC desktop-client OAuth metadata (from the
//! community Grok flow `ysnock404/opencode-grok-auth`, `src/constants.ts`); it is
//! NOT a secret.

use serde::Deserialize;
use serde_json::{json, Value};

/// Public desktop-client OAuth client_id for the xAI (Grok) device flow.
pub const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
/// Device-flow scopes: coding + api access + a refresh token (offline_access).
pub const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
/// OpenAI-compatible chat base (version path included; client appends /chat/completions).
pub const XAI_API_BASE: &str = "https://api.x.ai/v1";
/// Default model when the caller sends an empty model string.
pub const DEFAULT_XAI_MODEL: &str = "grok-4.3";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
}
fn default_interval() -> u64 {
    5
}
fn default_expires() -> u64 {
    900
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    /// (access_token, refresh_token, expires_in seconds)
    Token(String, String, i64),
    Denied,
    Expired,
    Err(String),
}

/// Device-flow + refresh client. Base URL is injectable for tests.
pub struct XaiOAuthClient {
    /// e.g. https://auth.x.ai
    pub auth_base: String,
    http: reqwest::Client,
}

impl XaiOAuthClient {
    pub fn new() -> Self {
        Self::with_base("https://auth.x.ai".into())
    }
    pub fn with_base(auth_base: String) -> Self {
        Self {
            auth_base: auth_base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// RFC 8628 device authorization request (form-encoded).
    pub async fn start_device_flow(&self) -> Result<DeviceCode, String> {
        let res = self
            .http
            .post(format!("{}/oauth2/device/code", self.auth_base))
            .header("accept", "application/json")
            .form(&[("client_id", XAI_CLIENT_ID), ("scope", XAI_SCOPE)])
            .send()
            .await
            .map_err(|e| format!("device/code request failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("device/code status {}", res.status()));
        }
        res.json::<DeviceCode>()
            .await
            .map_err(|e| format!("device/code parse failed: {e}"))
    }

    /// Poll the token endpoint with the device_code grant. The token endpoint
    /// returns HTTP 400 with an `error` field while pending, so the body is
    /// parsed regardless of status (same approach as the Copilot poll).
    pub async fn poll_token(&self, device_code: &str) -> PollOutcome {
        let res = self
            .http
            .post(format!("{}/oauth2/token", self.auth_base))
            .header("accept", "application/json")
            .form(&[
                ("client_id", XAI_CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await;
        let res = match res {
            Ok(r) => r,
            Err(e) => return PollOutcome::Err(format!("poll failed: {e}")),
        };
        let body: Value = match res.json().await {
            Ok(v) => v,
            Err(e) => return PollOutcome::Err(format!("poll parse: {e}")),
        };
        if let Some(at) = body.get("access_token").and_then(|v| v.as_str()) {
            let rt = body
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let exp = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
            return PollOutcome::Token(at.to_string(), rt, exp);
        }
        match body.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => PollOutcome::Pending,
            Some("slow_down") => PollOutcome::SlowDown,
            Some("access_denied") => PollOutcome::Denied,
            Some("expired_token") => PollOutcome::Expired,
            other => PollOutcome::Err(format!("poll error: {other:?}")),
        }
    }
}

impl Default for XaiOAuthClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn device_flow_start_and_poll_success() {
        let auth = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dc",
                "user_code": "WXYZ-1234",
                "verification_uri": "https://x.ai/device",
                "verification_uri_complete": "https://x.ai/device?code=WXYZ-1234",
                "interval": 1,
                "expires_in": 900
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "xai_at",
                "refresh_token": "xai_rt",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        let c = XaiOAuthClient::with_base(auth.uri());
        let dc = c.start_device_flow().await.unwrap();
        assert_eq!(dc.user_code, "WXYZ-1234");
        assert_eq!(
            c.poll_token(&dc.device_code).await,
            PollOutcome::Token("xai_at".into(), "xai_rt".into(), 3600)
        );
    }

    #[tokio::test]
    async fn poll_maps_pending_and_denied() {
        let pending = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": "authorization_pending" })),
            )
            .mount(&pending)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(pending.uri()).poll_token("dc").await,
            PollOutcome::Pending
        );

        let denied = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({ "error": "access_denied" })),
            )
            .mount(&denied)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(denied.uri()).poll_token("dc").await,
            PollOutcome::Denied
        );
    }

    #[tokio::test]
    async fn client_id_is_the_pinned_public_value() {
        assert_eq!(XAI_CLIENT_ID, "b1a00492-073a-47ea-816f-4c329264a828");
    }
}
