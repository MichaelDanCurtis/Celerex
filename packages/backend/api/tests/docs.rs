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

#[tokio::test]
async fn patch_block_replaces_payload() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let doc = server.post("/api/docs").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "T" })).await.json::<serde_json::Value>();
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let block = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "text": "old" })).await.json::<serde_json::Value>();
    let block_id = block["id"].as_str().unwrap().to_string();

    let resp = server.patch(&format!("/api/docs/{doc_id}/blocks/{block_id}"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "text": "new" })).await;
    resp.assert_status_ok();

    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM doc_blocks WHERE id = $1::uuid"
    ).bind(&block_id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(payload["text"], "new");
}

#[tokio::test]
async fn list_engagement_docs_filters_by_engagement() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PA') RETURNING id")
        .bind(eng_a).fetch_one(&db.pool).await.unwrap();
    let proj_b: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PB') RETURNING id")
        .bind(eng_b).fetch_one(&db.pool).await.unwrap();

    let t_a = token(eng_a, proj_a);
    let t_b = token(eng_b, proj_b);

    // 2 docs in A
    for title in &["A1", "A2"] {
        server.post("/api/docs")
            .add_header("authorization", format!("Bearer {t_a}"))
            .json(&json!({ "title": title })).await
            .assert_status(axum::http::StatusCode::CREATED);
    }
    // 1 doc in B
    server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t_b}"))
        .json(&json!({ "title": "B1" })).await
        .assert_status(axum::http::StatusCode::CREATED);

    // List A's docs with A's token
    let resp = server.get(&format!("/api/engagements/{eng_a}/docs"))
        .add_header("authorization", format!("Bearer {t_a}"))
        .await;
    resp.assert_status_ok();
    let arr: Vec<serde_json::Value> = resp.json();
    assert_eq!(arr.len(), 2);
    let titles: Vec<&str> = arr.iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"A1") && titles.contains(&"A2"));
    assert_eq!(arr[0]["block_count"].as_i64().unwrap(), 0);

    // Cross-engagement listing must be 403
    let cross = server.get(&format!("/api/engagements/{eng_b}/docs"))
        .add_header("authorization", format!("Bearer {t_a}"))
        .await;
    cross.assert_status(axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_doc_returns_404_for_unknown() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let bogus = Uuid::new_v4();
    let resp = server.get(&format!("/api/docs/{bogus}"))
        .add_header("authorization", format!("Bearer {t}"))
        .await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_blocks_returns_blocks_in_position_order() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let doc = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "Ordered" })).await
        .json::<serde_json::Value>();
    let doc_id_str = doc["id"].as_str().unwrap().to_string();
    let doc_id = Uuid::parse_str(&doc_id_str).unwrap();

    // Insert blocks out of order directly via SQL to verify ORDER BY position
    for (pos, marker) in &[(2_i32, "third"), (0_i32, "first"), (1_i32, "second")] {
        sqlx::query("INSERT INTO doc_blocks (document_id, position, payload) VALUES ($1, $2, $3)")
            .bind(doc_id)
            .bind(pos)
            .bind(json!({ "marker": marker }))
            .execute(&db.pool).await.unwrap();
    }

    let resp = server.get(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t}"))
        .await;
    resp.assert_status_ok();
    let arr: Vec<serde_json::Value> = resp.json();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["position"], 0);
    assert_eq!(arr[0]["payload"]["marker"], "first");
    assert_eq!(arr[1]["position"], 1);
    assert_eq!(arr[1]["payload"]["marker"], "second");
    assert_eq!(arr[2]["position"], 2);
    assert_eq!(arr[2]["payload"]["marker"], "third");
}

#[tokio::test]
async fn cross_engagement_get_doc_returns_404() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng_a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let eng_b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PA') RETURNING id")
        .bind(eng_a).fetch_one(&db.pool).await.unwrap();
    let proj_b: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'PB') RETURNING id")
        .bind(eng_b).fetch_one(&db.pool).await.unwrap();

    let t_a = token(eng_a, proj_a);
    let t_b = token(eng_b, proj_b);

    let doc = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t_a}"))
        .json(&json!({ "title": "Secret" })).await
        .json::<serde_json::Value>();
    let doc_id = doc["id"].as_str().unwrap().to_string();

    // B tries to fetch A's doc. With store-layer engagement scoping (A2 P2 T4),
    // the doc is invisible to B — `get_document` returns None, so the handler
    // answers 404 (not 403), leaking no existence signal across tenants.
    let resp = server.get(&format!("/api/docs/{doc_id}"))
        .add_header("authorization", format!("Bearer {t_b}"))
        .await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);

    // And B can't list A's blocks (same store-scoped invisibility -> 404).
    let blocks = server.get(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t_b}"))
        .await;
    blocks.assert_status(axum::http::StatusCode::NOT_FOUND);
}
