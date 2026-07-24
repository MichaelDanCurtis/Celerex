use async_trait::async_trait;
use serde_json::Value;

/// OpenAI function-parameters JSON Schema object. Port of daemon `tools.ts`.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }
    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// The extensibility seam: a named group of tools that can be listed + executed.
#[async_trait]
pub trait ToolSource: Send + Sync {
    /// 'file' | 'mcp:<server>' | ...
    fn id(&self) -> &str;
    async fn list_tools(&self) -> Vec<ToolSpec>;
    async fn execute(&self, name: &str, args: &Value) -> ToolResult;
}

const MCP_SOURCE_PREFIX: &str = "mcp:";
const MCP_TOOL_DELIMITER: &str = "__";

/// File tools keep bare names; MCP tools become `mcp__<server>__<tool>`; any
/// other source becomes `<sourceId>__<tool>`.
pub fn namespace_tool_name(source_id: &str, tool_name: &str) -> String {
    if source_id == "file" {
        return tool_name.to_string();
    }
    if let Some(server) = source_id.strip_prefix(MCP_SOURCE_PREFIX) {
        return format!("mcp{MCP_TOOL_DELIMITER}{server}{MCP_TOOL_DELIMITER}{tool_name}");
    }
    format!("{source_id}{MCP_TOOL_DELIMITER}{tool_name}")
}

/// True when `tool_name` could have been produced by `namespace_tool_name(source_id, ...)`.
pub fn owns_tool_name(source_id: &str, tool_name: &str) -> bool {
    if source_id == "file" {
        return !tool_name.contains(MCP_TOOL_DELIMITER);
    }
    tool_name.starts_with(&namespace_prefix(source_id))
}

pub(crate) fn namespace_prefix(source_id: &str) -> String {
    if let Some(server) = source_id.strip_prefix(MCP_SOURCE_PREFIX) {
        return format!("mcp{MCP_TOOL_DELIMITER}{server}{MCP_TOOL_DELIMITER}");
    }
    format!("{source_id}{MCP_TOOL_DELIMITER}")
}
