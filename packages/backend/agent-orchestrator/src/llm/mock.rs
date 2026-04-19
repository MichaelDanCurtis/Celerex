use super::provider::{Completion, Provider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Mutex;

pub struct MockLlm {
    queue: Mutex<std::collections::VecDeque<Value>>,
}

impl MockLlm {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Default::default()),
        }
    }

    pub fn use_tool(self, name: &str, input: Value) -> Self {
        self.queue.lock().unwrap().push_back(json!({
            "stop_reason": "tool_use",
            "content": [{
                "type": "tool_use",
                "id": format!("tu_{}", uuid::Uuid::new_v4()),
                "name": name,
                "input": input
            }]
        }));
        self
    }

    pub fn finish(self, text: &str) -> Self {
        self.queue.lock().unwrap().push_back(json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": text }]
        }));
        self
    }
}

impl Default for MockLlm {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for MockLlm {
    async fn complete(
        &self,
        _model: &str,
        _messages: Value,
        _tools: Value,
    ) -> Result<Completion, String> {
        let raw = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "MockLlm: out of scripted responses".to_string())?;
        Ok(Completion { raw })
    }
}
