//! Living Documents v2 — Task 1: the `slot_fill` executor.
//!
//! Drives `SlotFillHandler::invoke` directly (the handler IS the executor's
//! runtime for a `slot_fill` node) with a wiremock-backed Anthropic provider,
//! stubbed exactly like `tests/living_docs_fill.rs`. Asserts that the handler:
//!   1. fills every instruction'd slot and persists each `last_value`, and
//!   2. emits a FLAT `{ slot_name: value }` object — the shape the downstream
//!      `render` node consumes as its `data` (it does `data.get("<slot>")`),
//!      NOT a `{ data: {...} }` envelope.

mod common;

use std::sync::Arc;

use celerex_api::design::slot_fill_handler::SlotFillHandler;
use celerex_api::llm::anthropic::AnthropicClient;
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_orchestrator::llm::{Provider, ProviderRegistry};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn anthropic_text_response(text: &str) -> serde_json::Value {
    json!({
        "id": "msg_test",
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": text }]
    })
}

async fn seed_engagement(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_template(pool: &PgPool, engagement_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO design_templates
            (engagement_id, title, kind, mode, render_engine, template_html, slots)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(engagement_id)
    .bind("Living Newsletter")
    .bind("newsletter")
    .bind("live")
    .bind("html")
    .bind("<h1>{{slot:headline}}</h1>")
    .bind(json!([{ "name": "headline", "type": "text", "required": true }]))
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Build a wiremock-backed Anthropic registry + key store, mirroring the v1 fill
/// test's house-mode setup so `run_fill` resolves the house provider.
async fn stub(
    pool: PgPool,
    text: &str,
) -> (
    Arc<dyn ProviderKeysStore>,
    Arc<ProviderRegistry>,
    MockServer,
) {
    // SAFETY: tests set process env at setup before any handler reads it.
    unsafe {
        std::env::set_var("CELEREX_HOUSE_MODE", "true");
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_text_response(text)))
        .mount(&server)
        .await;
    let client = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let provider: Arc<dyn Provider> = Arc::new(client);
    let mut registry = ProviderRegistry::new();
    registry.register("anthropic", provider.clone());
    registry.register("default", provider);
    let provider_keys: Arc<dyn ProviderKeysStore> =
        Arc::new(PgProviderKeysStore::from_env(pool).expect("CELEREX_CRED_KEY must be set"));
    (provider_keys, Arc::new(registry), server)
}

#[tokio::test]
async fn slot_fill_node_fills_persists_and_outputs_flat_object() {
    let db = common::start_test_db().await;
    let (provider_keys, registry, _server) = stub(db.pool.clone(), "Big News Today").await;

    let eng = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng).await;

    // Seed the slot instruction (what the v1 set_instruction endpoint would do).
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

    let handler = SlotFillHandler::new(db.pool.clone(), provider_keys, registry);
    let config = json!({
        "execution": "slot_fill",
        "template_id": template_id.to_string(),
        "engagement_id": eng.to_string(),
    });
    let output = handler
        .invoke(config, json!({}))
        .await
        .expect("slot_fill invoke ok");

    // (2) Flat `{ slot: value }` — NOT `{ data: {...} }`. This is the exact shape
    // the render node consumes as its `data` (render does `data.get("headline")`).
    assert_eq!(output, json!({ "headline": "Big News Today" }));
    assert!(
        output.get("data").is_none(),
        "output must be flat, not wrapped in `data`"
    );

    // (1) The store row's last_value is persisted.
    let last_value: Option<String> = sqlx::query_scalar(
        "SELECT last_value FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_id)
    .bind("headline")
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(last_value.as_deref(), Some("Big News Today"));
}

#[tokio::test]
async fn slot_fill_node_requires_template_and_engagement_ids() {
    let db = common::start_test_db().await;
    let (provider_keys, registry, _server) = stub(db.pool.clone(), "unused").await;
    let handler = SlotFillHandler::new(db.pool.clone(), provider_keys, registry);

    // Missing engagement_id → handler error (never fills a foreign/unknown tenant).
    let err = handler
        .invoke(
            json!({ "template_id": Uuid::new_v4().to_string() }),
            json!({}),
        )
        .await
        .expect_err("missing engagement_id must error");
    assert!(format!("{err}").contains("engagement_id"), "err was: {err}");

    // Missing template_id → handler error.
    let err = handler
        .invoke(
            json!({ "engagement_id": Uuid::new_v4().to_string() }),
            json!({}),
        )
        .await
        .expect_err("missing template_id must error");
    assert!(format!("{err}").contains("template_id"), "err was: {err}");
}

/// IDOR guard: a `slot_fill` node reads `template_id` from its (flow-author
/// supplied) config, so the shared fill fn MUST verify the run's engagement owns
/// that template. A crafted node pointing at engagement A's template while the
/// run belongs to engagement B must NOT fill, disclose, or overwrite A's slots.
#[tokio::test]
async fn slot_fill_node_cannot_touch_another_engagements_template() {
    let db = common::start_test_db().await;
    let (provider_keys, registry, _server) = stub(db.pool.clone(), "Leaked!").await;

    // Engagement A owns a template with an instruction'd slot (no value yet).
    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_a = seed_template(&db.pool, eng_a).await;
    celerex_api::design::slot_fills::upsert_instruction(
        &db.pool,
        template_a,
        "headline",
        "Write a punchy headline",
        None,
        None,
    )
    .await
    .unwrap();

    // Engagement B crafts a slot_fill node targeting A's template_id.
    let eng_b = seed_engagement(&db.pool, "Engagement B").await;
    let handler = SlotFillHandler::new(db.pool.clone(), provider_keys, registry);
    let err = handler
        .invoke(
            json!({
                "execution": "slot_fill",
                "template_id": template_a.to_string(),
                "engagement_id": eng_b.to_string(),
            }),
            json!({}),
        )
        .await
        .expect_err("cross-engagement slot_fill must error, not leak/overwrite");
    // "not found" — never disclose the foreign template's existence.
    assert!(format!("{err}").contains("not found"), "err was: {err}");

    // A's slot was NOT written — last_value remains NULL (no cross-tenant write).
    let last_value: Option<String> = sqlx::query_scalar(
        "SELECT last_value FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_a)
    .bind("headline")
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        last_value, None,
        "A's slot must be untouched by B's crafted node"
    );
}
