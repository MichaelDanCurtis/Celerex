mod common;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn webhook_accepted_then_deduped_on_replay() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let manifest = json!({ "id": "test", "name": "T", "version": "0.1",
        "inbound": [{ "id": "ping", "kind": "webhook", "input_schema": {} }],
        "outbound": [] });
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'test', $2)")
        .bind(eng).bind(&manifest).execute(&db.pool).await.unwrap();

    let body = json!({ "id": "evt-1", "data": {} });
    let r1 = server.post("/api/webhooks/test/ping")
        .add_header("x-event-id", "evt-1")
        .json(&body).await;
    r1.assert_status(axum::http::StatusCode::ACCEPTED);

    let r2 = server.post("/api/webhooks/test/ping")
        .add_header("x-event-id", "evt-1")
        .json(&body).await;
    r2.assert_status(axum::http::StatusCode::OK);
    let body2: serde_json::Value = r2.json();
    assert_eq!(body2["deduped"], true);
}

#[tokio::test]
async fn webhook_dispatches_event_triggered_job() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let manifest = json!({
        "id": "test", "name": "T", "version": "0.1",
        "inbound": [{ "id": "ping", "kind": "webhook", "input_schema": {} }],
        "outbound": [],
        "on_event_pipeline": {
            "ping": {
                "steps": [{
                    "id": "noop",
                    "depends_on": [],
                    "kind": { "type": "filter", "expression": "true" }
                }]
            }
        }
    });
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'test', $2)")
        .bind(eng).bind(&manifest).execute(&db.pool).await.unwrap();

    let r = server.post("/api/webhooks/test/ping").json(&json!({})).await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);

    // wait briefly for async dispatch + step-record write
    for _ in 0..20 {
        let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_steps WHERE step_id = 'noop'")
            .fetch_one(&db.pool).await.unwrap();
        if c >= 1 { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_steps WHERE step_id = 'noop'")
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
}
