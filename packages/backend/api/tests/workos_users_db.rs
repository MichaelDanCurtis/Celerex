mod common;
use uuid::Uuid;

#[tokio::test]
async fn provision_and_find_workos_user() {
    let db = common::start_test_db().await;
    // find on empty → None
    assert!(celerex_api::db::find_engagement_by_workos_user(&db.pool, "wos_x").await.unwrap().is_none());
    // create an engagement + workos user, then find returns it
    let eng = celerex_api::db::create_engagement(&db.pool, "Alice WS").await.unwrap();
    let uid = celerex_api::db::create_workos_user(&db.pool, "alice@x.test", "wos_x", eng.id).await.unwrap();
    assert_ne!(uid, Uuid::nil());
    let found = celerex_api::db::find_engagement_by_workos_user(&db.pool, "wos_x").await.unwrap();
    assert_eq!(found, Some((eng.id, "Alice WS".to_string())));
}
