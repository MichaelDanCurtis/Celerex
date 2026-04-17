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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
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
