use celerex_api::llm::kimi::KimiProvider;
use celerex_orchestrator::llm::provider::Provider;
use futures_util::StreamExt;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn kimi_provider_complete_returns_raw_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-kimi-1",
            "object": "chat.completion",
            "model": "moonshot-v1-8k",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ni hao" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    let provider = KimiProvider::with_base_url(server.uri(), "test-key".into());
    let completion = provider
        .complete(
            "moonshot-v1-8k",
            json!([{ "role": "user", "content": "hi" }]),
            json!([]),
        )
        .await
        .expect("complete should succeed");

    assert_eq!(completion.raw["id"], "chatcmpl-kimi-1");
    assert_eq!(completion.raw["choices"][0]["message"]["content"], "ni hao");
}

#[tokio::test]
async fn kimi_provider_sends_bearer_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer secret-kimi-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = KimiProvider::with_base_url(server.uri(), "secret-kimi-key".into());
    let completion = provider
        .complete(
            "moonshot-v1-8k",
            json!([{ "role": "user", "content": "ping" }]),
            json!([]),
        )
        .await
        .expect("complete should succeed");

    assert_eq!(completion.raw["ok"], true);
    // Drop server to trigger wiremock's expect(1) verification.
    drop(server);
}

#[tokio::test]
async fn kimi_provider_omits_tools_key_when_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-kimi-2",
            "choices": [{
                "message": { "role": "assistant", "content": "ok" }
            }]
        })))
        .mount(&server)
        .await;

    let provider = KimiProvider::with_base_url(server.uri(), "test-key".into());
    provider
        .complete(
            "moonshot-v1-8k",
            json!([{ "role": "user", "content": "hi" }]),
            json!([]),
        )
        .await
        .expect("complete should succeed");

    let requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().expect("valid JSON body");
    assert!(
        body.get("tools").is_none(),
        "empty tools array must be omitted from the request body entirely, got: {body}"
    );
}

#[tokio::test]
async fn kimi_provider_stream_omits_tools_key_when_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: [DONE]\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let provider = KimiProvider::with_base_url(server.uri(), "test-key".into());
    let mut stream = provider.complete_stream(
        "kimi-k2.5",
        json!([{"role":"user","content":"hi"}]),
        json!([]),
    );
    while let Some(item) = stream.next().await {
        item.expect("stream chunk should be Ok");
    }

    let requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().expect("valid JSON body");
    assert!(
        body.get("tools").is_none(),
        "empty tools array must be omitted from the streaming request body, got: {body}"
    );
    // The non-tools keys must survive the gate.
    assert_eq!(body.get("stream"), Some(&json!(true)));
    assert!(body.get("messages").is_some());
}

#[tokio::test]
async fn kimi_provider_streams_text_deltas() {
    let server = MockServer::start().await;

    // Kimi uses the OpenAI-compatible SSE shape: data lines with
    // choices[0].delta.content, terminated by a literal `data: [DONE]`.
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n",
        "\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n",
        "\n",
        "data: [DONE]\n",
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let provider = KimiProvider::with_base_url(server.uri(), "test-key".into());
    let mut stream = provider.complete_stream(
        "moonshot-v1-8k",
        json!([{ "role": "user", "content": "hi" }]),
        json!([]),
    );

    let mut collected = Vec::new();
    while let Some(item) = stream.next().await {
        collected.push(item.expect("stream chunk should be Ok"));
    }

    assert_eq!(collected, vec!["Hello", " world"]);
    assert_eq!(collected.concat(), "Hello world");
}
