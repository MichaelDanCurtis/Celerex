use axum::{
    Router,
    extract::{Path, State},
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
};
use futures_util::{SinkExt, StreamExt};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::sync::{Arc, Mutex};
use axum::http::{HeaderValue, Method};
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
struct ChatRequest {
    message: String,
    context: Option<String>,
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
        // WebSocket endpoint (will be extended for Yjs document sync)
        .route("/api/ws", get(ws_handler))
        // Chat proxy
        .route("/api/chat", post(chat_handler))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap())
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                ]),
        );

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

// ---------------------------------------------------------------------------
// WebSocket handler — echo server, to be extended for Yjs document sync
// ---------------------------------------------------------------------------

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws_connection)
}

async fn handle_ws_connection(socket: WebSocket) {
    tracing::info!("WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("WebSocket receive error: {e}");
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                if sender.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            Message::Binary(data) => {
                if sender.send(Message::Binary(data)).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => {
                break;
            }
            // Ping/Pong are handled automatically by axum/tungstenite
            _ => {}
        }
    }

    tracing::info!("WebSocket client disconnected");
}

// ---------------------------------------------------------------------------
// Chat proxy handler
// ---------------------------------------------------------------------------

async fn chat_handler(Json(body): Json<ChatRequest>) -> impl IntoResponse {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "ANTHROPIC_API_KEY is not set" })),
            );
        }
    };

    let user_content = match &body.context {
        Some(ctx) if !ctx.is_empty() => format!("{}\n\n{}", ctx, body.message),
        _ => body.message.clone(),
    };

    let request_body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": user_content
            }
        ]
    });

    let client = reqwest::Client::new();
    let response = match client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to reach Anthropic API: {}", e) })),
            );
        }
    };

    let status = response.status();
    let resp_body: Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to parse Anthropic response: {}", e) })),
            );
        }
    };

    if !status.is_success() {
        let api_error = resp_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown Anthropic API error");
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({ "error": api_error })),
        );
    }

    let reply = resp_body
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    (StatusCode::OK, Json(json!({ "reply": reply })))
}
