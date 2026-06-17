mod common;
use uuid::Uuid;

#[tokio::test]
async fn resolvers_return_owning_engagement() {
    let db = common::start_test_db().await;

    // Create a fresh engagement.
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    // --- engagement_for_project ---
    let proj: Uuid =
        sqlx::query_scalar("INSERT INTO projects (engagement_id,name) VALUES ($1,'P') RETURNING id")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        celerex_api::db::engagement_for_project(&db.pool, proj)
            .await
            .unwrap(),
        Some(eng)
    );
    // Unknown id → None.
    assert_eq!(
        celerex_api::db::engagement_for_project(&db.pool, Uuid::new_v4())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn resolver_for_block_returns_engagement() {
    let db = common::start_test_db().await;

    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    let doc: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (engagement_id, title) VALUES ($1,'Doc') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    let block: Uuid = sqlx::query_scalar(
        "INSERT INTO doc_blocks (document_id, position, payload) VALUES ($1, 0, '{}') RETURNING id",
    )
    .bind(doc)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert_eq!(
        celerex_api::db::engagement_for_block(&db.pool, block)
            .await
            .unwrap(),
        Some(eng)
    );
    assert_eq!(
        celerex_api::db::engagement_for_block(&db.pool, Uuid::new_v4())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn resolver_for_install_and_run_return_engagement() {
    let db = common::start_test_db().await;

    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('C') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();

    // --- engagement_for_install ---
    let install: Uuid = sqlx::query_scalar(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1,'p','{}') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert_eq!(
        celerex_api::db::engagement_for_install(&db.pool, install)
            .await
            .unwrap(),
        Some(eng)
    );
    assert_eq!(
        celerex_api::db::engagement_for_install(&db.pool, Uuid::new_v4())
            .await
            .unwrap(),
        None
    );

    // --- engagement_for_run ---
    // `trigger` is JSONB NOT NULL; `triggered_by` has a CHECK constraint: scheduled|manual|webhook.
    let run: Uuid = sqlx::query_scalar(
        "INSERT INTO runs (engagement_id, trigger, triggered_by) VALUES ($1,'{}','manual') RETURNING id",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();

    assert_eq!(
        celerex_api::db::engagement_for_run(&db.pool, run)
            .await
            .unwrap(),
        Some(eng)
    );
    assert_eq!(
        celerex_api::db::engagement_for_run(&db.pool, Uuid::new_v4())
            .await
            .unwrap(),
        None
    );
}
