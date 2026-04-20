//! Sprint 1 — Bob the Researcher E2E. Mirror of Jean's slice but exercises
//! `ExecutionType::Agent` (real A2A via `FixtureA2AClient` serving the Galvo
//! weekly-digest fixture) alongside `ExecutionType::Doc` for the outbound
//! document step. Confirms the dispatcher can host A2A and doc-create in one
//! graph — the collision that motivated adding the `Doc` variant.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use celerex_api::plugins::credentials::LocalAeadStore;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::handlers::agent::AgentHandler;
use celerex_orchestrator::handlers::agent_stub::FixtureA2AClient;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;

const GALVO_MANIFEST: &str = include_str!("fixtures/plugins/galvo/manifest.json");
const GALVO_FIXTURE_MD: &str = include_str!("fixtures/plugins/galvo/research_weekly.md");

/// No-op tick inbound; the graph needs a leading node but there's nothing to
/// fetch for Bob (Galvo is itself the data source).
struct TickHandler;

#[async_trait]
impl Handler for TickHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({"fired": true}))
    }
}

/// Registered under `ExecutionType::Doc`: inserts a row into `documents`
/// plus a single markdown block into `doc_blocks`. Same shape Jean uses.
struct DocCreateHandler {
    pool: PgPool,
    engagement_id: uuid::Uuid,
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
        let doc_id: uuid::Uuid = sqlx::query_scalar(
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

fn build_dispatcher(pool: PgPool, engagement_id: uuid::Uuid) -> Arc<Dispatcher> {
    let a2a = Arc::new(
        FixtureA2AClient::new().with_fixture("galvo.research.weekly", GALVO_FIXTURE_MD),
    );
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Script, Arc::new(TickHandler));
    dispatcher.register(ExecutionType::Agent, Arc::new(AgentHandler { client: a2a }));
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler { pool, engagement_id }),
    );
    Arc::new(dispatcher)
}

fn bob_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "tick",
                "type": "inbound",
                "config": { "execution": "script" }
            },
            {
                "id": "galvo",
                "type": "action",
                "config": {
                    "execution": "agent",
                    "target": "galvo.research.weekly"
                }
            },
            {
                "id": "doc",
                "type": "outbound",
                "config": {
                    "execution": "doc",
                    "title": "Weekly Research Digest",
                    "inputs": { "markdown": GALVO_FIXTURE_MD }
                }
            }
        ],
        "edges": [
            {"from":"tick","to":"galvo"},
            {"from":"galvo","to":"doc"}
        ]
    })
}

#[tokio::test]
async fn e2e_bob_researcher_end_to_end() {
    let db = common::start_test_db().await;

    let eng: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO engagements (name) VALUES ('AiLuxi R&D') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let project: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Bob') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let dispatcher = build_dispatcher(db.pool.clone(), eng);
    let creds = Arc::new(
        LocalAeadStore::from_env().expect("CELEREX_CRED_KEY set by start_test_db"),
    );
    let app = celerex_api::build_app_with_dispatcher(db.pool.clone(), creds, dispatcher);
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    // 1. Install the Galvo plugin so the outbound capability is listed under
    //    this engagement — same flow Bob would use in the UI before wiring
    //    the node into his graph.
    let manifest: Value = serde_json::from_str(GALVO_MANIFEST).unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(json!({ "manifest": manifest }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "install galvo plugin");

    // 2. Create a member with Bob's graph (tick → galvo A2A → doc outbound).
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
                        "name": "Bob — Weekly Research Digest",
                        "graph_json": bob_graph(),
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "create member");
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    // 3. Fire manually.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/project_members/{member_id}/run"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED, "run manual");
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    let run_id = body["run_id"].as_str().unwrap().to_string();
    let run_uuid = uuid::Uuid::parse_str(&run_id).unwrap();

    // 4. Poll run_steps until all 3 steps succeed (tick, galvo, doc).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut succeeded_count: i64 = 0;
    while std::time::Instant::now() < deadline {
        succeeded_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_steps WHERE run_id = $1 AND status = 'succeeded'",
        )
        .bind(run_uuid)
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
        .bind(run_uuid)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        panic!("expected 3 succeeded steps, got {succeeded_count}. rows: {rows:?}");
    }

    // 5. Wait for the notifier task to flip runs.status; confirm trigger/source.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut run_row: (Option<uuid::Uuid>, String, String) = (None, String::new(), String::new());
    while std::time::Instant::now() < deadline {
        run_row = sqlx::query_as(
            "SELECT snapshot_id, triggered_by, status FROM runs WHERE id = $1",
        )
        .bind(run_uuid)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        if run_row.2 == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(run_row.0.is_some(), "snapshot_id is set on the run");
    assert_eq!(run_row.1, "manual", "triggered_by = manual");
    assert_eq!(run_row.2, "succeeded", "run marked succeeded by notifier task");

    // 6. The Galvo A2A output must have been recorded — fixture payload reached
    //    the step record. This is the load-bearing check that `ExecutionType::Agent`
    //    is routing through the real AgentHandler and not the doc-create shim.
    let galvo_output: Option<Value> = sqlx::query_scalar(
        "SELECT outputs FROM run_steps WHERE run_id = $1 AND step_id = 'galvo'",
    )
    .bind(run_uuid)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let galvo_output = galvo_output.expect("galvo step has outputs");
    let result = galvo_output["result"].as_str().expect("A2A result string");
    assert!(
        result.contains("# Weekly Research Digest"),
        "Galvo fixture payload must reach the step output: {galvo_output}"
    );

    // 7. Digest document created under the engagement with expected title
    //    and a markdown block carrying the fixture content.
    let (doc_id, doc_title): (uuid::Uuid, String) = sqlx::query_as(
        "SELECT id, title FROM documents WHERE engagement_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(doc_title, "Weekly Research Digest");
    let block_payload: Value = sqlx::query_scalar(
        "SELECT payload FROM doc_blocks WHERE document_id = $1 ORDER BY position LIMIT 1",
    )
    .bind(doc_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(block_payload["type"], "markdown");
    assert!(
        block_payload["text"]
            .as_str()
            .unwrap_or("")
            .contains("# Weekly Research Digest"),
        "digest markdown block must contain the Galvo fixture body"
    );

    // 8. Fleet endpoint reports Bob's member succeeded.
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/project_members/fleet")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let rows: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == member_id)
        .unwrap();
    assert_eq!(row["last_status"], "succeeded");
    assert_eq!(row["running"], false);
}
