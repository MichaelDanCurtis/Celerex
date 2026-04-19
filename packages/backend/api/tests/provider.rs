use celerex_orchestrator::llm::{provider::Provider, stub::StubProvider};
use serde_json::json;

#[tokio::test]
async fn stub_provider_returns_scripted_then_errors() {
    let p = StubProvider::text("hello");
    let c = p.complete("any", json!([]), json!([])).await.unwrap();
    assert_eq!(c.raw["content"][0]["text"], "hello");
    let err = p.complete("any", json!([]), json!([])).await.unwrap_err();
    assert!(err.contains("no more"));
}
