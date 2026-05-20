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

struct FailingProv {
    id: &'static str,
}

#[async_trait]
impl Provider for FailingProv {
    async fn complete(&self, _model: &str, _m: Value, _t: Value) -> Result<Completion, String> {
        Err(format!("{} boom", self.id))
    }
}

struct OkProv {
    text: &'static str,
}

#[async_trait]
impl Provider for OkProv {
    async fn complete(&self, _model: &str, _m: Value, _t: Value) -> Result<Completion, String> {
        Ok(Completion {
            raw: json!({ "text": self.text }),
        })
    }
}

#[tokio::test]
async fn llm_handler_falls_back_on_provider_error() {
    let mut registry = ProviderRegistry::new();
    registry.register(
        "primary",
        Arc::new(FailingProv { id: "primary" }) as Arc<dyn Provider>,
    );
    registry.register(
        "backup",
        Arc::new(OkProv { text: "from-backup" }) as Arc<dyn Provider>,
    );
    let h = LlmHandler {
        registry: Arc::new(registry),
    };

    let out = h
        .invoke(
            json!({ "provider_id": "primary", "provider_fallback_ids": ["backup"] }),
            json!({ "messages": [] }),
        )
        .await
        .unwrap();
    assert_eq!(out["text"], "from-backup");
}

#[tokio::test]
async fn llm_handler_returns_last_error_when_all_fail() {
    let mut registry = ProviderRegistry::new();
    registry.register(
        "primary",
        Arc::new(FailingProv { id: "primary" }) as Arc<dyn Provider>,
    );
    registry.register(
        "backup",
        Arc::new(FailingProv { id: "backup" }) as Arc<dyn Provider>,
    );
    let h = LlmHandler {
        registry: Arc::new(registry),
    };

    let err = h
        .invoke(
            json!({ "provider_id": "primary", "provider_fallback_ids": ["backup"] }),
            json!({ "messages": [] }),
        )
        .await
        .unwrap_err();
    let msg = match err {
        celerex_orchestrator::tool_dispatch::DispatchError::Handler(s) => s,
        other => panic!("expected Handler error, got {other:?}"),
    };
    assert!(
        msg.contains("backup"),
        "expected last error to mention 'backup', got: {msg}"
    );
}

#[tokio::test]
async fn llm_handler_skips_missing_provider_in_fallback() {
    let mut registry = ProviderRegistry::new();
    registry.register(
        "primary",
        Arc::new(FailingProv { id: "primary" }) as Arc<dyn Provider>,
    );
    registry.register(
        "backup",
        Arc::new(OkProv { text: "from-backup" }) as Arc<dyn Provider>,
    );
    let h = LlmHandler {
        registry: Arc::new(registry),
    };

    let out = h
        .invoke(
            json!({
                "provider_id": "primary",
                "provider_fallback_ids": ["ghost", "backup"]
            }),
            json!({ "messages": [] }),
        )
        .await
        .unwrap();
    assert_eq!(out["text"], "from-backup");
}
