use celerex_orchestrator::triggers::Trigger;
use serde_json::json;

#[test]
fn trigger_serde_round_trips_all_variants() {
    let cases = vec![
        Trigger::UserClick {
            user_id: "u1".into(),
        },
        Trigger::Scheduled {
            cron: "0 9 * * MON".into(),
        },
        Trigger::Event {
            plugin_id: "teams".into(),
            event: "meeting_complete".into(),
            payload: json!({"meetingId": "m1"}),
        },
    ];
    for t in cases {
        let s = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&s).unwrap();
        assert_eq!(format!("{:?}", t), format!("{:?}", back));
    }
}
