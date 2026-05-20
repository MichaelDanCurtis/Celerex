use crate::llm::provider::Provider;
use crate::llm::registry::ProviderRegistry;
use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct LlmHandler {
    pub registry: Arc<ProviderRegistry>,
}

impl LlmHandler {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        let mut registry = ProviderRegistry::new();
        registry.register("default", provider);
        Self {
            registry: Arc::new(registry),
        }
    }
}

#[async_trait]
impl Handler for LlmHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let provider_id = config
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let provider = self
            .registry
            .get(provider_id)
            .ok_or_else(|| DispatchError::Handler(format!("no provider '{provider_id}'")))?;
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs
            .get("messages")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        let c = provider
            .complete(model, messages, tools)
            .await
            .map_err(DispatchError::Handler)?;
        Ok(c.raw)
    }
}
