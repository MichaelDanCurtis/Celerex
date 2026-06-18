mod common;

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

/// Seed an engagement and return its id.
async fn seed_engagement(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Seed a live `design_templates` row owned by `engagement_id` and return its id.
/// Mirrors the ingest handler's columns (NOT NULL: engagement_id, title, kind,
/// mode, template_html); `id`/`render_engine`/`slots` carry DB defaults but we
/// set render_engine + slots explicitly so T3's render/fill reuse the shape.
async fn seed_template(pool: &sqlx::PgPool, engagement_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO design_templates
            (engagement_id, title, kind, mode, render_engine, template_html, slots)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(engagement_id)
    .bind("Living Newsletter")
    .bind("newsletter")
    .bind("live")
    .bind("html")
    .bind("<h1>{{slot:headline}}</h1>")
    .bind(json!([{ "name": "headline", "type": "text", "required": true }]))
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn set_instruction_and_list_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let server =
        axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).expect("server");

    // Engagement A owns a template.
    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng_a).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    // PUT instruction (Bearer A) -> 200.
    let put = server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "instruction": "Write a punchy headline" }))
        .await;
    put.assert_status_ok();

    // GET slots (Bearer A) -> 200, contains the instruction we just set.
    let list = server
        .get(&format!("/api/design/templates/{template_id}/slots"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .await;
    list.assert_status_ok();
    let body: serde_json::Value = list.json();
    let slots = body["slots"].as_array().expect("slots array");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0]["slot_name"], "headline");
    assert_eq!(slots[0]["instruction"], "Write a punchy headline");

    // Engagement B doesn't own the template.
    let eng_b = seed_engagement(&db.pool, "Engagement B").await;
    let token_b = celerex_api::auth::jwt::mint_for_tests(eng_b);

    // GET slots (Bearer B) -> 404 (cross-tenant mirrors the design 404 convention).
    let cross_get = server
        .get(&format!("/api/design/templates/{template_id}/slots"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .await;
    cross_get.assert_status(StatusCode::NOT_FOUND);

    // PUT instruction (Bearer B) -> 404.
    let cross_put = server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({ "instruction": "sneaky" }))
        .await;
    cross_put.assert_status(StatusCode::NOT_FOUND);

    // Unauthenticated PUT -> 401.
    let unauth = server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .json(&json!({ "instruction": "no token" }))
        .await;
    unauth.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn empty_instruction_is_rejected() {
    let db = common::start_test_db().await;
    let server =
        axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).expect("server");

    let eng_a = seed_engagement(&db.pool, "Engagement A").await;
    let template_id = seed_template(&db.pool, eng_a).await;
    let token_a = celerex_api::auth::jwt::mint_for_tests(eng_a);

    let put = server
        .put(&format!(
            "/api/design/templates/{template_id}/slots/headline/instruction"
        ))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "instruction": "   " }))
        .await;
    put.assert_status(StatusCode::BAD_REQUEST);
}
