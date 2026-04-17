pub mod db;
pub mod engagements;
pub mod auth;
pub mod projects;

use axum::{Router, routing::get, Json};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState { pub pool: PgPool }

pub fn build_app(pool: PgPool) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .route("/api/projects", get(projects::list))
        .with_state(AppState { pool })
}
