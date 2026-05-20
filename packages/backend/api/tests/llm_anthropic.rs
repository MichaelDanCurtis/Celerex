use celerex_api::llm::anthropic::AnthropicClient;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::{method, path}};

#[tokio::test]
async fn anthropic_client_handles_text_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_1",
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "Hello!" }]
        })))
        .mount(&server).await;

    let c = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let resp = c.create_message(
        "claude-sonnet-4-6",
        vec![json!({"role":"user","content":"hi"})],
        vec![],
    ).await.unwrap();
    assert_eq!(resp["stop_reason"], "end_turn");
}

#[tokio::test]
async fn anthropic_client_as_provider_returns_completion() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_2",
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "hi back" }]
        })))
        .mount(&server).await;

    let c = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let completion = <AnthropicClient as celerex_orchestrator::llm::provider::Provider>::complete(
        &c,
        "claude-sonnet-4-6",
        json!([{"role":"user","content":"hi"}]),
        json!([]),
    )
    .await
    .unwrap();
    assert_eq!(completion.raw["stop_reason"], "end_turn");
}
