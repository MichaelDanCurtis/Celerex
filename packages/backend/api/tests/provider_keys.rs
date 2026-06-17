//! Integration tests for the per-engagement encrypted provider-keys API.
//!
//!   * round-trip a key via PUT → list and verify the masked preview
//!   * verify cross-engagement isolation (engagement A's key invisible to B)
//!   * verify DELETE clears the row
//!   * verify unknown provider_id returns 400 on both PUT and DELETE
//!
//! All routes go through the same `AuthClaims` extractor production uses, so
//! these tests also exercise that JWT path end-to-end.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, put};
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{executor::Executor, llm::ProviderRegistry, tool_dispatch::Dispatcher};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

fn build_router(pool: PgPool) -> Router {
    let dispatcher = Arc::new(Dispatcher::new());
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(
        pool.clone(),
        run_bus.clone(),
    ));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn celerex_api::plugins::credentials::CredentialStore> = Arc::new(
        celerex_api::plugins::credentials::LocalAeadStore::from_env()
            .expect("CELEREX_CRED_KEY must be set"),
    );
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        notifiers::console::ConsoleNotifier,
    )]);
    let provider_keys: Arc<dyn ProviderKeysStore> = Arc::new(
        PgProviderKeysStore::from_env(pool.clone()).expect("CELEREX_CRED_KEY must be set"),
    );
    let state = AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(Vec::new()),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        hub_admin_engagements: Default::default(),
    };
    Router::new()
        .route(
            "/api/providers/keys",
            get(celerex_api::providers::routes::list),
        )
        .route(
            "/api/providers/keys/{provider_id}",
            put(celerex_api::providers::routes::set_key)
                .delete(celerex_api::providers::routes::delete_key),
        )
        .with_state(state)
}

async fn seed_engagement(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn auth(eng: Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(eng)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn set_get_round_trip() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let eng = seed_engagement(&db.pool, "RoundTrip").await;
    let token = auth(eng);

    // PUT — sets a key.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/anthropic")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({ "key": "sk-ant-test-key-1234567890" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["provider_id"], "anthropic");
    // Sanity: preview is masked, contains "..." and does NOT contain the
    // middle of the plaintext.
    let preview = body["key_preview"].as_str().unwrap();
    assert!(
        preview.contains("..."),
        "expected '...' separator in preview, got {preview}"
    );
    assert!(
        !preview.contains("test-key"),
        "preview must mask the middle of the plaintext, got {preview}"
    );
    // Updated_at should match created_at on first insert.
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());

    // GET — list shows the same provider with the same preview.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers/keys")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["provider_id"], "anthropic");
    assert_eq!(arr[0]["key_preview"].as_str().unwrap(), preview);

    // PUT again — updates. The DB row should be replaced (no UniqueViolation).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/anthropic")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({ "key": "sk-ant-replacement-9999" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_preview = body["key_preview"].as_str().unwrap();
    assert_ne!(new_preview, preview, "upsert should refresh the preview");

    // Still exactly one row in the DB for this (engagement_id, provider_id).
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engagement_provider_keys
         WHERE engagement_id = $1 AND provider_id = 'anthropic'",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "PUT must upsert, not insert duplicates");
}

#[tokio::test]
async fn cross_engagement_isolation() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let eng_a = seed_engagement(&db.pool, "EngA").await;
    let eng_b = seed_engagement(&db.pool, "EngB").await;
    let token_a = auth(eng_a);
    let token_b = auth(eng_b);

    // Engagement A sets a key.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/openai")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::from(json!({ "key": "sk-eng-a-only-9876543" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Engagement B's list is empty.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers/keys")
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(
        body.as_array().unwrap().len(),
        0,
        "engagement B must not see engagement A's keys"
    );

    // Engagement A still sees its own key.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers/keys")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["provider_id"], "openai");
}

#[tokio::test]
async fn delete_removes_key() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let eng = seed_engagement(&db.pool, "DelKey").await;
    let token = auth(eng);

    // Seed a key via PUT.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/kimi")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "key": "kimi-key-xyz-0123456" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // DELETE returns 204.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/providers/keys/kimi")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // List is empty.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers/keys")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body.as_array().unwrap().len(), 0);

    // DB row is gone.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engagement_provider_keys
         WHERE engagement_id = $1 AND provider_id = 'kimi'",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);

    // Repeat DELETE returns 404.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/providers/keys/kimi")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_provider_id_returns_400() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let eng = seed_engagement(&db.pool, "BadProvider").await;
    let token = auth(eng);

    // PUT with unknown provider — rejected.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/azure")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "key": "az-key-12345" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert!(
        body["error"].as_str().unwrap().contains("azure"),
        "error should name the bad provider"
    );

    // DELETE with unknown provider — also 400.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/providers/keys/azure")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Empty-string key is also rejected (even for a valid provider).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/providers/keys/gemini")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "key": "   " }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
