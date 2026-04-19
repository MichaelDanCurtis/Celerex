use async_trait::async_trait;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::{json, Value};
use std::sync::Arc;

struct StubProv;

#[async_trait]
impl Provider for StubProv {
    async fn complete(&self, model: &str, _m: Value, _t: Value) -> Result<Completion, String> {
        Ok(Completion {
            raw: json!({ "model": model, "text": "ok" }),
        })
    }
}

#[tokio::test]
async fn llm_handler_passes_model() {
    let h = LlmHandler {
        provider: Arc::new(StubProv),
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
