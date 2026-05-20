use celerex_api::llm::openai::OpenAiProvider;
use celerex_orchestrator::llm::provider::Provider;
use serde_json::json;
use wiremock::{
    matchers::{body_partial_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
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
        .complete(
            "gpt-4o",
            json!([{"role":"user","content":"hi"}]),
            json!([]),
        )
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
        .complete(
            "gpt-4o",
            json!([{"role":"user","content":"hi"}]),
            json!([]),
        )
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
        .complete(
            "gpt-4o",
            json!([{"role":"user","content":"hi"}]),
            json!([]),
        )
        .await
        .unwrap();
    assert_eq!(c.raw["choices"][0]["message"]["content"], "ok");
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
        .complete(
            "gpt-4o",
            json!([{"role":"user","content":"hi"}]),
            json!([]),
        )
        .await
        .expect_err("expected non-2xx body to fail JSON decode");
    assert!(!err.is_empty(), "error message should not be empty");
}
