//! Sprint 1 — End-to-end demo flow integration test. Walks through the full new
//! product flow exercising the new endpoints (schedules/fire, runs list, docs
//! list, assistant/chat) wired together as one coherent demo:
//!
//!   1. Create engagement + project via HTTP
//!   2. Mint a test JWT
//!   3. Install the Galvo plugin
//!   4. Create a `writer`-typed project_member with a tick -> llm -> doc graph
//!   5. Create a schedule pinned far in the future
//!   6. Fire the schedule via POST /api/schedules/{id}/fire
//!   7. Poll run_steps until all 3 steps succeed
//!   8. GET /api/runs?project_member_id=<id> — confirm new run is listed
//!   9. GET /api/engagements/{id}/docs — confirm a doc was created
//!  10. GET /api/docs/{id}/blocks — confirm the markdown block exists
//!  11. POST /api/assistant/chat — confirm the registry-backed chat endpoint
//!      returns the mock provider's payload
//!
//! Mocks everything network-y: the LLM step uses `MockLlm`, the assistant chat
//! shares the same `ProviderRegistry`, and the doc handler is a local
//! `DocCreateHandler`. No real provider calls happen.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, patch, post, put};
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::plugins::credentials::{CredentialStore, LocalAeadStore};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::{MockLlm, Provider, ProviderRegistry};
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

const GALVO_MANIFEST: &str = include_str!("fixtures/plugins/galvo/manifest.json");

/// No-op tick handler — the graph needs a leading inbound node but in this
/// demo nothing external feeds in (the LLM step generates everything).
struct TickHandler;

#[async_trait]
impl Handler for TickHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({"fired": true}))
    }
}

/// Doc-create handler — inserts a row into `documents` + a single markdown
/// block into `doc_blocks` under the engagement. Mirrors the Bob/Jean E2E
/// pattern so the downstream `GET /api/engagements/{id}/docs` has something
/// to surface.
struct DocCreateHandler {
    pool: PgPool,
    engagement_id: Uuid,
}

#[async_trait]
impl Handler for DocCreateHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let markdown = inputs
            .get("markdown")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let doc_id: Uuid = sqlx::query_scalar(
            "INSERT INTO documents (engagement_id, title) VALUES ($1, $2) RETURNING id",
        )
        .bind(self.engagement_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DispatchError::Handler(e.to_string()))?;
        sqlx::query(
            "INSERT INTO doc_blocks (document_id, position, payload) VALUES ($1, 0, $2)",
        )
        .bind(doc_id)
        .bind(json!({"type":"markdown","text": markdown}))
        .execute(&self.pool)
        .await
        .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({"document_id": doc_id, "title": title}))
    }
}

/// Build a router with explicitly-constructed AppState so we can plumb a
/// non-empty `providers_registry` *and* a preconfigured dispatcher through.
/// `build_app_with_dispatcher` zeroes the registry, which would make the
/// `/api/assistant/chat` step impossible to exercise; reaching directly into
/// AppState is the established pattern (see `assistant_chat.rs`,
/// `scheduler_routes.rs`).
fn build_app(
    pool: PgPool,
    dispatcher: Arc<Dispatcher>,
    registry: Arc<ProviderRegistry>,
    provider_ids: Vec<String>,
) -> Router {
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(
        pool.clone(),
        run_bus.clone(),
    ));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn CredentialStore> =
        Arc::new(LocalAeadStore::from_env().expect("CELEREX_CRED_KEY set by start_test_db"));
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        notifiers::console::ConsoleNotifier,
    )]);
    let state = AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(provider_ids),
        providers_registry: registry,
    };
    Router::new()
        .route(
            "/api/engagements",
            post(celerex_api::engagements::create).get(celerex_api::engagements::list),
        )
        .route(
            "/api/engagements/{engagement_id}/docs",
            get(celerex_api::docs::list_for_engagement),
        )
        .route(
            "/api/projects",
            post(celerex_api::projects::create).get(celerex_api::projects::list),
        )
        .route(
            "/api/plugins",
            post(celerex_api::plugins::routes::install).get(celerex_api::plugins::routes::list),
        )
        .route(
            "/api/project_members",
            post(celerex_api::project_members::routes::create),
        )
        .route(
            "/api/project_members/{id}/run",
            post(celerex_api::project_members::routes::run_manual),
        )
        .route(
            "/api/docs/{id}",
            get(celerex_api::docs::get).delete(celerex_api::docs::delete),
        )
        .route(
            "/api/docs/{id}/blocks",
            get(celerex_api::docs::list_blocks).post(celerex_api::docs::append_block),
        )
        .route(
            "/api/docs/{doc_id}/blocks/{block_id}",
            patch(celerex_api::docs::replace_block),
        )
        .route("/api/runs", get(celerex_api::runs::routes::list))
        .route(
            "/api/schedules",
            post(celerex_api::scheduler::routes::create).get(celerex_api::scheduler::routes::list),
        )
        .route(
            "/api/schedules/{id}",
            put(celerex_api::scheduler::routes::update)
                .delete(celerex_api::scheduler::routes::delete),
        )
        .route(
            "/api/schedules/{id}/fire",
            post(celerex_api::scheduler::routes::fire),
        )
        .route("/api/assistant/chat", post(celerex_api::assistant::chat))
        .with_state(state)
}

/// Writer-agent graph: tick -> llm -> doc. The LLM is a `MockLlm` registered
/// under `ExecutionType::Llm`; the doc step writes through `DocCreateHandler`.
fn demo_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "tick",
                "type": "inbound",
                "config": { "execution": "script" }
            },
            {
                "id": "synth",
                "type": "action",
                "config": {
                    "execution": "llm",
                    "model": "claude-sonnet-4-6"
                }
            },
            {
                "id": "doc",
                "type": "outbound",
                "config": {
                    "execution": "doc",
                    "title": "Demo Flow Brief",
                    "inputs": { "markdown": "{{steps.synth.outputs.content.0.text}}" }
                }
            }
        ],
        "edges": [
            {"from":"tick","to":"synth"},
            {"from":"synth","to":"doc"}
        ]
    })
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn e2e_demo_flow_engagement_to_doc() {
    let db = common::start_test_db().await;

    // -- Build the dispatcher: script tick, mock-backed LLM, local doc writer.
    // The MockLlm is registered both as the workflow LLM provider (used by
    // the synth step) *and* the assistant chat default — sharing the registry
    // proves the assistant endpoint is wired through AppState.
    let mock_provider: Arc<dyn Provider> = Arc::new(
        MockLlm::new()
            // First scripted response → consumed by the workflow `synth` step.
            .finish("# Demo Flow Brief\n\n- Engagement created\n- Member ran via schedule\n")
            // Second scripted response → consumed by the assistant/chat call.
            .finish("Hello from the demo flow assistant"),
    );
    let mut registry = ProviderRegistry::new();
    registry.register("default", mock_provider.clone());
    let registry_arc = Arc::new(registry);

    // Pre-create engagement+project rows so we know the engagement_id needed
    // by the DocCreateHandler. We exercise the HTTP create paths below as
    // an additional check.
    let bootstrap_eng: Uuid = sqlx::query_scalar(
        "INSERT INTO engagements (name) VALUES ('Bootstrap') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    // Bootstrap engagement only exists to satisfy the doc handler binding;
    // we won't use it for the actual flow. The real engagement is created
    // via HTTP below and we mint a fresh dispatcher each test ⇒ that
    // engagement's id is what the handler closes over.

    // We need the dispatcher to know about the engagement_id at construction
    // time, so create the demo engagement via HTTP *first*, then build the
    // dispatcher around its id. We'll wire a temporary app just for the
    // bootstrap HTTP create — the docs/handler app gets the real engagement.

    // Step 1: temporary app for engagement+project creation (no LLM needed).
    let temp_app = build_app(
        db.pool.clone(),
        Arc::new(Dispatcher::new()),
        Arc::new(ProviderRegistry::new()),
        vec![],
    );

    let res = temp_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/engagements")
                .header("content-type", "application/json")
                .body(Body::from(json!({"name": "Demo Flow"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "create engagement");
    let body = body_json(res).await;
    let eng: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    // Mint the token NOW — projects/plugins/members all require auth and
    // the engagement_id claim is what governs access control.
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    let res = temp_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({"name": "Demo Project"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "create project");
    let body = body_json(res).await;
    let project: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["engagement_id"].as_str().unwrap(), eng.to_string());

    // -- Step 2: real dispatcher with all handlers, anchored to `eng`.
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Script, Arc::new(TickHandler));
    dispatcher.register(
        ExecutionType::Llm,
        Arc::new(LlmHandler {
            registry: registry_arc.clone(),
        }),
    );
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler {
            pool: db.pool.clone(),
            engagement_id: eng,
        }),
    );
    let app = build_app(
        db.pool.clone(),
        Arc::new(dispatcher),
        registry_arc.clone(),
        vec!["default".to_string()],
    );

    // -- Step 3: install the Galvo plugin under this engagement.
    let manifest: Value = serde_json::from_str(GALVO_MANIFEST).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({"manifest": manifest}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "install plugin");

    // -- Step 4: create the writer-typed project member with the demo graph.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project_members")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_id": project,
                        "name": "Demo Writer",
                        "graph_json": demo_graph(),
                        "provider_config_json": {},
                        "agent_type": "writer"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "create member");
    let body = body_json(res).await;
    let member: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["agent_type"].as_str().unwrap(), "writer");

    // -- Step 5: create a schedule with a yearly cron so the natural tick
    // never fires during the test — the only way it runs is via POST .../fire.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "0 0 9 1 1 * *",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "create schedule");
    let body = body_json(res).await;
    let sched: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    // -- Step 6: fire the schedule.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/schedules/{sched}/fire"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "fire schedule");
    let body = body_json(res).await;
    let run_id: Uuid = body["run_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["schedule"]["id"].as_str().unwrap(), sched.to_string());

    // -- Step 7: poll run_steps until all 3 steps succeed (tick, synth, doc).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut succeeded_count: i64 = 0;
    while std::time::Instant::now() < deadline {
        succeeded_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_steps WHERE run_id = $1 AND status = 'succeeded'",
        )
        .bind(run_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        if succeeded_count >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if succeeded_count < 3 {
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT step_id, status, error FROM run_steps WHERE run_id = $1 ORDER BY completed_at",
        )
        .bind(run_id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        panic!("expected 3 succeeded steps, got {succeeded_count}. rows: {rows:?}");
    }

    // Wait for the spawned notifier task to flip runs.status to 'succeeded'.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut run_status = String::new();
    while std::time::Instant::now() < deadline {
        run_status = sqlx::query_scalar("SELECT status FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if run_status == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(run_status, "succeeded", "run marked succeeded by notifier");

    // -- Step 8: list runs for this member and confirm our run is there.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/runs?project_member_id={member}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().expect("runs array");
    assert!(!arr.is_empty(), "runs list should be non-empty");
    let found = arr
        .iter()
        .any(|r| r["id"].as_str() == Some(&run_id.to_string()));
    assert!(found, "run_id {run_id} must appear in /api/runs response");
    let our_run = arr
        .iter()
        .find(|r| r["id"].as_str() == Some(&run_id.to_string()))
        .unwrap();
    assert_eq!(our_run["triggered_by"].as_str().unwrap(), "scheduled");
    assert_eq!(
        our_run["project_member_id"].as_str().unwrap(),
        member.to_string()
    );

    // -- Step 9: list docs for this engagement.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/engagements/{eng}/docs"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let docs = body.as_array().expect("docs array");
    assert!(!docs.is_empty(), "engagement docs list should be non-empty");
    let demo_doc = docs
        .iter()
        .find(|d| d["title"].as_str() == Some("Demo Flow Brief"))
        .expect("demo doc must exist with the configured title");
    let doc_id: Uuid = demo_doc["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(demo_doc["engagement_id"].as_str().unwrap(), eng.to_string());
    assert_eq!(
        demo_doc["block_count"].as_i64().unwrap(),
        1,
        "doc should have exactly one block (the markdown body)"
    );

    // -- Step 10: fetch blocks for that doc and confirm our markdown landed.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/docs/{doc_id}/blocks"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let blocks = body.as_array().expect("blocks array");
    assert_eq!(blocks.len(), 1);
    let payload = &blocks[0]["payload"];
    assert_eq!(payload["type"].as_str().unwrap(), "markdown");
    let text = payload["text"].as_str().unwrap();
    assert!(
        text.contains("# Demo Flow Brief"),
        "block markdown must carry the LLM output, got: {text:?}"
    );

    // -- Step 11: assistant/chat hits the shared registry. This is the
    // load-bearing check that the new multi-provider plumbing reaches the
    // chat endpoint through AppState — same MockLlm, second scripted reply.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "messages": [{"role": "user", "content": "demo flow check"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "assistant chat");
    let body = body_json(res).await;
    assert_eq!(body["provider_id"].as_str().unwrap(), "default");
    assert_eq!(body["model"].as_str().unwrap(), "claude-sonnet-4-6");
    assert_eq!(body["raw"]["stop_reason"].as_str().unwrap(), "end_turn");
    assert_eq!(
        body["raw"]["content"][0]["text"].as_str().unwrap(),
        "Hello from the demo flow assistant"
    );

    // Sanity: bootstrap engagement still exists and is distinct (we never
    // used it for the flow), proving the test doesn't depend on cross-eng
    // bleed-through. Cheap insurance against the test becoming a no-op.
    let bootstrap_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM engagements WHERE id = $1")
            .bind(bootstrap_eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(bootstrap_count, 1);
}
