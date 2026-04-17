mod common;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).expect("server");
    let resp = server.get("/api/health").await;
    resp.assert_status_ok();
}
