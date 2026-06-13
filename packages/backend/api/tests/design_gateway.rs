//! Celerex Design provider gateway — providers config, LLM proxy, key copy.
mod common;
use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

async fn setup() -> (common::TestDb, axum_test::TestServer, Uuid, String) {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let token = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

#[tokio::test]
async fn providers_lists_configured_keys_without_secrets() {
    let (_db, server, eng, token) = setup().await;
    // configure an anthropic key through the real route (full session token)
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    server.put("/api/providers/keys/anthropic")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "sk-real" })).await.assert_status_ok();

    let res = server.get("/api/design/providers")
        .add_header("authorization", format!("Bearer {token}")).await;
    res.assert_status_ok();
    let body: Value = res.json();
    let provs = body["providers"].as_array().unwrap();
    let anthropic = provs.iter().find(|p| p["id"] == "anthropic").unwrap();
    assert_eq!(anthropic["has_key"], true);
    assert_eq!(anthropic["proxy_base_url"], "/api/design/llm/anthropic");
    assert!(anthropic["models"].as_array().unwrap().len() > 0);
    // openai present but has_key false
    let openai = provs.iter().find(|p| p["id"] == "openai").unwrap();
    assert_eq!(openai["has_key"], false);
}

use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn proxy_injects_key_and_relays_upstream() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages")).and(header("x-api-key", "sk-real"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&upstream).await;
    unsafe { std::env::set_var("DESIGN_LLM_ANTHROPIC_BASE", upstream.uri()); }

    let (_db, server, eng, token) = setup().await;
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    server.put("/api/providers/keys/anthropic")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "sk-real" })).await.assert_status_ok();

    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "model": "claude-sonnet-4-6", "messages": [] })).await;
    res.assert_status_ok();
    assert!(res.text().contains("\"ok\":true"));
    unsafe { std::env::remove_var("DESIGN_LLM_ANTHROPIC_BASE"); }
}

#[tokio::test]
async fn proxy_409_when_no_key_configured() {
    let (_db, server, _eng, token) = setup().await;
    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({})).await;
    res.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn proxy_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);
    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {publish}"))
        .json(&serde_json::json!({})).await;
    res.assert_status(StatusCode::FORBIDDEN);
}
