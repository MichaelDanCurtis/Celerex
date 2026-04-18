use async_trait::async_trait;
use celerex_orchestrator::handlers::agent::{A2AClient, AgentHandler};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::{json, Value};
use std::sync::Arc;

struct StubClient;

#[async_trait]
impl A2AClient for StubClient {
    async fn call(&self, target: &str, _params: Value) -> Result<Value, String> {
        Ok(json!({ "called": target }))
    }
}

#[tokio::test]
async fn agent_handler_delegates_to_client() {
    let h = AgentHandler {
        client: Arc::new(StubClient),
    };
    let out = h
        .invoke(json!({ "target": "galvo.research.weekly" }), json!({}))
        .await
        .unwrap();
    assert_eq!(out["called"], "galvo.research.weekly");
}
