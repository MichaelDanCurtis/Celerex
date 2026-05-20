use async_trait::async_trait;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use celerex_orchestrator::llm::ProviderRegistry;
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
    let h = LlmHandler::new(Arc::new(StubProv));
    let out = h
        .invoke(
            json!({ "model": "claude-haiku-4-5" }),
            json!({"messages": []}),
        )
        .await
        .unwrap();
    assert_eq!(out["model"], "claude-haiku-4-5");
}

struct ProvA;
struct ProvB;

#[async_trait]
impl Provider for ProvA {
    async fn complete(&self, _model: &str, _m: Value, _t: Value) -> Result<Completion, String> {
        Ok(Completion {
            raw: json!({ "text": "from-a" }),
        })
    }
}

#[async_trait]
impl Provider for ProvB {
    async fn complete(&self, _model: &str, _m: Value, _t: Value) -> Result<Completion, String> {
        Ok(Completion {
            raw: json!({ "text": "from-b" }),
        })
    }
}

#[tokio::test]
async fn llm_handler_routes_by_provider_id() {
    let mut registry = ProviderRegistry::new();
    registry.register("a", Arc::new(ProvA) as Arc<dyn Provider>);
    registry.register("b", Arc::new(ProvB) as Arc<dyn Provider>);
    let h = LlmHandler {
        registry: Arc::new(registry),
    };

    let out_a = h
        .invoke(json!({ "provider_id": "a" }), json!({"messages": []}))
        .await
        .unwrap();
    assert_eq!(out_a["text"], "from-a");

    let out_b = h
        .invoke(json!({ "provider_id": "b" }), json!({"messages": []}))
        .await
        .unwrap();
    assert_eq!(out_b["text"], "from-b");
}
