use celerex_api::llm::openai::OpenAiProvider;
use celerex_orchestrator::llm::provider::Provider;
use futures_util::StreamExt;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

#[tokio::test]
async fn openai_provider_complete_returns_raw_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_x",
            "choices": [{
                "message": { "role": "assistant", "content": "hi" }
            }]
        })))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let c = provider
        .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
        .await
        .unwrap();
    assert_eq!(c.raw["choices"][0]["message"]["content"], "hi");
}

#[tokio::test]
async fn openai_provider_sends_bearer_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_x",
            "choices": [{
                "message": { "role": "assistant", "content": "ok" }
            }]
        })))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let c = provider
        .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
        .await
        .unwrap();
    assert_eq!(c.raw["choices"][0]["message"]["content"], "ok");
}

#[tokio::test]
async fn openai_provider_sends_model_and_messages_in_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({
            "model": "gpt-4o",
            "messages": [{"role":"user","content":"hi"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_x",
            "choices": [{
                "message": { "role": "assistant", "content": "ok" }
            }]
        })))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let c = provider
        .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
        .await
        .unwrap();
    assert_eq!(c.raw["choices"][0]["message"]["content"], "ok");
}

#[tokio::test]
async fn openai_provider_streams_text_deltas() {
    let server = MockServer::start().await;

    // OpenAI-compatible SSE: two `data:` JSON lines carrying delta.content,
    // then a literal `data: [DONE]` terminator.
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

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let mut stream =
        provider.complete_stream("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]));

    let mut collected = Vec::new();
    while let Some(item) = stream.next().await {
        collected.push(item.expect("stream chunk should be Ok"));
    }

    assert_eq!(collected, vec!["Hello", " world"]);
    assert_eq!(collected.concat(), "Hello world");
}

#[tokio::test]
async fn openai_provider_omits_tools_key_when_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_x",
            "choices": [{
                "message": { "role": "assistant", "content": "ok" }
            }]
        })))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    provider
        .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
        .await
        .unwrap();

    let requests = server.received_requests().await.expect("recording enabled");
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().expect("valid JSON body");
    assert!(
        body.get("tools").is_none(),
        "empty tools array must be omitted from the request body entirely, got: {body}"
    );
}

#[tokio::test]
async fn openai_provider_stream_omits_tools_key_when_empty() {
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

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let mut stream =
        provider.complete_stream("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]));
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
async fn openai_provider_propagates_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&server)
        .await;

    let provider = OpenAiProvider::with_base_url(server.uri(), "test-key".into());
    let err = provider
        .complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([]))
        .await
        .expect_err("expected non-2xx body to fail JSON decode");
    assert!(!err.is_empty(), "error message should not be empty");
}
