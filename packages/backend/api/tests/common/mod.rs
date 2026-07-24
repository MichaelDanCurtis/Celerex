use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use celerex_api::AppState;
use celerex_api::notifiers;
use celerex_api::plugins::credentials::{CredentialStore, LocalAeadStore};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::llm::ProviderRegistry;
use celerex_orchestrator::tool_dispatch::Dispatcher;
use sqlx::PgPool;
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

pub struct TestDb {
    pub pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

pub async fn start_test_db() -> TestDb {
    // SAFETY: tests run in a single process; set before any route handler reads env.
    unsafe {
        std::env::set_var("CELEREX_CRED_KEY", "0123456789abcdef0123456789abcdef");
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let container = Postgres::default().start().await.expect("postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("connect");
    sqlx::migrate!("./src/db/migrations")
        .run(&pool)
        .await
        .expect("migrate");
    TestDb {
        pool,
        _container: container,
    }
}

/// Mint a full-session bearer token scoped to `eng`. Thin wrapper over the
/// production `mint_for_tests` helper (already used by 20+ integration tests)
/// rather than a duplicate ad-hoc JWT-encoding pattern, so there is exactly one
/// place that defines what a "test session token" looks like.
///
/// `common/mod.rs` is compiled once per integration-test binary (each `tests/*.rs`
/// file is its own crate), so this `pub fn` is genuinely unused — and flagged
/// dead_code — in every binary except `design_generation.rs`. `#[allow(dead_code)]`
/// suppresses that expected cross-binary noise; it is not hiding a real issue.
#[allow(dead_code)]
pub fn mint_token(eng: Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(eng)
}

/// Build an `AppState` + `Router` exposing just the two live-artifact serving
/// routes (Task 8), scoped to a freshly-created engagement. Mirrors the
/// `build_state`/`build_router` pattern in `tests/assistant_chat.rs`, trimmed
/// to what the artifact routes need (no LLM provider wiring required).
/// See `mint_token` above re: the `#[allow(dead_code)]`.
#[allow(dead_code)]
pub async fn build_app_for_generation_tests(pool: PgPool) -> (Router, Uuid) {
    let eng = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Gen Test Engagement') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let dispatcher = Arc::new(Dispatcher::new());
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(
        pool.clone(),
        run_bus.clone(),
    ));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn CredentialStore> =
        Arc::new(LocalAeadStore::from_env().expect("CELEREX_CRED_KEY must be set"));
    let notifiers_vec: Arc<Vec<Arc<dyn notifiers::Notifier>>> =
        Arc::new(vec![Arc::new(notifiers::console::ConsoleNotifier)]);
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
        design_runs: celerex_api::runs::design_chat::new_registry(),
    };

    let router = Router::new()
        .route(
            "/api/live-artifacts/{artifact_id}",
            get(celerex_api::runs::generation::artifact_routes::get_artifact_meta),
        )
        .route(
            "/api/live-artifacts/{artifact_id}/preview",
            get(celerex_api::runs::generation::artifact_routes::get_artifact_preview),
        )
        .with_state(state);

    (router, eng)
}
