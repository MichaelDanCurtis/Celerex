//! Generic OpenAI-compatible chat-completions provider.
//!
//! Unlike [`crate::llm::openai::OpenAiProvider`] (which hardcodes a
//! `/v1/chat/completions` suffix), this client posts to
//! `{base_url}/chat/completions` — the caller supplies the FULL base including
//! any version path segment. That lets one client serve endpoints whose version
//! path differs (`.../v1`, `.../coding/v1`, `.../paas/v4`). It also checks the
//! upstream status and returns `Err` on non-2xx, so a failed upstream surfaces
//! as a provider error (HTTP 502 at the handler) rather than a silent 200.

use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use serde_json::{json, Value};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiCompatProvider {
    /// `base_url` must include the version path (e.g. `https://api.x.ai/v1`);
    /// the client appends `/chat/completions`.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let mut body = json!({ "model": model, "messages": messages });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = tools;
        }
        let res = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("openai-compat request failed: {e}"))?;
        let status = res.status();
        let raw: Value = res
            .json()
            .await
            .map_err(|e| format!("openai-compat parse: {e}"))?;
        if !status.is_success() {
            return Err(format!("openai-compat status {status}: {raw}"));
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
    async fn posts_to_base_plus_chat_completions_and_returns_raw() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/coding/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&srv)
            .await;
        let base = format!("{}/coding/v1", srv.uri());
        let p = OpenAiCompatProvider::new(base, "sk-test".into());
        let out = p
            .complete("glm-4", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "ok");
    }

    #[tokio::test]
    async fn non_2xx_is_a_provider_error() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({ "error": "bad key" })),
            )
            .mount(&srv)
            .await;
        let base = format!("{}/v1", srv.uri());
        let p = OpenAiCompatProvider::new(base, "nope".into());
        let err = p
            .complete("grok", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap_err();
        assert!(err.contains("401"), "error should carry the upstream status: {err}");
    }
}
