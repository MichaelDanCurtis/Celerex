mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid, proj: Uuid) -> String {
    // SAFETY: tests serialized within one binary; env set before middleware reads it.
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
async fn create_doc_persists_under_project() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let resp = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "Newsletter Draft" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["title"], "Newsletter Draft");
    assert_eq!(body["project_id"], proj.to_string());
    assert_eq!(body["engagement_id"], eng.to_string());
}

#[tokio::test]
async fn append_block_to_doc() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let create = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "T" })).await;
    let doc_id = create.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let resp = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "type": "paragraph", "text": "hello" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_blocks WHERE document_id = $1::uuid")
        .bind(&doc_id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
}
