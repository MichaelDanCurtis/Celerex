//! Human-in-the-loop Review & Approve, end-to-end.
//!
//! A flow `synth -> review -> doc` must PAUSE at the review node:
//!   * the run goes `awaiting_review` and the doc is NOT written;
//!   * `GET /api/approvals` surfaces the draft;
//!   * `POST /api/runs/{id}/review` with `approve` + edited content resumes the
//!     run, the gate opens, and the doc is written with the EDITED content;
//!   * a second run that is `reject`ed resumes, the gate closes, and no doc is
//!     written — yet the run still completes (succeeded).

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::plugins::credentials::{CredentialStore, LocalAeadStore};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::llm::ProviderRegistry;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

/// Synth step: emits a fixed draft body the review node will show the human.
struct SynthHandler;
#[async_trait]
impl Handler for SynthHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({ "text": "DRAFT BODY from the agent" }))
    }
}

/// Doc step: writes the (approved) markdown into `documents` + `doc_blocks`.
struct DocCreateHandler {
    pool: PgPool,
    engagement_id: Uuid,
}
#[async_trait]
impl Handler for DocCreateHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let title = config.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
        let markdown = inputs.get("markdown").and_then(|v| v.as_str()).unwrap_or("");
        let doc_id: Uuid = sqlx::query_scalar(
            "INSERT INTO documents (engagement_id, title) VALUES ($1, $2) RETURNING id",
        )
        .bind(self.engagement_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DispatchError::Handler(e.to_string()))?;
        sqlx::query("INSERT INTO doc_blocks (document_id, position, payload) VALUES ($1, 0, $2)")
            .bind(doc_id)
            .bind(json!({ "type": "markdown", "text": markdown }))
            .execute(&self.pool)
            .await
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({ "document_id": doc_id, "title": title }))
    }
}

fn build_app(pool: PgPool, dispatcher: Arc<Dispatcher>) -> Router {
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(pool.clone(), run_bus.clone()));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn CredentialStore> =
        Arc::new(LocalAeadStore::from_env().expect("CELEREX_CRED_KEY set by start_test_db"));
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> =
        Arc::new(vec![Arc::new(notifiers::console::ConsoleNotifier)]);
    let provider_keys: Arc<dyn celerex_api::providers::store::ProviderKeysStore> = Arc::new(
        celerex_api::providers::store::PgProviderKeysStore::from_env(pool.clone())
            .expect("CELEREX_CRED_KEY must be set"),
    );
    let state = AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(vec![]),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
    };
    Router::new()
        .route("/api/projects", post(celerex_api::projects::create))
        .route("/api/project_members", post(celerex_api::project_members::routes::create))
        .route(
            "/api/project_members/{id}/run",
            post(celerex_api::project_members::routes::run_manual),
        )
        .route("/api/approvals", get(celerex_api::reviews::list_approvals))
        .route("/api/runs/{run_id}/review", post(celerex_api::reviews::submit_review))
        .with_state(state)
}

/// synth -> review -> doc. The review draft is the synth output; the doc body is
/// the review's (possibly edited) approved content.
fn review_graph() -> Value {
    json!({
        "nodes": [
            { "id": "synth", "type": "action", "config": { "execution": "script" } },
            {
                "id": "review",
                "type": "filter",
                "config": {
                    "execution": "review",
                    "prompt": "Approve the newsletter draft",
                    "content": "{{steps.synth.outputs.text}}"
                }
            },
            {
                "id": "doc",
                "type": "outbound",
                "config": {
                    "execution": "doc",
                    "title": "Reviewed Doc",
                    "inputs": { "markdown": "{{steps.review.outputs.content}}" }
                }
            }
        ],
        "edges": [
            { "from": "synth", "to": "review" },
            { "from": "review", "to": "doc" }
        ]
    })
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn poll_run_status(pool: &PgPool, run_id: Uuid, want: &str, secs: u64) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut status = String::new();
    while std::time::Instant::now() < deadline {
        status = sqlx::query_scalar("SELECT status FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(pool)
            .await
            .unwrap();
        if status == want {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
    status
}

async fn doc_count(pool: &PgPool, eng: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE engagement_id = $1")
        .bind(eng)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn start_run(app: &Router, token: &str, member: Uuid) -> Uuid {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/project_members/{member}/run"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED, "start run");
    body_json(res).await["run_id"].as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn review_pauses_then_approve_publishes_edited_content() {
    let db = common::start_test_db().await;
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Rev') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Script, Arc::new(SynthHandler));
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler { pool: db.pool.clone(), engagement_id: eng }),
    );
    let app = build_app(db.pool.clone(), Arc::new(dispatcher));

    // Project + member with the review graph.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "name": "P" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let project: Uuid = body_json(res).await["id"].as_str().unwrap().parse().unwrap();

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
                        "name": "Reviewer Flow",
                        "graph_json": review_graph(),
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let member: Uuid = body_json(res).await["id"].as_str().unwrap().parse().unwrap();

    // ---- Run #1: approve with edited content ---------------------------------
    let run1 = start_run(&app, &token, member).await;
    let status = poll_run_status(&db.pool, run1, "awaiting_review", 10).await;
    assert_eq!(status, "awaiting_review", "run must pause at the review node");
    assert_eq!(doc_count(&db.pool, eng).await, 0, "doc must NOT be written before approval");

    // Approvals inbox surfaces the draft.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/approvals")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let items = body_json(res).await;
    let arr = items.as_array().unwrap();
    assert_eq!(arr.len(), 1, "exactly one pending approval");
    assert_eq!(arr[0]["run_id"].as_str().unwrap(), run1.to_string());
    assert_eq!(arr[0]["draft"].as_str().unwrap(), "DRAFT BODY from the agent");
    assert_eq!(arr[0]["prompt"].as_str().unwrap(), "Approve the newsletter draft");

    // Approve with an EDIT — the edited content is what must reach the doc.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/runs/{run1}/review"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({ "decision": "approve", "content": "EDITED BY HUMAN" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let ack = body_json(res).await;
    assert_eq!(ack["approved"].as_bool().unwrap(), true);

    let status = poll_run_status(&db.pool, run1, "succeeded", 10).await;
    assert_eq!(status, "succeeded", "approved run resumes to completion");
    assert_eq!(doc_count(&db.pool, eng).await, 1, "doc written after approval");
    let text: String = sqlx::query_scalar(
        "SELECT payload->>'text' FROM doc_blocks db
         JOIN documents d ON d.id = db.document_id WHERE d.engagement_id = $1",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(text, "EDITED BY HUMAN", "edited content flows downstream");

    // ---- Run #2: reject — gate closes, no doc, but run completes -------------
    let run2 = start_run(&app, &token, member).await;
    let status = poll_run_status(&db.pool, run2, "awaiting_review", 10).await;
    assert_eq!(status, "awaiting_review");

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/runs/{run2}/review"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "decision": "reject" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(res).await["approved"].as_bool().unwrap(), false);

    let status = poll_run_status(&db.pool, run2, "succeeded", 10).await;
    assert_eq!(status, "succeeded", "rejected run still completes (gate closed)");
    assert_eq!(
        doc_count(&db.pool, eng).await,
        1,
        "still only the first doc — rejection skipped the publish step"
    );
}
