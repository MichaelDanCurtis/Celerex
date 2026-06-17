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
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
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
async fn settings_are_per_engagement_and_isolated() {
    let db = common::start_test_db().await;
    let server =
        axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let b: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    // A: default empty
    let g0 = server
        .get("/api/settings")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    g0.assert_status_ok();
    assert_eq!(g0.json::<serde_json::Value>()["settings"], serde_json::json!({}));

    // A: write theme=dark
    let p = server
        .put("/api/settings")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .json(&serde_json::json!({"settings": {"theme": "dark"}}))
        .await;
    p.assert_status_ok();

    // A: reads back its own setting
    let ga = server
        .get("/api/settings")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    assert_eq!(
        ga.json::<serde_json::Value>()["settings"],
        serde_json::json!({"theme": "dark"})
    );

    // B sees only its own (empty) — never A's
    let gb = server
        .get("/api/settings")
        .add_header("authorization", format!("Bearer {}", token(b)))
        .await;
    assert_eq!(gb.json::<serde_json::Value>()["settings"], serde_json::json!({}));

    // unauthenticated → 401 (AuthClaims extractor rejection)
    let anon = server.get("/api/settings").await;
    assert_eq!(anon.status_code(), axum::http::StatusCode::UNAUTHORIZED);
}
