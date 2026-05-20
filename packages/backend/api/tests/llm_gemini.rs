use celerex_api::llm::gemini::GeminiProvider;
use celerex_orchestrator::llm::provider::Provider;
use futures_util::StreamExt;
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

#[tokio::test]
async fn gemini_provider_streams_text_deltas() {
    let server = MockServer::start().await;

    // Gemini SSE: two `data:` JSON lines carrying candidates[0].content.parts[0].text.
    // No terminal marker — Gemini just closes the stream.
    let sse_body = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n",
        "\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]}}]}\n",
        "\n",
    );

    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-pro:streamGenerateContent"))
        .and(query_param("key", "test-key"))
        .and(query_param("alt", "sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let provider = GeminiProvider::with_base_url(server.uri(), "test-key".into());
    let mut stream = provider.complete_stream(
        "gemini-1.5-pro",
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
