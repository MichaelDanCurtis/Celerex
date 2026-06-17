mod common;

use axum::http::StatusCode;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

// SAFETY: tests are single-threaded per-binary at the point env is read here; we
// set the JWT secret before the app verifies tokens. Mirrors tests/runs_sse_auth.rs.
fn token(eng: Uuid) -> String {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

/// (a) The assistant must require authentication. With no bearer token, the
/// request is rejected (never 200).
#[tokio::test]
async fn assistant_chat_requires_auth() {
    // SAFETY: env set before the app is built (single-threaded test setup).
    unsafe {
        std::env::set_var("CELEREX_HOUSE_MODE", "false");
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let res = server
        .post("/api/assistant/chat")
        .json(&serde_json::json!({
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;

    assert_ne!(
        res.status_code(),
        StatusCode::OK,
        "unauthenticated assistant chat must be rejected"
    );
}

/// (b) No silent house spend. House mode OFF + engagement with no vaulted key →
/// authenticated assistant chat returns 409 CONFLICT (not a house-key success,
/// not a 502 upstream attempt).
#[tokio::test]
async fn assistant_chat_no_key_is_conflict_when_house_off() {
    // SAFETY: env set before the app is built (single-threaded test setup).
    unsafe {
        std::env::set_var("CELEREX_HOUSE_MODE", "false");
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    let res = server
        .post("/api/assistant/chat")
        .add_header("authorization", format!("Bearer {}", token(eng_a)))
        .json(&serde_json::json!({
            "provider_id": "anthropic",
            "messages": [{ "role": "user", "content": "hi" }]
        }))
        .await;

    assert_eq!(
        res.status_code(),
        StatusCode::CONFLICT,
        "no vaulted key + house mode off must be 409, got {}: {}",
        res.status_code(),
        res.text()
    );
}
