use async_trait::async_trait;
use celerex_orchestrator::handlers::llm::{LlmClient, LlmHandler};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::{json, Value};
use std::sync::Arc;

struct StubLlm;

#[async_trait]
impl LlmClient for StubLlm {
    async fn complete(&self, model: &str, _m: Value, _t: Value) -> Result<Value, String> {
        Ok(json!({ "model": model, "text": "ok" }))
    }
}

#[tokio::test]
async fn llm_handler_passes_model() {
    let h = LlmHandler {
        client: Arc::new(StubLlm),
    };
    let out = h
        .invoke(
            json!({ "model": "claude-haiku-4-5" }),
            json!({"messages": []}),
        )
        .await
        .unwrap();
    assert_eq!(out["model"], "claude-haiku-4-5");
}
