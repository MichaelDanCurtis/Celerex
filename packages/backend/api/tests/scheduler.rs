mod common;

use chrono::{TimeZone, Utc};
use celerex_api::scheduler::store::{PgSchedulesStore, Schedule, SchedulesStore};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn due_returns_only_schedules_past_next_fire_at() {
    let db = common::start_test_db().await;
    let store = PgSchedulesStore::new(db.pool.clone());

    // Seed: two schedules, one due, one in the future.
    let past = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let future = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
    let a = store
        .create(Schedule {
            id: Uuid::new_v4(),
            project_member_id: Uuid::new_v4(),
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: past,
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();
    let _b = store
        .create(Schedule {
            id: Uuid::new_v4(),
            project_member_id: Uuid::new_v4(),
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: future,
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    let due = store.due(Utc::now()).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, a);
}

#[tokio::test]
async fn loop_dispatches_due_schedules_and_advances_next_fire_at() {
    let db = common::start_test_db().await;
    let store = Arc::new(PgSchedulesStore::new(db.pool.clone()));

    let past = Utc.with_ymd_and_hms(2026, 4, 18, 0, 0, 0).unwrap();
    let _id = store
        .create(Schedule {
            id: Uuid::new_v4(),
            project_member_id: Uuid::new_v4(),
            cron_expr: "0 0 9 * * MON *".into(),
            timezone: "UTC".into(),
            next_fire_at: past,
            last_fired_at: None,
            enabled: true,
        })
        .await
        .unwrap();

    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let dispatch = Arc::new(move |_s: Schedule| {
        let c = counter.clone();
        tokio::spawn(async move {
            c.fetch_add(1, Ordering::SeqCst);
        })
    });

    let store_for_loop: Arc<dyn SchedulesStore> = store.clone();
    let handle = celerex_api::scheduler::r#loop::SchedulerLoop {
        store: store_for_loop,
        tick: Duration::from_millis(50),
        dispatch,
    }
    .spawn();

    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.abort();

    assert!(
        count.load(Ordering::SeqCst) >= 1,
        "dispatch fired at least once"
    );
    let remaining_due = store.due(Utc::now()).await.unwrap();
    assert!(
        remaining_due.is_empty(),
        "next_fire_at advanced past now"
    );
}
