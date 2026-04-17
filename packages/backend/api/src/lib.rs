pub mod db;
pub mod engagements;
pub mod auth;
pub mod projects;
pub mod docs;

use axum::{Router, routing::{get, post}, Json};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState { pub pool: PgPool }

pub fn build_app(pool: PgPool) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .route("/api/projects", get(projects::list).post(projects::create))
        .route("/api/docs", post(docs::create))
        .route("/api/docs/{id}/blocks", post(docs::append_block))
        .with_state(AppState { pool })
}
