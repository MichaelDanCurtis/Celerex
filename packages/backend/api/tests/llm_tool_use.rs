use celerex_api::llm::anthropic::{AnthropicClient, ToolDispatch};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::{method, path}};

struct StubTools;
#[async_trait]
impl ToolDispatch for StubTools {
    async fn invoke(&self, name: &str, _input: Value) -> Result<Value, String> {
        Ok(json!({ "result": format!("ran {name}") }))
    }
}

#[tokio::test]
async fn tool_use_round_trip_completes_with_text() {
    let server = MockServer::start().await;

    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m1", "stop_reason": "tool_use",
            "content": [{ "type": "tool_use", "id": "tu1", "name": "echo", "input": {"x": 1} }]
        })))
        .up_to_n_times(1)
        .mount(&server).await;

    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m2", "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "done" }]
        })))
        .mount(&server).await;

    let c = AnthropicClient::with_base_url(server.uri(), "k".into());
    let final_text = c.run_with_tools(
        "claude-sonnet-4-6",
        vec![json!({"role":"user","content":"go"})],
        vec![json!({"name":"echo","description":"e","input_schema":{"type":"object"}})],
        Arc::new(StubTools),
    ).await.unwrap();
    assert_eq!(final_text, "done");
}
