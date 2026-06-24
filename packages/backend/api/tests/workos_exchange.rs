mod common;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The static Designer SPA forwards the WorkOS authorization `code` to
/// `POST /api/auth/workos/exchange`; the backend exchanges it server-side (the
/// SPA can't — CORS + client secret), provisions the personal engagement, and
/// mints the session JWT. This is the single login for the unified service.
///
/// One test (not two) because `WORKOS_API_BASE` is process-global; parallel tests
/// setting it to different mocks would race.
#[tokio::test]
async fn workos_code_exchange_drives_the_single_login() {
    let workos = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/user_management/authenticate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user": { "id": "user_abc123", "email": "designer@omnivyx.test" },
            "access_token": "wos_at",
            "refresh_token": "wos_rt"
        })))
        .mount(&workos)
        .await;

    // SAFETY: single-process test. The Agent reads these to do the exchange.
    unsafe {
        std::env::set_var("WORKOS_API_BASE", workos.uri());
        std::env::set_var("WORKOS_CLIENT_ID", "client_test");
        std::env::set_var("WORKOS_API_KEY", "sk_test_secret");
    }

    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    // First exchange: provisions a fresh engagement + returns a session token.
    let first = server
        .post("/api/auth/workos/exchange")
        .json(&json!({ "code": "wos_auth_code_1" }))
        .await;
    first.assert_status_ok();
    let body = first.json::<serde_json::Value>();
    assert!(
        body["token"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "expected a non-empty session token"
    );
    let eng_id = body["engagement"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        body["engagement"]["name"],
        json!("designer@omnivyx.test's workspace")
    );

    // The minted token must be accepted by an existing protected route.
    let token = body["token"].as_str().unwrap();
    let settings = server
        .get("/api/settings")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    settings.assert_status_ok();

    // Second exchange for the SAME WorkOS user → SAME engagement (find, not create).
    let second = server
        .post("/api/auth/workos/exchange")
        .json(&json!({ "code": "wos_auth_code_2" }))
        .await;
    second.assert_status_ok();
    assert_eq!(
        second.json::<serde_json::Value>()["engagement"]["id"]
            .as_str()
            .unwrap(),
        eng_id,
        "same WorkOS user must map to the same engagement"
    );

    // A WorkOS rejection of the code surfaces as 401, not 500. Repoint the base
    // at a rejecting mock (same process, after the happy-path assertions).
    let bad = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/user_management/authenticate"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant", "error_description": "authorization code is invalid"
        })))
        .mount(&bad)
        .await;
    // SAFETY: single-process test.
    unsafe {
        std::env::set_var("WORKOS_API_BASE", bad.uri());
    }
    let rejected = server
        .post("/api/auth/workos/exchange")
        .json(&json!({ "code": "bogus" }))
        .await;
    assert_eq!(rejected.status_code(), axum::http::StatusCode::UNAUTHORIZED);
}
