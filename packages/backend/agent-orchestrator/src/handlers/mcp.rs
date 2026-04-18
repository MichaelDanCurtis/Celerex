use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;

pub struct McpHandler;

#[async_trait]
impl Handler for McpHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Err(DispatchError::Handler(
            "MCP execution deferred to V2".into(),
        ))
    }
}
