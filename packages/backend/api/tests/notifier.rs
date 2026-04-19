use celerex_api::notifiers::{
    console::ConsoleNotifier, teams_stub::StubTeamsNotifier, NotificationEvent, Notifier,
};
use uuid::Uuid;

#[tokio::test]
async fn teams_stub_buffers_dispatched_events() {
    let n = StubTeamsNotifier::default();
    let ev = NotificationEvent::RunFailed {
        run_id: Uuid::new_v4(),
        project_member_id: Uuid::new_v4(),
        step_id: "s1".into(),
        error: "boom".into(),
    };
    n.dispatch(&ev).await.unwrap();
    assert_eq!(n.sent.lock().await.len(), 1);
}

#[tokio::test]
async fn console_dispatches_without_error() {
    let n = ConsoleNotifier;
    let ev = NotificationEvent::RunSucceeded {
        run_id: Uuid::new_v4(),
        project_member_id: Uuid::new_v4(),
    };
    n.dispatch(&ev).await.unwrap();
}
