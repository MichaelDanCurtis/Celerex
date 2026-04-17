use axum::{Router, routing::get, Json};
use serde_json::json;
use sqlx::PgPool;

pub fn build_app(_pool: PgPool) -> Router {
    Router::new().route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
}
