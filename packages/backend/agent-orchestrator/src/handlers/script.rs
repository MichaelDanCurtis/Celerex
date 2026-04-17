use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

pub struct ScriptHandler;

#[async_trait]
impl Handler for ScriptHandler {
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let cmd = config
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing command".into()))?;
        let args: Vec<String> = config
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let out = Command::new(cmd)
            .args(args)
            .output()
            .await
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({
            "stdout": String::from_utf8_lossy(&out.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&out.stderr).to_string(),
            "exit_code": out.status.code().unwrap_or(-1),
        }))
    }
}
