use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Completion {
    pub raw: Value,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String>;
}
