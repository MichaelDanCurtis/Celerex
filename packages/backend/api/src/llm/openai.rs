// OpenAI provider — implements the orchestrator Provider trait.
// Uses /v1/chat/completions; base_url is overrideable for wiremock in tests.

use async_trait::async_trait;
use serde_json::{json, Value};

use celerex_orchestrator::llm::provider::{Completion, Provider};

pub struct OpenAiProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiProvider {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("OPENAI_API_KEY")?;
        Ok(Self::with_base_url("https://api.openai.com".into(), key))
    }

    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self { base_url, api_key, http: reqwest::Client::new() }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let body = json!({
            "model": model,
            "messages": messages,
            "tools": tools,
        });
        let raw = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<Value>()
            .await
            .map_err(|e| e.to_string())?;
        Ok(Completion { raw })
    }
}
