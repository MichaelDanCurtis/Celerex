//! Proves `"copilot"` is registered as a selectable per-engagement provider.
//!
//! Strategy (HTTP boundary, no network stub needed):
//!   - `resolve_engagement_provider` (private) gates on `SUPPORTED_PROVIDERS`
//!     and then on a vaulted key, so we drive it through `POST /api/assistant/chat`.
//!   - House mode is OFF, so the handler fails closed when no provider is built.
//!   - Engagement A has a vaulted `"copilot"` token → a `CopilotProvider` IS
//!     built and selected → `.complete()` reaches the real (unstubbed) GitHub
//!     host and fails → 502 BAD_GATEWAY. A 502 proves the provider was BUILT
//!     (an unsupported/unconfigured provider returns 409 instead).
//!   - Engagement B has no copilot token → nothing is built → 409 CONFLICT.
//!
//! Full network completion of a copilot chat is already covered by the wiremock
//! test in `src/llm/copilot.rs`; we deliberately do not re-test it here.

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
// House mode is forced OFF so the handler fails closed (409) when no provider is
// built — this is what lets us distinguish "built" (502) from "not built" (409).
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
        hub_admin_engagements: Default::default(),
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/assistant/chat", post(celerex_api::assistant::chat))
        .with_state(state)
}

async fn chat(app: &Router, eng: Uuid) -> StatusCode {
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
                        "provider_id": "copilot",
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
async fn copilot_provider_built_only_when_token_vaulted() {
    let db = common::start_test_db().await;

    // Build the real encrypted store so we can vault a copilot token directly,
    // and hand the SAME instance to the app state.
    let store = Arc::new(
        PgProviderKeysStore::from_env(db.pool.clone()).expect("CELEREX_CRED_KEY must be set"),
    );
    let provider_keys: Arc<dyn ProviderKeysStore> = store.clone();

    let app = build_router(build_state(db.pool.clone(), provider_keys));

    // Engagement A: has a vaulted copilot token.
    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    store
        .set(a, "copilot", "ghu_test_token")
        .await
        .expect("vault copilot token");

    // Engagement B: no copilot token.
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    // A: "copilot" is in SUPPORTED_PROVIDERS AND a key is vaulted, so a
    // CopilotProvider is built and selected. .complete() then reaches the real
    // (unstubbed) GitHub host and fails → 502. The 502 (not 409/400) proves the
    // provider was BUILT — i.e. "copilot" is a registered, selectable provider.
    let status_a = chat(&app, a).await;
    assert_eq!(
        status_a,
        StatusCode::BAD_GATEWAY,
        "engagement WITH a vaulted copilot token should build a provider and reach the upstream (502)"
    );

    // B: no vaulted key, house mode off → nothing built → fail closed (409).
    let status_b = chat(&app, b).await;
    assert_eq!(
        status_b,
        StatusCode::CONFLICT,
        "engagement WITHOUT a copilot token should build nothing and fail closed (409)"
    );
}
