//! A2 hardening Phase 2, Task 2: engagement-scoping for the scheduler store
//! (defense-in-depth). Mirrors `tests/members_scope.rs`: a direct store-layer
//! cross-tenant test on `PgSchedulesStore`, plus an HTTP test asserting denial
//! through the real routes.

mod common;

use serde_json::json;
use uuid::Uuid;

/// Seed an engagement + project + project_member, return their ids.
async fn seed_member(pool: &sqlx::PgPool, label: &str) -> (Uuid, Uuid) {
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
            .bind(label)
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
    (eng, member)
}

/// Direct store-layer assertions: a foreign engagement cannot read, list,
/// update, or delete another engagement's schedule.
#[tokio::test]
async fn store_get_list_update_delete_scoped_cross_tenant() {
    use celerex_api::scheduler::store::{PgSchedulesStore, Schedule, SchedulesStore};

    let db = common::start_test_db().await;
    let store = PgSchedulesStore::new(db.pool.clone());

    let (eng_a, member_a) = seed_member(&db.pool, "Store A").await;
    let (eng_b, _member_b) = seed_member(&db.pool, "Store B").await;

    let sched_id = Uuid::new_v4();
    store
        .create(Schedule {
            id: sched_id,
            project_member_id: member_a,
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: chrono::Utc::now(),
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // get: A sees it, B does not (None).
    assert!(store.get(sched_id, eng_a).await.unwrap().is_some());
    assert!(store.get(sched_id, eng_b).await.unwrap().is_none());

    // list_for_project_member: A gets one row, B gets empty.
    assert_eq!(
        store
            .list_for_project_member(member_a, eng_a)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .list_for_project_member(member_a, eng_b)
            .await
            .unwrap()
            .is_empty()
    );

    // update: A succeeds, B returns the not-found error (no cross-tenant mutation).
    assert!(
        store
            .update(
                sched_id,
                eng_a,
                "0 0 10 * * MON *",
                "UTC",
                false,
                chrono::Utc::now()
            )
            .await
            .is_ok()
    );
    assert!(
        store
            .update(
                sched_id,
                eng_b,
                "0 0 11 * * MON *",
                "UTC",
                true,
                chrono::Utc::now()
            )
            .await
            .is_err(),
        "cross-tenant update must error (not-found)"
    );

    // delete: B cannot delete A's schedule (not-found); A can.
    assert!(
        store.delete(sched_id, eng_b).await.is_err(),
        "cross-tenant delete must error (not-found)"
    );
    assert!(store.delete(sched_id, eng_a).await.is_ok());
}

/// HTTP-layer denial: a schedule created by engagement A cannot be read,
/// updated, or deleted with engagement B's token. Handlers gate at 403/404
/// before/at the store; either status proves the cross-tenant access is denied.
#[tokio::test]
async fn http_schedule_access_denied_cross_tenant() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let (eng_a, member_a) = seed_member(&db.pool, "HTTP A").await;
    let (eng_b, _member_b) = seed_member(&db.pool, "HTTP B").await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);
    let token_b = celerex_api::auth::jwt::mint_for_tests(eng_b);

    // A creates a schedule.
    let res = server
        .post("/api/schedules")
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({
            "project_member_id": member_a,
            "cron_expr": "0 0 9 * * MON *",
            "timezone": "UTC"
        }))
        .await;
    res.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = res.json();
    let sched_id = body["id"].as_str().unwrap();

    // B cannot read A's schedule via list (filtered/denied).
    let list_b = server
        .get(&format!("/api/schedules?project_member_id={member_a}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await;
    // Handler gates the member at 403 before the store; either way B sees nothing.
    assert!(
        list_b.status_code() == axum::http::StatusCode::FORBIDDEN
            || list_b.status_code() == axum::http::StatusCode::NOT_FOUND
            || {
                let v: serde_json::Value = list_b.json();
                v.as_array().map(|a| a.is_empty()).unwrap_or(false)
            },
        "B must not enumerate A's schedules"
    );

    // B cannot update A's schedule.
    let upd_b = server
        .put(&format!("/api/schedules/{sched_id}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({
            "project_member_id": member_a,
            "cron_expr": "0 0 12 * * MON *",
            "timezone": "UTC"
        }))
        .await;
    assert!(
        upd_b.status_code() == axum::http::StatusCode::FORBIDDEN
            || upd_b.status_code() == axum::http::StatusCode::NOT_FOUND,
        "B must not update A's schedule, got {}",
        upd_b.status_code()
    );

    // B cannot delete A's schedule.
    let del_b = server
        .delete(&format!("/api/schedules/{sched_id}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await;
    assert!(
        del_b.status_code() == axum::http::StatusCode::FORBIDDEN
            || del_b.status_code() == axum::http::StatusCode::NOT_FOUND,
        "B must not delete A's schedule, got {}",
        del_b.status_code()
    );

    // A can still delete its own schedule.
    let del_a = server
        .delete(&format!("/api/schedules/{sched_id}"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    assert_eq!(del_a.status_code(), axum::http::StatusCode::NO_CONTENT);
}
