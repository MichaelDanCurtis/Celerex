use celerex_api::llm::gemini::GeminiProvider;
use celerex_orchestrator::llm::provider::Provider;
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_partial_json, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn gemini_provider_complete_returns_raw_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-pro:generateContent"))
        .and(query_param("key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{
                "content": { "parts": [{ "text": "hi" }] }
            }]
        })))
        .mount(&server)
        .await;

    let provider = GeminiProvider::with_base_url(server.uri(), "test-key".into());
    let completion = provider
        .complete(
            "gemini-1.5-pro",
            json!([{ "role": "user", "content": "hello" }]),
            json!([]),
        )
        .await
        .expect("complete should succeed");

    assert_eq!(
        completion.raw["candidates"][0]["content"]["parts"][0]["text"],
        "hi"
    );
}

#[tokio::test]
async fn gemini_provider_translates_openai_messages_to_contents() {
    let server = MockServer::start().await;
    // Match a body that has the translated `contents` shape — role "user" with
    // a parts array containing our text.
    let expected_body = json!({
        "contents": [
            { "role": "user", "parts": [{ "text": "hello" }] }
        ]
    });

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-pro:generateContent"))
        .and(query_param("key", "abc"))
        .and(body_partial_json(&expected_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{ "content": { "parts": [{ "text": "ok" }] } }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = GeminiProvider::with_base_url(server.uri(), "abc".into());
    let completion = provider
        .complete(
            "gemini-1.5-pro",
            json!([{ "role": "user", "content": "hello" }]),
            json!([]),
        )
        .await
        .expect("complete should succeed");

    let raw: &Value = &completion.raw;
    assert_eq!(raw["candidates"][0]["content"]["parts"][0]["text"], "ok");

    // Drop server to assert the body_partial_json matcher was hit exactly once.
    drop(server);
}
