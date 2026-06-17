use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;

#[allow(dead_code)]
pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.4-mini";

/// The vaulted Codex credential (stored as a JSON string under provider id `codex`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodexCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
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

/// Token-refresh client. Base URL injectable for tests.
pub struct CodexAuthClient {
    /// e.g. https://auth.openai.com
    pub auth_base: String,
    http: reqwest::Client,
}

impl CodexAuthClient {
    pub fn new() -> Self {
        Self::with_base("https://auth.openai.com".into())
    }
    pub fn with_base(auth_base: String) -> Self {
        Self {
            auth_base: auth_base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Exchange a refresh_token for a fresh access_token (+ maybe a rotated refresh_token).
    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<String>, i64), String> {
        let res = self
            .http
            .post(format!("{}/oauth/token", self.auth_base))
            .header("accept", "application/json")
            .json(&json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": CODEX_CLIENT_ID
            }))
            .send()
            .await
            .map_err(|e| format!("codex refresh failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("codex refresh status {}", res.status()));
        }
        let r: RefreshResp = res
            .json()
            .await
            .map_err(|e| format!("codex refresh parse: {e}"))?;
        let exp = chrono::Utc::now().timestamp() + r.expires_in.unwrap_or(864_000);
        Ok((r.access_token, r.refresh_token, exp))
    }
}

impl Default for CodexAuthClient {
    fn default() -> Self {
        Self::new()
    }
}

/// A per-engagement Codex provider built from the vaulted credential blob.
/// `persist` is invoked after a refresh so the new blob is written back to the vault.
pub struct CodexProvider {
    cred: Mutex<CodexCredential>,
    auth: CodexAuthClient,
    /// override the chat host in tests; otherwise chatgpt.com
    chat_base: String,
    /// optional sink to persist a refreshed credential (engagement-scoped); None in unit tests
    #[allow(clippy::type_complexity)]
    persist: Option<Box<dyn Fn(CodexCredential) + Send + Sync>>,
}

impl CodexProvider {
    /// Production: parse the vaulted JSON blob.
    pub fn from_blob(blob: &str) -> Result<Self, String> {
        let cred: CodexCredential =
            serde_json::from_str(blob).map_err(|e| format!("codex cred parse: {e}"))?;
        Ok(Self {
            cred: Mutex::new(cred),
            auth: CodexAuthClient::new(),
            chat_base: "https://chatgpt.com".into(),
            persist: None,
        })
    }
    /// Test factory: point refresh + chat at stubs.
    #[allow(dead_code)]
    pub fn with_bases(cred: CodexCredential, auth_base: String, chat_base: String) -> Self {
        Self {
            cred: Mutex::new(cred),
            auth: CodexAuthClient::with_base(auth_base),
            chat_base: chat_base.trim_end_matches('/').to_string(),
            persist: None,
        }
    }
    #[allow(dead_code)]
    pub fn with_persist(mut self, f: Box<dyn Fn(CodexCredential) + Send + Sync>) -> Self {
        self.persist = Some(f);
        self
    }

    /// Return a valid access token + account_id, refreshing if within 60s of expiry.
    async fn access(&self) -> Result<(String, String), String> {
        let now = chrono::Utc::now().timestamp();
        let (access, refresh, account, exp) = {
            let c = self.cred.lock().unwrap();
            (
                c.access_token.clone(),
                c.refresh_token.clone(),
                c.account_id.clone(),
                c.expires_at,
            )
        };
        if exp - 60 > now {
            return Ok((access, account));
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
        Ok((new_access, account))
    }

    /// Translate chat-completions messages → (instructions, input items).
    fn translate(messages: &Value) -> (String, Vec<Value>) {
        let mut instructions = String::new();
        let mut input = Vec::new();
        if let Some(arr) = messages.as_array() {
            for m in arr {
                let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if role == "system" {
                    if !instructions.is_empty() {
                        instructions.push_str("\n\n");
                    }
                    instructions.push_str(content);
                } else {
                    input.push(json!({
                        "type": "message", "role": role,
                        "content": [{ "type": "input_text", "text": content }]
                    }));
                }
            }
        }
        if instructions.is_empty() {
            instructions = "You are a helpful assistant.".into();
        }
        (instructions, input)
    }

    /// Accumulate a Responses-API SSE body into a chat-completions-shaped `raw`.
    fn destream(body: &str) -> Value {
        let mut text = String::new();
        let mut usage = Value::Null;
        for line in body.lines() {
            let line = line.trim();
            let data = match line.strip_prefix("data:") {
                Some(d) => d.trim(),
                None => continue,
            };
            if data == "[DONE]" {
                continue;
            }
            let ev: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match ev.get("type").and_then(|v| v.as_str()) {
                Some("response.output_text.delta") => {
                    if let Some(d) = ev.get("delta").and_then(|v| v.as_str()) {
                        text.push_str(d);
                    }
                }
                Some("response.completed") => {
                    usage = ev
                        .get("response")
                        .and_then(|r| r.get("usage"))
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                _ => {}
            }
        }
        json!({
            "choices": [{ "message": { "role": "assistant", "content": text } }],
            "usage": usage
        })
    }
}

#[async_trait]
impl Provider for CodexProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        _tools: Value,
    ) -> Result<Completion, String> {
        let (access, account) = self.access().await?;
        let (instructions, input) = Self::translate(&messages);
        let body = json!({
            "model": if model.is_empty() { DEFAULT_CODEX_MODEL } else { model },
            "instructions": instructions,
            "input": input,
            "stream": true,
            "store": false
        });
        let res = reqwest::Client::new()
            .post(format!(
                "{}/backend-api/codex/responses",
                self.chat_base.trim_end_matches('/')
            ))
            .header("authorization", format!("Bearer {access}"))
            .header("chatgpt-account-id", account)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("codex responses failed: {e}"))?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| format!("codex responses read: {e}"))?;
        if !status.is_success() {
            return Err(format!("codex responses status {status}: {text}"));
        }
        Ok(Completion {
            raw: Self::destream(&text),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cred(exp_offset: i64) -> CodexCredential {
        CodexCredential {
            access_token: "at_old".into(),
            refresh_token: "rt_1".into(),
            account_id: "acc_1".into(),
            expires_at: chrono::Utc::now().timestamp() + exp_offset,
        }
    }

    #[tokio::test]
    async fn refreshes_when_stale_then_completes_destreamed() {
        let auth = MockServer::start().await; // auth.openai.com
        let chat = MockServer::start().await; // chatgpt.com
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token":"at_new","refresh_token":"rt_2","expires_in":864000
            })))
            .mount(&auth)
            .await;
        let sse = "event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"pong\"}\n\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n";
        Mock::given(method("POST"))
            .and(path("/backend-api/codex/responses"))
            .and(header("authorization", "Bearer at_new")) // used the refreshed token
            .and(header("chatgpt-account-id", "acc_1"))
            .and(body_partial_json(
                json!({ "stream": true, "store": false, "instructions": "You are terse." }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse))
            .mount(&chat)
            .await;
        // expires_at in the past → must refresh
        let p = CodexProvider::with_bases(cred(-10), auth.uri(), chat.uri());
        let out = p
            .complete(
                "gpt-5.4-mini",
                json!([{"role":"system","content":"You are terse."},{"role":"user","content":"reply pong"}]),
                json!([]),
            )
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "pong");
        assert_eq!(out.raw["usage"]["output_tokens"], 1);
    }

    #[tokio::test]
    async fn skips_refresh_when_fresh_and_promotes_default_instructions() {
        let auth = MockServer::start().await; // must NOT be called
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/backend-api/codex/responses"))
            .and(header("authorization", "Bearer at_old")) // original token, no refresh
            .and(body_partial_json(
                json!({ "instructions": "You are a helpful assistant." }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
            ))
            .mount(&chat)
            .await;
        let p = CodexProvider::with_bases(cred(3600), auth.uri(), chat.uri());
        let out = p
            .complete("", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "hi");
    }
}
