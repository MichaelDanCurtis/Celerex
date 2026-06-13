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
