use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;

pub const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Device-flow + token-exchange client. Base URLs are injectable for tests.
#[allow(dead_code)]
pub struct CopilotOAuthClient {
    /// e.g. https://github.com  (device/code + oauth/access_token)
    pub github_base: String,
    /// e.g. https://api.github.com  (copilot_internal/v2/token)
    pub api_github_base: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
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
    Token(String),
    Denied,
    Expired,
    Err(String),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CopilotTokenResp {
    pub token: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub endpoints: CopilotEndpoints,
}
#[derive(Debug, Default, Deserialize)]
pub struct CopilotEndpoints {
    #[serde(default)]
    pub api: Option<String>,
}

impl CopilotOAuthClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_bases("https://github.com".into(), "https://api.github.com".into())
    }
    pub fn with_bases(github_base: String, api_github_base: String) -> Self {
        Self {
            github_base: github_base.trim_end_matches('/').to_string(),
            api_github_base: api_github_base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    #[allow(dead_code)]
    pub async fn start_device_flow(&self) -> Result<DeviceCode, String> {
        let res = self
            .http
            .post(format!("{}/login/device/code", self.github_base))
            .header("accept", "application/json")
            .json(&json!({ "client_id": COPILOT_CLIENT_ID, "scope": "read:user" }))
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

    #[allow(dead_code)]
    pub async fn poll_access_token(&self, device_code: &str) -> PollOutcome {
        let res = self
            .http
            .post(format!("{}/login/oauth/access_token", self.github_base))
            .header("accept", "application/json")
            .json(&json!({
                "client_id": COPILOT_CLIENT_ID,
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
            }))
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
        if let Some(tok) = body.get("access_token").and_then(|v| v.as_str()) {
            return PollOutcome::Token(tok.to_string());
        }
        match body.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => PollOutcome::Pending,
            Some("slow_down") => PollOutcome::SlowDown,
            Some("access_denied") => PollOutcome::Denied,
            Some("expired_token") => PollOutcome::Expired,
            other => PollOutcome::Err(format!("poll error: {:?}", other)),
        }
    }

    /// Exchange a `ghu_` token for a short-lived Copilot token + the API host.
    pub async fn exchange_copilot_token(&self, ghu: &str) -> Result<CopilotTokenResp, String> {
        let res = self
            .http
            .get(format!(
                "{}/copilot_internal/v2/token",
                self.api_github_base
            ))
            .header("authorization", format!("token {ghu}"))
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("copilot token exchange failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("copilot token exchange status {}", res.status()));
        }
        res.json::<CopilotTokenResp>()
            .await
            .map_err(|e| format!("copilot token parse: {e}"))
    }
}

/// A per-engagement Copilot LLM provider built from the vaulted `ghu_` token.
#[allow(dead_code)]
pub struct CopilotProvider {
    ghu: String,
    client: CopilotOAuthClient,
    /// override the chat host in tests; otherwise use the exchange's endpoints.api
    chat_base_override: Option<String>,
    cache: Mutex<Option<(String, String, i64)>>, // (copilot_token, api_base, expires_at)
}

impl CopilotProvider {
    #[allow(dead_code)]
    pub fn new(ghu: String) -> Self {
        Self {
            ghu,
            client: CopilotOAuthClient::new(),
            chat_base_override: None,
            cache: Mutex::new(None),
        }
    }
    /// Test factory: point both the exchange and the chat at a stub.
    #[allow(dead_code)]
    pub fn with_bases(
        ghu: String,
        github_base: String,
        api_github_base: String,
        chat_base: String,
    ) -> Self {
        Self {
            ghu,
            client: CopilotOAuthClient::with_bases(github_base, api_github_base),
            chat_base_override: Some(chat_base.trim_end_matches('/').to_string()),
            cache: Mutex::new(None),
        }
    }

    async fn copilot_token(&self) -> Result<(String, String), String> {
        // now() — tests pass exp far in the future; production re-exchanges when stale.
        let now = chrono::Utc::now().timestamp();
        if let Some((tok, api, exp)) = self.cache.lock().unwrap().clone() {
            if exp - 60 > now {
                return Ok((tok, api));
            }
        }
        let resp = self.client.exchange_copilot_token(&self.ghu).await?;
        // Treat an empty `chat_base_override` as "unset" so we fall through to
        // the exchange's `endpoints.api`.
        let api = self
            .chat_base_override
            .clone()
            .filter(|s| !s.is_empty())
            .or(resp.endpoints.api.clone())
            .unwrap_or_else(|| "https://api.githubcopilot.com".to_string());
        *self.cache.lock().unwrap() = Some((resp.token.clone(), api.clone(), resp.expires_at));
        Ok((resp.token, api))
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let (copilot_token, api_base) = self.copilot_token().await?;
        let mut body = json!({ "model": model, "messages": messages });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = tools;
        }
        let res = reqwest::Client::new()
            .post(format!(
                "{}/chat/completions",
                api_base.trim_end_matches('/')
            ))
            .header("authorization", format!("Bearer {copilot_token}"))
            .header("copilot-integration-id", "vscode-chat")
            .header("editor-version", "OmniVyx/1.0")
            .header("editor-plugin-version", "OmniVyx/1.0")
            .header("user-agent", "OmniVyx/1.0")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("copilot chat failed: {e}"))?;
        let status = res.status();
        let raw: Value = res
            .json()
            .await
            .map_err(|e| format!("copilot chat parse: {e}"))?;
        if !status.is_success() {
            return Err(format!("copilot chat status {status}: {}", raw));
        }
        Ok(Completion { raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn device_flow_start_and_poll() {
        let gh = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code":"dc","user_code":"WXYZ-1234","verification_uri":"https://github.com/login/device","interval":5,"expires_in":900
            })))
            .mount(&gh)
            .await;
        // first poll pending, then a token
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "access_token":"ghu_abc" })),
            )
            .mount(&gh)
            .await;
        let c = CopilotOAuthClient::with_bases(gh.uri(), gh.uri());
        let dc = c.start_device_flow().await.unwrap();
        assert_eq!(dc.user_code, "WXYZ-1234");
        assert_eq!(
            c.poll_access_token(&dc.device_code).await,
            PollOutcome::Token("ghu_abc".into())
        );
    }

    #[tokio::test]
    async fn provider_exchanges_then_completes() {
        let api = MockServer::start().await; // serves copilot_internal/v2/token
        let chat = MockServer::start().await; // serves /chat/completions
        Mock::given(method("GET"))
            .and(path("/copilot_internal/v2/token"))
            .and(header("authorization", "token ghu_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "token":"cop_tok","expires_at": (chrono::Utc::now().timestamp()+1800),
                "endpoints": { "api": chat.uri() }
            })))
            .mount(&api)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer cop_tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices":[{"message":{"role":"assistant","content":"hi from copilot"}}]
            })))
            .mount(&chat)
            .await;
        // chat_base_override is "" so we use endpoints.api from the exchange
        let p = CopilotProvider::with_bases(
            "ghu_abc".into(),
            "https://unused".into(),
            api.uri(),
            String::new(),
        );
        let out = p
            .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(
            out.raw["choices"][0]["message"]["content"],
            "hi from copilot"
        );
    }
}
