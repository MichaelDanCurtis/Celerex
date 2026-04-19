mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::util::ServiceExt;

async fn token_for(engagement: uuid::Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(engagement)
}

#[tokio::test]
async fn edit_creates_new_snapshot_and_bumps_version() {
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
    let token = token_for(eng).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project_members")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_id": project,
                        "name": "Weekly Newsletter",
                        "graph_json": { "nodes": [], "edges": [] },
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/project_members/{member_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "name": "Weekly Newsletter",
                        "graph_json": { "nodes": [{"id":"a","type":"inbound","config":{}}], "edges": [] },
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let snaps: Vec<(i32,)> = sqlx::query_as(
        "SELECT version FROM project_member_snapshots WHERE project_member_id = $1 ORDER BY version",
    )
    .bind(uuid::Uuid::parse_str(&member_id).unwrap())
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(snaps, vec![(1,), (2,)]);

    let current: (i32,) = sqlx::query_as(
        "SELECT version FROM project_members WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&member_id).unwrap())
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(current.0, 2);
}

#[tokio::test]
#[ignore = "depends on runs.snapshot_id + triggered_by columns added in Jean Task 7"]
async fn post_run_creates_run_row_with_snapshot_and_manual_trigger() {
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
    let token = token_for(eng).await;

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project_members")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    json!({
                        "project_id": project,
                        "name": "Jean",
                        "graph_json": { "nodes": [], "edges": [] },
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), 64 * 1024).await.unwrap(),
    )
    .unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/project_members/{member_id}/run"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let runs: Vec<(uuid::Uuid, Option<uuid::Uuid>, String)> = sqlx::query_as(
        "SELECT id, snapshot_id, triggered_by FROM runs",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].1.is_some(), "snapshot_id is set");
    assert_eq!(runs[0].2, "manual");
}
