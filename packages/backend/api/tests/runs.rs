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
use celerex_orchestrator::{executor::Executor, tool_dispatch::Dispatcher};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

fn build_router(pool: PgPool) -> Router {
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
        providers: Arc::new(Vec::new()),
        providers_registry: Arc::new(celerex_orchestrator::llm::ProviderRegistry::new()),
        provider_keys,
        hub_admin_engagements: Default::default(),
        design_runs: celerex_api::runs::design_chat::new_registry(),
    };
    Router::new()
        .route("/api/runs", get(celerex_api::runs::routes::list))
        .route("/api/runs/{run_id}/steps", get(celerex_api::runs::routes::steps))
        .route(
            "/api/project_members/{id}",
            axum::routing::delete(celerex_api::project_members::routes::delete),
        )
        .with_state(state)
}

async fn seed_engagement_project_member(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let eng = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Runs') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let project = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id",
    )
    .bind(eng)
    .fetch_one(pool)
    .await
    .unwrap();
    let member = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO project_members (project_id, name, graph_json, provider_config_json)
         VALUES ($1, 'M', $2, $3) RETURNING id",
    )
    .bind(project)
    .bind(json!({ "nodes": [], "edges": [] }))
    .bind(json!({}))
    .fetch_one(pool)
    .await
    .unwrap();
    (eng, project, member)
}

async fn insert_run(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: Uuid,
    project_member_id: Uuid,
    triggered_by: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO runs (id, engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, $4, $5::jsonb, 'running', $6)",
    )
    .bind(id)
    .bind(engagement_id)
    .bind(project_id)
    .bind(project_member_id)
    .bind(json!({"source": triggered_by}).to_string())
    .bind(triggered_by)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn auth(eng: Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(eng)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn list_runs_returns_for_project_member() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    // Three runs for this member; small sleeps so created_at is monotonic.
    let _r1 = insert_run(&db.pool, eng, project, member, "manual").await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let _r2 = insert_run(&db.pool, eng, project, member, "scheduled").await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let r3 = insert_run(&db.pool, eng, project, member, "manual").await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/runs?project_member_id={member}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().expect("array");
    assert_eq!(arr.len(), 3);
    // Newest first.
    assert_eq!(arr[0]["id"].as_str().unwrap(), r3.to_string());
    // Carries the expected fields.
    assert_eq!(arr[0]["project_member_id"].as_str().unwrap(), member.to_string());
    assert_eq!(arr[0]["engagement_id"].as_str().unwrap(), eng.to_string());
    assert_eq!(arr[0]["status"].as_str().unwrap(), "running");
    assert_eq!(arr[0]["triggered_by"].as_str().unwrap(), "manual");
    assert!(arr[0]["created_at"].is_string());
}

#[tokio::test]
async fn list_runs_cross_engagement_returns_403() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (_eng_a, project_a, member_a) = seed_engagement_project_member(&db.pool).await;
    let (eng_b, _project_b, _member_b) = seed_engagement_project_member(&db.pool).await;

    // Seed a run on engagement A's member so the row exists.
    let _ = insert_run(
        &db.pool,
        sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM projects WHERE id = $1")
            .bind(project_a)
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        project_a,
        member_a,
        "manual",
    )
    .await;

    let token_b = auth(eng_b);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/runs?project_member_id={member_a}"))
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_runs_empty_for_member_without_runs() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/runs?project_member_id={member}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().expect("array");
    assert_eq!(arr.len(), 0);
}

#[tokio::test]
async fn run_steps_returns_history_and_enforces_tenancy() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, project, member) = seed_engagement_project_member(&db.pool).await;
    let run_id = insert_run(&db.pool, eng, project, member, "manual").await;

    sqlx::query(
        "INSERT INTO run_steps (run_id, step_id, status, inputs, outputs, completed_at)
         VALUES ($1, 'a', 'succeeded', '{}'::jsonb, '{\"ok\":true}'::jsonb, NOW()),
                ($1, 'b', 'failed', '{}'::jsonb, NULL, NOW() + interval '1 second')",
    )
    .bind(run_id)
    .execute(&db.pool)
    .await
    .unwrap();

    // Owner sees ordered step history.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{run_id}/steps"))
                .header("authorization", format!("Bearer {}", auth(eng)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let arr = body_json(res).await;
    assert_eq!(arr.as_array().unwrap().len(), 2);
    assert_eq!(arr[0]["step_id"], "a");
    assert_eq!(arr[0]["outputs"]["ok"], true);
    assert_eq!(arr[1]["step_id"], "b");
    assert_eq!(arr[1]["status"], "failed");

    // A different engagement gets 403.
    let other = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Other') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{run_id}/steps"))
                .header("authorization", format!("Bearer {}", auth(other)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_member_cascades_and_enforces_tenancy() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, project, member) = seed_engagement_project_member(&db.pool).await;
    let run_id = insert_run(&db.pool, eng, project, member, "manual").await;

    // Wrong engagement → 403, member survives.
    let other = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Intruder') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/project_members/{member}"))
                .header("authorization", format!("Bearer {}", auth(other)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // Owner → 204, member + dependent runs are gone.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/project_members/{member}"))
                .header("authorization", format!("Bearer {}", auth(eng)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_members WHERE id = $1")
            .bind(member)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(member_count, 0);
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(run_count, 0);
}
