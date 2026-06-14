//! Connectors-via-Agent (2a): the Agent serves a connector catalog + confirm-gated
//! credential adopt over the `design.device` seam. Mirrors `design_gateway.rs`.

use axum::http::StatusCode;
use serde_json::Value;

mod common;

async fn setup() -> (common::TestDb, axum_test::TestServer, uuid::Uuid, String) {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

#[tokio::test]
async fn lists_connectors_with_configured_flags() {
    let (_db, server, _eng, token) = setup().await;
    let res = server
        .get("/api/design/connectors")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    let connectors = body["connectors"].as_array().unwrap();
    // galvo-memory is internal → always configured; http-api needs a key → not yet.
    let mem = connectors
        .iter()
        .find(|c| c["id"] == "galvo-memory")
        .unwrap();
    assert_eq!(mem["configured"], true);
    let http = connectors.iter().find(|c| c["id"] == "http-api").unwrap();
    assert_eq!(http["configured"], false);
    // catalog carries read tool descriptions, no secrets.
    assert!(http["tools"].as_array().unwrap().iter().any(|t| t["name"] == "get"));
}

#[tokio::test]
async fn credential_requires_confirm_and_returns_adopted_value() {
    let (_db, server, eng, token) = setup().await;
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    // store an http-api credential via the connector setter (validated against the catalog)
    server
        .put("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "value": "secret-123" }))
        .await
        .assert_status_ok();

    // without confirm → 400
    server
        .get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // with confirm → adopted value
    let res = server
        .get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true")
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["kind"], "api_key");
    assert_eq!(body["value"], "secret-123");
}

#[tokio::test]
async fn internal_connector_credential_is_null() {
    let (_db, server, _eng, token) = setup().await;
    let res = server
        .get("/api/design/connectors/galvo-memory/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true")
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["kind"], "internal");
    assert!(body["value"].is_null());
}

#[tokio::test]
async fn credential_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);
    server
        .get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {publish}"))
        .add_header("x-celerex-confirm-copy", "true")
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_connector_is_404() {
    let (_db, server, _eng, token) = setup().await;
    server
        .get("/api/design/connectors/nope/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
