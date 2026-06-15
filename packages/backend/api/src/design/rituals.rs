//! Rituals: scheduled agent work, hosted on the Agent. A Ritual is a
//! `project_member` (the body) plus a scheduler entry that fires it on cron.
//! The device seam speaks Design's vocabulary (`RitualSpec`); this module
//! translates it into the engine's `project_member` graph. See
//! docs/superpowers/specs/2026-06-14-rituals-via-agent-design.md.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

/// The schedule half of a Ritual.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RitualSchedule {
    pub cron_expr: String,
    pub timezone: String,
}

/// The work half of a Ritual — Design's vocabulary, not raw graph JSON.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RitualBody {
    /// A scheduled agent conversation (today's routine / Orbit shape).
    AgentRun {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill_template_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// An existing flow becomes the Ritual body directly.
    FlowRef { project_member_id: Uuid },
}

/// The full create/update payload from the daemon.
#[derive(Debug, Clone, Deserialize)]
pub struct RitualSpec {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub schedule: RitualSchedule,
    pub body: RitualBody,
}

fn default_enabled() -> bool {
    true
}

/// Build a minimal, self-contained `project_member` graph for an `agent_run`
/// Ritual: a trigger feeding a single agent action that carries the prompt.
pub fn build_ritual_graph(body: &RitualBody) -> Value {
    match body {
        RitualBody::AgentRun {
            prompt,
            skill_template_id,
            target,
        } => json!({
            "nodes": [
                { "id": "trigger", "type": "inbound", "config": { "execution": "input" } },
                {
                    "id": "agent",
                    "type": "action",
                    "config": {
                        "execution": "agent",
                        "prompt": prompt,
                        "skill_template_id": skill_template_id,
                        "target": target,
                        "inputs": "{{steps.trigger.outputs}}"
                    }
                }
            ],
            "edges": [ { "from": "trigger", "to": "agent" } ]
        }),
        // FlowRef bodies reuse an existing graph; this is never called for them.
        RitualBody::FlowRef { .. } => json!({ "nodes": [], "edges": [] }),
    }
}
