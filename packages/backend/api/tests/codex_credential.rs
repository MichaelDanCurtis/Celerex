mod common;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    // SAFETY: single-process test
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
async fn codex_credential_upload_status_and_isolation() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    let expires_at = chrono::Utc::now().timestamp() + 864_000;

    // Upload the captured Codex tokens (Bearer A).
    let up = server
        .post("/api/design/codex/credential")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .json(&serde_json::json!({
            "access_token": "at",
            "refresh_token": "rt",
            "account_id": "acc",
            "expires_at": expires_at
        }))
        .await;
    up.assert_status_ok();
    assert_eq!(
        up.json::<serde_json::Value>()["ok"],
        serde_json::json!(true)
    );

    // Status for A → connected.
    let sa = server
        .get("/api/design/codex/status")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    sa.assert_status_ok();
    assert_eq!(
        sa.json::<serde_json::Value>()["connected"],
        serde_json::json!(true)
    );

    // Cross-engagement isolation: B has no codex blob.
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let sb = server
        .get("/api/design/codex/status")
        .add_header("authorization", format!("Bearer {}", token(b)))
        .await;
    sb.assert_status_ok();
    assert_eq!(
        sb.json::<serde_json::Value>()["connected"],
        serde_json::json!(false)
    );

    // Unauthenticated upload → 401.
    let anon = server
        .post("/api/design/codex/credential")
        .json(&serde_json::json!({
            "access_token": "at",
            "refresh_token": "rt",
            "account_id": "acc",
            "expires_at": expires_at
        }))
        .await;
    assert_eq!(anon.status_code(), axum::http::StatusCode::UNAUTHORIZED);

    // Validation: empty access_token → 400.
    let bad = server
        .post("/api/design/codex/credential")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .json(&serde_json::json!({
            "access_token": "",
            "refresh_token": "rt",
            "account_id": "acc",
            "expires_at": expires_at
        }))
        .await;
    assert_eq!(bad.status_code(), axum::http::StatusCode::BAD_REQUEST);
}
