//! Feature-gated (`test-support`): serves canned A2A responses keyed by
//! `target`. Used by integration tests and local dev. Any peer plugin can
//! use it by seeding a (target -> payload) map.

use crate::handlers::agent::A2AClient;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct FixtureA2AClient {
    pub fixtures: HashMap<String, String>,
}

impl FixtureA2AClient {
    pub fn new() -> Self { Self { fixtures: HashMap::new() } }
    pub fn with_fixture(mut self, target: &str, body: &str) -> Self {
        self.fixtures.insert(target.to_string(), body.to_string());
        self
    }
}

impl Default for FixtureA2AClient {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl A2AClient for FixtureA2AClient {
    async fn call(&self, target: &str, _params: Value) -> Result<Value, String> {
        match self.fixtures.get(target) {
            Some(body) => Ok(json!({
                "request_id": uuid::Uuid::new_v4().to_string(),
                "status": "success",
                "result": body,
            })),
            None => Err(format!("fixture A2A: unknown target: {target}")),
        }
    }
}
