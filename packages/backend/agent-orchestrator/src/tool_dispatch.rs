use crate::execution_type::ExecutionType;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("no handler registered for {0:?}")]
    NoHandler(ExecutionType),
    #[error("handler error: {0}")]
    Handler(String),
}

#[async_trait]
pub trait Handler: Send + Sync {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError>;
}

pub struct Dispatcher {
    handlers: HashMap<ExecutionType, Arc<dyn Handler>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(&mut self, kind: ExecutionType, handler: Arc<dyn Handler>) {
        self.handlers.insert(kind, handler);
    }

    pub async fn invoke(
        &self,
        kind: ExecutionType,
        config: Value,
        inputs: Value,
    ) -> Result<Value, DispatchError> {
        let h = self
            .handlers
            .get(&kind)
            .ok_or(DispatchError::NoHandler(kind))?;
        h.invoke(config, inputs).await
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
