use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::{json, Value};

/// Handler for trigger / seed nodes (`ExecutionType::Input`).
///
/// Entry points (manual run, schedule tick, webhook) have no upstream to pull
/// from, so they simply emit a payload that downstream steps can template
/// against. It echoes the resolved `inputs`; if none were provided it falls
/// back to `config.payload`, then to a minimal `{ "triggered": true }`.
pub struct InputHandler;

#[async_trait]
impl Handler for InputHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let has_inputs = match &inputs {
            Value::Null => false,
            Value::Object(o) => !o.is_empty(),
            Value::Array(a) => !a.is_empty(),
            _ => true,
        };
        if has_inputs {
            return Ok(inputs);
        }
        Ok(config
            .get("payload")
            .cloned()
            .unwrap_or_else(|| json!({ "triggered": true })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echoes_inputs_when_present() {
        let out = InputHandler
            .invoke(json!({}), json!({ "window_days": 7 }))
            .await
            .unwrap();
        assert_eq!(out, json!({ "window_days": 7 }));
    }

    #[tokio::test]
    async fn falls_back_to_triggered_marker() {
        let out = InputHandler.invoke(json!({}), json!({})).await.unwrap();
        assert_eq!(out, json!({ "triggered": true }));
    }

    #[tokio::test]
    async fn uses_config_payload_fallback() {
        let out = InputHandler
            .invoke(json!({ "payload": { "seed": 1 } }), Value::Null)
            .await
            .unwrap();
        assert_eq!(out, json!({ "seed": 1 }));
    }
}
