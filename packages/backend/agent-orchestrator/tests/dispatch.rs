use async_trait::async_trait;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{json, Value};
use std::sync::Arc;

struct EchoHandler;

#[async_trait]
impl Handler for EchoHandler {
    async fn invoke(&self, _config: Value, inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({ "echo": inputs }))
    }
}

#[tokio::test]
async fn dispatcher_routes_to_registered_handler() {
    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(EchoHandler));
    let out = d
        .invoke(ExecutionType::Http, json!({}), json!({"x": 1}))
        .await
        .unwrap();
    assert_eq!(out, json!({ "echo": { "x": 1 } }));
}

#[tokio::test]
async fn dispatcher_errors_on_unregistered_type() {
    let d = Dispatcher::new();
    let err = d
        .invoke(ExecutionType::Mcp, json!({}), json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, DispatchError::NoHandler(_)));
}
