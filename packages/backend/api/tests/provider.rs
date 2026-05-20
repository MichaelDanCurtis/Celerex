use celerex_orchestrator::llm::{provider::Provider, stub::StubProvider};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn stub_provider_returns_scripted_then_errors() {
    let p = StubProvider::text("hello");
    let c = p.complete("any", json!([]), json!([])).await.unwrap();
    assert_eq!(c.raw["content"][0]["text"], "hello");
    let err = p.complete("any", json!([]), json!([])).await.unwrap_err();
    assert!(err.contains("no more"));
}

#[tokio::test]
async fn registry_routes_by_provider_id() {
    use celerex_orchestrator::handlers::llm::LlmHandler;
    use celerex_orchestrator::llm::ProviderRegistry;
    use celerex_orchestrator::tool_dispatch::Handler;

    let mut registry = ProviderRegistry::new();
    registry.register("alpha", Arc::new(StubProvider::text("from-alpha")));
    registry.register("beta", Arc::new(StubProvider::text("from-beta")));

    let handler = LlmHandler { registry: Arc::new(registry) };

    let out = handler
        .invoke(json!({ "provider_id": "alpha" }), json!({ "messages": [] }))
        .await
        .unwrap();
    assert_eq!(out["content"][0]["text"], "from-alpha");

    let err = handler
        .invoke(json!({ "provider_id": "missing" }), json!({ "messages": [] }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("missing"), "error names the missing provider");
}
