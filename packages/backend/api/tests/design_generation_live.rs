//! Gated LIVE end-to-end check for the ported design-generation engine
//! (`run_generation`, Task 9): uses a REAL provider key from the environment,
//! resolved through the same per-engagement vault path production uses
//! (`assistant::resolve_engagement_provider_with`), to drive the full
//! agentic loop against a live model — then asserts a real artifact row was
//! persisted. This makes a real network call with real credentials and is
//! `#[ignore]`d, so the default `cargo test` (and CI) never runs it. Run
//! manually:
//!
//!   OMNIVYX_LIVE_KEY=... \
//!     [OMNIVYX_LIVE_PROVIDER=copilot] [OMNIVYX_LIVE_MODEL=gpt-4o] \
//!     cargo test -p celerex-api --test design_generation_live \
//!     -- --ignored --nocapture
//!
//! Never commit real credentials. Provider id/model defaults may drift as the
//! supported-provider list evolves; override via the env vars above.

mod common;

use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::generation::run_generation;
use std::sync::Arc;
use uuid::Uuid;

/// Real end-to-end: uses a real provider key from the environment to drive the
/// full loop against a live model, then asserts an artifact row was persisted.
/// Ignored by default — run with:
///   OMNIVYX_LIVE_KEY=... cargo test -p celerex-api --test design_generation_live \
///     -- --ignored --nocapture
#[tokio::test]
#[ignore = "requires a real provider key via OMNIVYX_LIVE_KEY"]
async fn live_generation_persists_a_real_artifact() {
    let Ok(key) = std::env::var("OMNIVYX_LIVE_KEY") else {
        eprintln!("skip: OMNIVYX_LIVE_KEY not set — this live test needs real creds");
        return;
    };
    let provider_id =
        std::env::var("OMNIVYX_LIVE_PROVIDER").unwrap_or_else(|_| "copilot".to_string());
    let model = std::env::var("OMNIVYX_LIVE_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('LIVE') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();

    // Store the key so resolve_engagement_provider_with builds the real client
    // via the exact same vault path production uses (assistant::mod.rs).
    let store = PgProviderKeysStore::from_env(db.pool.clone()).unwrap();
    store.set(eng, &provider_id, &key).await.unwrap();
    let provider_keys: Arc<dyn ProviderKeysStore> = Arc::new(store);
    let provider =
        celerex_api::assistant::resolve_engagement_provider_with(&provider_keys, eng, &provider_id)
            .await
            .unwrap()
            .expect("provider built from stored key");

    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    run_generation(
        &tx,
        provider,
        &provider_id,
        &model,
        eng,
        "proj-live",
        Uuid::new_v4(),
        "Build a simple centered hero section with a headline and a subtitle. No scripts.",
        &db.pool,
        12,
        &[],
    )
    .await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "a real run should persist exactly one artifact");
}
