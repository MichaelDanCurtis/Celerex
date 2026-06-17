mod common;
use uuid::Uuid;

// Route: PUT /api/plugins/{id}/credentials
// Body: { "direction": "inbound"|"outbound"|"both", "payload": <any JSON> }

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    // SAFETY: single-process test.
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
async fn upsert_credentials_enforces_engagement_ownership() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    // Seed two engagements.
    let eng_a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('CredA') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let eng_b: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('CredB') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    // Seed a plugin install owned by engagement A.
    // plugin_installs columns (from 0003_plugins.sql):
    //   id, engagement_id, project_id (nullable), plugin_id, manifest (JSONB), installed_at
    let install_id: Uuid = sqlx::query_scalar(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) \
         VALUES ($1, 'test-plugin', '{}'::jsonb) RETURNING id",
    )
    .bind(eng_a)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let cred_body = serde_json::json!({
        "direction": "inbound",
        "payload": { "api_key": "secret" }
    });

    // Engagement B's token → 403 (must not write to A's install).
    let resp = server
        .put(&format!("/api/plugins/{install_id}/credentials"))
        .add_header("authorization", format!("Bearer {}", token(eng_b)))
        .json(&cred_body)
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);

    // Engagement A's token → 200 (owner can write).
    let resp = server
        .put(&format!("/api/plugins/{install_id}/credentials"))
        .add_header("authorization", format!("Bearer {}", token(eng_a)))
        .json(&cred_body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
}
