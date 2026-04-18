use celerex_orchestrator::handlers::http::HttpHandler;
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn http_handler_calls_url_and_returns_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/things"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [1,2,3]})))
        .mount(&server)
        .await;

    let h = HttpHandler::new();
    let cfg = json!({ "method": "GET", "url": format!("{}/things", server.uri()) });
    let out = h.invoke(cfg, json!({})).await.unwrap();
    assert_eq!(out["items"], json!([1, 2, 3]));
}
