//! A2 hardening Phase 2, Task 3: engagement-scoping for the automations store
//! (defense-in-depth). Mirrors `tests/schedules_scope.rs`: a direct store-layer
//! cross-tenant test on `PgAutomationTriggersStore`, plus an HTTP test asserting
//! denial through the real `/api/design/automations` routes.

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

/// Direct store-layer assertions: a foreign engagement cannot read, update, or
/// delete another engagement's automation trigger.
#[tokio::test]
async fn store_get_update_delete_scoped_cross_tenant() {
    use celerex_api::automations::store::{
        AutomationTrigger, AutomationTriggersStore, PgAutomationTriggersStore,
    };

    let db = common::start_test_db().await;
    let store = PgAutomationTriggersStore::new(db.pool.clone());

    let (eng_a, member_a) = seed_member(&db.pool, "Store A").await;
    let (eng_b, _member_b) = seed_member(&db.pool, "Store B").await;

    let trig_id = Uuid::new_v4();
    store
        .create(AutomationTrigger {
            id: trig_id,
            project_member_id: member_a,
            connector_id: "http-api".into(),
            source_config: json!({ "url": "https://x.example/feed" }),
            filter: None,
            poll_interval_secs: 120,
            cursor: None,
            last_polled_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // get: A sees it, B does not (None).
    assert!(store.get(trig_id, eng_a).await.unwrap().is_some());
    assert!(store.get(trig_id, eng_b).await.unwrap().is_none());

    // update: A succeeds, B returns the not-found error (no cross-tenant mutation).
    assert!(
        store
            .update(
                trig_id,
                eng_a,
                &json!({ "url": "https://x.example/v2" }),
                None,
                300,
                false
            )
            .await
            .is_ok()
    );
    assert!(
        store
            .update(
                trig_id,
                eng_b,
                &json!({ "url": "https://evil/v2" }),
                None,
                600,
                true
            )
            .await
            .is_err(),
        "cross-tenant update must error (not-found)"
    );

    // delete: B cannot delete A's trigger (not-found); A can.
    assert!(
        store.delete(trig_id, eng_b).await.is_err(),
        "cross-tenant delete must error (not-found)"
    );
    assert!(store.delete(trig_id, eng_a).await.is_ok());
}

/// HTTP-layer denial: an automation created by engagement A cannot be updated
/// or deleted with engagement B's token. Handlers gate at 403/404 before/at the
/// store; either status proves the cross-tenant access is denied.
#[tokio::test]
async fn http_automation_access_denied_cross_tenant() {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('HTTP A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let eng_b: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('HTTP B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token_a = celerex_api::auth::jwt::mint_scoped(eng_a, "dev", "design.device", 3600);
    let token_b = celerex_api::auth::jwt::mint_scoped(eng_b, "dev", "design.device", 3600);

    // A creates an automation (real ids via the API).
    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({
            "name": "Cross-tenant monitor",
            "connector_id": "http-api",
            "source_config": { "url": "https://x.example/items", "items_path": "items", "id_field": "id" },
            "poll_interval_secs": 120,
            "body": { "kind": "agent_run", "prompt": "Summarise" }
        }))
        .await;
    res.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = res.json();
    let automation_id = body["automation_id"].as_str().unwrap();

    // B cannot update A's automation.
    let upd_b = server
        .put(&format!("/api/design/automations/{automation_id}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({
            "source_config": { "url": "https://evil/items" },
            "poll_interval_secs": 600,
            "enabled": true
        }))
        .await;
    assert!(
        upd_b.status_code() == axum::http::StatusCode::FORBIDDEN
            || upd_b.status_code() == axum::http::StatusCode::NOT_FOUND,
        "B must not update A's automation, got {}",
        upd_b.status_code()
    );

    // B cannot delete A's automation.
    let del_b = server
        .delete(&format!("/api/design/automations/{automation_id}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await;
    assert!(
        del_b.status_code() == axum::http::StatusCode::FORBIDDEN
            || del_b.status_code() == axum::http::StatusCode::NOT_FOUND,
        "B must not delete A's automation, got {}",
        del_b.status_code()
    );

    // A can still delete its own automation.
    let del_a = server
        .delete(&format!("/api/design/automations/{automation_id}"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    assert_eq!(del_a.status_code(), axum::http::StatusCode::NO_CONTENT);
}
