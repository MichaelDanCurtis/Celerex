mod common;

use std::sync::Arc;

use async_trait::async_trait;
use celerex_api::notifiers::Notifier;
use celerex_api::project_members::run_job::{self, TRIGGERED_BY_SCHEDULED};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::PgRunStore;
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::MockLlm;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;

struct NoopHandler;

#[async_trait]
impl Handler for NoopHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({}))
    }
}

struct WorkIqHandler {
    fixture: celerex_api::plugins::fixtures::workiq::WorkIqFixture,
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

struct DocCreateHandler {
    pool: PgPool,
    engagement_id: uuid::Uuid,
}

#[async_trait]
impl Handler for DocCreateHandler {
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let doc_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO documents (engagement_id, title) VALUES ($1, $2) RETURNING id",
        )
        .bind(self.engagement_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({"document_id": doc_id}))
    }
}

fn jean_graph() -> Value {
    json!({
        "nodes": [
            {"id":"tick",   "type":"inbound",  "config":{"execution":"script"}},
            {"id":"workiq", "type":"action",   "config":{"execution":"http"}},
            {"id":"synth",  "type":"action",   "config":{"execution":"llm","model":"claude-sonnet-4-6"}},
            {"id":"doc",    "type":"outbound", "config":{"execution":"doc","title":"Scheduled Newsletter"}}
        ],
        "edges": [
            {"from":"tick","to":"workiq"},
            {"from":"workiq","to":"synth"},
            {"from":"synth","to":"doc"}
        ]
    })
}

#[tokio::test]
async fn enqueue_run_with_scheduled_trigger_runs_graph_and_marks_run_scheduled() {
    let db = common::start_test_db().await;

    // Seed engagement + project + member.
    let eng: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Acme') RETURNING id")
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

    // Build dispatcher with fixture handlers.
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Script, Arc::new(NoopHandler));
    dispatcher.register(
        ExecutionType::Http,
        Arc::new(WorkIqHandler {
            fixture: celerex_api::plugins::fixtures::workiq::WorkIqFixture::from_fixture_dir(),
        }),
    );
    dispatcher.register(
        ExecutionType::Llm,
        Arc::new(LlmHandler::new(Arc::new(MockLlm::new().finish("# Scheduled Newsletter")))),
    );
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler {
            pool: db.pool.clone(),
            engagement_id: eng,
        }),
    );
    let dispatcher = Arc::new(dispatcher);

    // Assemble the same pieces that build_app_with_dispatcher_inner does, so
    // we can call `enqueue_run` with the exact collaborators the scheduler
    // closure would hand it in production.
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(PgRunStore::with_bus(db.pool.clone(), run_bus));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(db.pool.clone()));
    let notifiers: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        celerex_api::notifiers::console::ConsoleNotifier,
    )]);

    let member = members
        .create(project, "Jean", "", &jean_graph(), &json!({}))
        .await
        .unwrap();

    // Confirm the engagement lookup the scheduler closure uses.
    let looked_up = members.engagement_id_for(member.id).await.unwrap();
    assert_eq!(looked_up, eng, "engagement_id_for resolves via projects join");

    // Fire the shared helper exactly like the scheduler closure would.
    let run_id = run_job::enqueue_run(
        db.pool.clone(),
        members.clone(),
        executor,
        notifiers,
        member.id,
        eng,
        TRIGGERED_BY_SCHEDULED,
    )
    .await
    .expect("enqueue_run ok");

    // Poll for all 4 steps succeeded.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut succeeded: i64 = 0;
    while std::time::Instant::now() < deadline {
        succeeded = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM run_steps WHERE run_id = $1 AND status = 'succeeded'",
        )
        .bind(run_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        if succeeded >= 4 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(succeeded, 4, "all 4 steps must succeed (scheduled path)");

    // Wait for the notifier task to flip runs.status; then assert trigger source.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut row: (Option<uuid::Uuid>, String, String) = (None, String::new(), String::new());
    while std::time::Instant::now() < deadline {
        row = sqlx::query_as("SELECT snapshot_id, triggered_by, status FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if row.2 == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(row.0.is_some(), "snapshot_id is set");
    assert_eq!(row.1, "scheduled", "triggered_by = scheduled");
    assert_eq!(row.2, "succeeded");

    // Document row created by the outbound agent step.
    let doc_title: String = sqlx::query_scalar(
        "SELECT title FROM documents WHERE engagement_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(doc_title, "Scheduled Newsletter");
}
