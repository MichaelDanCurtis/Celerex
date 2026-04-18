use serde_json::{json, Value};

pub struct AnthropicClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        Ok(Self::with_base_url("https://api.anthropic.com".into(), key))
    }

    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self { base_url, api_key, http: reqwest::Client::new() }
    }

    pub async fn create_message(
        &self,
        model: &str,
        messages: Vec<Value>,
        tools: Vec<Value>,
    ) -> anyhow::Result<Value> {
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": messages,
            "tools": tools,
        });
        let resp = self.http.post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        Ok(resp.json::<Value>().await?)
    }
}
