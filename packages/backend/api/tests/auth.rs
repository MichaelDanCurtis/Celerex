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
async fn signup_then_login_round_trips_and_grants_access() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    // Signup provisions the engagement + the account and returns a token.
    let resp = server
        .post("/api/auth/signup")
        .json(&serde_json::json!({
            "email": "alice@acme.test",
            "password": "correct horse battery staple",
            "engagement_name": "Acme Co"
        }))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert!(body["token"].as_str().unwrap().len() > 0);
    assert_eq!(body["engagement"]["name"].as_str().unwrap(), "Acme Co");
    let signup_eng = body["engagement"]["id"].as_str().unwrap().to_string();

    // Login with the right password works, returns the SAME engagement, and
    // the token grants access to a protected route.
    let resp = server
        .post("/api/auth/login")
        .json(&serde_json::json!({ "email": "ALICE@acme.test", "password": "correct horse battery staple" }))
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["engagement"]["id"].as_str().unwrap(), signup_eng);
    let token = body["token"].as_str().unwrap();
    let protected = server
        .get("/api/projects")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    protected.assert_status_ok();
}

#[tokio::test]
async fn login_rejects_wrong_password_and_unknown_email_with_401() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    server
        .post("/api/auth/signup")
        .json(&serde_json::json!({
            "email": "bob@acme.test", "password": "hunter2hunter2", "engagement_name": "Bob Co"
        }))
        .await
        .assert_status_ok();

    // Wrong password → 401.
    server
        .post("/api/auth/login")
        .json(&serde_json::json!({ "email": "bob@acme.test", "password": "wrongwrongwrong" }))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);

    // Unknown email → 401 (not 404 — no account-enumeration oracle).
    server
        .post("/api/auth/login")
        .json(&serde_json::json!({ "email": "nobody@acme.test", "password": "whatever12" }))
        .await
        .assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signup_rejects_duplicate_email() {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let body = serde_json::json!({
        "email": "dup@acme.test", "password": "password123", "engagement_name": "Dup Co"
    });
    server.post("/api/auth/signup").json(&body).await.assert_status_ok();
    // Case-insensitive duplicate → 409.
    server
        .post("/api/auth/signup")
        .json(&serde_json::json!({
            "email": "DUP@acme.test", "password": "password123", "engagement_name": "Dup Co"
        }))
        .await
        .assert_status(axum::http::StatusCode::CONFLICT);
}
