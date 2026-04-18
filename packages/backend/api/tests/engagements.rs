mod common;
use serde_json::json;

#[tokio::test]
async fn create_then_list_engagement_persists_in_db() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).expect("server");

    let resp = server.post("/api/engagements")
        .json(&json!({ "name": "Acme R&D" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "Acme R&D");

    let list = server.get("/api/engagements").await;
    list.assert_status_ok();
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "Acme R&D");
}
