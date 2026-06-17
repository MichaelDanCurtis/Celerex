mod common;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: Uuid) -> String {
    // SAFETY: single-threaded test setup; sets the JWT secret the app verifies against.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None,
        exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn engagements_list_requires_auth_and_scopes_to_caller() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id").fetch_one(&db.pool).await.unwrap();
    let _eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id").fetch_one(&db.pool).await.unwrap();

    // no token → rejected by extractor (not 200)
    let anon = server.get("/api/engagements").await;
    assert_ne!(anon.status_code(), axum::http::StatusCode::OK);

    // A's token → 200, sees ONLY A
    let resp = server.get("/api/engagements")
        .add_header("authorization", format!("Bearer {}", token(eng_a))).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let rows: Vec<serde_json::Value> = resp.json();
    assert_eq!(rows.len(), 1, "caller should see exactly their own engagement");
    assert_eq!(rows[0]["id"].as_str().unwrap(), eng_a.to_string(), "should be eng_a, not eng_b");
}
