// Unit-level coverage for RitualSpec → graph translation. Full HTTP/Postgres
// tests live further down this file (added in later tasks).
use celerex_api::design::rituals::{build_ritual_graph, RitualBody};

#[test]
fn agent_run_body_builds_a_valid_minimal_graph() {
    let body = RitualBody::AgentRun {
        prompt: "Summarize my unread mail".into(),
        skill_template_id: Some("orbit-general".into()),
        target: None,
    };
    let graph = build_ritual_graph(&body);
    // graph must pass the engine's own validator.
    celerex_api::project_members::graph::validate(&graph).expect("ritual graph must validate");
    // carries the prompt so the run is self-contained.
    assert!(graph.to_string().contains("Summarize my unread mail"));
}

// ===========================================================================
// HTTP integration tests — POST /api/design/rituals
// Mirrors design_connectors.rs: testcontainers Postgres, axum_test::TestServer,
// device-scoped token via `celerex_api::auth::jwt::mint_scoped`.
// ===========================================================================

mod common;

async fn setup() -> (common::TestDb, axum_test::TestServer, uuid::Uuid, String) {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('RitualWS') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token =
        celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

#[tokio::test]
async fn create_ritual_agent_run_returns_201_with_required_fields() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Morning briefing",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Summarize unread mail" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = res.json();
    assert!(body["ritual_id"].is_string(), "ritual_id must be present");
    assert!(body["project_member_id"].is_string(), "project_member_id must be present");
    assert_eq!(body["enabled"], true);
    assert!(body["next_fire_at"].is_string(), "next_fire_at must be present");
}

#[tokio::test]
async fn create_ritual_rejects_bad_cron() {
    let (_db, server, _eng, token) = setup().await;

    let res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Bad cron",
            "enabled": true,
            "schedule": { "cron_expr": "not-a-cron", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Do something" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_ritual_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);

    let res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {publish}"))
        .json(&serde_json::json!({
            "name": "Should fail",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Do something" }
        }))
        .await;

    res.assert_status(axum::http::StatusCode::FORBIDDEN);
}

// ===========================================================================
// HTTP integration tests — GET /api/design/rituals, DELETE /api/design/rituals/{id}
// ===========================================================================

#[tokio::test]
async fn list_then_delete_ritual_lifecycle() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create a ritual.
    let create_res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "Daily digest",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 8 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Send daily digest" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let ritual_id = created["ritual_id"].as_str().expect("ritual_id must be a string");

    // 2. GET list → ritual_id appears.
    let list_res = server
        .get("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    list_res.assert_status(axum::http::StatusCode::OK);
    let list_body: serde_json::Value = list_res.json();
    let rituals = list_body["rituals"].as_array().expect("rituals must be an array");
    assert!(
        rituals.iter().any(|r| r["ritual_id"].as_str() == Some(ritual_id)),
        "created ritual_id must appear in list"
    );

    // 3. DELETE → 204.
    let delete_res = server
        .delete(&format!("/api/design/rituals/{ritual_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    delete_res.assert_status(axum::http::StatusCode::NO_CONTENT);

    // 4. GET list again → ritual_id gone.
    let list_res2 = server
        .get("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    list_res2.assert_status(axum::http::StatusCode::OK);
    let list_body2: serde_json::Value = list_res2.json();
    let rituals2 = list_body2["rituals"].as_array().expect("rituals must be an array");
    assert!(
        !rituals2.iter().any(|r| r["ritual_id"].as_str() == Some(ritual_id)),
        "deleted ritual_id must not appear in list"
    );
}

// ===========================================================================
// HTTP integration tests — PUT /api/design/rituals/{id}
// ===========================================================================

#[tokio::test]
async fn update_changes_schedule_and_enabled() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create a ritual.
    let create_res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "To be updated",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Initial prompt" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let ritual_id = created["ritual_id"].as_str().expect("ritual_id must be a string");

    // 2. PUT with new cron + enabled=false.
    let update_res = server
        .put(&format!("/api/design/rituals/{ritual_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "cron_expr": "0 0 18 * * *",
            "timezone": "UTC",
            "enabled": false
        }))
        .await;
    update_res.assert_status(axum::http::StatusCode::OK);
    let updated: serde_json::Value = update_res.json();
    assert_eq!(updated["enabled"], false, "enabled must be false after update");
    assert_eq!(
        updated["cron_expr"].as_str().expect("cron_expr must be a string"),
        "0 0 18 * * *",
        "cron_expr must reflect the new schedule"
    );
}

// ===========================================================================
// HTTP integration tests — POST /api/design/rituals/{id}/run
//                          GET  /api/design/rituals/{id}/runs
// ===========================================================================

#[tokio::test]
async fn run_now_enqueues_a_run_visible_in_runs() {
    let (_db, server, _eng, token) = setup().await;

    // 1. Create a ritual.
    let create_res = server
        .post("/api/design/rituals")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "On-demand ritual",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 9 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Do something now" }
        }))
        .await;
    create_res.assert_status(axum::http::StatusCode::CREATED);
    let created: serde_json::Value = create_res.json();
    let ritual_id = created["ritual_id"].as_str().expect("ritual_id must be a string");

    // 2. POST /run → 202 + run_id.
    // enqueue_run inserts the runs row synchronously (status 'running') before
    // spawning the async executor task, so run_id is valid immediately.
    let run_res = server
        .post(&format!("/api/design/rituals/{ritual_id}/run"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    run_res.assert_status(axum::http::StatusCode::ACCEPTED);
    let run_body: serde_json::Value = run_res.json();
    assert!(run_body["run_id"].is_string(), "run_id must be present in run-now response");

    // 3. GET /runs → the runs array is non-empty (the row was inserted before enqueue_run returned).
    let runs_res = server
        .get(&format!("/api/design/rituals/{ritual_id}/runs"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    runs_res.assert_status(axum::http::StatusCode::OK);
    let runs_body: serde_json::Value = runs_res.json();
    let runs = runs_body["runs"].as_array().expect("runs must be an array");
    assert!(!runs.is_empty(), "at least one run must be visible after run-now");
    // Confirm the returned run_id appears in the list.
    let enqueued_run_id = run_body["run_id"].as_str().unwrap();
    assert!(
        runs.iter().any(|r| r["run_id"].as_str() == Some(enqueued_run_id)),
        "the enqueued run_id must appear in the runs list"
    );
}
