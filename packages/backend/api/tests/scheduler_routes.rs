mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{post, put};
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::runs::bus::RunBus;
use celerex_api::scheduler::store::{PgSchedulesStore, Schedule, SchedulesStore};
use celerex_orchestrator::{
    executor::Executor,
    tool_dispatch::Dispatcher,
};
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
    };
    Router::new()
        .route(
            "/api/schedules",
            post(celerex_api::scheduler::routes::create)
                .get(celerex_api::scheduler::routes::list),
        )
        .route(
            "/api/schedules/{id}",
            put(celerex_api::scheduler::routes::update)
                .delete(celerex_api::scheduler::routes::delete),
        )
        .route(
            "/api/schedules/{id}/fire",
            post(celerex_api::scheduler::routes::fire),
        )
        .with_state(state)
}

async fn seed_engagement_project_member(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let eng = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Sched') RETURNING id",
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

fn auth(eng: Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(eng)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn create_schedule_returns_201_and_persists() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "0 0 9 * * MON *",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let sched_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    let next: chrono::DateTime<chrono::Utc> = body["next_fire_at"].as_str().unwrap().parse().unwrap();
    assert!(next > chrono::Utc::now(), "next_fire_at should be in the future");

    // DB row exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schedules WHERE id = $1")
        .bind(sched_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn list_schedules_filters_by_project_member() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, project_id, member_a) = seed_engagement_project_member(&db.pool).await;
    // seed second member in same project/engagement
    let member_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO project_members (project_id, name, graph_json, provider_config_json)
         VALUES ($1, 'B', $2, $3) RETURNING id",
    )
    .bind(project_id)
    .bind(json!({ "nodes": [], "edges": [] }))
    .bind(json!({}))
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let token = auth(eng);

    for member in [member_a, member_a, member_b] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/schedules")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::from(
                        json!({
                            "project_member_id": member,
                            "cron_expr": "0 0 9 * * MON *",
                            "timezone": "UTC"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/schedules?project_member_id={member_a}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn update_schedule_recomputes_next_fire_at() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "0 0 9 * * MON *",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let sched_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    let original_next: chrono::DateTime<chrono::Utc> =
        body["next_fire_at"].as_str().unwrap().parse().unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/schedules/{sched_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "0 30 14 * * FRI *",
                        "timezone": "UTC",
                        "enabled": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_next: chrono::DateTime<chrono::Utc> =
        body["next_fire_at"].as_str().unwrap().parse().unwrap();
    assert_ne!(new_next, original_next, "next_fire_at must change on cron update");
}

#[tokio::test]
async fn delete_schedule_removes_row() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let store = PgSchedulesStore::new(db.pool.clone());
    let sched_id = Uuid::new_v4();
    store
        .create(Schedule {
            id: sched_id,
            project_member_id: member,
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: chrono::Utc::now() + chrono::Duration::days(1),
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/schedules/{sched_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schedules WHERE id = $1")
        .bind(sched_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // subsequent DELETE on a missing schedule returns 404
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/schedules/{sched_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_cron_returns_400() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "not a cron",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn cross_engagement_access_returns_403() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (_eng_a, _project_a, member_a) = seed_engagement_project_member(&db.pool).await;
    let (eng_b, _project_b, _member_b) = seed_engagement_project_member(&db.pool).await;
    let token_b = auth(eng_b);

    // engagement B tries to create a schedule for member belonging to engagement A
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member_a,
                        "cron_expr": "0 0 9 * * MON *",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn fire_schedule_runs_member_and_marks_fired() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    // Create a schedule whose natural cron tick is far in the future
    // (yearly on Jan 1 at 09:00 UTC) so the firing must come from /fire.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedules")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_member_id": member,
                        "cron_expr": "0 0 9 1 1 * *",
                        "timezone": "UTC"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let sched_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let before_fire = chrono::Utc::now();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/schedules/{sched_id}/fire"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    // Response carries a run_id and the updated schedule.
    let run_id: Uuid = body["run_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        body["schedule"]["id"].as_str().unwrap(),
        sched_id.to_string()
    );
    let last_fired: chrono::DateTime<chrono::Utc> = body["schedule"]["last_fired_at"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let after_fire = chrono::Utc::now();
    assert!(
        last_fired >= before_fire - chrono::Duration::seconds(1),
        "last_fired_at should be ~now (>= before_fire), got {last_fired}"
    );
    assert!(
        last_fired <= after_fire + chrono::Duration::seconds(1),
        "last_fired_at should be ~now (<= after_fire), got {last_fired}"
    );

    // DB row reflects the same.
    let (db_last_fired, db_next_fire): (
        Option<chrono::DateTime<chrono::Utc>>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as("SELECT last_fired_at, next_fire_at FROM schedules WHERE id = $1")
        .bind(sched_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!(db_last_fired.is_some(), "last_fired_at written to DB");
    assert!(
        db_next_fire > chrono::Utc::now(),
        "next_fire_at recomputed to a future occurrence"
    );

    // A `runs` row was enqueued with triggered_by = scheduled.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut triggered_by: Option<String> = None;
    while std::time::Instant::now() < deadline {
        triggered_by = sqlx::query_scalar("SELECT triggered_by FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(&db.pool)
            .await
            .unwrap();
        if triggered_by.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(triggered_by.as_deref(), Some("scheduled"));
}

#[tokio::test]
async fn fire_unknown_schedule_returns_404() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (eng, _project, _member) = seed_engagement_project_member(&db.pool).await;
    let token = auth(eng);

    let missing_id = Uuid::new_v4();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/schedules/{missing_id}/fire"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn fire_cross_engagement_returns_403() {
    let db = common::start_test_db().await;
    let app = build_router(db.pool.clone());
    let (_eng_a, _project_a, member_a) = seed_engagement_project_member(&db.pool).await;
    let (eng_b, _project_b, _member_b) = seed_engagement_project_member(&db.pool).await;

    // Seed a schedule owned by engagement A directly via the store.
    let store = PgSchedulesStore::new(db.pool.clone());
    let sched_id = Uuid::new_v4();
    store
        .create(Schedule {
            id: sched_id,
            project_member_id: member_a,
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: chrono::Utc::now() + chrono::Duration::days(1),
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // Engagement B tries to fire it.
    let token_b = auth(eng_b);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/schedules/{sched_id}/fire"))
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
