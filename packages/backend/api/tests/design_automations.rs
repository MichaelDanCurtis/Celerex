// Integration tests for POST/GET/PUT/DELETE /api/design/automations
// and the sub-routes /{id}/run, /{id}/runs.
// Mirrors design_rituals.rs: testcontainers Postgres + axum_test::TestServer +
// device-scoped token via `celerex_api::auth::jwt::mint_scoped`.
mod common;

async fn setup() -> (common::TestDb, axum_test::TestServer, uuid::Uuid, String) {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('AutomationWS') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

// ===========================================================================
// Task 6: POST /api/design/automations — create
// ===========================================================================

#[tokio::test]
async fn create_automation_agent_run_returns_201_with_required_fields() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "New item monitor",
            "connector_id": "http-api",
            "source_config": {
                "url": "https://example.com/items",
                "items_path": "items",
                "id_field": "id"
            },
            "poll_interval_secs": 120,
            "body": { "kind": "agent_run", "prompt": "Summarise the new item" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = res.json();
    assert!(body["automation_id"].is_string(), "automation_id must be present");
    assert!(body["project_member_id"].is_string(), "project_member_id must be present");
    assert_eq!(body["connector_id"], "http-api");
    assert_eq!(body["poll_interval_secs"], 120);
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn create_automation_defaults_poll_interval_and_enabled() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Default interval",
            "connector_id": "http-api",
            "source_config": { "url": "https://x.example/feed", "items_path": "data", "id_field": "id" },
            "body": { "kind": "agent_run", "prompt": "Do something" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = res.json();
    assert_eq!(body["poll_interval_secs"], 300, "default poll_interval_secs must be 300");
    assert_eq!(body["enabled"], true, "default enabled must be true");
}

#[tokio::test]
async fn create_automation_rejects_empty_connector_id() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Bad connector",
            "connector_id": "",
            "source_config": { "url": "https://x.example/feed", "items_path": "data", "id_field": "id" },
            "body": { "kind": "agent_run", "prompt": "Do something" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_automation_rejects_non_object_source_config() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Bad source_config",
            "connector_id": "http-api",
            "source_config": "not-an-object",
            "body": { "kind": "agent_run", "prompt": "Do something" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_automation_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);

    let res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {publish}"))
        .json(&serde_json::json!({
            "name": "Should fail",
            "connector_id": "http-api",
            "source_config": { "url": "https://x/f", "items_path": "i", "id_field": "id" },
            "body": { "kind": "agent_run", "prompt": "nope" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::FORBIDDEN);
}

// ===========================================================================
// Task 7: lifecycle — create → list → delete
// ===========================================================================

#[tokio::test]
async fn list_then_delete_automation_lifecycle() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create.
    let create_res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Item watcher",
            "connector_id": "http-api",
            "source_config": { "url": "https://example.com/feed", "items_path": "items", "id_field": "id" },
            "poll_interval_secs": 60,
            "body": { "kind": "agent_run", "prompt": "Process item" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let automation_id = created["automation_id"]
        .as_str()
        .expect("automation_id must be a string");

    // 2. GET list → automation_id appears.
    let list_res = server
        .get("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    list_res.assert_status(axum::http::StatusCode::OK);
    let list_body: serde_json::Value = list_res.json();
    let automations = list_body["automations"]
        .as_array()
        .expect("automations must be an array");
    assert!(
        automations
            .iter()
            .any(|a| a["automation_id"].as_str() == Some(automation_id)),
        "created automation_id must appear in list"
    );

    // 3. DELETE → 204.
    let delete_res = server
        .delete(&format!("/api/design/automations/{automation_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    delete_res.assert_status(axum::http::StatusCode::NO_CONTENT);

    // 4. GET list again → automation_id gone.
    let list_res2 = server
        .get("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    list_res2.assert_status(axum::http::StatusCode::OK);
    let list_body2: serde_json::Value = list_res2.json();
    let automations2 = list_body2["automations"]
        .as_array()
        .expect("automations must be an array");
    assert!(
        !automations2
            .iter()
            .any(|a| a["automation_id"].as_str() == Some(automation_id)),
        "deleted automation_id must not appear in list"
    );
}

// ===========================================================================
// Task 7: update — PUT /{id} toggles enabled + interval
// ===========================================================================

#[tokio::test]
async fn update_changes_poll_interval_and_enabled() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create.
    let create_res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "To be updated",
            "connector_id": "http-api",
            "source_config": { "url": "https://example.com/a", "items_path": "items", "id_field": "id" },
            "poll_interval_secs": 120,
            "body": { "kind": "agent_run", "prompt": "Initial prompt" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let automation_id = created["automation_id"]
        .as_str()
        .expect("automation_id must be a string");

    // 2. PUT with new interval + enabled=false.
    let update_res = server
        .put(&format!("/api/design/automations/{automation_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "source_config": { "url": "https://example.com/b", "items_path": "items", "id_field": "id" },
            "poll_interval_secs": 600,
            "enabled": false
        }))
        .await;
    update_res.assert_status(axum::http::StatusCode::OK);
    let updated: serde_json::Value = update_res.json();
    assert_eq!(updated["enabled"], false, "enabled must be false after update");
    assert_eq!(
        updated["poll_interval_secs"], 600,
        "poll_interval_secs must reflect the new value"
    );
}

// ===========================================================================
// Task 7: run-now + runs
// ===========================================================================

#[tokio::test]
async fn run_now_enqueues_a_run_visible_in_runs() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create.
    let create_res = server
        .post("/api/design/automations")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "On-demand automation",
            "connector_id": "http-api",
            "source_config": { "url": "https://example.com/feed", "items_path": "items", "id_field": "id" },
            "body": { "kind": "agent_run", "prompt": "Do something now" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let automation_id = created["automation_id"]
        .as_str()
        .expect("automation_id must be a string");

    // 2. POST /run → 202 + run_id.
    let run_res = server
        .post(&format!("/api/design/automations/{automation_id}/run"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    run_res.assert_status(axum::http::StatusCode::ACCEPTED);
    let run_body: serde_json::Value = run_res.json();
    assert!(
        run_body["run_id"].is_string(),
        "run_id must be present in run-now response"
    );

    // 3. GET /runs → non-empty, triggered_by=manual.
    let runs_res = server
        .get(&format!("/api/design/automations/{automation_id}/runs"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    runs_res.assert_status(axum::http::StatusCode::OK);
    let runs_body: serde_json::Value = runs_res.json();
    let runs = runs_body["runs"].as_array().expect("runs must be an array");
    assert!(!runs.is_empty(), "at least one run must be visible after run-now");
    let enqueued_run_id = run_body["run_id"].as_str().unwrap();
    let matched = runs
        .iter()
        .find(|r| r["run_id"].as_str() == Some(enqueued_run_id))
        .expect("the enqueued run_id must appear in the runs list");
    assert_eq!(
        matched["triggered_by"], "manual",
        "an on-demand run-now must record triggered_by=manual"
    );
}
