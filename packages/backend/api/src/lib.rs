pub mod db;
pub mod engagements;
pub mod auth;
pub mod projects;
pub mod docs;
pub mod plugins;
pub mod webhooks;
pub mod runs;
pub mod llm;
pub mod scheduler;
pub mod project_members;
pub mod notifiers;

use std::sync::Arc;
use axum::{Router, routing::{get, patch, post, put}, Json};
use serde_json::json;
use sqlx::PgPool;

use crate::plugins::credentials::{CredentialStore, LocalAeadStore};
use crate::project_members::store::{MembersStore, PgMembersStore};
use crate::runs::bus::RunBus;
use celerex_orchestrator::{executor::Executor, tool_dispatch::Dispatcher};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub credentials: Arc<dyn CredentialStore>,
    pub executor: Arc<Executor>,
    pub members: Arc<dyn MembersStore>,
    pub run_bus: Arc<RunBus>,
    pub notifiers: Arc<Vec<Arc<dyn notifiers::Notifier>>>,
}

pub fn build_app(pool: PgPool) -> Router {
    let store: Arc<dyn CredentialStore> = Arc::new(
        LocalAeadStore::from_env().expect("CELEREX_CRED_KEY must be set")
    );
    build_app_with_store(pool, store)
}

pub fn build_app_with_store(pool: PgPool, credentials: Arc<dyn CredentialStore>) -> Router {
    let dispatcher = Arc::new(Dispatcher::new());
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(runs::PgRunStore::with_bus(pool.clone(), run_bus.clone()));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));

    let schedules: Arc<dyn scheduler::store::SchedulesStore> =
        Arc::new(scheduler::store::PgSchedulesStore::new(pool.clone()));
    let exec_for_sched = executor.clone();
    let pool_for_sched = pool.clone();
    let dispatch = Arc::new(move |s: scheduler::store::Schedule| {
        let exec = exec_for_sched.clone();
        let pool = pool_for_sched.clone();
        tokio::spawn(async move {
            // project_members integration lands in Task 2; for now, log only.
            tracing::info!(member=%s.project_member_id, "scheduler tick (dispatch stub until Task 2)");
            let _ = (exec, pool);
        })
    });
    scheduler::r#loop::SchedulerLoop {
        store: schedules,
        tick: std::time::Duration::from_secs(30),
        dispatch,
    }
    .spawn();

    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .route("/api/projects", get(projects::list).post(projects::create))
        .route("/api/docs", post(docs::create))
        .route("/api/docs/{id}/blocks", post(docs::append_block))
        .route("/api/docs/{doc_id}/blocks/{block_id}", patch(docs::replace_block))
        .route("/api/plugins", get(plugins::routes::list).post(plugins::routes::install))
        .route("/api/plugins/{id}/credentials", put(plugins::routes::upsert_credentials))
        .route("/api/webhooks/{plugin_id}/{event}", post(webhooks::ingest))
        .route("/api/project_members", post(project_members::routes::create))
        .route(
            "/api/project_members/fleet",
            get(project_members::routes::fleet),
        )
        .route(
            "/api/project_members/{id}",
            get(project_members::routes::get).put(project_members::routes::update),
        )
        .route("/api/projects/{project_id}/members", get(project_members::routes::list))
        .route("/api/project_members/{id}/run", post(project_members::routes::run_manual))
        .route("/api/runs/{run_id}/events", get(runs::sse::stream))
        .with_state(AppState {
            pool,
            credentials,
            executor,
            members,
            run_bus,
            notifiers: Arc::new(vec![
                Arc::new(notifiers::console::ConsoleNotifier),
                Arc::new(notifiers::teams_stub::StubTeamsNotifier::default()),
            ]),
        })
}
