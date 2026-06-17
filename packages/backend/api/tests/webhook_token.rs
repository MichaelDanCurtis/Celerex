mod common;
use serde_json::json;
use uuid::Uuid;

// Manifest with no pipeline — we only care about correct-engagement run attribution.
fn manifest() -> serde_json::Value {
    json!({
        "id": "p", "name": "P", "version": "0.1",
        "inbound": [{ "id": "ping", "kind": "webhook", "input_schema": {} }],
        "outbound": []
    })
}

#[tokio::test]
async fn webhook_routes_to_correct_engagement_by_token() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    // Seed engagements A and B.
    let eng_a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let eng_b: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    // Two installs of the SAME plugin_id="p" in different engagements, with distinct tokens.
    sqlx::query(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest, webhook_token) VALUES ($1, 'p', $2, 'tokA')",
    )
    .bind(eng_a)
    .bind(manifest())
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest, webhook_token) VALUES ($1, 'p', $2, 'tokB')",
    )
    .bind(eng_b)
    .bind(manifest())
    .execute(&db.pool)
    .await
    .unwrap();

    // tokA -> a run under engagement A (and ONLY A).
    let r_a = server
        .post("/api/webhooks/tokA/ping")
        .json(&json!({}))
        .await;
    r_a.assert_status(axum::http::StatusCode::ACCEPTED);

    let a_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE engagement_id = $1")
        .bind(eng_a)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        a_runs, 1,
        "tokA must create exactly one run under engagement A"
    );
    let b_runs_after_a: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE engagement_id = $1")
            .bind(eng_b)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        b_runs_after_a, 0,
        "cross-tenant: tokA must NOT create a run under B"
    );

    // tokB -> a run under engagement B.
    let r_b = server
        .post("/api/webhooks/tokB/ping")
        .json(&json!({}))
        .await;
    r_b.assert_status(axum::http::StatusCode::ACCEPTED);

    let b_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE engagement_id = $1")
        .bind(eng_b)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        b_runs, 1,
        "tokB must create exactly one run under engagement B"
    );
    let a_runs_final: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE engagement_id = $1")
            .bind(eng_a)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        a_runs_final, 1,
        "engagement A still has exactly its one run"
    );

    // Unknown token -> 404.
    let r_unknown = server
        .post("/api/webhooks/unknowntok/ping")
        .json(&json!({}))
        .await;
    r_unknown.assert_status(axum::http::StatusCode::NOT_FOUND);
    let body: serde_json::Value = r_unknown.json();
    assert_eq!(body["error"], "unknown webhook");
}

#[tokio::test]
async fn app_created_install_gets_nonempty_webhook_token() {
    let db = common::start_test_db().await;
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let manifest = manifest();
    let id = celerex_api::db::install_plugin(&db.pool, eng, "p", &manifest)
        .await
        .unwrap();

    let token: String =
        sqlx::query_scalar("SELECT webhook_token FROM plugin_installs WHERE id = $1")
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(
        !token.is_empty(),
        "app-created install must have a non-empty webhook_token"
    );
}
