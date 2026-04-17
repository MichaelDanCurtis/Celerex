use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::job::{JobRequest, RunStatus, Step, StepKind};
use uuid::Uuid;

#[test]
fn job_request_round_trips_json() {
    let req = JobRequest {
        run_id: Uuid::new_v4(),
        engagement_id: Uuid::new_v4(),
        project_id: Some(Uuid::new_v4()),
        steps: vec![Step {
            id: "fetch".into(),
            depends_on: vec![],
            kind: StepKind::Tool {
                execution: ExecutionType::Http,
                config: serde_json::json!({ "url": "https://example.com" }),
                inputs: serde_json::json!({}),
            },
        }],
    };
    let s = serde_json::to_string(&req).unwrap();
    let back: JobRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(back.steps.len(), 1);
    assert_eq!(back.steps[0].id, "fetch");
    assert!(matches!(RunStatus::Running, RunStatus::Running));
}
