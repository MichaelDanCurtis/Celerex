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
    // SAFETY: tests run in a single process; set before any route handler reads env.
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

/// A member lives in engagement A. User B (a different engagement) must NOT be
/// able to run A's flow — that would execute A's graph stamped/keyed to B
/// (cross-tenant execution). The ownership check resolves the member's true
/// owner and 403s a mismatch before the run is ever enqueued.
#[tokio::test]
async fn run_manual_blocks_other_engagement_and_allows_owner() {
    let db = common::start_test_db().await;
    let server =
        axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let eng_b: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let proj_a: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (engagement_id,name) VALUES ($1,'PA') RETURNING id",
    )
    .bind(eng_a)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    // A minimal graph that compiles (zero steps), so the A-202 path runs cleanly.
    let member_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_members (project_id,name,graph_json)
         VALUES ($1,'M',$2) RETURNING id",
    )
    .bind(proj_a)
    .bind(serde_json::json!({"nodes":[],"edges":[]}))
    .fetch_one(&db.pool)
    .await
    .unwrap();

    // B (foreign engagement) → 403, run is NOT enqueued.
    let resp_b = server
        .post(&format!("/api/project_members/{member_id}/run"))
        .add_header("authorization", format!("Bearer {}", token(eng_b)))
        .await;
    resp_b.assert_status(axum::http::StatusCode::FORBIDDEN);

    // A (true owner) → 202, no regression.
    let resp_a = server
        .post(&format!("/api/project_members/{member_id}/run"))
        .add_header("authorization", format!("Bearer {}", token(eng_a)))
        .await;
    resp_a.assert_status(axum::http::StatusCode::ACCEPTED);
}

/// The run's true engagement_id must ALWAYS overwrite whatever a flow author
/// hardcoded into a node's `config.engagement_id`. Otherwise an author who
/// pins a foreign engagement_id wins, leaking the run into another tenant.
#[test]
fn apply_doc_engagement_overwrites_foreign_engagement_id() {
    let foreign = Uuid::new_v4();
    let real = Uuid::new_v4();
    let mut g = serde_json::json!({
        "nodes": [
            {
                "id": "a",
                "type": "action",
                "config": {
                    "execution": "llm",
                    "engagement_id": foreign.to_string()
                }
            }
        ],
        "edges": []
    });
    celerex_api::project_members::run_job::apply_doc_engagement(&mut g, real);
    assert_eq!(
        g["nodes"][0]["config"]["engagement_id"],
        serde_json::json!(real.to_string()),
        "the run's true engagement_id must overwrite the author's hardcoded one"
    );
}
