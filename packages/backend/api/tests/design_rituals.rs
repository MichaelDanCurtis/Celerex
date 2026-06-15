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
