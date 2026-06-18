//! Living Documents v2 — Task 3: the `schedule` convenience endpoint.
//!
//! `POST /api/design/templates/{template_id}/schedule` attaches a recurring
//! Ritual (a `schedules` row) to a live template's existing publish flow member
//! (`design_templates.flow_member_id`), so the scaffold fills + publishes on a
//! cadence. It reuses the rituals/scheduler create path (`compute_next_fire_at`
//! cron/timezone validation + `PgSchedulesStore`) and is engagement-scoped via
//! v1's `own_template` gate.
//!
//! v2 = ONE schedule per template's flow member: a second POST with a different
//! cron updates the existing row (no duplicate).

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{executor::Executor, llm::ProviderRegistry, tool_dispatch::Dispatcher};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn build_state(pool: PgPool) -> AppState {
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
        providers: Arc::new(vec![]),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        hub_admin_engagements: Default::default(),
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/design/templates/{template_id}/schedule",
            post(celerex_api::design::living::schedule),
        )
        .with_state(state)
}

async fn seed_engagement(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Seed a live template owned by `engagement_id`. When `with_flow` is true, also
/// create a project + a project_member to act as the publish flow and set the
/// template's `flow_member_id` to it (mirrors `ensure_delivery_flow`'s end state).
/// Returns `(template_id, flow_member_id)`.
async fn seed_template(
    pool: &PgPool,
    engagement_id: Uuid,
    with_flow: bool,
) -> (Uuid, Option<Uuid>) {
    let template_id: Uuid = sqlx::query_scalar(
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
    .unwrap();

    let flow_member_id = if with_flow {
        let project_id: Uuid = sqlx::query_scalar(
            "INSERT INTO projects (engagement_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(engagement_id)
        .bind("Design Deliveries")
        .fetch_one(pool)
        .await
        .unwrap();
        let member_id: Uuid = sqlx::query_scalar(
            "INSERT INTO project_members (project_id, name, description, graph_json)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(project_id)
        .bind("Living Newsletter — delivery")
        .bind("Auto-scaffolded by Celerex Design publish.")
        .bind(json!({ "nodes": [], "edges": [] }))
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE design_templates SET flow_member_id = $1 WHERE id = $2")
            .bind(member_id)
            .bind(template_id)
            .execute(pool)
            .await
            .unwrap();
        Some(member_id)
    } else {
        None
    };
    (template_id, flow_member_id)
}

async fn schedules_for_member(pool: &PgPool, member_id: Uuid) -> Vec<(Uuid, String)> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, cron_expr FROM schedules WHERE project_member_id = $1",
    )
    .bind(member_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn schedule_creates_then_upserts_one_row() {
    let db = common::start_test_db().await;
    let state = build_state(db.pool.clone());
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let (template_id, flow) = seed_template(&db.pool, eng_a, true).await;
    let flow_member_id = flow.expect("flow member seeded");
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    // First schedule: weekly Monday 09:00.
    let res = server
        .post(&format!("/api/design/templates/{template_id}/schedule"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "cron": "0 0 9 * * 1", "timezone": "UTC" }))
        .await;
    res.assert_status_ok();
    let body: serde_json::Value = res.json();
    let ritual_id = body["ritual_id"].as_str().expect("ritual_id present");
    assert!(Uuid::parse_str(ritual_id).is_ok(), "ritual_id is a uuid");
    assert!(
        body["next_fire_at"].is_string(),
        "next_fire_at present: {body}"
    );

    let rows = schedules_for_member(&db.pool, flow_member_id).await;
    assert_eq!(rows.len(), 1, "exactly one schedule on the flow member");
    assert_eq!(rows[0].1, "0 0 9 * * 1");

    // Second schedule with a different cron → UPDATES the same row (still one).
    let res2 = server
        .post(&format!("/api/design/templates/{template_id}/schedule"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "cron": "0 0 9 * * *", "timezone": "UTC" }))
        .await;
    res2.assert_status_ok();

    let rows2 = schedules_for_member(&db.pool, flow_member_id).await;
    assert_eq!(
        rows2.len(),
        1,
        "still exactly one schedule after re-schedule"
    );
    assert_eq!(rows2[0].1, "0 0 9 * * *", "cron updated");
    assert_eq!(rows2[0].0, rows[0].0, "same schedule row id (upsert)");
}

#[tokio::test]
async fn schedule_template_without_flow_returns_409() {
    let db = common::start_test_db().await;
    let state = build_state(db.pool.clone());
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let (template_id, _) = seed_template(&db.pool, eng_a, false).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    server
        .post(&format!("/api/design/templates/{template_id}/schedule"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "cron": "0 0 9 * * 1", "timezone": "UTC" }))
        .await
        .assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn schedule_is_engagement_scoped_and_authed() {
    let db = common::start_test_db().await;
    let state = build_state(db.pool.clone());
    let server = axum_test::TestServer::new(build_router(state)).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let (template_id, _) = seed_template(&db.pool, eng_a, true).await;

    // Engagement B doesn't own the template → 404.
    let eng_b = seed_engagement(&db.pool, "Engagement B").await;
    let token_b = celerex_api::auth::jwt::mint_for_tests(eng_b);
    server
        .post(&format!("/api/design/templates/{template_id}/schedule"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({ "cron": "0 0 9 * * 1", "timezone": "UTC" }))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Unauthenticated → 401.
    server
        .post(&format!("/api/design/templates/{template_id}/schedule"))
        .json(&json!({ "cron": "0 0 9 * * 1", "timezone": "UTC" }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
