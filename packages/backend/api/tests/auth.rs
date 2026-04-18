mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn make_token(secret: &str, eng: Uuid, proj: Option<Uuid>) -> String {
    let claims = Claims {
        sub: "user-1".into(),
        engagement_id: eng.to_string(),
        project_id: proj.map(|p| p.to_string()),
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
}

#[tokio::test]
async fn protected_route_rejects_missing_token() {
    // SAFETY: tests serialized within one binary; env is set before any middleware
    // reads it. Rust 2024 flagged set_var as unsafe because mid-execution writes
    // can race with reads in other threads.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let resp = server.get("/api/projects").await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_accepts_valid_token() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let token = make_token("test-secret", Uuid::new_v4(), None);
    let resp = server.get("/api/projects")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
}
