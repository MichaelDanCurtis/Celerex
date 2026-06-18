//! A2 hardening Phase 2, Task 4: engagement-scoping sweep for the id-addressed
//! `db` store fns behind docs/doc_blocks (defense-in-depth). Mirrors the
//! per-surface scope tests from Tasks 1-3: a direct store-layer cross-tenant
//! assertion on each scoped fn, plus HTTP assertions through the real routes.
//!
//! Scope note (see the task report): runs-by-id and reviews are NOT store fns —
//! `runs::routes::steps` and `reviews::submit_review` read the run with inline
//! SQL behind an explicit handler 403 gate, so there is no store fn to add an
//! `engagement_id` param to. `plugin_installs` likewise has no id-addressed READ
//! store fn (creds are gated by `engagement_for_install` at the handler). The
//! HTTP tests below cover those surfaces end-to-end to prove the gate holds.

mod common;

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use celerex_api::auth::jwt::mint_for_tests;

/// Seed an engagement + project + document, return (engagement_id, document_id).
async fn seed_doc(pool: &sqlx::PgPool, label: &str) -> (Uuid, Uuid) {
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
            .bind(label)
            .fetch_one(pool)
            .await
            .unwrap();
    let project = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id",
    )
    .bind(eng)
    .fetch_one(pool)
    .await
    .unwrap();
    let doc = celerex_api::db::create_document(pool, eng, Some(project), "Doc")
        .await
        .unwrap();
    (eng, doc.id)
}

fn bearer(server: &axum_test::TestServer, eng: Uuid) -> String {
    let _ = server;
    format!("Bearer {}", mint_for_tests(eng))
}

/// Direct store-layer assertions: every scoped doc/doc_block fn refuses a
/// foreign engagement (None / empty / NotFound / 0-rows) and serves the owner.
#[tokio::test]
async fn store_doc_and_block_fns_scoped_cross_tenant() {
    let db = common::start_test_db().await;
    let pool = &db.pool;

    let (eng_a, doc_a) = seed_doc(pool, "Sweep store A").await;
    let (eng_b, _doc_b) = seed_doc(pool, "Sweep store B").await;

    // get_document: A sees it, B does not.
    assert!(celerex_api::db::get_document(pool, doc_a, eng_a)
        .await
        .unwrap()
        .is_some());
    assert!(celerex_api::db::get_document(pool, doc_a, eng_b)
        .await
        .unwrap()
        .is_none());

    // append_block: B cannot append into A's doc (Err); A can.
    let payload = json!({ "type": "markdown", "text": "hi" });
    assert!(celerex_api::db::append_block(pool, doc_a, eng_b, &payload)
        .await
        .is_err());
    let block_id = celerex_api::db::append_block(pool, doc_a, eng_a, &payload)
        .await
        .unwrap();

    // list_blocks: A sees the block, B sees an empty list.
    assert_eq!(
        celerex_api::db::list_blocks(pool, doc_a, eng_a)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(celerex_api::db::list_blocks(pool, doc_a, eng_b)
        .await
        .unwrap()
        .is_empty());

    // replace_block: B matches 0 rows (handler maps 0 -> 404); A updates 1.
    let new_payload = json!({ "type": "markdown", "text": "edited" });
    assert_eq!(
        celerex_api::db::replace_block(pool, block_id, eng_b, &new_payload)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        celerex_api::db::replace_block(pool, block_id, eng_a, &new_payload)
            .await
            .unwrap(),
        1
    );

    // delete_document: B matches 0 rows; A removes it.
    assert_eq!(
        celerex_api::db::delete_document(pool, doc_a, eng_b)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        celerex_api::db::delete_document(pool, doc_a, eng_a)
            .await
            .unwrap(),
        1
    );
}

/// HTTP: a foreign token cannot read another engagement's document by id, but
/// the owner can.
#[tokio::test]
async fn http_get_document_scoped() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let (eng_a, doc_a) = seed_doc(&db.pool, "Sweep http get A").await;
    let (eng_b, _) = seed_doc(&db.pool, "Sweep http get B").await;

    let res_b = server
        .get(&format!("/api/docs/{doc_a}"))
        .add_header("authorization", bearer(&server, eng_b))
        .await;
    assert!(
        matches!(res_b.status_code(), StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "B got {}",
        res_b.status_code()
    );

    let res_a = server
        .get(&format!("/api/docs/{doc_a}"))
        .add_header("authorization", bearer(&server, eng_a))
        .await;
    assert_eq!(res_a.status_code(), StatusCode::OK);
}

/// HTTP: append + list + replace blocks are denied cross-tenant, allowed for
/// the owner.
#[tokio::test]
async fn http_block_ops_scoped() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let (eng_a, doc_a) = seed_doc(&db.pool, "Sweep http block A").await;
    let (eng_b, _) = seed_doc(&db.pool, "Sweep http block B").await;

    let body = json!({ "type": "markdown", "text": "owner block" });

    // B cannot append to A's doc.
    let res_b = server
        .post(&format!("/api/docs/{doc_a}/blocks"))
        .add_header("authorization", bearer(&server, eng_b))
        .json(&body)
        .await;
    assert!(
        matches!(res_b.status_code(), StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "B append got {}",
        res_b.status_code()
    );

    // A appends successfully.
    let res_a = server
        .post(&format!("/api/docs/{doc_a}/blocks"))
        .add_header("authorization", bearer(&server, eng_a))
        .json(&body)
        .await;
    assert_eq!(res_a.status_code(), StatusCode::CREATED);
    let block_id = res_a.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // B cannot list A's blocks.
    let list_b = server
        .get(&format!("/api/docs/{doc_a}/blocks"))
        .add_header("authorization", bearer(&server, eng_b))
        .await;
    assert!(
        matches!(list_b.status_code(), StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "B list got {}",
        list_b.status_code()
    );

    // B cannot replace A's block.
    let replace_b = server
        .patch(&format!("/api/docs/{doc_a}/blocks/{block_id}"))
        .add_header("authorization", bearer(&server, eng_b))
        .json(&json!({ "type": "markdown", "text": "tamper" }))
        .await;
    assert!(
        matches!(replace_b.status_code(), StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
        "B replace got {}",
        replace_b.status_code()
    );
}

/// HTTP: a run's step history (`runs::routes::steps`) is gated to the owning
/// engagement. Runs is handler-gated (no store fn), included here so the sweep
/// surface is covered end-to-end.
#[tokio::test]
async fn http_run_steps_scoped() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let eng_a =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('Run A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let eng_b =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('Run B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO runs (id, engagement_id, trigger, status, triggered_by)
         VALUES ($1, $2, '{}'::jsonb, 'running', 'manual')",
    )
    .bind(run_id)
    .bind(eng_a)
    .execute(&db.pool)
    .await
    .unwrap();

    let res_b = server
        .get(&format!("/api/runs/{run_id}/steps"))
        .add_header("authorization", bearer(&server, eng_b))
        .await;
    assert_eq!(res_b.status_code(), StatusCode::FORBIDDEN);

    let res_a = server
        .get(&format!("/api/runs/{run_id}/steps"))
        .add_header("authorization", bearer(&server, eng_a))
        .await;
    assert_eq!(res_a.status_code(), StatusCode::OK);
}
