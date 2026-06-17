mod common;
use uuid::Uuid;

#[tokio::test]
async fn settings_default_and_upsert_roundtrip() {
    let db = common::start_test_db().await;
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        celerex_api::db::get_settings(&db.pool, eng).await.unwrap(),
        serde_json::json!({})
    );
    celerex_api::db::upsert_settings(&db.pool, eng, &serde_json::json!({"theme":"dark"}))
        .await
        .unwrap();
    assert_eq!(
        celerex_api::db::get_settings(&db.pool, eng).await.unwrap(),
        serde_json::json!({"theme":"dark"})
    );
    celerex_api::db::upsert_settings(
        &db.pool,
        eng,
        &serde_json::json!({"theme":"light","accent":"red"}),
    )
    .await
    .unwrap();
    assert_eq!(
        celerex_api::db::get_settings(&db.pool, eng).await.unwrap(),
        serde_json::json!({"theme":"light","accent":"red"})
    );
}
