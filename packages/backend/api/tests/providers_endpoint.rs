mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{
    executor::Executor,
    llm::ProviderRegistry,
    tool_dispatch::Dispatcher,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;

fn build_state(pool: PgPool, provider_ids: Vec<String>) -> AppState {
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
    let provider_keys: Arc<dyn celerex_api::providers::store::ProviderKeysStore> = Arc::new(
        celerex_api::providers::store::PgProviderKeysStore::from_env(pool.clone())
            .expect("CELEREX_CRED_KEY must be set"),
    );
    AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(provider_ids),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        hub_admin_engagements: Default::default(),
        design_runs: celerex_api::runs::design_chat::new_registry(),
    }
}

// Re-implement the same handler the production app exposes at GET /api/providers
// so we can mount it on a minimal router. Mirrors src/lib.rs::list_providers.
async fn list_providers(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<serde_json::Value> {
    axum::Json(json!({ "providers": &*state.providers }))
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/providers", get(list_providers))
        .with_state(state)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn providers_endpoint_returns_configured_ids() {
    let db = common::start_test_db().await;
    let state = build_state(
        db.pool.clone(),
        vec![
            "anthropic".to_string(),
            "default".to_string(),
            "openai".to_string(),
        ],
    );
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body["providers"]
        .as_array()
        .expect("providers field should be array");
    let ids: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(ids, vec!["anthropic", "default", "openai"]);
}

#[tokio::test]
async fn providers_endpoint_returns_empty_when_none_configured() {
    let db = common::start_test_db().await;
    let state = build_state(db.pool.clone(), Vec::new());
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body, json!({ "providers": [] }));
}
