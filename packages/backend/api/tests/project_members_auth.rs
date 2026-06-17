mod common;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: Uuid, proj: Option<Uuid>) -> String {
    // SAFETY: single-process test.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: proj.map(|p| p.to_string()),
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
async fn project_members_cross_engagement_is_forbidden() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id,name) VALUES ($1,'PA') RETURNING id")
        .bind(eng_a).fetch_one(&db.pool).await.unwrap();

    // Seed a member in A via POST as A.
    let create = server
        .post("/api/project_members")
        .add_header("authorization", format!("Bearer {}", token(eng_a, Some(proj_a))))
        .json(&serde_json::json!({
            "project_id": proj_a,
            "name": "Member A",
            "graph_json": { "nodes": [], "edges": [] },
            "provider_config_json": {}
        }))
        .await;
    create.assert_status(axum::http::StatusCode::CREATED);
    let member_id = create.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let tok_b = token(eng_b, None);

    // GET another engagement's member -> 403
    let r = server
        .get(&format!("/api/project_members/{member_id}"))
        .add_header("authorization", format!("Bearer {tok_b}"))
        .await;
    r.assert_status(axum::http::StatusCode::FORBIDDEN);

    // PUT another engagement's member -> 403
    let r = server
        .put(&format!("/api/project_members/{member_id}"))
        .add_header("authorization", format!("Bearer {tok_b}"))
        .json(&serde_json::json!({
            "name": "Hijacked",
            "graph_json": { "nodes": [], "edges": [] },
            "provider_config_json": {}
        }))
        .await;
    r.assert_status(axum::http::StatusCode::FORBIDDEN);

    // LIST another engagement's project members -> 403
    let r = server
        .get(&format!("/api/projects/{proj_a}/members"))
        .add_header("authorization", format!("Bearer {tok_b}"))
        .await;
    r.assert_status(axum::http::StatusCode::FORBIDDEN);

    // CREATE into another engagement's project -> 403
    let r = server
        .post("/api/project_members")
        .add_header("authorization", format!("Bearer {tok_b}"))
        .json(&serde_json::json!({
            "project_id": proj_a,
            "name": "Intruder",
            "graph_json": { "nodes": [], "edges": [] },
            "provider_config_json": {}
        }))
        .await;
    r.assert_status(axum::http::StatusCode::FORBIDDEN);

    // As A: GET own member -> 200 (no regression)
    let r = server
        .get(&format!("/api/project_members/{member_id}"))
        .add_header("authorization", format!("Bearer {}", token(eng_a, Some(proj_a))))
        .await;
    r.assert_status(axum::http::StatusCode::OK);
}
