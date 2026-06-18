//! Living Documents v2 — Task 1: the publish scaffold runs unattended on a
//! scheduled trigger, up to the review gate.
//!
//! Mirrors `tests/scheduler_dispatch.rs`: seed a live template + its scaffold
//! member (whose `fill` node is a real `slot_fill`), then fire the shared
//! `enqueue_run(..., TRIGGERED_BY_SCHEDULED)` exactly as the scheduler closure
//! would. The slot_fill handler is wiremock-backed (house mode), like
//! `tests/living_docs_fill.rs`.
//!
//! NOTE — T1 scope: the scaffold's `review` node pauses the run (human gate). The
//! review auto-approve for scheduled runs is Task 2; until it lands, a scheduled
//! run cannot proceed THROUGH `deliver`. So this test asserts the run progresses
//! `fill → render` correctly (the slot value is filled, persisted, and
//! substituted into the rendered HTML) and then lands `awaiting_review`. The
//! end-to-end through-deliver assertion is deferred to after Task 2.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use celerex_api::design::scaffold::scaffold_live_flow;
use celerex_api::design::slot_fill_handler::SlotFillHandler;
use celerex_api::llm::anthropic::AnthropicClient;
use celerex_api::notifiers::Notifier;
use celerex_api::project_members::run_job::{self, TRIGGERED_BY_SCHEDULED};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::PgRunStore;
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::executor::Executor;
use celerex_orchestrator::llm::{Provider, ProviderRegistry};
use celerex_orchestrator::tool_dispatch::{DispatchError, Dispatcher, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// A doc outbound handler that records the markdown it was handed (so a later
/// through-deliver assertion, post-T2, can verify the delivered content).
struct DocCreateHandler {
    pool: PgPool,
    engagement_id: Uuid,
}

#[async_trait]
impl Handler for DocCreateHandler {
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
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

fn anthropic_text_response(text: &str) -> Value {
    json!({
        "id": "msg_test",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": text }]
    })
}

#[tokio::test]
async fn scheduled_publish_flow_fills_and_renders_up_to_review_gate() {
    // SAFETY: tests set process env at setup before any handler reads it.
    unsafe {
        std::env::set_var("CELEREX_HOUSE_MODE", "true");
    }
    let db = common::start_test_db().await;

    // Wiremock Anthropic provider for the slot_fill completion.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(anthropic_text_response("Big News Today")),
        )
        .mount(&server)
        .await;
    let client = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let provider: Arc<dyn Provider> = Arc::new(client);
    let mut registry = ProviderRegistry::new();
    registry.register("anthropic", provider.clone());
    registry.register("default", provider);
    let registry = Arc::new(registry);

    // Seed engagement + project + a live template with one instruction'd slot.
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Acme') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Deliveries') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let slots = json!([{ "name": "headline", "type": "text", "required": true }]);
    let template_id: Uuid = sqlx::query_scalar(
        "INSERT INTO design_templates
            (engagement_id, title, kind, mode, render_engine, template_html, slots)
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id",
    )
    .bind(eng)
    .bind("Living Newsletter")
    .bind("newsletter")
    .bind("live")
    .bind("html")
    .bind("<h1>{{slot:headline}}</h1>")
    .bind(&slots)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    celerex_api::design::slot_fills::upsert_instruction(
        &db.pool,
        template_id,
        "headline",
        "Write a punchy headline",
        None,
        None,
    )
    .await
    .unwrap();

    // Build the dispatcher with the real production handlers for the scaffold's
    // node types: slot_fill (under test), render, input (trigger), doc (deliver).
    let provider_keys: Arc<dyn ProviderKeysStore> =
        Arc::new(PgProviderKeysStore::from_env(db.pool.clone()).unwrap());
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(
        ExecutionType::Input,
        Arc::new(celerex_orchestrator::handlers::input::InputHandler),
    );
    dispatcher.register(
        ExecutionType::SlotFill,
        Arc::new(SlotFillHandler::new(
            db.pool.clone(),
            provider_keys,
            registry,
        )),
    );
    dispatcher.register(
        ExecutionType::Render,
        Arc::new(celerex_orchestrator::handlers::render::RenderHandler),
    );
    dispatcher.register(
        ExecutionType::Doc,
        Arc::new(DocCreateHandler {
            pool: db.pool.clone(),
            engagement_id: eng,
        }),
    );
    let dispatcher = Arc::new(dispatcher);

    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(PgRunStore::with_bus(db.pool.clone(), run_bus));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(db.pool.clone()));
    let notifiers: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        celerex_api::notifiers::console::ConsoleNotifier,
    )]);

    // The scaffold member: trigger → fill(slot_fill) → render → review → deliver.
    let graph = scaffold_live_flow(
        template_id,
        "Living Newsletter",
        "html",
        "<h1>{{slot:headline}}</h1>",
        &slots,
    );
    let member = members
        .create(
            project,
            "Living Newsletter — delivery",
            "",
            &graph,
            &json!({}),
            "writer",
        )
        .await
        .unwrap();

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

    // Poll until the fill + render steps have succeeded (the run pauses at review).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut fill_ok = false;
    let mut render_ok = false;
    while std::time::Instant::now() < deadline {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT step_id, status FROM run_steps WHERE run_id = $1")
                .bind(run_id)
                .fetch_all(&db.pool)
                .await
                .unwrap();
        fill_ok = rows
            .iter()
            .any(|(id, st)| id == "fill" && st == "succeeded");
        render_ok = rows
            .iter()
            .any(|(id, st)| id == "render" && st == "succeeded");
        if fill_ok && render_ok {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        fill_ok,
        "fill (slot_fill) step must succeed on the scheduled run"
    );
    assert!(render_ok, "render step must succeed on the scheduled run");

    // The slot value was filled + persisted.
    let last_value: Option<String> = sqlx::query_scalar(
        "SELECT last_value FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_id)
    .bind("headline")
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(last_value.as_deref(), Some("Big News Today"));

    // The render step substituted the filled slot value into the template HTML —
    // proof the slot_fill output shape matches what the render node consumes.
    let render_out: Value =
        sqlx::query_scalar("SELECT outputs FROM run_steps WHERE run_id=$1 AND step_id='render'")
            .bind(run_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(render_out["html"], "<h1>Big News Today</h1>");

    // T1 scope: the run pauses at the human review gate (auto-approve = Task 2).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut status = String::new();
    while std::time::Instant::now() < deadline {
        status = sqlx::query_scalar("SELECT status FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        if status == "awaiting_review" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        status, "awaiting_review",
        "scheduled run pauses at the review gate until Task 2's auto-approve lands"
    );
}
