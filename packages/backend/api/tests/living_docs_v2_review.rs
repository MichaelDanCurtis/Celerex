//! Living Documents v2 — Task 2: the `review` node auto-approves when the run is
//! triggered by the scheduler, so an unattended run completes through deliver;
//! manual/interactive runs keep the existing human gate (pause at
//! `awaiting_review`).
//!
//! Two runs of the SAME graph (`start → review → deliver`) prove the asymmetry:
//!   * `TRIGGERED_BY_SCHEDULED` → review passes through approved → deliver runs →
//!     run completes (`succeeded`), and the delivered content is the reviewed
//!     content (flowed through as `{{steps.review.outputs.content}}`).
//!   * `TRIGGERED_BY_MANUAL`    → review pauses → run lands `awaiting_review`,
//!     deliver does NOT run. This proves the manual gate is not weakened.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use celerex_api::notifiers::Notifier;
use celerex_api::project_members::run_job::{self, TRIGGERED_BY_MANUAL, TRIGGERED_BY_SCHEDULED};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::PgRunStore;
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Mutex;
use uuid::Uuid;

/// A doc outbound handler that records the markdown content it was handed, so a
/// test can assert WHAT flowed through the (auto-)approved review gate.
struct DocCreateHandler {
    pool: PgPool,
    engagement_id: Uuid,
    last_markdown: Arc<Mutex<Option<String>>>,
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
            .map(|s| s.to_string());
        *self.last_markdown.lock().unwrap() = markdown;
        let doc_id: Uuid = sqlx::query_scalar(
            "INSERT INTO documents (engagement_id, title) VALUES ($1, $2) RETURNING id",
        )
        .bind(self.engagement_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({ "document_id": doc_id }))
    }
}

/// Build the executor + a member whose graph is `start → review → deliver`. The
/// `review` node's content is a literal so the test does not depend on any LLM.
async fn seed_member(
    pool: &PgPool,
    eng: Uuid,
    last_markdown: Arc<Mutex<Option<String>>>,
) -> (
    Arc<Executor>,
    Arc<dyn MembersStore>,
    Arc<Vec<Arc<dyn Notifier>>>,
    Uuid,
) {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Deliveries') RETURNING id",
    )
    .bind(eng)
    .fetch_one(pool)
    .await
    .unwrap();

    let mut dispatcher = Dispatcher::new();
    dispatcher.register(
        ExecutionType::Input,
        Arc::new(celerex_orchestrator::handlers::input::InputHandler),
    );
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler {
            pool: pool.clone(),
            engagement_id: eng,
            last_markdown,
        }),
    );
    let dispatcher = Arc::new(dispatcher);

    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(PgRunStore::with_bus(pool.clone(), run_bus));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let notifiers: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        celerex_api::notifiers::console::ConsoleNotifier,
    )]);

    // start (input) → review (human gate) → deliver (doc). The review content is
    // a literal so the reviewed/approved content is deterministic.
    let graph = json!({
        "nodes": [
            { "id": "start", "type": "inbound", "config": { "execution": "input" } },
            {
                "id": "review",
                "type": "filter",
                "config": {
                    "execution": "review",
                    "prompt": "Approve before shipping",
                    "content": "Reviewed content goes out"
                }
            },
            {
                "id": "deliver",
                "type": "outbound",
                "config": {
                    "execution": "doc",
                    "title": "Reviewed Doc",
                    "inputs": { "markdown": "{{steps.review.outputs.content}}" }
                }
            }
        ],
        "edges": [
            { "from": "start", "to": "review" },
            { "from": "review", "to": "deliver" }
        ]
    });
    let member = members
        .create(
            project,
            "Reviewed delivery",
            "",
            &graph,
            &json!({}),
            "writer",
        )
        .await
        .unwrap();
    (executor, members, notifiers, member.id)
}

async fn run_status(pool: &PgPool, run_id: Uuid, want: &str, secs: u64) -> String {
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
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    status
}

#[tokio::test]
async fn scheduled_run_auto_approves_review_and_completes_through_deliver() {
    let db = common::start_test_db().await;
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Acme') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let last_markdown = Arc::new(Mutex::new(None));
    let (executor, members, notifiers, member_id) =
        seed_member(&db.pool, eng, last_markdown.clone()).await;

    let run_id = run_job::enqueue_run(
        db.pool.clone(),
        members,
        executor,
        notifiers,
        member_id,
        eng,
        TRIGGERED_BY_SCHEDULED,
    )
    .await
    .expect("enqueue_run ok");

    // The run must COMPLETE (review auto-approved → deliver ran). It must never
    // sit at awaiting_review.
    let status = run_status(&db.pool, run_id, "succeeded", 15).await;
    assert_eq!(
        status, "succeeded",
        "scheduled run auto-approves review and completes through deliver"
    );

    // The review step is recorded succeeded with an approved decision.
    let review_out: Value =
        sqlx::query_scalar("SELECT outputs FROM run_steps WHERE run_id=$1 AND step_id='review'")
            .bind(run_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(review_out["approved"], json!(true));
    assert_eq!(review_out["content"], json!("Reviewed content goes out"));

    // Deliver ran and received the reviewed content (flowed through approved).
    assert_eq!(
        last_markdown.lock().unwrap().as_deref(),
        Some("Reviewed content goes out"),
        "the approved content flowed downstream to deliver"
    );
    let delivered: i64 =
        sqlx::query_scalar("SELECT count(*) FROM documents WHERE engagement_id=$1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(delivered, 1, "deliver produced exactly one document");
}

#[tokio::test]
async fn manual_run_keeps_the_human_review_gate() {
    let db = common::start_test_db().await;
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Beta') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let last_markdown = Arc::new(Mutex::new(None));
    let (executor, members, notifiers, member_id) =
        seed_member(&db.pool, eng, last_markdown.clone()).await;

    let run_id = run_job::enqueue_run(
        db.pool.clone(),
        members,
        executor,
        notifiers,
        member_id,
        eng,
        TRIGGERED_BY_MANUAL,
    )
    .await
    .expect("enqueue_run ok");

    // The manual run pauses at the human gate — EXACTLY the pre-existing behavior.
    let status = run_status(&db.pool, run_id, "awaiting_review", 10).await;
    assert_eq!(
        status, "awaiting_review",
        "manual run keeps the human review gate (not weakened)"
    );

    // Deliver did NOT run: no document, no markdown captured.
    assert!(
        last_markdown.lock().unwrap().is_none(),
        "deliver must not run while the manual run is paused at review"
    );
    let delivered: i64 =
        sqlx::query_scalar("SELECT count(*) FROM documents WHERE engagement_id=$1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(delivered, 0, "no delivery before a human approves");

    // The review step is parked awaiting_review (not auto-approved).
    let review_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM run_steps WHERE run_id=$1 AND step_id='review' ORDER BY completed_at DESC LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(&db.pool)
    .await
    .unwrap();
    assert_eq!(review_status.as_deref(), Some("awaiting_review"));
}
