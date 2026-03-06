use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Engagement {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    docs: Vec<DocMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DocMeta {
    id: Uuid,
    title: String,
    engagement_id: Uuid,
    created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CreateEngagement {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateDoc {
    title: String,
}

#[derive(Debug, Deserialize)]
struct UpdateDoc {
    title: String,
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

type AppState = Arc<Mutex<Vec<Engagement>>>;

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Seed default data
    let personal_id = Uuid::new_v4();
    let now = Utc::now();
    let default_doc = DocMeta {
        id: Uuid::new_v4(),
        title: "Untitled".into(),
        engagement_id: personal_id,
        created_at: now,
    };
    let default_engagement = Engagement {
        id: personal_id,
        name: "Personal".into(),
        created_at: now,
        docs: vec![default_doc],
    };
    let state: AppState = Arc::new(Mutex::new(vec![default_engagement]));

    let app = Router::new()
        // Existing routes
        .route("/api/health", get(health))
        .route("/api/workspace", get(workspace_info))
        // Engagement CRUD
        .route("/api/engagements", get(list_engagements))
        .route("/api/engagements", post(create_engagement))
        .route("/api/engagements/{id}", delete(delete_engagement))
        // Document CRUD
        .route("/api/engagements/{id}/docs", post(create_doc))
        .route("/api/engagements/{id}/docs/{doc_id}", put(update_doc))
        .route(
            "/api/engagements/{id}/docs/{doc_id}",
            delete(delete_doc),
        )
        .with_state(state)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;
    tracing::info!("Celerex API listening on http://localhost:3001");
    axum::serve(listener, app).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Existing handlers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Engagement handlers
// ---------------------------------------------------------------------------

async fn list_engagements(State(state): State<AppState>) -> Json<Value> {
    let engagements = state.lock().unwrap();
    Json(json!(*engagements))
}

async fn create_engagement(
    State(state): State<AppState>,
    Json(body): Json<CreateEngagement>,
) -> impl IntoResponse {
    let engagement = Engagement {
        id: Uuid::new_v4(),
        name: body.name,
        created_at: Utc::now(),
        docs: Vec::new(),
    };
    let mut engagements = state.lock().unwrap();
    engagements.push(engagement.clone());
    (StatusCode::CREATED, Json(json!(engagement)))
}

async fn delete_engagement(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut engagements = state.lock().unwrap();
    let Some(pos) = engagements.iter().position(|e| e.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Engagement not found" })),
        );
    };
    if !engagements[pos].docs.is_empty() {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Cannot delete engagement that still contains documents" })),
        );
    }
    let removed = engagements.remove(pos);
    (StatusCode::OK, Json(json!(removed)))
}

// ---------------------------------------------------------------------------
// Document handlers
// ---------------------------------------------------------------------------

async fn create_doc(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateDoc>,
) -> impl IntoResponse {
    let mut engagements = state.lock().unwrap();
    let Some(engagement) = engagements.iter_mut().find(|e| e.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Engagement not found" })),
        );
    };
    let doc = DocMeta {
        id: Uuid::new_v4(),
        title: body.title,
        engagement_id: id,
        created_at: Utc::now(),
    };
    engagement.docs.push(doc.clone());
    (StatusCode::CREATED, Json(json!(doc)))
}

async fn update_doc(
    State(state): State<AppState>,
    Path((id, doc_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateDoc>,
) -> impl IntoResponse {
    let mut engagements = state.lock().unwrap();
    let Some(engagement) = engagements.iter_mut().find(|e| e.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Engagement not found" })),
        );
    };
    let Some(doc) = engagement.docs.iter_mut().find(|d| d.id == doc_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Document not found" })),
        );
    };
    doc.title = body.title;
    (StatusCode::OK, Json(json!(*doc)))
}

async fn delete_doc(
    State(state): State<AppState>,
    Path((id, doc_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let mut engagements = state.lock().unwrap();
    let Some(engagement) = engagements.iter_mut().find(|e| e.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Engagement not found" })),
        );
    };
    let Some(pos) = engagement.docs.iter().position(|d| d.id == doc_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Document not found" })),
        );
    };
    let removed = engagement.docs.remove(pos);
    (StatusCode::OK, Json(json!(removed)))
}
