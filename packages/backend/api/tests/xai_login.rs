mod common;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    // SAFETY: single-process test; JWT secret matches start_test_db().
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

#[tokio::test]
async fn xai_device_flow_vaults_blob_and_is_isolated() {
    // xAI OIDC device-flow mock: device/code then a token success.
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dev-123",
            "user_code": "WXYZ-1234",
            "verification_uri": "https://x.ai/device",
            "verification_uri_complete": "https://x.ai/device?code=WXYZ-1234",
            "interval": 1,
            "expires_in": 900
        })))
        .mount(&auth)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "xai_at",
            "refresh_token": "xai_rt",
            "expires_in": 3600
        })))
        .mount(&auth)
        .await;

    // SAFETY: single-process test; set before any handler builds the oauth client.
    unsafe {
        std::env::set_var("XAI_OAUTH_BASE", auth.uri());
    }

    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    let start = server
        .post("/api/design/xai/login/start")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    start.assert_status_ok();
    assert_eq!(
        start.json::<serde_json::Value>()["user_code"],
        serde_json::json!("WXYZ-1234")
    );

    // Background poll (interval 1s) should vault the xai blob shortly. Bounded poll.
    let mut connected = false;
    for _ in 0..20 {
        let s = server
            .get("/api/design/xai/login/status")
            .add_header("authorization", format!("Bearer {}", token(a)))
            .await;
        s.assert_status_ok();
        if s.json::<serde_json::Value>()["connected"] == serde_json::json!(true) {
            connected = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(connected, "expected xai token blob to be vaulted for engagement A");

    // Cross-engagement isolation: B has no login row and no vaulted blob.
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let sb = server
        .get("/api/design/xai/login/status")
        .add_header("authorization", format!("Bearer {}", token(b)))
        .await;
    sb.assert_status_ok();
    let sbody = sb.json::<serde_json::Value>();
    assert_eq!(sbody["connected"], serde_json::json!(false));
    assert_eq!(sbody["status"], serde_json::json!("none"));

    // Unauthenticated start → 401.
    let anon = server.post("/api/design/xai/login/start").await;
    assert_eq!(anon.status_code(), axum::http::StatusCode::UNAUTHORIZED);
}
