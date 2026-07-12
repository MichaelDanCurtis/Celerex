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
use serde_json::Value;

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

use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use std::sync::Mutex;

use crate::llm::openai_compat::OpenAiCompatProvider;

/// The vaulted xAI credential (stored as a JSON string under provider id `xai`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XaiCredential {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: i64, // unix seconds
}

#[derive(Debug, Deserialize)]
struct RefreshResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl XaiOAuthClient {
    /// Exchange a refresh_token for a fresh access_token (+ maybe a rotated
    /// refresh_token). Returns (access_token, rotated_refresh, expires_at_unix).
    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<String>, i64), String> {
        let res = self
            .http
            .post(format!("{}/oauth2/token", self.auth_base))
            .header("accept", "application/json")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", XAI_CLIENT_ID),
            ])
            .send()
            .await
            .map_err(|e| format!("xai refresh failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("xai refresh status {}", res.status()));
        }
        let r: RefreshResp = res
            .json()
            .await
            .map_err(|e| format!("xai refresh parse: {e}"))?;
        let exp = chrono::Utc::now().timestamp() + r.expires_in.unwrap_or(3600);
        Ok((r.access_token, r.refresh_token, exp))
    }
}

/// A per-engagement xAI (Grok) provider built from the vaulted credential blob.
/// `persist` is invoked after a refresh so the new blob is written back to the vault.
pub struct XaiProvider {
    cred: Mutex<XaiCredential>,
    auth: XaiOAuthClient,
    /// OpenAI-compatible chat base (version path included); real = XAI_API_BASE.
    chat_base: String,
    #[allow(clippy::type_complexity)]
    persist: Option<Box<dyn Fn(XaiCredential) + Send + Sync>>,
}

impl XaiProvider {
    /// Production: parse the vaulted JSON blob; chat at the real xAI base.
    pub fn from_blob(blob: &str) -> Result<Self, String> {
        let cred: XaiCredential =
            serde_json::from_str(blob).map_err(|e| format!("xai cred parse: {e}"))?;
        Ok(Self {
            cred: Mutex::new(cred),
            auth: XaiOAuthClient::new(),
            chat_base: XAI_API_BASE.to_string(),
            persist: None,
        })
    }
    /// Test factory: point refresh + chat at stubs.
    #[allow(dead_code)]
    pub fn with_bases(cred: XaiCredential, auth_base: String, chat_base: String) -> Self {
        Self {
            cred: Mutex::new(cred),
            auth: XaiOAuthClient::with_base(auth_base),
            chat_base: chat_base.trim_end_matches('/').to_string(),
            persist: None,
        }
    }
    #[allow(dead_code)]
    pub fn with_persist(mut self, f: Box<dyn Fn(XaiCredential) + Send + Sync>) -> Self {
        self.persist = Some(f);
        self
    }

    /// Return a valid access token, refreshing if within 60s of expiry. The
    /// credential mutex is never held across the `.await` on refresh.
    async fn access(&self) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp();
        let (access, refresh, exp) = {
            let c = self.cred.lock().unwrap();
            (c.access_token.clone(), c.refresh_token.clone(), c.expires_at)
        };
        if exp - 60 > now {
            return Ok(access);
        }
        let (new_access, new_refresh, new_exp) = self.auth.refresh(&refresh).await?;
        let updated = {
            let mut c = self.cred.lock().unwrap();
            c.access_token = new_access.clone();
            if let Some(rt) = new_refresh {
                c.refresh_token = rt;
            }
            c.expires_at = new_exp;
            c.clone()
        };
        if let Some(p) = &self.persist {
            p(updated);
        }
        Ok(new_access)
    }
}

#[async_trait]
impl Provider for XaiProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let access = self.access().await?;
        let model = if model.is_empty() { DEFAULT_XAI_MODEL } else { model };
        OpenAiCompatProvider::new(self.chat_base.clone(), access)
            .complete(model, messages, tools)
            .await
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use serde_json::json;
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

    #[tokio::test]
    async fn poll_maps_slow_down() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "slow_down" })))
            .mount(&server)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(server.uri()).poll_token("dc").await,
            PollOutcome::SlowDown
        );
    }

    #[tokio::test]
    async fn poll_maps_expired_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({ "error": "expired_token" })),
            )
            .mount(&server)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(server.uri()).poll_token("dc").await,
            PollOutcome::Expired
        );
    }

    #[tokio::test]
    async fn poll_pending_then_success_progression() {
        let server = MockServer::start().await;
        // Mounted FIRST so it takes priority while it still has allowance
        // (wiremock 0.6: earlier-mounted matching mocks win until exhausted).
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": "authorization_pending" })),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "xai_at2",
                "refresh_token": "xai_rt2",
                "expires_in": 7200
            })))
            .mount(&server)
            .await;

        let c = XaiOAuthClient::with_base(server.uri());
        assert_eq!(c.poll_token("dc").await, PollOutcome::Pending);
        assert_eq!(
            c.poll_token("dc").await,
            PollOutcome::Token("xai_at2".into(), "xai_rt2".into(), 7200)
        );
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use celerex_orchestrator::llm::provider::Provider;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cred(exp_offset: i64) -> XaiCredential {
        XaiCredential {
            access_token: "at_old".into(),
            refresh_token: "rt_1".into(),
            expires_at: chrono::Utc::now().timestamp() + exp_offset,
        }
    }

    #[tokio::test]
    async fn refreshes_when_stale_then_completes() {
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "refresh_token": "rt_2",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer at_new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "pong" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        // expires_at in the past → must refresh before completing.
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base);
        let out = p
            .complete(
                "grok-4.3",
                json!([{"role":"user","content":"ping"}]),
                json!([]),
            )
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "pong");
    }

    #[tokio::test]
    async fn skips_refresh_when_fresh_and_defaults_model() {
        let auth = MockServer::start().await; // must NOT be called
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer at_old"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "hi" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let p = XaiProvider::with_bases(cred(3600), auth.uri(), chat_base);
        // empty model string → DEFAULT_XAI_MODEL is used (no assertion on it here,
        // just that the request succeeds without hitting the refresh endpoint).
        let out = p
            .complete("", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "hi");
    }

    #[tokio::test]
    async fn persists_rotated_refresh_token() {
        use std::sync::{Arc, Mutex};
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "refresh_token": "rt_2",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let seen: Arc<Mutex<Option<XaiCredential>>> = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        // stale cred (cred(-10)) → access() must refresh before completing.
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base)
            .with_persist(Box::new(move |c: XaiCredential| {
                *sink.lock().unwrap() = Some(c);
            }));
        p.complete("grok-4.3", json!([{"role":"user","content":"ping"}]), json!([]))
            .await
            .unwrap();
        let got = seen.lock().unwrap().clone().expect("persist must fire after refresh");
        assert_eq!(got.access_token, "at_new");
        assert_eq!(got.refresh_token, "rt_2", "rotated refresh_token must be persisted");
        assert!(
            got.expires_at > chrono::Utc::now().timestamp(),
            "expiry must be pushed forward on refresh"
        );
    }

    #[tokio::test]
    async fn persist_retains_refresh_token_when_response_omits_it() {
        use std::sync::{Arc, Mutex};
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "expires_in": 3600
                // no refresh_token → rotation absent; the prior rt_1 must be retained.
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let seen: Arc<Mutex<Option<XaiCredential>>> = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base)
            .with_persist(Box::new(move |c: XaiCredential| {
                *sink.lock().unwrap() = Some(c);
            }));
        p.complete("grok-4.3", json!([{"role":"user","content":"ping"}]), json!([]))
            .await
            .unwrap();
        let got = seen.lock().unwrap().clone().expect("persist must fire after refresh");
        assert_eq!(got.access_token, "at_new");
        assert_eq!(
            got.refresh_token, "rt_1",
            "an absent refresh_token must retain the prior vaulted token"
        );
    }
}
