// Integration tests for the automations foundation (migration 0017).
// Uses the same testcontainers + axum_test harness as design_rituals.rs.
mod common;

use serde_json::json;
use uuid::Uuid;

/// Seed: engagement → project → project_member. Returns (engagement_id, project_id, member_id).
async fn seed_member(pool: &sqlx::PgPool) -> (Uuid, Uuid, Uuid) {
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('AutomationTest') RETURNING id")
            .fetch_one(pool)
            .await
            .unwrap();
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'AutoP') RETURNING id",
    )
    .bind(eng)
    .fetch_one(pool)
    .await
    .unwrap();
    let member: Uuid = sqlx::query_scalar(
        "INSERT INTO project_members (project_id, name, graph_json, provider_config_json)
         VALUES ($1, 'AutoM', $2, $3) RETURNING id",
    )
    .bind(project)
    .bind(json!({ "nodes": [], "edges": [] }))
    .bind(json!({}))
    .fetch_one(pool)
    .await
    .unwrap();
    (eng, project, member)
}

// ---------------------------------------------------------------------------
// (a) automation_triggers table has ≥8 columns
// ---------------------------------------------------------------------------
#[tokio::test]
async fn automation_triggers_table_has_at_least_8_columns() {
    let db = common::start_test_db().await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns
         WHERE table_name = 'automation_triggers'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert!(
        count >= 8,
        "automation_triggers must have ≥8 columns, found {count}"
    );
}

// ---------------------------------------------------------------------------
// (b1) runs CHECK now accepts triggered_by='connector'
// ---------------------------------------------------------------------------
#[tokio::test]
async fn runs_accepts_triggered_by_connector() {
    let db = common::start_test_db().await;
    let (eng, project, member) = seed_member(&db.pool).await;

    let result = sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', 'connector')",
    )
    .bind(eng)
    .bind(project)
    .bind(member)
    .bind(json!({"source": "connector"}).to_string())
    .execute(&db.pool)
    .await;

    assert!(
        result.is_ok(),
        "INSERT with triggered_by='connector' must succeed; got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (b2) runs CHECK rejects triggered_by='bogus'
// ---------------------------------------------------------------------------
#[tokio::test]
async fn runs_rejects_triggered_by_bogus() {
    let db = common::start_test_db().await;
    let (eng, project, member) = seed_member(&db.pool).await;

    let result = sqlx::query(
        "INSERT INTO runs (engagement_id, project_id, project_member_id, trigger, status, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', 'bogus')",
    )
    .bind(eng)
    .bind(project)
    .bind(member)
    .bind(json!({"source": "bogus"}).to_string())
    .execute(&db.pool)
    .await;

    assert!(
        result.is_err(),
        "INSERT with triggered_by='bogus' must be rejected by the CHECK constraint"
    );
}

// ---------------------------------------------------------------------------
// (c) AutomationTriggersStore: due/mark_polled/cursor/list/update/delete
// ---------------------------------------------------------------------------
#[tokio::test]
async fn store_due_filters_by_interval_and_advances_cursor() {
    let db = common::start_test_db().await;
    let (eng, _project, member) = seed_member(&db.pool).await;

    use celerex_api::automations::store::{
        AutomationTrigger, AutomationTriggersStore, PgAutomationTriggersStore,
    };

    let store = PgAutomationTriggersStore::new(db.pool.clone());

    let id = store
        .create(AutomationTrigger {
            id: uuid::Uuid::new_v4(),
            project_member_id: member,
            connector_id: "http-api".into(),
            source_config: json!({"url":"http://x","items_path":"items","id_field":"id"}),
            filter: None,
            poll_interval_secs: 300,
            cursor: None,
            last_polled_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    // never-polled trigger is due
    let due = store.due(chrono::Utc::now()).await.unwrap();
    assert!(due.iter().any(|t| t.id == id), "never-polled trigger should be due");

    // after mark_polled it is not due within the interval, and the cursor persisted
    store
        .mark_polled(id, chrono::Utc::now(), Some("5".into()))
        .await
        .unwrap();
    let due2 = store.due(chrono::Utc::now()).await.unwrap();
    assert!(
        !due2.iter().any(|t| t.id == id),
        "just-polled trigger should not be due again immediately"
    );
    let got = store.get(id).await.unwrap().unwrap();
    assert_eq!(got.cursor.as_deref(), Some("5"));

    // list_for_engagement returns it
    let listed = store.list_for_engagement(eng).await.unwrap();
    assert!(listed.iter().any(|t| t.id == id), "list_for_engagement should return the trigger");

    // update -> enabled=false makes it not due even if interval would fire
    store
        .update(
            id,
            &json!({"url":"http://y","items_path":"items","id_field":"id"}),
            None,
            300,
            false,
        )
        .await
        .unwrap();
    // reset last_polled_at to NULL so interval-check passes, but enabled=false should exclude it
    sqlx::query("UPDATE automation_triggers SET last_polled_at=NULL WHERE id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let due3 = store.due(chrono::Utc::now()).await.unwrap();
    assert!(
        !due3.iter().any(|t| t.id == id),
        "disabled trigger should not be due"
    );

    // delete -> get returns None
    store.delete(id).await.unwrap();
    let gone = store.get(id).await.unwrap();
    assert!(gone.is_none(), "deleted trigger should not be found");
}
