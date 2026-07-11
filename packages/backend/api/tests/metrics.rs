//! Integration tests for `GET /api/metrics/summary`.
//!
//! These exercise the full stack: real `build_app`, real JWT auth, real
//! testcontainer-backed Postgres. We seed rows directly through the pool and
//! then call the endpoint to confirm the aggregation produces the expected
//! shape.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{
    executor::Executor,
    llm::ProviderRegistry,
    tool_dispatch::Dispatcher,
};
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

/// Build a router with a state that mirrors production but uses a no-op
/// dispatcher and a configurable `providers` list. We deliberately don't go
/// through `build_app` because we want explicit control over which provider
/// ids the metrics handler reports.
fn build_router(pool: PgPool, provider_ids: Vec<String>) -> Router {
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
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> = Arc::new(vec![Arc::new(
        notifiers::console::ConsoleNotifier,
    )]);
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
        providers: Arc::new(provider_ids),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        hub_admin_engagements: Default::default(),
        design_runs: celerex_api::runs::design_chat::new_registry(),
    };
    Router::new()
        .route("/api/metrics/summary", get(celerex_api::metrics::summary))
        .with_state(state)
}

async fn seed_engagement(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_project(pool: &PgPool, engagement_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(engagement_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_member(pool: &PgPool, project_id: Uuid, name: &str, agent_type: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO project_members (project_id, name, graph_json, provider_config_json, agent_type)
         VALUES ($1, $2, '{\"nodes\":[],\"edges\":[]}'::jsonb, '{}'::jsonb, $3) RETURNING id",
    )
    .bind(project_id)
    .bind(name)
    .bind(agent_type)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn auth(eng: Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(eng)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn get_summary(app: &Router, token: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/metrics/summary")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = body_json(res).await;
    (status, body)
}

#[tokio::test]
async fn summary_returns_zeros_for_empty_engagement() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone(), vec!["anthropic".into()]);
    let eng = seed_engagement(&db.pool, "Empty").await;
    let token = auth(eng);

    let (status, body) = get_summary(&app, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["engagements_total"], 1);
    assert_eq!(body["projects_total"], 0);
    assert_eq!(body["members_total"], 0);
    assert!(body["members_by_agent_type"].is_object());
    assert_eq!(
        body["members_by_agent_type"].as_object().unwrap().len(),
        0,
        "no members → empty map"
    );
    assert_eq!(body["schedules_total"], 0);
    assert_eq!(body["schedules_enabled"], 0);
    assert_eq!(body["runs_24h"], 0);
    assert_eq!(body["runs_succeeded_24h"], 0);
    assert_eq!(body["runs_failed_24h"], 0);
    assert_eq!(body["docs_total"], 0);
    assert_eq!(body["plugins_installed"], 0);
    let providers: Vec<String> = body["providers_configured"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(providers, vec!["anthropic".to_string()]);
}

#[tokio::test]
async fn summary_counts_members_by_agent_type() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone(), Vec::new());
    let eng = seed_engagement(&db.pool, "Mix").await;
    let project = seed_project(&db.pool, eng, "P").await;
    let _r1 = seed_member(&db.pool, project, "R1", "reasoning").await;
    let _r2 = seed_member(&db.pool, project, "R2", "reasoning").await;
    let _w1 = seed_member(&db.pool, project, "W1", "writer").await;
    let token = auth(eng);

    let (status, body) = get_summary(&app, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["members_total"], 3);
    let by_type = body["members_by_agent_type"].as_object().unwrap();
    assert_eq!(by_type.get("reasoning").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(by_type.get("writer").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(body["projects_total"], 1);
}

#[tokio::test]
async fn summary_counts_runs_in_last_24h() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone(), Vec::new());
    let eng = seed_engagement(&db.pool, "Runs").await;
    let project = seed_project(&db.pool, eng, "P").await;
    let member = seed_member(&db.pool, project, "M", "reasoning").await;

    // Recent runs (default created_at = NOW()): 2 succeeded, 1 failed, 1 running.
    for status in ["succeeded", "succeeded", "failed", "running"] {
        sqlx::query(
            "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
             VALUES ($1, $2, $3, '{\"source\":\"test\"}'::jsonb, $4, 'manual')",
        )
        .bind(eng)
        .bind(project)
        .bind(member)
        .bind(status)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    // An old run (>24h ago) — should NOT appear in any 24h counter.
    sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by, created_at)
         VALUES ($1, $2, $3, '{\"source\":\"test\"}'::jsonb, 'succeeded', 'manual', NOW() - INTERVAL '2 days')",
    )
    .bind(eng)
    .bind(project)
    .bind(member)
    .execute(&db.pool)
    .await
    .unwrap();

    let token = auth(eng);
    let (status, body) = get_summary(&app, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["runs_24h"], 4, "4 recent runs across all statuses");
    assert_eq!(body["runs_succeeded_24h"], 2);
    assert_eq!(body["runs_failed_24h"], 1);
}

#[tokio::test]
async fn summary_is_engagement_scoped() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone(), Vec::new());

    // Engagement A: 1 project, 2 members, 1 doc, 1 plugin install, 1 schedule, 1 recent run.
    let eng_a = seed_engagement(&db.pool, "A").await;
    let proj_a = seed_project(&db.pool, eng_a, "Pa").await;
    let mem_a1 = seed_member(&db.pool, proj_a, "Ma1", "reasoning").await;
    let _mem_a2 = seed_member(&db.pool, proj_a, "Ma2", "writer").await;
    sqlx::query("INSERT INTO documents (engagement_id, project_id, title) VALUES ($1, NULL, 'docA')")
        .bind(eng_a)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'p.a', '{}'::jsonb)")
        .bind(eng_a)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO schedules (project_member_id, cron_expr, timezone, next_fire_at, enabled)
         VALUES ($1, '* * * * *', 'UTC', NOW(), TRUE)",
    )
    .bind(mem_a1)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, '{\"source\":\"test\"}'::jsonb, 'succeeded', 'manual')",
    )
    .bind(eng_a)
    .bind(proj_a)
    .bind(mem_a1)
    .execute(&db.pool)
    .await
    .unwrap();

    // Engagement B: a *lot* of noise that must NOT leak into A's metrics.
    let eng_b = seed_engagement(&db.pool, "B").await;
    let proj_b1 = seed_project(&db.pool, eng_b, "Pb1").await;
    let proj_b2 = seed_project(&db.pool, eng_b, "Pb2").await;
    let mem_b1 = seed_member(&db.pool, proj_b1, "Mb1", "reasoning").await;
    let _mem_b2 = seed_member(&db.pool, proj_b1, "Mb2", "reasoning").await;
    let _mem_b3 = seed_member(&db.pool, proj_b2, "Mb3", "writer").await;
    for _ in 0..3 {
        sqlx::query("INSERT INTO documents (engagement_id, project_id, title) VALUES ($1, NULL, 'docB')")
            .bind(eng_b)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'p.b1', '{}'::jsonb)")
        .bind(eng_b)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'p.b2', '{}'::jsonb)")
        .bind(eng_b)
        .execute(&db.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO schedules (project_member_id, cron_expr, timezone, next_fire_at, enabled)
         VALUES ($1, '* * * * *', 'UTC', NOW(), FALSE)",
    )
    .bind(mem_b1)
    .execute(&db.pool)
    .await
    .unwrap();
    for _ in 0..5 {
        sqlx::query(
            "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
             VALUES ($1, $2, $3, '{\"source\":\"test\"}'::jsonb, 'failed', 'manual')",
        )
        .bind(eng_b)
        .bind(proj_b1)
        .bind(mem_b1)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    // Query AS engagement A.
    let token = auth(eng_a);
    let (status, body) = get_summary(&app, &token).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["projects_total"], 1, "only A's project");
    assert_eq!(body["members_total"], 2, "only A's members");
    let by_type = body["members_by_agent_type"].as_object().unwrap();
    assert_eq!(by_type.get("reasoning").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(by_type.get("writer").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(body["docs_total"], 1, "only A's doc");
    assert_eq!(body["plugins_installed"], 1, "only A's plugin");
    assert_eq!(body["schedules_total"], 1);
    assert_eq!(body["schedules_enabled"], 1);
    assert_eq!(body["runs_24h"], 1, "only A's recent run");
    assert_eq!(body["runs_succeeded_24h"], 1);
    assert_eq!(body["runs_failed_24h"], 0, "B's failed runs do not leak");
}
