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

#[tokio::test]
async fn login_returns_token_for_existing_engagement() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    // Seed an engagement directly so we control the name we'll look up.
    let eng = sqlx::query_as::<_, (Uuid, String)>(
        "INSERT INTO engagements (name) VALUES ($1) RETURNING id, name",
    )
    .bind("Acme Co")
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    // Case-insensitive lookup ("ACME CO" should match "Acme Co").
    let resp = server
        .post("/api/auth/login")
        .json(&serde_json::json!({ "engagement_name": "ACME CO" }))
        .await;
    resp.assert_status_ok();

    let body: serde_json::Value = resp.json();
    assert!(body["token"].as_str().unwrap().len() > 0);
    assert_eq!(body["engagement"]["id"].as_str().unwrap(), eng.0.to_string());
    assert_eq!(body["engagement"]["name"].as_str().unwrap(), "Acme Co");

    // The returned token should grant access to a protected route.
    let token = body["token"].as_str().unwrap();
    let protected = server
        .get("/api/projects")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    protected.assert_status_ok();
}

#[tokio::test]
async fn login_returns_404_for_unknown_engagement() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let resp = server
        .post("/api/auth/login")
        .json(&serde_json::json!({ "engagement_name": "no-such-engagement" }))
        .await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}
