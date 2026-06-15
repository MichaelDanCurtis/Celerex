// Unit-level coverage for RitualSpec → graph translation. Full HTTP/Postgres
// tests live further down this file (added in later tasks).
use celerex_api::design::rituals::{build_ritual_graph, RitualBody};

#[test]
fn agent_run_body_builds_a_valid_minimal_graph() {
    let body = RitualBody::AgentRun {
        prompt: "Summarize my unread mail".into(),
        skill_template_id: Some("orbit-general".into()),
        target: None,
    };
    let graph = build_ritual_graph(&body);
    // graph must pass the engine's own validator.
    celerex_api::project_members::graph::validate(&graph).expect("ritual graph must validate");
    // carries the prompt so the run is self-contained.
    assert!(graph.to_string().contains("Summarize my unread mail"));
}
