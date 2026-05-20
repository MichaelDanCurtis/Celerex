mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use celerex_api::plugins::credentials::LocalAeadStore;
use celerex_api::plugins::fixtures::workiq::WorkIqFixture;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::MockLlm;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;

/// Registered under `ExecutionType::Http`: proxies to the WorkIQ fixture.
struct WorkIqHandler {
    fixture: WorkIqFixture,
}

#[async_trait]
impl Handler for WorkIqHandler {
    async fn invoke(&self, _config: Value, inputs: Value) -> Result<Value, DispatchError> {
        self.fixture
            .invoke("workiq.pull_metrics", inputs)
            .await
            .map_err(DispatchError::Handler)
    }
}

/// Registered under `ExecutionType::Script`: no-op tick trigger.
struct TickHandler;

#[async_trait]
impl Handler for TickHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({"fired": true}))
    }
}

/// Registered under `ExecutionType::Agent`: inserts a row into `documents`
/// plus a single markdown block into `doc_blocks`.
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
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(
        ExecutionType::Script,
        Arc::new(TickHandler),
    );
    dispatcher.register(
        ExecutionType::Http,
        Arc::new(WorkIqHandler {
            fixture: WorkIqFixture::from_fixture_dir(),
        }),
    );
    dispatcher.register(
        ExecutionType::Llm,
        Arc::new(LlmHandler::new(Arc::new(
            MockLlm::new()
                .finish("# Weekly Newsletter\n\n- Emails sent: 1284\n- Meetings: 32\n"),
        ))),
    );
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler { pool, engagement_id }),
    );
    Arc::new(dispatcher)
}

fn jean_graph() -> Value {
    json!({
        "nodes": [
            {
                "id": "tick",
                "type": "inbound",
                "config": { "execution": "script" }
            },
            {
                "id": "workiq",
                "type": "action",
                "config": { "execution": "http", "capability": "workiq.pull_metrics" }
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
                    "title": "Weekly Newsletter",
                    "inputs": { "markdown": "{{steps.synth.outputs.content.0.text}}" }
                }
            }
        ],
        "edges": [
            {"from":"tick","to":"workiq"},
            {"from":"workiq","to":"synth"},
            {"from":"synth","to":"doc"}
        ]
    })
}

#[tokio::test]
async fn e2e_jean_newsletter_end_to_end() {
    let db = common::start_test_db().await;

    let eng: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let project: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id",
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

    // 1. Create the member with the Jean graph.
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
                        "name": "Jean — Weekly Newsletter",
                        "graph_json": jean_graph(),
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

    // 2. Fire it manually.
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

    // 3. Poll `run_steps` until all 4 steps succeed, or timeout.
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
        if succeeded_count >= 4 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // Diagnostic: dump any failures for easier debugging.
    if succeeded_count < 4 {
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT step_id, status, error FROM run_steps WHERE run_id = $1 ORDER BY completed_at",
        )
        .bind(run_uuid)
        .fetch_all(&db.pool)
        .await
        .unwrap();
        panic!("expected 4 succeeded steps, got {succeeded_count}. rows: {rows:?}");
    }

    // 4. The run row itself should be marked succeeded + carry snapshot_id + manual trigger.
    // The status update happens inside the spawned notifier task — wait briefly for it.
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

    // 5. A document was created under this engagement with the expected title,
    //    and has a doc_blocks row carrying the markdown.
    let (doc_id, doc_title): (uuid::Uuid, String) = sqlx::query_as(
        "SELECT id, title FROM documents WHERE engagement_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(doc_title, "Weekly Newsletter");
    let block_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM doc_blocks WHERE document_id = $1",
    )
    .bind(doc_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(block_count, 1, "newsletter markdown block present");

    // 6. Fleet endpoint reports this member with last_status = succeeded.
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
    let row = rows.as_array().unwrap().iter().find(|r| r["id"] == member_id).unwrap();
    assert_eq!(row["last_status"], "succeeded");
    assert_eq!(row["running"], false);
}
