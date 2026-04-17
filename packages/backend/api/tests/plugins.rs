mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid) -> String {
    // SAFETY: tests serialized within one binary; env set before middleware reads it.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn install_then_list_plugin() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let manifest = json!({ "id": "teams", "name": "Teams", "version": "0.1.0", "inbound": [], "outbound": [] });
    let resp = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await;
    resp.assert_status(axum::http::StatusCode::CREATED);

    let list = server.get("/api/plugins").add_header("authorization", format!("Bearer {t}")).await;
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["plugin_id"], "teams");
}
