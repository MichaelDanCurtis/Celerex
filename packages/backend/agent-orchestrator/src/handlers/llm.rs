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
        let primary_id = config
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
        let fallback_ids: Vec<String> = config
            .get("provider_fallback_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut chain = vec![primary_id];
        chain.extend(fallback_ids);

        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs
            .get("messages")
            .cloned()
            .unwrap_or(Value::Array(vec![]));

        let mut last_err: Option<String> = None;
        for id in &chain {
            let Some(provider) = self.registry.get(id) else {
                last_err = Some(format!("no provider '{id}'"));
                continue;
            };
            match provider
                .complete(model, messages.clone(), tools.clone())
                .await
            {
                Ok(c) => return Ok(c.raw),
                Err(e) => {
                    last_err = Some(format!("provider '{id}' failed: {e}"));
                    continue;
                }
            }
        }
        Err(DispatchError::Handler(
            last_err.unwrap_or_else(|| "no providers attempted".into()),
        ))
    }
}
