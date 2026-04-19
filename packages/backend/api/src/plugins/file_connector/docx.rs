use super::{ConnectorError, FileConnector, ToolDef};
use async_trait::async_trait;
use docx_rs::{read_docx, DocumentChild, Paragraph, ParagraphChild, RunChild};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
pub struct DocxConnector;

fn para_text(p: &Paragraph) -> String {
    let mut out = String::new();
    for child in &p.children {
        if let ParagraphChild::Run(r) = child {
            for rc in &r.children {
                if let RunChild::Text(t) = rc {
                    out.push_str(&t.text);
                }
            }
        }
    }
    out
}

fn para_to_md(p: &Paragraph) -> String {
    let txt = para_text(p);
    if let Some(style) = &p.property.style {
        let s = &style.val;
        if s == "Heading1" {
            return format!("# {txt}");
        }
        if s == "Heading2" {
            return format!("## {txt}");
        }
        if s == "Heading3" {
            return format!("### {txt}");
        }
        if s.starts_with("ListBullet") {
            return format!("- {txt}");
        }
    }
    txt
}

#[async_trait]
impl FileConnector for DocxConnector {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError> {
        let bytes = tokio::fs::read(file)
            .await
            .map_err(|e| ConnectorError::Io(e.to_string()))?;
        let doc = read_docx(&bytes).map_err(|e| ConnectorError::Parse(e.to_string()))?;
        let mut lines = Vec::new();
        for ch in doc.document.children {
            if let DocumentChild::Paragraph(p) = ch {
                lines.push(para_to_md(&p));
            }
        }
        Ok(lines.join("\n\n"))
    }

    async fn tool_definitions(&self, _file: &Path) -> Result<Vec<ToolDef>, ConnectorError> {
        Ok(vec![])
    }

    async fn invoke_tool(
        &self,
        _file: &Path,
        name: &str,
        _args: Value,
    ) -> Result<Value, ConnectorError> {
        Err(ConnectorError::UnknownTool(name.into()))
    }
}
