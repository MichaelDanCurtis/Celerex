mod common;

use celerex_api::runs::generation::artifact_store::{
    generate_artifact_id, get_artifact, insert_artifact,
};
use uuid::Uuid;

#[tokio::test]
async fn insert_then_get_roundtrips_scoped_by_engagement() {
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let other =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let run_id = Uuid::new_v4();
    let aid = generate_artifact_id("My Landing Page");
    assert!(aid.starts_with("la-"));
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "My Landing Page",
        "<h1>t</h1>",
        "<h1>t</h1>",
        run_id,
    )
    .await
    .unwrap();

    let got = get_artifact(&db.pool, eng, "proj-1", &aid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.title, "My Landing Page");
    assert_eq!(got.rendered_html, "<h1>t</h1>");

    // Cross-engagement read returns None (isolation).
    assert!(
        get_artifact(&db.pool, other, "proj-1", &aid)
            .await
            .unwrap()
            .is_none()
    );
}
