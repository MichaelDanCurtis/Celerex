mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use celerex_api::AppState;
use celerex_api::llm::anthropic::AnthropicClient;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{
    executor::Executor,
    llm::{Provider, ProviderRegistry},
    tool_dispatch::Dispatcher,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

#[derive(serde::Serialize)]
struct TestClaims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

// SAFETY: tests run single-threaded at setup; JWT secret matches start_test_db().
// The assistant is now authenticated + engagement-scoped, so every request needs
// a bearer token. These tests exercise the house-registry path, so they also flip
// CELEREX_HOUSE_MODE on.
fn token(eng: Uuid) -> String {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
        std::env::set_var("CELEREX_HOUSE_MODE", "true");
    }
    let c = TestClaims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

fn build_state(pool: PgPool, registry: Arc<ProviderRegistry>, provider_ids: Vec<String>) -> AppState {
    let dispatcher = Arc::new(Dispatcher::new());
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(
        pool.clone(),
        run_bus.clone(),
    ));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn celerex_api::plugins::credentials::CredentialStore> = Arc::new(
        celerex_api::plugins::credentials::LocalAeadStore::from_env()
            .expect("CELEREX_CRED_KEY must be set"),
    );
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        notifiers::console::ConsoleNotifier,
    )]);
    let provider_keys: Arc<dyn celerex_api::providers::store::ProviderKeysStore> = Arc::new(
        celerex_api::providers::store::PgProviderKeysStore::from_env(pool.clone())
            .expect("CELEREX_CRED_KEY must be set"),
    );
    AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(provider_ids),
        providers_registry: registry,
        provider_keys,
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/assistant/chat", post(celerex_api::assistant::chat))
        .route(
            "/api/assistant/chat/stream",
            post(celerex_api::assistant::chat_stream),
        )
        .with_state(state)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn body_text(res: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn anthropic_text_response(text: &str) -> serde_json::Value {
    json!({
        "id": "msg_test",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": text }]
    })
}

#[tokio::test]
async fn assistant_chat_routes_to_default_provider() {
    let db = common::start_test_db().await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_text_response("hello from default")))
        .mount(&server)
        .await;

    let client = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let provider: Arc<dyn Provider> = Arc::new(client);
    let mut registry = ProviderRegistry::new();
    registry.register("anthropic", provider.clone());
    registry.register("default", provider);
    let state = build_state(
        db.pool.clone(),
        Arc::new(registry),
        vec!["anthropic".to_string(), "default".to_string()],
    );
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // Default provider_id is now "anthropic" (a supported real provider). With no
    // vaulted engagement key and house mode on, it resolves via the house registry.
    assert_eq!(body["provider_id"], "anthropic");
    assert_eq!(body["model"], "claude-sonnet-4-6");
    assert_eq!(body["raw"]["stop_reason"], "end_turn");
    assert_eq!(body["raw"]["content"][0]["text"], "hello from default");
}

#[tokio::test]
async fn assistant_chat_routes_to_explicit_provider() {
    let db = common::start_test_db().await;

    // Two wiremock servers, each returning a distinct payload so we can prove
    // the explicit provider_id picks the right backend.
    let server_alpha = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_text_response("from-alpha")))
        .mount(&server_alpha)
        .await;

    let server_beta = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_text_response("from-beta")))
        .mount(&server_beta)
        .await;

    let alpha: Arc<dyn Provider> = Arc::new(AnthropicClient::with_base_url(
        server_alpha.uri(),
        "alpha-key".into(),
    ));
    let beta: Arc<dyn Provider> = Arc::new(AnthropicClient::with_base_url(
        server_beta.uri(),
        "beta-key".into(),
    ));
    let mut registry = ProviderRegistry::new();
    registry.register("alpha", alpha);
    registry.register("beta", beta);
    let state = build_state(
        db.pool.clone(),
        Arc::new(registry),
        vec!["alpha".to_string(), "beta".to_string()],
    );
    let app = build_router(state);

    // alpha
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "provider_id": "alpha",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["provider_id"], "alpha");
    assert_eq!(body["raw"]["content"][0]["text"], "from-alpha");

    // beta
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "provider_id": "beta",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["provider_id"], "beta");
    assert_eq!(body["raw"]["content"][0]["text"], "from-beta");
}

#[tokio::test]
async fn assistant_chat_missing_provider_returns_400() {
    let db = common::start_test_db().await;

    let registry = ProviderRegistry::new();
    let state = build_state(db.pool.clone(), Arc::new(registry), vec![]);
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "provider_id": "nonexistent",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let text = body_text(res).await;
    assert!(
        text.contains("nonexistent"),
        "error should name the missing provider, got {text:?}"
    );
}

#[tokio::test]
async fn assistant_chat_propagates_provider_error() {
    let db = common::start_test_db().await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(500).set_body_string("upstream blew up"))
        .mount(&server)
        .await;

    let client = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let provider: Arc<dyn Provider> = Arc::new(client);
    let mut registry = ProviderRegistry::new();
    registry.register("default", provider);
    let state = build_state(
        db.pool.clone(),
        Arc::new(registry),
        vec!["default".to_string()],
    );
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "provider_id": "default",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn chat_stream_yields_delta_then_done() {
    let db = common::start_test_db().await;

    // Anthropic-style SSE body with two text deltas + message_stop.
    let sse_body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_x\"}}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n",
        "\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n",
        "\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let client = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let provider: Arc<dyn Provider> = Arc::new(client);
    let mut registry = ProviderRegistry::new();
    registry.register("default", provider);
    let state = build_state(
        db.pool.clone(),
        Arc::new(registry),
        vec!["default".to_string()],
    );
    let app = build_router(state);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat/stream")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(Uuid::new_v4())))
                .body(Body::from(
                    json!({
                        "provider_id": "default",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    // SSE content-type sanity check.
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        ct.starts_with("text/event-stream"),
        "expected SSE content-type, got {ct:?}"
    );

    let body = body_text(res).await;
    // Each named event lands as `event: <name>\ndata: <payload>\n\n`.
    assert!(
        body.contains("event: delta"),
        "expected at least one `delta` event in SSE body, got:\n{body}"
    );
    assert!(
        body.contains("data: Hello"),
        "expected first delta text fragment, got:\n{body}"
    );
    assert!(
        body.contains("data:  world") || body.contains("data: world"),
        "expected second delta text fragment, got:\n{body}"
    );
    assert!(
        body.contains("event: done"),
        "expected terminal `done` event, got:\n{body}"
    );
}
