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
    provider: Option<String>,  // "anthropic", "github-copilot", "openrouter", "zhipu", "kimi"
    model: Option<String>,     // model ID override
    api_key: Option<String>,   // API key from frontend (fallback: env var)
}

#[derive(Debug, Deserialize)]
struct CreateDoc {
    title: String,
}

#[derive(Debug, Deserialize)]
struct UpdateDoc {
    title: String,
}

#[derive(Debug, Deserialize)]
struct CopilotPollRequest {
    device_code: String,
}

// ---------------------------------------------------------------------------
// Copilot state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct CopilotState {
    github_token: Option<String>,
    copilot_token: Option<String>,
    copilot_token_expires_at: Option<i64>,
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    engagements: Arc<Mutex<Vec<Engagement>>>,
    copilot: Arc<Mutex<CopilotState>>,
}

// ---------------------------------------------------------------------------
// Copilot constants
// ---------------------------------------------------------------------------

const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_SCOPE: &str = "read:user";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.104.3";
const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const COPILOT_API_VERSION: &str = "2025-04-01";

// ---------------------------------------------------------------------------
// Copilot token persistence helpers
// ---------------------------------------------------------------------------

fn copilot_token_path() -> std::path::PathBuf {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~/.local/share"));
    data_dir.join("copilot-api").join("github_token")
}

fn load_github_token_from_disk() -> Option<String> {
    let path = copilot_token_path();
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn save_github_token_to_disk(token: &str) -> Result<(), String> {
    let path = copilot_token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    std::fs::write(&path, token)
        .map_err(|e| format!("Failed to write token file: {}", e))?;

    // Set file permissions to owner-only on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set file permissions: {}", e))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Copilot token exchange
// ---------------------------------------------------------------------------

async fn exchange_copilot_token(github_token: &str) -> Result<(String, i64), String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("authorization", format!("token {}", github_token))
        .header("accept", "application/json")
        .header("editor-version", COPILOT_EDITOR_VERSION)
        .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
        .header("user-agent", COPILOT_USER_AGENT)
        .header("x-github-api-version", COPILOT_API_VERSION)
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub API: {}", e))?;

    let status = resp.status();
    let body: Value = resp.json().await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "GitHub token exchange failed ({}): {}",
            status,
            body.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error")
        ));
    }

    let token = body.get("token")
        .and_then(|t| t.as_str())
        .ok_or("Missing 'token' in response")?
        .to_string();
    let expires_at = body.get("expires_at")
        .and_then(|e| e.as_i64())
        .ok_or("Missing 'expires_at' in response")?;

    Ok((token, expires_at))
}

/// Ensure we have a valid Copilot token, refreshing if needed.
/// Returns the token string or an error message.
async fn ensure_copilot_token(copilot_state: &Arc<Mutex<CopilotState>>) -> Result<String, String> {
    let (github_token, copilot_token, expires_at) = {
        let state = copilot_state.lock().unwrap();
        (
            state.github_token.clone(),
            state.copilot_token.clone(),
            state.copilot_token_expires_at,
        )
    };

    // If we have a valid (non-expired) copilot token, return it
    let now = Utc::now().timestamp();
    if let (Some(token), Some(exp)) = (&copilot_token, expires_at) {
        if now < exp - 60 {
            // Token is still valid (with 60s buffer)
            return Ok(token.clone());
        }
    }

    // Need to refresh — must have a github token
    let github_token = github_token
        .ok_or("Not authenticated with GitHub Copilot. Use POST /api/copilot/auth/start to begin authentication.")?;

    let (new_token, new_expires_at) = exchange_copilot_token(&github_token).await?;

    // Store the refreshed token
    {
        let mut state = copilot_state.lock().unwrap();
        state.copilot_token = Some(new_token.clone());
        state.copilot_token_expires_at = Some(new_expires_at);
    }

    Ok(new_token)
}

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

    // Initialize Copilot state — try loading persisted GitHub token
    let mut copilot_state = CopilotState::default();
    if let Some(token) = load_github_token_from_disk() {
        tracing::info!("Found persisted GitHub token, will exchange for Copilot token");
        copilot_state.github_token = Some(token);
    }

    let state = AppState {
        engagements: Arc::new(Mutex::new(vec![default_engagement])),
        copilot: Arc::new(Mutex::new(copilot_state)),
    };

    // If we have a GitHub token, try to exchange it on startup
    {
        let copilot = state.copilot.clone();
        let has_github_token = copilot.lock().unwrap().github_token.is_some();
        if has_github_token {
            tokio::spawn(async move {
                match ensure_copilot_token(&copilot).await {
                    Ok(_) => tracing::info!("Successfully obtained Copilot token on startup"),
                    Err(e) => tracing::warn!("Failed to obtain Copilot token on startup: {}", e),
                }
            });
        }
    }

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
        // Copilot auth routes
        .route("/api/copilot/auth/start", post(copilot_auth_start))
        .route("/api/copilot/auth/poll", post(copilot_auth_poll))
        .route("/api/copilot/auth/status", get(copilot_auth_status))
        // Copilot models
        .route("/api/copilot/models", get(copilot_models))
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
    let engagements = state.engagements.lock().unwrap();
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
    let mut engagements = state.engagements.lock().unwrap();
    engagements.push(engagement.clone());
    (StatusCode::CREATED, Json(json!(engagement)))
}

async fn delete_engagement(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut engagements = state.engagements.lock().unwrap();
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
    let mut engagements = state.engagements.lock().unwrap();
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
    let mut engagements = state.engagements.lock().unwrap();
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
    let mut engagements = state.engagements.lock().unwrap();
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
// WebSocket handler -- echo server, to be extended for Yjs document sync
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
// Copilot auth handlers
// ---------------------------------------------------------------------------

/// POST /api/copilot/auth/start - Start the GitHub Device OAuth flow
async fn copilot_auth_start() -> impl IntoResponse {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://github.com/login/device/code")
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .json(&json!({
            "client_id": COPILOT_CLIENT_ID,
            "scope": COPILOT_SCOPE
        }))
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to start device flow: {}", e) })),
            );
        }
    };

    let status = resp.status();
    let body: Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to parse device flow response: {}", e) })),
            );
        }
    };

    if !status.is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("GitHub device flow error: {}", body) })),
        );
    }

    let device_code = body.get("device_code").and_then(|v| v.as_str()).unwrap_or("");
    let user_code = body.get("user_code").and_then(|v| v.as_str()).unwrap_or("");
    let verification_uri = body.get("verification_uri").and_then(|v| v.as_str()).unwrap_or("");

    (
        StatusCode::OK,
        Json(json!({
            "device_code": device_code,
            "user_code": user_code,
            "verification_uri": verification_uri
        })),
    )
}

/// POST /api/copilot/auth/poll - Poll for device flow completion
async fn copilot_auth_poll(
    State(state): State<AppState>,
    Json(body): Json<CopilotPollRequest>,
) -> impl IntoResponse {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .json(&json!({
            "client_id": COPILOT_CLIENT_ID,
            "device_code": body.device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
        }))
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to poll OAuth: {}", e) })),
            );
        }
    };

    let poll_body: Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to parse OAuth response: {}", e) })),
            );
        }
    };

    // Check if we got an access token
    if let Some(access_token) = poll_body.get("access_token").and_then(|v| v.as_str()) {
        let token = access_token.to_string();

        // Save to disk
        if let Err(e) = save_github_token_to_disk(&token) {
            tracing::warn!("Failed to persist GitHub token: {}", e);
        }

        // Store in state
        {
            let mut copilot = state.copilot.lock().unwrap();
            copilot.github_token = Some(token.clone());
        }

        // Exchange for Copilot token
        match exchange_copilot_token(&token).await {
            Ok((copilot_token, expires_at)) => {
                let mut copilot = state.copilot.lock().unwrap();
                copilot.copilot_token = Some(copilot_token);
                copilot.copilot_token_expires_at = Some(expires_at);
            }
            Err(e) => {
                tracing::warn!("GitHub token obtained but Copilot token exchange failed: {}", e);
                // Not fatal -- we have the github token and can retry later
            }
        }

        return (
            StatusCode::OK,
            Json(json!({ "status": "complete" })),
        );
    }

    // Check for known pending/slow_down errors
    let error = poll_body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    match error {
        "authorization_pending" | "slow_down" => {
            (StatusCode::OK, Json(json!({ "status": "pending" })))
        }
        "expired_token" => {
            (StatusCode::GONE, Json(json!({ "error": "Device code expired. Please restart authentication." })))
        }
        "access_denied" => {
            (StatusCode::FORBIDDEN, Json(json!({ "error": "User denied access." })))
        }
        _ => {
            (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("OAuth error: {}", error) })))
        }
    }
}

/// GET /api/copilot/auth/status - Check if authenticated
async fn copilot_auth_status(State(state): State<AppState>) -> Json<Value> {
    let copilot = state.copilot.lock().unwrap();
    let authenticated = copilot.github_token.is_some();
    let has_copilot_token = copilot.copilot_token.is_some();
    let expired = copilot.copilot_token_expires_at
        .map(|exp| Utc::now().timestamp() >= exp - 60)
        .unwrap_or(true);

    Json(json!({
        "authenticated": authenticated,
        "copilot_token_active": has_copilot_token && !expired
    }))
}

/// GET /api/copilot/models - List available Copilot models
async fn copilot_models(State(state): State<AppState>) -> impl IntoResponse {
    let copilot_token = match ensure_copilot_token(&state.copilot).await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": e })),
            );
        }
    };

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.githubcopilot.com/models")
        .header("Authorization", format!("Bearer {}", copilot_token))
        .header("content-type", "application/json")
        .header("copilot-integration-id", "vscode-chat")
        .header("editor-version", COPILOT_EDITOR_VERSION)
        .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
        .header("user-agent", COPILOT_USER_AGENT)
        .header("x-github-api-version", COPILOT_API_VERSION)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to fetch models: {}", e) })),
            );
        }
    };

    let status = resp.status();
    let body: Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to parse models response: {}", e) })),
            );
        }
    };

    if !status.is_success() {
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({ "error": format!("Models API error: {}", body) })),
        );
    }

    (StatusCode::OK, Json(body))
}

// ---------------------------------------------------------------------------
// Chat proxy handler -- multi-provider
// ---------------------------------------------------------------------------

/// Resolve the API key: prefer the request-supplied key, then fall back to env.
fn resolve_api_key(request_key: &Option<String>, env_var: &str, provider_label: &str)
    -> Result<String, (StatusCode, Json<Value>)>
{
    if let Some(k) = request_key {
        if !k.is_empty() {
            return Ok(k.clone());
        }
    }
    env::var(env_var).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("API key for provider '{}' is not set. Supply it in the request body or set the {} environment variable.", provider_label, env_var) })),
        )
    })
}

/// Send a request to the Anthropic Messages API and return the reply text.
async fn send_anthropic(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    user_content: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let request_body = json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [
            { "role": "user", "content": user_content }
        ]
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to reach Anthropic API: {}", e) })),
            )
        })?;

    let status = response.status();
    let resp_body: Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to parse Anthropic response: {}", e) })),
        )
    })?;

    if !status.is_success() {
        let api_error = resp_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown Anthropic API error");
        return Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({ "error": api_error })),
        ));
    }

    let reply = resp_body
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok(reply)
}

/// Send a request to any OpenAI-compatible chat completions endpoint.
async fn send_openai_compatible(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    user_content: &str,
    extra_headers: Vec<(&str, &str)>,
) -> Result<String, (StatusCode, Json<Value>)> {
    let request_body = json!({
        "model": model,
        "messages": [
            { "role": "user", "content": user_content }
        ],
        "max_tokens": 1024
    });

    let mut req = client
        .post(base_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json");

    for (name, value) in &extra_headers {
        req = req.header(*name, *value);
    }

    let response = req.json(&request_body).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to reach provider API: {}", e) })),
        )
    })?;

    let status = response.status();
    let resp_body: Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to parse provider response: {}", e) })),
        )
    })?;

    if !status.is_success() {
        let api_error = resp_body
            .get("error")
            .and_then(|e| e.get("message").or_else(|| Some(e)))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown provider API error");
        return Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({ "error": api_error })),
        ));
    }

    let reply = resp_body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok(reply)
}

/// Send a request to GitHub Copilot chat completions endpoint.
async fn send_copilot(
    client: &reqwest::Client,
    copilot_token: &str,
    model: &str,
    user_content: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let request_id = Uuid::new_v4().to_string();
    let request_body = json!({
        "model": model,
        "messages": [
            { "role": "user", "content": user_content }
        ],
        "max_tokens": 1024
    });

    let response = client
        .post("https://api.githubcopilot.com/chat/completions")
        .header("Authorization", format!("Bearer {}", copilot_token))
        .header("content-type", "application/json")
        .header("copilot-integration-id", "vscode-chat")
        .header("editor-version", COPILOT_EDITOR_VERSION)
        .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
        .header("user-agent", COPILOT_USER_AGENT)
        .header("openai-intent", "conversation-panel")
        .header("x-github-api-version", COPILOT_API_VERSION)
        .header("x-request-id", &request_id)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Failed to reach Copilot API: {}", e) })),
            )
        })?;

    let status = response.status();
    let resp_body: Value = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("Failed to parse Copilot response: {}", e) })),
        )
    })?;

    if !status.is_success() {
        let api_error = resp_body
            .get("error")
            .and_then(|e| e.get("message").or_else(|| Some(e)))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown Copilot API error");
        return Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({ "error": api_error })),
        ));
    }

    let reply = resp_body
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Ok(reply)
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> impl IntoResponse {
    let provider = body.provider.as_deref().unwrap_or("anthropic");

    // Build user content (prepend context if present)
    let user_content = match &body.context {
        Some(ctx) if !ctx.is_empty() => format!("{}\n\n{}", ctx, body.message),
        _ => body.message.clone(),
    };

    let client = reqwest::Client::new();

    // Handle github-copilot provider separately (uses Copilot token, not API key)
    if provider == "github-copilot" {
        let model = body.model.as_deref().unwrap_or("gpt-4o");

        let copilot_token = match ensure_copilot_token(&state.copilot).await {
            Ok(t) => t,
            Err(e) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": e })),
                );
            }
        };

        let result = send_copilot(&client, &copilot_token, model, &user_content).await;
        return match result {
            Ok(reply) => (StatusCode::OK, Json(json!({ "reply": reply }))),
            Err(e) => e,
        };
    }

    // All other providers use API key auth
    let (default_model, base_url, env_var) = match provider {
        "anthropic" => (
            "claude-sonnet-4-20250514",
            "https://api.anthropic.com/v1/messages",
            "ANTHROPIC_API_KEY",
        ),
        "openrouter" => (
            "anthropic/claude-sonnet-4-20250514",
            "https://openrouter.ai/api/v1/chat/completions",
            "OPENROUTER_API_KEY",
        ),
        "zhipu" => (
            "glm-5",
            "https://open.bigmodel.cn/api/paas/v4/chat/completions",
            "ZHIPUAI_API_KEY",
        ),
        "xai" => (
            "grok-3",
            "https://api.x.ai/v1/chat/completions",
            "XAI_API_KEY",
        ),
        "kimi" => (
            "kimi-k2-0711",
            "https://api.moonshot.cn/v1/chat/completions",
            "MOONSHOT_API_KEY",
        ),
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Unknown provider: '{}'. Supported: anthropic, github-copilot, openrouter, zhipu, xai, kimi", other) })),
            );
        }
    };

    let key = match resolve_api_key(&body.api_key, env_var, provider) {
        Ok(k) => k,
        Err(e) => return e,
    };

    let model = body.model.as_deref().unwrap_or(default_model);

    let result = if provider == "anthropic" {
        send_anthropic(&client, &key, model, &user_content).await
    } else {
        // Build extra headers per provider
        let extra_headers: Vec<(&str, &str)> = match provider {
            "openrouter" => vec![
                ("HTTP-Referer", "http://localhost:5173"),
                ("X-Title", "Celerex"),
            ],
            _ => vec![],
        };
        send_openai_compatible(&client, base_url, &key, model, &user_content, extra_headers).await
    };

    match result {
        Ok(reply) => (StatusCode::OK, Json(json!({ "reply": reply }))),
        Err(e) => e,
    }
}
