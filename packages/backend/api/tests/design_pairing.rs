//! Celerex Design handoff — device credentials + live-flow auto-scaffold (piece C).
//!
//! Three guarantees:
//!  1. A `design.publish` scoped token can publish bundles but is inert on every
//!     other API route (so a leaked desktop credential can't drain the engagement).
//!  2. The device-pairing handshake (start → approve → poll) hands the desktop a
//!     working scoped token, delivered exactly once.
//!  3. Ingesting a `live` bundle auto-scaffolds a 5-node delivery flow, and a
//!     re-publish refreshes that same flow instead of duplicating it.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn live_bundle() -> Value {
    json!({
        "manifest": {
            "cdb_version": "0.1",
            "kind": "newsletter",
            "mode": "live",
            "title": "Weekly Digest",
            "render": { "engine": "mjml" },
            "source": { "project_ref": "desk-1" }
        },
        "template_html": "<h1>{{slot:headline}}</h1>",
        "tokens_css": ":root{--c:#fff}",
        "slots": [{ "name": "headline", "type": "text", "required": true }],
        "assets": []
    })
}

#[tokio::test]
async fn scoped_token_can_publish_but_nothing_else() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let session = celerex_api::auth::jwt::mint_for_tests(eng);

    // Mint a design.publish device token using the full session token.
    let res = server
        .post("/api/design/tokens")
        .add_header("authorization", format!("Bearer {session}"))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["scope"], "design.publish");
    let scoped = body["token"].as_str().unwrap().to_string();

    // The scoped token CAN publish.
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {scoped}"))
        .json(&live_bundle())
        .await;
    res.assert_status(StatusCode::CREATED);

    // …but is rejected on the design library (full-session-only) route…
    let res = server
        .get("/api/design/templates")
        .add_header("authorization", format!("Bearer {scoped}"))
        .await;
    res.assert_status(StatusCode::FORBIDDEN);

    // …and on an unrelated route.
    let res = server
        .get("/api/projects")
        .add_header("authorization", format!("Bearer {scoped}"))
        .await;
    res.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn device_pairing_hands_over_a_working_token_once() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let session = celerex_api::auth::jwt::mint_for_tests(eng);

    // 1. Desktop (no creds) starts pairing.
    let res = server
        .post("/api/design/pair/start")
        .json(&json!({ "label": "Mike's MacBook" }))
        .await;
    res.assert_status(StatusCode::CREATED);
    let started: Value = res.json();
    let device_code = started["device_code"].as_str().unwrap().to_string();
    let user_code = started["user_code"].as_str().unwrap().to_string();

    // 2. Before approval, poll is pending with no token.
    let res = server.post("/api/design/pair/poll").json(&json!({ "device_code": device_code })).await;
    res.assert_status_ok();
    assert_eq!(res.json::<Value>()["status"], "pending");

    // 3. The signed-in human approves the short code.
    let res = server
        .post("/api/design/pair/approve")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&json!({ "user_code": user_code }))
        .await;
    res.assert_status_ok();
    assert_eq!(res.json::<Value>()["label"], "Mike's MacBook");

    // 4. Desktop polls again → approved + token.
    let res = server.post("/api/design/pair/poll").json(&json!({ "device_code": device_code })).await;
    res.assert_status_ok();
    let polled: Value = res.json();
    assert_eq!(polled["status"], "approved");
    let token = polled["token"].as_str().unwrap().to_string();

    // 5. Token is delivered ONCE — a second poll no longer returns it.
    let res = server.post("/api/design/pair/poll").json(&json!({ "device_code": device_code })).await;
    assert!(res.json::<Value>()["token"].is_null(), "token must be handed over only once");

    // 6. The handed-over token actually works for publishing.
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&live_bundle())
        .await;
    res.assert_status(StatusCode::CREATED);

    // Unknown code → 404.
    let res = server.post("/api/design/pair/approve")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&json!({ "user_code": "ZZZZ-ZZZZ" }))
        .await;
    res.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn live_ingest_scaffolds_a_delivery_flow_and_republish_refreshes_it() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let session = celerex_api::auth::jwt::mint_for_tests(eng);

    // First publish → flow scaffolded.
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&live_bundle())
        .await;
    res.assert_status(StatusCode::CREATED);
    let body: Value = res.json();
    let flow_id = body["flow_member_id"].as_str().expect("live ingest returns a flow id").to_string();

    // The scaffolded member exists with the contract's 5-node shape.
    let graph: Value = sqlx::query_scalar("SELECT graph_json FROM project_members WHERE id = $1")
        .bind(Uuid::parse_str(&flow_id).unwrap())
        .fetch_one(&db.pool).await.unwrap();
    let ids: Vec<&str> = graph["nodes"].as_array().unwrap()
        .iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["trigger", "fill", "render", "review", "deliver"]);

    // It lives under an auto-created "Design Deliveries" project.
    let proj_name: String = sqlx::query_scalar(
        "SELECT p.name FROM projects p JOIN project_members m ON m.project_id = p.id WHERE m.id = $1")
        .bind(Uuid::parse_str(&flow_id).unwrap())
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(proj_name, "Design Deliveries");

    // Re-publish (new design) → SAME flow id, refreshed, no duplicate member/project.
    let mut bundle2 = live_bundle();
    bundle2["template_html"] = json!("<h2>{{slot:headline}}</h2>");
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&bundle2)
        .await;
    res.assert_status(StatusCode::CREATED);
    assert_eq!(res.json::<Value>()["flow_member_id"].as_str().unwrap(), flow_id, "re-publish reuses the flow");

    let member_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_members")
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(member_count, 1, "re-publish refreshes, not duplicates");

    // The refresh propagated the new design into the Render node.
    let graph: Value = sqlx::query_scalar("SELECT graph_json FROM project_members WHERE id = $1")
        .bind(Uuid::parse_str(&flow_id).unwrap())
        .fetch_one(&db.pool).await.unwrap();
    let render = graph["nodes"].as_array().unwrap()
        .iter().find(|n| n["id"] == "render").unwrap();
    assert_eq!(render["config"]["template_html"], "<h2>{{slot:headline}}</h2>");
}
