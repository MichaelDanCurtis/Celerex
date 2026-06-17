mod common;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn create_then_list_engagement_persists_in_db() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).expect("server");

    // Engagement create + list now require auth (T8 hardening). The token's
    // engagement_id claim is what `GET /api/engagements` scopes to, and minting
    // a token requires an existing engagement, so seed one directly via SQL and
    // authenticate every call with a token scoped to it.
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Scope Anchor') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    // create persists a brand-new engagement row.
    let resp = server
        .post("/api/engagements")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "Acme R&D" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "Acme R&D");

    // list is scoped to the caller's engagement (the seeded anchor), not the
    // engagement just created under a different scope.
    let list = server
        .get("/api/engagements")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    list.assert_status_ok();
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], eng.to_string());
    assert_eq!(arr[0]["name"], "Scope Anchor");
}
