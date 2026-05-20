use celerex_api::llm::kimi::KimiProvider;
use celerex_orchestrator::llm::provider::Provider;
use serde_json::json;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
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

    let provider =
        KimiProvider::with_base_url(server.uri(), "secret-kimi-key".into());
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
