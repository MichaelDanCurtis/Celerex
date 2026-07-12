//! Proves `xai`, `kimi-code`, and `glm` are registered, selectable per-engagement
//! providers. Strategy mirrors `tests/copilot_provider.rs`:
//!   - House mode OFF → the chat handler fails closed (409) when nothing is built.
//!   - A vaulted key/blob for a supported id → the factory builds a client → the
//!     provider's real (unstubbed) upstream call fails → 502. A 502 (not 409/400)
//!     proves the arm exists. No live credentials are required.
//!   - An engagement with no key → nothing built → 409.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{executor::Executor, llm::ProviderRegistry, tool_dispatch::Dispatcher};
use serde_json::json;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct TestClaims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

// SAFETY: tests run single-threaded at setup; JWT secret matches start_test_db().
// House mode forced OFF so "built" (502) is distinguishable from "not built" (409).
fn token(eng: Uuid) -> String {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
        std::env::set_var("CELEREX_HOUSE_MODE", "false");
    }
    let c = TestClaims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

fn build_state(pool: PgPool, provider_keys: Arc<dyn ProviderKeysStore>) -> AppState {
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
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> =
        Arc::new(vec![Arc::new(notifiers::console::ConsoleNotifier)]);
    AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(Vec::new()),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        // NOTE: `AppState` gained a `design_runs` field (lib.rs:62) AFTER the
        // `copilot_provider.rs` helper was last written, so that older helper is
        // missing this field. It MUST be set or the struct literal won't compile.
        design_runs: celerex_api::runs::design_chat::new_registry(),
        hub_admin_engagements: Default::default(),
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/assistant/chat", post(celerex_api::assistant::chat))
        .with_state(state)
}

async fn chat(app: &Router, eng: Uuid, provider_id: &str) -> StatusCode {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(eng)))
                .body(Body::from(
                    json!({
                        "provider_id": provider_id,
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

#[tokio::test]
async fn new_providers_built_only_when_key_vaulted() {
    let db = common::start_test_db().await;
    let store = Arc::new(
        PgProviderKeysStore::from_env(db.pool.clone()).expect("CELEREX_CRED_KEY must be set"),
    );
    let provider_keys: Arc<dyn ProviderKeysStore> = store.clone();
    let app = build_router(build_state(db.pool.clone(), provider_keys));

    // xai: vault a VALID JSON blob with a fresh (unexpired) token so from_blob
    // parses and access() skips refresh, then the real api.x.ai call fails → 502.
    let xa: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('xa') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let blob = json!({
        "access_token": "xai_live_at",
        "refresh_token": "xai_live_rt",
        "expires_at": chrono::Utc::now().timestamp() + 3600
    })
    .to_string();
    store.set(xa, "xai", &blob).await.expect("vault xai blob");
    assert_eq!(
        chat(&app, xa, "xai").await,
        StatusCode::BAD_GATEWAY,
        "vaulted xai blob should build a provider and reach the upstream (502)"
    );

    // kimi-code: any non-empty key → built → real api.kimi.com call fails → 502.
    let kc: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('kc') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    store.set(kc, "kimi-code", "sk-kimi-test").await.expect("vault kimi-code key");
    assert_eq!(
        chat(&app, kc, "kimi-code").await,
        StatusCode::BAD_GATEWAY,
        "vaulted kimi-code key should build a provider and reach the upstream (502)"
    );

    // glm: any non-empty key → built → real api.z.ai call fails → 502.
    let gl: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('gl') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    store.set(gl, "glm", "glm-test-key").await.expect("vault glm key");
    assert_eq!(
        chat(&app, gl, "glm").await,
        StatusCode::BAD_GATEWAY,
        "vaulted glm key should build a provider and reach the upstream (502)"
    );

    // no key for any id → nothing built, house mode off → fail closed (409).
    let none: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('none') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        chat(&app, none, "xai").await,
        StatusCode::CONFLICT,
        "engagement without an xai blob should build nothing and fail closed (409)"
    );
    assert_eq!(chat(&app, none, "glm").await, StatusCode::CONFLICT);
}
