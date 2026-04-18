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

#[tokio::test]
async fn store_and_retrieve_credentials_encrypted_at_rest() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let manifest = json!({ "id": "teams", "name": "T", "version": "0.1", "inbound": [], "outbound": [] });
    let install = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await
        .json::<serde_json::Value>();
    let install_id = install["id"].as_str().unwrap();

    let put = server.put(&format!("/api/plugins/{install_id}/credentials"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "direction": "outbound", "payload": { "token": "secret-abc-123" } }))
        .await;
    put.assert_status_ok();

    let install_uuid = Uuid::parse_str(install_id).unwrap();
    let ct: Vec<u8> = sqlx::query_scalar(
        "SELECT encrypted_payload FROM plugin_credentials WHERE install_id = $1"
    ).bind(install_uuid).fetch_one(&db.pool).await.unwrap();
    let s = String::from_utf8_lossy(&ct);
    assert!(!s.contains("secret-abc-123"), "ciphertext leaked plaintext: {s}");
}

#[tokio::test]
async fn credential_store_trait_is_injectable() {
    use celerex_api::plugins::credentials::{CredentialStore, SealedBlob};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingStore { calls: Mutex<Vec<String>> }
    #[async_trait]
    impl CredentialStore for RecordingStore {
        async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String> {
            self.calls.lock().unwrap().push("seal".into());
            Ok(SealedBlob { ciphertext: plaintext.to_vec(), nonce: vec![0u8; 12] })
        }
        async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String> {
            self.calls.lock().unwrap().push("open".into());
            Ok(blob.ciphertext.clone())
        }
    }

    let recording = Arc::new(RecordingStore::default());
    let store: Arc<dyn CredentialStore> = recording.clone();
    let db = common::start_test_db().await;
    let app = celerex_api::build_app_with_store(db.pool.clone(), store);
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);
    let manifest = json!({ "id": "p", "name": "P", "version": "0.1", "inbound": [], "outbound": [] });
    let install = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await.json::<serde_json::Value>();
    let id = install["id"].as_str().unwrap();
    server.put(&format!("/api/plugins/{id}/credentials"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "direction": "outbound", "payload": { "k": "v" } })).await.assert_status_ok();

    let recorded = recording.calls.lock().unwrap().clone();
    assert!(recorded.iter().any(|c| c == "seal"), "routes must go through the trait");
}
