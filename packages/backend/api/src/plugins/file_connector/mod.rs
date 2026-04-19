pub mod xlsx;
pub mod docx;

use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[async_trait]
pub trait FileConnector: Send + Sync {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError>;
    async fn tool_definitions(&self, file: &Path) -> Result<Vec<ToolDef>, ConnectorError>;
    async fn invoke_tool(&self, file: &Path, name: &str, args: Value) -> Result<Value, ConnectorError>;
}
