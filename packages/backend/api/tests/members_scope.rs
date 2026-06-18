mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::util::ServiceExt;

fn token_for(engagement: uuid::Uuid) -> String {
    celerex_api::auth::jwt::mint_for_tests(engagement)
}

/// Defense-in-depth: the project_members store scopes id/parent-addressed reads
/// and mutations to the caller's engagement. A cross-tenant id must behave as
/// not-found at the store layer (proven here end-to-end via the HTTP routes).
#[tokio::test]
async fn members_get_update_list_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());

    // Engagement A + a project (seed the project directly; member via the API).
    let eng_a = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme A') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let project_a = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Proj A') RETURNING id",
    )
    .bind(eng_a)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let token_a = token_for(eng_a);

    // Engagement B + token.
    let eng_b = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme B') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let token_b = token_for(eng_b);

    // Create a member under A via the authenticated API so the id is real.
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project_members")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::from(
                    json!({
                        "project_id": project_a,
                        "name": "A's Member",
                        "graph_json": { "nodes": [], "edges": [] },
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    // 1. GET the member by id with token(B) -> denied (B can't see A's member).
    //    The handler gates cross-tenant access with 403 (FORBIDDEN) via the
    //    ownership resolver before reaching the store; either 403 or 404 means
    //    "B cannot read A's member". The store-layer 404/empty guarantee is
    //    asserted directly in `store_get_list_return_notfound_cross_tenant`.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/project_members/{member_id}"))
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.status() == StatusCode::FORBIDDEN || res.status() == StatusCode::NOT_FOUND,
        "B must not read A's member (got {})",
        res.status()
    );

    // 2. GET the same member with token(A) -> 200.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/project_members/{member_id}"))
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "A can read its own member");

    // 3. PUT/update the member with token(B) -> 404 (no cross-tenant mutation).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/project_members/{member_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::from(
                    json!({
                        "name": "Hijacked",
                        "graph_json": { "nodes": [], "edges": [] },
                        "provider_config_json": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.status() == StatusCode::FORBIDDEN || res.status() == StatusCode::NOT_FOUND,
        "B must not update A's member (got {})",
        res.status()
    );

    // 4. list members for A's project with token(B) -> 404 (can't enumerate A's project).
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/projects/{project_a}/members"))
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.status() == StatusCode::FORBIDDEN || res.status() == StatusCode::NOT_FOUND,
        "B must not enumerate A's project members (got {})",
        res.status()
    );
}

/// Direct store-layer assertion of the plan's contract: a cross-tenant
/// `engagement_id` makes `get` return `None`, `list_for_project` return empty,
/// and `update` return the not-found error — independent of the handler gate.
#[tokio::test]
async fn store_get_list_update_scoped_cross_tenant() {
    use celerex_api::project_members::store::{MembersStore, PgMembersStore};
    use serde_json::Value;

    let db = common::start_test_db().await;
    let store = PgMembersStore::new(db.pool.clone());

    let eng_a = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Store A') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let eng_b = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Store B') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let project_a = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Proj A') RETURNING id",
    )
    .bind(eng_a)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let empty = json!({ "nodes": [], "edges": [] });
    let member = store
        .create(
            project_a,
            "M",
            "",
            &empty,
            &Value::Object(Default::default()),
            "reasoning",
        )
        .await
        .unwrap();

    // get: A sees it, B does not (None).
    assert!(store.get(member.id, eng_a).await.unwrap().is_some());
    assert!(store.get(member.id, eng_b).await.unwrap().is_none());

    // list_for_project: A gets one row, B gets empty.
    assert_eq!(
        store
            .list_for_project(project_a, eng_a)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .list_for_project(project_a, eng_b)
            .await
            .unwrap()
            .is_empty()
    );

    // update: A succeeds, B returns the not-found error (no cross-tenant mutation).
    assert!(
        store
            .update(
                member.id,
                eng_a,
                "M2",
                "",
                &empty,
                &Value::Object(Default::default()),
                "reasoning"
            )
            .await
            .is_ok()
    );
    assert!(
        store
            .update(
                member.id,
                eng_b,
                "Hijack",
                "",
                &empty,
                &Value::Object(Default::default()),
                "reasoning"
            )
            .await
            .is_err(),
        "cross-tenant update must error (not-found)"
    );
}
