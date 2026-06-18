mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid, proj: Uuid) -> String {
    // SAFETY: single-process test.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: Some(proj.to_string()),
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn block_append_replace_enforce_engagement_ownership() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PA') RETURNING id")
        .bind(eng_a).fetch_one(&db.pool).await.unwrap();
    let eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj_b: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PB') RETURNING id")
        .bind(eng_b).fetch_one(&db.pool).await.unwrap();

    let token_a = token(eng_a, proj_a);
    let token_b = token(eng_b, proj_b);

    // A creates a doc.
    let create = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "title": "A's doc" })).await;
    create.assert_status(axum::http::StatusCode::CREATED);
    let doc_id = create.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    // ---- append_block ----
    // B appends a block to A's doc → 404. With store-layer engagement scoping
    // (A2 P2 T4) the append handler's `get_document` lookup is engagement-scoped,
    // so A's doc is invisible to B and the handler answers 404 (not 403) — no
    // cross-tenant existence leak.
    let b_append = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({ "type": "paragraph", "text": "intruder" })).await;
    b_append.assert_status(axum::http::StatusCode::NOT_FOUND);

    // A appends a block → 201 (no regression).
    let a_append = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "type": "paragraph", "text": "hello" })).await;
    a_append.assert_status(axum::http::StatusCode::CREATED);
    let block_id = a_append.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    // ---- replace_block ----
    // B replaces A's block → 403.
    let b_replace = server.patch(&format!("/api/docs/{doc_id}/blocks/{block_id}"))
        .add_header("authorization", format!("Bearer {token_b}"))
        .json(&json!({ "text": "hijacked" })).await;
    b_replace.assert_status(axum::http::StatusCode::FORBIDDEN);

    // A replaces A's block → 200.
    let a_replace = server.patch(&format!("/api/docs/{doc_id}/blocks/{block_id}"))
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({ "text": "updated" })).await;
    a_replace.assert_status(axum::http::StatusCode::OK);
}
