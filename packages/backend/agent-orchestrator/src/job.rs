use crate::execution_type::ExecutionType;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub run_id: Uuid,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub depends_on: Vec<String>,
    pub kind: StepKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    Tool {
        execution: ExecutionType,
        config: serde_json::Value,
        inputs: serde_json::Value,
    },
    Filter {
        expression: String,
    },
    Outbound {
        execution: ExecutionType,
        config: serde_json::Value,
        inputs: serde_json::Value,
    },
    /// Human-in-the-loop gate. When reached without a recorded decision the run
    /// PAUSES (`RunStatus::AwaitingReview`); once a person approves/rejects, the
    /// run resumes. `prompt` is shown to the reviewer; `content` is a template
    /// resolved against upstream outputs to produce the draft they review, and
    /// the (possibly edited) approved content flows downstream as this step's
    /// `outputs.content`. Approve = gate open (downstream runs); reject = gate
    /// closed (downstream skipped), same semantics as `Filter`.
    ///
    /// `auto_approve` short-circuits the human gate: when true and the step has
    /// no seeded decision, the executor approves the resolved `content` and the
    /// run continues without pausing. Set ONLY for scheduler-triggered runs (see
    /// `apply_scheduled_auto_approve`); manual/interactive runs leave it false
    /// and keep the human gate.
    Review {
        prompt: String,
        content: serde_json::Value,
        #[serde(default)]
        auto_approve: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    /// Non-terminal: a `Review` step paused the run pending a human decision.
    AwaitingReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub run_id: Uuid,
    pub step_id: String,
    pub status: RunStatus,
    pub inputs: serde_json::Value,
    pub outputs: Option<serde_json::Value>,
    pub error: Option<String>,
}
