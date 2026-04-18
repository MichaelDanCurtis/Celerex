pub mod db;
pub mod engagements;
pub mod auth;
pub mod projects;
pub mod docs;
pub mod plugins;
pub mod webhooks;
pub mod runs;
pub mod llm;

use std::sync::Arc;
use axum::{Router, routing::{get, patch, post, put}, Json};
use serde_json::json;
use sqlx::PgPool;

use crate::plugins::credentials::{CredentialStore, LocalAeadStore};
use celerex_orchestrator::{executor::Executor, tool_dispatch::Dispatcher};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub credentials: Arc<dyn CredentialStore>,
    pub executor: Arc<Executor>,
}

pub fn build_app(pool: PgPool) -> Router {
    let store: Arc<dyn CredentialStore> = Arc::new(
        LocalAeadStore::from_env().expect("CELEREX_CRED_KEY must be set")
    );
    build_app_with_store(pool, store)
}

pub fn build_app_with_store(pool: PgPool, credentials: Arc<dyn CredentialStore>) -> Router {
    let dispatcher = Arc::new(Dispatcher::new());
    let run_store = Arc::new(runs::PgRunStore::new(pool.clone()));
    let executor = Arc::new(Executor::new(dispatcher, run_store));

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
        .with_state(AppState { pool, credentials, executor })
}
