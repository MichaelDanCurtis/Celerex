use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    UserClick {
        user_id: String,
    },
    Scheduled {
        cron: String,
    },
    Event {
        plugin_id: String,
        event: String,
        payload: serde_json::Value,
    },
}
