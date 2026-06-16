use async_trait::async_trait;
use serde_json::Value;

/// One new item surfaced by a connector poll.
#[derive(Debug, Clone)]
pub struct PolledItem {
    pub id: String,
    pub payload: Value,
}

/// A connector-specific event source. `cursor` is the last-seen item id; `poll`
/// returns items strictly newer than it (oldest→newest) and the new high-water cursor.
#[async_trait]
pub trait EventSource: Send + Sync {
    async fn poll(
        &self,
        source_config: &Value,
        cursor: Option<&str>,
    ) -> Result<(Vec<PolledItem>, Option<String>), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polled_item_carries_id_and_payload() {
        let it = PolledItem { id: "42".into(), payload: serde_json::json!({"x":1}) };
        assert_eq!(it.id, "42");
        assert_eq!(it.payload["x"], 1);
    }
}
