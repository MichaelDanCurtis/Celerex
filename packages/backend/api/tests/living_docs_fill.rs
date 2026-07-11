//! Living Documents v1 — Task 3: slot fill (one-shot completion) + fill-all +
//! render endpoints.
//!
//! The fill path reuses the assistant one-shot provider acquisition. We stub the
//! provider exactly like `tests/assistant_chat.rs`: a wiremock-backed Anthropic
//! `ProviderRegistry` plus `CELEREX_HOUSE_MODE=true` and NO vaulted engagement
//! key, so `run_fill` resolves the house registry and gets a fixed completion.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, post};
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
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn build_state(
    pool: PgPool,
    registry: Arc<ProviderRegistry>,
    provider_ids: Vec<String>,
) -> AppState {
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
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> =
        Arc::new(vec![Arc::new(notifiers::console::ConsoleNotifier)]);
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
        hub_admin_engagements: Default::default(),
        design_runs: celerex_api::runs::design_chat::new_registry(),
    }
}

/// A router mounting just the living-docs slot routes under test, wired to a
/// state whose provider registry is wiremock-backed.
fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/design/templates/{template_id}/slots/{slot}/instruction",
            axum::routing::put(celerex_api::design::living::set_instruction),
        )
        .route(
            "/api/design/templates/{template_id}/slots",
            get(celerex_api::design::living::list_slots),
        )
        .route(
            "/api/design/templates/{template_id}/slots/{slot}/fill",
            post(celerex_api::design::living::fill_slot),
        )
        .route(
            "/api/design/templates/{template_id}/fill",
            post(celerex_api::design::living::fill_all),
        )
        .route(
            "/api/design/templates/{template_id}/render",
            get(celerex_api::design::living::render),
        )
        .with_state(state)
}

/// Anthropic-shaped completion body (the default provider's `raw` shape).
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

/// Seed a live `design_templates` row owned by `engagement_id` (mirrors T2's
/// INSERT columns) with `<h1>{{slot:headline}}</h1>` + a single text slot.
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

/// Build a wiremock-backed Anthropic registry (provider id "anthropic") that
/// always returns `text`. House mode is enabled by `mint_for_tests`'s env setup
/// path — we set it explicitly here too so `run_fill` takes the house branch.
async fn stub_state(pool: PgPool, text: &str) -> (AppState, MockServer) {
    // SAFETY: tests set process env at setup; the JWT/house env mirrors mint_for_tests.
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
    let state = build_state(
        pool,
        Arc::new(registry),
        vec!["anthropic".to_string(), "default".to_string()],
    );
    (state, server)
}

#[tokio::test]
async fn fill_slot_runs_completion_and_persists_value() {
    let db = common::start_test_db().await;
    let (state, _server) = stub_state(db.pool.clone(), "Big News Today").await;
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng_a).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    // Set the slot instruction.
    server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "instruction": "Write a punchy headline" }))
        .await
        .assert_status_ok();

    // Fill it → 200 { value: "<stub text>" }.
    let fill = server
        .post(&format!(
            "/api/design/templates/{template_id}/slots/headline/fill"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    fill.assert_status_ok();
    let body: serde_json::Value = fill.json();
    assert_eq!(body["value"], "Big News Today");

    // The store row's last_value is updated.
    let last_value: Option<String> = sqlx::query_scalar(
        "SELECT last_value FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_id)
    .bind("headline")
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(last_value.as_deref(), Some("Big News Today"));

    // Render substitutes the value into the {{slot:headline}} marker.
    let render = server
        .get(&format!("/api/design/templates/{template_id}/render"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    render.assert_status_ok();
    let rbody: serde_json::Value = render.json();
    assert_eq!(rbody["html"], "<h1>Big News Today</h1>");
}

#[tokio::test]
async fn fill_with_no_instruction_returns_409() {
    let db = common::start_test_db().await;
    let (state, _server) = stub_state(db.pool.clone(), "unused").await;
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng_a).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    // No instruction set for "headline" → fill is a conflict.
    let fill = server
        .post(&format!(
            "/api/design/templates/{template_id}/slots/headline/fill"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    fill.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn fill_and_render_are_engagement_scoped() {
    let db = common::start_test_db().await;
    let (state, _server) = stub_state(db.pool.clone(), "secret").await;
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng_a).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);
    server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "instruction": "Write a headline" }))
        .await
        .assert_status_ok();

    // Engagement B doesn't own the template.
    let eng_b = seed_engagement(&db.pool, "Engagement B").await;
    let token_b = celerex_api::auth::jwt::mint_for_tests(eng_b);

    // Cross-engagement fill → 404.
    server
        .post(&format!(
            "/api/design/templates/{template_id}/slots/headline/fill"
        ))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Cross-engagement fill-all → 404.
    server
        .post(&format!("/api/design/templates/{template_id}/fill"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Cross-engagement render → 404.
    server
        .get(&format!("/api/design/templates/{template_id}/render"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Unauthenticated fill → 401.
    server
        .post(&format!(
            "/api/design/templates/{template_id}/slots/headline/fill"
        ))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Unauthenticated render → 401.
    server
        .get(&format!("/api/design/templates/{template_id}/render"))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
