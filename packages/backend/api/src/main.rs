use axum::{
    Router,
    routing::get,
    response::Json,
};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/workspace", get(workspace_info))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;
    tracing::info!("Celerex API listening on http://localhost:3001");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "celerex-api",
        "version": "0.1.0"
    }))
}

async fn workspace_info() -> Json<Value> {
    Json(json!({
        "name": "Celerex Workspace",
        "version": "0.1.0",
        "features": {
            "editor": true,
            "agents": false,
            "plugins": false,
            "ai_panel": true,
            "vscode_integration": false
        }
    }))
}
