use celerex_api::runs::bus::{RunBus, RunEvent};
use uuid::Uuid;

#[tokio::test]
async fn bus_fans_out_events_to_subscribers() {
    let bus = RunBus::new();
    let run_id = Uuid::new_v4();
    let mut rx = bus.subscribe(run_id);
    bus.publish(
        run_id,
        RunEvent::RunStepSucceeded {
            run_id,
            step_id: "a".into(),
            outputs: serde_json::json!({"ok": true}),
        },
    );
    let ev = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match ev {
        RunEvent::RunStepSucceeded { step_id, .. } => assert_eq!(step_id, "a"),
        _ => panic!("wrong event"),
    }
}
