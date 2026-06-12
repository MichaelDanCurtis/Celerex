//! Doc execution handler — the runtime behind `ExecutionType::Doc` nodes.
//!
//! This is what makes the "doc builder" story real: a flow can `create` a
//! document, `append` generated content to it, and a later node (or a later
//! flow) can `read` it back — so one document feeds another. Because the
//! executor resolves `config` templates before dispatch, a downstream
//! `append` node can target `{{steps.<create_node>.outputs.document_id}}`,
//! chaining writes onto a doc produced earlier in the same run.
//!
//! Config contract (all string fields support `{{steps.X.outputs.Y}}`):
//!   { "execution": "doc",
//!     "op": "create" | "append" | "read",
//!     "title": "Weekly Digest {{...}}",   // create
//!     "engagement_id": "<uuid>",          // create (injected by enqueue_run)
//!     "project_id": "<uuid>",             // create (optional)
//!     "document_id": "<uuid>",            // append | read
//!     "inputs": { ... } }                 // content to write (append/create)
//!
//! The block text is extracted leniently from `inputs`: a raw string, a
//! `{markdown|text|content|result}` field, or an Anthropic-style
//! `{content:[{type:"text",text}]}` payload (what the LLM handler emits).

use crate::db;
use async_trait::async_trait;
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub struct DocHandler {
    pool: PgPool,
}

impl DocHandler {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn handler_err(msg: impl Into<String>) -> DispatchError {
    DispatchError::Handler(msg.into())
}

fn cfg_str<'a>(config: &'a Value, key: &str) -> Option<&'a str> {
    config.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn parse_uuid(config: &Value, key: &str) -> Result<Uuid, DispatchError> {
    let raw = cfg_str(config, key).ok_or_else(|| handler_err(format!("doc: missing `{key}`")))?;
    Uuid::parse_str(raw).map_err(|e| handler_err(format!("doc: `{key}` is not a uuid: {e}")))
}

/// Pull human-readable text out of whatever an upstream node produced.
pub fn extract_text(inputs: &Value) -> String {
    match inputs {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            for k in ["markdown", "text", "content", "result", "reply"] {
                if let Some(Value::String(s)) = o.get(k) {
                    return s.clone();
                }
            }
            // Anthropic-style: { content: [ { type: "text", text: "..." }, ... ] }
            if let Some(Value::Array(arr)) = o.get("content") {
                let mut out = String::new();
                for item in arr {
                    if let Some(Value::String(s)) = item.get("text") {
                        out.push_str(s);
                    }
                }
                if !out.is_empty() {
                    return out;
                }
            }
            serde_json::to_string_pretty(inputs).unwrap_or_default()
        }
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[async_trait]
impl Handler for DocHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let op = cfg_str(&config, "op").unwrap_or("append");
        match op {
            "create" => {
                let title = cfg_str(&config, "title").unwrap_or("Untitled").to_string();
                let engagement_id = parse_uuid(&config, "engagement_id")?;
                let project_id = match cfg_str(&config, "project_id") {
                    Some(p) => Some(
                        Uuid::parse_str(p)
                            .map_err(|e| handler_err(format!("doc: bad project_id: {e}")))?,
                    ),
                    None => None,
                };
                let doc = db::create_document(&self.pool, engagement_id, project_id, &title)
                    .await
                    .map_err(|e| handler_err(format!("doc create: {e}")))?;
                // If upstream piped content in, seed the document with a first block.
                let text = extract_text(&inputs);
                let mut block_id: Option<Uuid> = None;
                if !text.trim().is_empty() {
                    let payload = json!({ "type": "markdown", "text": text });
                    block_id = Some(
                        db::append_block(&self.pool, doc.id, &payload)
                            .await
                            .map_err(|e| handler_err(format!("doc create-block: {e}")))?,
                    );
                }
                Ok(json!({
                    "document_id": doc.id,
                    "title": doc.title,
                    "block_id": block_id,
                    "created": true,
                }))
            }
            "append" => {
                let doc_id = parse_uuid(&config, "document_id")?;
                let text = extract_text(&inputs);
                let payload = json!({ "type": "markdown", "text": text });
                let block_id = db::append_block(&self.pool, doc_id, &payload)
                    .await
                    .map_err(|e| handler_err(format!("doc append: {e}")))?;
                Ok(json!({
                    "document_id": doc_id,
                    "block_id": block_id,
                    "appended": true,
                    "chars": text.chars().count(),
                }))
            }
            "read" => {
                let doc_id = parse_uuid(&config, "document_id")?;
                let blocks = db::list_blocks(&self.pool, doc_id)
                    .await
                    .map_err(|e| handler_err(format!("doc read: {e}")))?;
                let mut text = String::new();
                for b in &blocks {
                    if let Some(t) = b.payload.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            text.push_str("\n\n");
                        }
                        text.push_str(t);
                    }
                }
                Ok(json!({
                    "document_id": doc_id,
                    "block_count": blocks.len(),
                    "text": text,
                    "blocks": blocks,
                }))
            }
            other => Err(handler_err(format!("doc: unknown op `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_plain_string() {
        assert_eq!(extract_text(&json!("hello")), "hello");
    }

    #[test]
    fn extracts_markdown_field() {
        assert_eq!(extract_text(&json!({ "markdown": "# Title" })), "# Title");
    }

    #[test]
    fn extracts_anthropic_content_array() {
        let v = json!({ "content": [ { "type": "text", "text": "part one " }, { "type": "text", "text": "part two" } ] });
        assert_eq!(extract_text(&v), "part one part two");
    }

    #[test]
    fn falls_back_to_pretty_json() {
        let v = json!({ "weird": 1 });
        assert!(extract_text(&v).contains("weird"));
    }
}
