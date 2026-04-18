use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait A2AClient: Send + Sync {
    async fn call(&self, target: &str, params: Value) -> Result<Value, String>;
}

pub struct AgentHandler {
    pub client: Arc<dyn A2AClient>,
}

#[async_trait]
impl Handler for AgentHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let target = config
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing target".into()))?;
        self.client
            .call(target, inputs)
            .await
            .map_err(DispatchError::Handler)
    }
}
