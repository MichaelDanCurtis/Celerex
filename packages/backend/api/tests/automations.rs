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
