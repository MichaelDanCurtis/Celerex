mod common;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: Uuid) -> String {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None,
        exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn sse_requires_auth_and_blocks_other_engagement() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id").fetch_one(&db.pool).await.unwrap();
    let eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id").fetch_one(&db.pool).await.unwrap();
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id,name) VALUES ($1,'PA') RETURNING id").bind(eng_a).fetch_one(&db.pool).await.unwrap();
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO runs (id,engagement_id,project_id,project_member_id,trigger,status,snapshot_id,triggered_by) VALUES ($1,$2,$3,NULL,'{}'::jsonb,'running',NULL,'manual')")
        .bind(run_id).bind(eng_a).bind(proj_a).execute(&db.pool).await.unwrap();

    // no token → not OK
    let anon = server.get(&format!("/api/runs/{run_id}/events")).await;
    assert_ne!(anon.status_code(), axum::http::StatusCode::OK);

    // B's token → 403
    let resp = server.get(&format!("/api/runs/{run_id}/events"))
        .add_header("authorization", format!("Bearer {}", token(eng_b))).await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}
