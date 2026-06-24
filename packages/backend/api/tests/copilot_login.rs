mod common;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    encode_token(eng, None)
}

/// Mint a token carrying an explicit capability `scope` (e.g. the daemon's
/// `design.device` pairing scope).
fn scoped_token(eng: Uuid, scope: &str) -> String {
    encode_token(eng, Some(scope.to_string()))
}

fn encode_token(eng: Uuid, scope: Option<String>) -> String {
    // SAFETY: single-process test
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        scope,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

#[tokio::test]
async fn copilot_device_flow_vaults_token_and_is_isolated() {
    // GitHub OAuth device-flow mock.
    let gh = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dev-123",
            "user_code": "WXYZ-1234",
            "verification_uri": "https://github.com/login/device",
            "interval": 1,
            "expires_in": 900
        })))
        .mount(&gh)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "ghu_abc"
        })))
        .mount(&gh)
        .await;

    // SAFETY: single-process test; set before any handler builds the oauth client.
    unsafe {
        std::env::set_var("GITHUB_OAUTH_BASE", gh.uri());
        std::env::set_var("GITHUB_API_BASE", gh.uri());
    }

    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    // Start the device flow.
    let start = server
        .post("/api/design/copilot/login/start")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    start.assert_status_ok();
    assert_eq!(
        start.json::<serde_json::Value>()["user_code"],
        serde_json::json!("WXYZ-1234")
    );

    // The background poll (interval 1s) should vault ghu_abc shortly. Bounded poll.
    let mut connected = false;
    for _ in 0..20 {
        let s = server
            .get("/api/design/copilot/login/status")
            .add_header("authorization", format!("Bearer {}", token(a)))
            .await;
        s.assert_status_ok();
        if s.json::<serde_json::Value>()["connected"] == serde_json::json!(true) {
            connected = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        connected,
        "expected copilot token to be vaulted for engagement A"
    );

    // Cross-engagement isolation: B has no login row and no vaulted token.
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let sb = server
        .get("/api/design/copilot/login/status")
        .add_header("authorization", format!("Bearer {}", token(b)))
        .await;
    sb.assert_status_ok();
    let sbody = sb.json::<serde_json::Value>();
    assert_eq!(sbody["connected"], serde_json::json!(false));
    assert_eq!(sbody["status"], serde_json::json!("none"));

    // Unauthenticated start → 401.
    let anon = server.post("/api/design/copilot/login/start").await;
    assert_eq!(anon.status_code(), axum::http::StatusCode::UNAUTHORIZED);
}

/// The local daemon drives this flow with its engagement-scoped `design.device`
/// pairing token. The routes are guarded by `DeviceAuth`, so that scope must be
/// accepted (it was rejected by the old `AuthClaims` guard), while an unrelated
/// capability scope must still be refused.
#[tokio::test]
async fn copilot_login_accepts_device_scope_rejects_foreign_scope() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    // status reads only the DB (no GitHub call), so it cleanly exercises the
    // extractor's scope policy across all three DeviceAuth branches.

    // (1) A full, unscoped session token must STILL be accepted (the web-direct
    // path keeps working after the AuthClaims -> DeviceAuth change).
    let session = server
        .get("/api/design/copilot/login/status")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    session.assert_status_ok();

    // (2) The daemon's engagement-scoped `design.device` pairing token must be
    // accepted (it was rejected by the old `AuthClaims` guard).
    let device = server
        .get("/api/design/copilot/login/status")
        .add_header(
            "authorization",
            format!("Bearer {}", scoped_token(a, "design.device")),
        )
        .await;
    device.assert_status_ok();
    assert_eq!(
        device.json::<serde_json::Value>()["connected"],
        serde_json::json!(false)
    );

    // A foreign capability scope must still be rejected (403), so a leaked
    // narrowly-scoped credential can't manage Copilot logins.
    let foreign = server
        .get("/api/design/copilot/login/status")
        .add_header(
            "authorization",
            format!("Bearer {}", scoped_token(a, "some.other.scope")),
        )
        .await;
    assert_eq!(foreign.status_code(), axum::http::StatusCode::FORBIDDEN);
}
