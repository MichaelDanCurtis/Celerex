//! Celerex Design handoff — ingest end-to-end (piece B).
//!
//! Ingest a live-mode bundle (template carries a <script> + javascript: URL to
//! prove sanitization, plus an asset), then list/fetch it, confirm a re-publish
//! from the same desktop project bumps the version (no duplicate), and that the
//! STORED template still renders through the Render engine.

mod common;

use axum::http::StatusCode;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use celerex_orchestrator::render::{parse_slots, render_template};
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn ingest_sanitizes_stores_lists_and_republishes() {
    // SAFETY: tests serialized in one binary; env set before middleware reads it.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Design WS') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    let bundle = json!({
        "manifest": {
            "cdb_version": "0.1",
            "kind": "newsletter",
            "mode": "live",
            "title": "Weekly Digest",
            "render": { "engine": "mjml" },
            "source": { "project_ref": "desk-1" }
        },
        "template_html": "<h1>{{slot:headline}}</h1><script>steal()</script><a href=\"javascript:bad()\" onclick=\"x()\">link</a>",
        "tokens_css": ":root{--c:#fff}",
        "slots": [{ "name": "headline", "type": "text", "required": true }],
        "assets": [{ "path": "assets/logo.png", "content_type": "image/png", "base64": STANDARD.encode(b"PNGDATA") }]
    });

    // --- Ingest -------------------------------------------------------------
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&bundle)
        .await;
    res.assert_status(StatusCode::CREATED);
    let body: Value = res.json();
    assert_eq!(body["sanitized"], true, "template had script/handler → sanitized");
    assert_eq!(body["mode"], "live");
    assert_eq!(body["kind"], "newsletter");
    assert_eq!(body["version"], 1);
    assert_eq!(body["asset_count"], 1);
    let template_id = body["template_id"].as_str().unwrap().to_string();

    // --- Library list -------------------------------------------------------
    let res = server
        .get("/api/design/templates")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    res.assert_status_ok();
    let list: Value = res.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["title"], "Weekly Digest");
    assert_eq!(list[0]["render_engine"], "mjml");

    // --- Full template: confirm it was sanitized but slots preserved --------
    let res = server
        .get(&format!("/api/design/templates/{template_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    res.assert_status_ok();
    let detail: Value = res.json();
    let html = detail["template_html"].as_str().unwrap();
    assert!(!html.contains("<script>"), "script stripped");
    assert!(!html.contains("javascript:"), "js scheme neutralized");
    assert!(!html.contains("onclick"), "event handler stripped");
    assert!(html.contains("{{slot:headline}}"), "slot placeholder preserved");

    // --- The stored template still renders through the Render engine --------
    let slots = parse_slots(&detail["slots"]);
    let rendered = render_template(html, &slots, &json!({ "headline": "Q3 Update" })).unwrap();
    assert!(rendered.contains("<h1>Q3 Update</h1>"));

    // --- Asset is auth-scoped + served --------------------------------------
    // (asset id isn't returned by ingest in v0.1; fetch via DB to exercise the route)
    let asset_id: Uuid = sqlx::query_scalar("SELECT id FROM design_assets WHERE template_id = $1")
        .bind(Uuid::parse_str(&template_id).unwrap())
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let res = server
        .get(&format!("/api/design/assets/{asset_id}"))
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    res.assert_status_ok();
    assert_eq!(res.as_bytes().to_vec(), b"PNGDATA");

    // --- Re-publish from the same desktop project bumps version, no dup -----
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&bundle)
        .await;
    res.assert_status(StatusCode::CREATED);
    assert_eq!(res.json::<Value>()["version"], 2);
    let res = server
        .get("/api/design/templates")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    assert_eq!(res.json::<Value>().as_array().unwrap().len(), 1, "re-publish updates, not duplicates");
}

#[tokio::test]
async fn ingest_rejects_bad_manifest() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('X') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    // Unsupported cdb_version → 400.
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "manifest": { "cdb_version": "9.9", "kind": "newsletter", "mode": "static" }, "template_html": "<p>x</p>" }))
        .await;
    res.assert_status(StatusCode::BAD_REQUEST);

    // live mode without slots → 400.
    let res = server
        .post("/api/design/ingest")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "manifest": { "cdb_version": "0.1", "kind": "newsletter", "mode": "live" }, "template_html": "<p>x</p>" }))
        .await;
    res.assert_status(StatusCode::BAD_REQUEST);
}
