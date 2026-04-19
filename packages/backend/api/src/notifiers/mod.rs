pub mod console;
pub mod teams_stub;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    RunFailed {
        run_id: Uuid,
        project_member_id: Uuid,
        step_id: String,
        error: String,
    },
    RunSucceeded {
        run_id: Uuid,
        project_member_id: Uuid,
    },
}

impl NotificationEvent {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::RunFailed { .. } => "run_failed",
            Self::RunSucceeded { .. } => "run_succeeded",
        }
    }

    pub fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }

    pub fn project_member_id(&self) -> Uuid {
        match self {
            Self::RunFailed { project_member_id, .. }
            | Self::RunSucceeded { project_member_id, .. } => *project_member_id,
        }
    }

    pub fn run_id(&self) -> Uuid {
        match self {
            Self::RunFailed { run_id, .. } | Self::RunSucceeded { run_id, .. } => *run_id,
        }
    }
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String>;
}
