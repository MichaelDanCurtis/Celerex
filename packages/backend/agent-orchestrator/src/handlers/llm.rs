use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Value, String>;
}

pub struct LlmHandler {
    pub client: Arc<dyn LlmClient>,
}

#[async_trait]
impl Handler for LlmHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs
            .get("messages")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        self.client
            .complete(model, messages, tools)
            .await
            .map_err(DispatchError::Handler)
    }
}
