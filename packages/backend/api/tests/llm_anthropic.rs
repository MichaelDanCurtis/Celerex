use celerex_api::llm::anthropic::AnthropicClient;
use celerex_orchestrator::llm::provider::Provider;
use futures_util::StreamExt;
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

#[tokio::test]
async fn anthropic_client_streams_text_deltas() {
    let server = MockServer::start().await;

    // SSE body with message_start (ignored), three text deltas, message_stop.
    // Each event ends with a blank line per the SSE spec, but our parser only
    // needs newline-terminated `data: ` lines to fire.
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n",
        "\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\", \"}}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"world!\"}}\n",
        "\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
        "\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n",
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let c = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let mut stream = c.complete_stream(
        "claude-sonnet-4-6",
        json!([{"role":"user","content":"hi"}]),
        json!([]),
    );

    let mut collected = Vec::new();
    while let Some(item) = stream.next().await {
        collected.push(item.expect("stream chunk should be Ok"));
    }

    assert_eq!(collected, vec!["Hello", ", ", "world!"]);
    assert_eq!(collected.concat(), "Hello, world!");
}
