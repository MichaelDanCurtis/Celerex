use super::provider::{Completion, Provider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct StubProvider {
    scripted: Mutex<VecDeque<Value>>,
}

impl StubProvider {
    pub fn new(responses: Vec<Value>) -> Self {
        Self {
            scripted: Mutex::new(responses.into()),
        }
    }

    pub fn text(body: &str) -> Self {
        Self::new(vec![json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": body }]
        })])
    }
}

#[async_trait]
impl Provider for StubProvider {
    async fn complete(
        &self,
        _model: &str,
        _messages: Value,
        _tools: Value,
    ) -> Result<Completion, String> {
        let raw = self
            .scripted
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "StubProvider: no more scripted responses".to_string())?;
        Ok(Completion { raw })
    }
}
