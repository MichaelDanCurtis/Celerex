mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

#[tokio::test]
async fn fleet_returns_member_rows_with_null_statuses_when_no_runs() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let eng = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let project = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO project_members (project_id, name, graph_json) \
         VALUES ($1, 'm1', '{\"nodes\":[],\"edges\":[]}'::jsonb)",
    )
    .bind(project)
    .execute(&db.pool)
    .await
    .unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/project_members/fleet")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["name"], "m1");
    assert_eq!(body[0]["running"], false);
    assert!(body[0]["last_status"].is_null());
    assert!(body[0]["next_fire_at"].is_null());
}
