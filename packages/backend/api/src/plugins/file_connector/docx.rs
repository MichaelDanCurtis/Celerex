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
        Ok(vec![ToolDef {
            name: "find_heading".into(),
            description: "Return the paragraph text under a given H1/H2 heading.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "heading": { "type": "string" } },
                "required": ["heading"]
            }),
        }])
    }

    async fn invoke_tool(
        &self,
        file: &Path,
        name: &str,
        args: Value,
    ) -> Result<Value, ConnectorError> {
        let bytes = tokio::fs::read(file)
            .await
            .map_err(|e| ConnectorError::Io(e.to_string()))?;
        let doc = read_docx(&bytes).map_err(|e| ConnectorError::Parse(e.to_string()))?;
        match name {
            "find_heading" => {
                let target = args["heading"]
                    .as_str()
                    .ok_or(ConnectorError::Parse("heading".into()))?
                    .to_string();
                let mut collecting = false;
                let mut out = Vec::new();
                for ch in doc.document.children {
                    if let DocumentChild::Paragraph(p) = ch {
                        let txt = para_text(&p);
                        let is_heading = p
                            .property
                            .style
                            .as_ref()
                            .map(|s| s.val.starts_with("Heading"))
                            .unwrap_or(false);
                        if is_heading {
                            if txt == target {
                                collecting = true;
                                continue;
                            }
                            if collecting {
                                break;
                            }
                        } else if collecting && !txt.is_empty() {
                            out.push(txt);
                        }
                    }
                }
                Ok(Value::String(out.join("\n")))
            }
            other => Err(ConnectorError::UnknownTool(other.into())),
        }
    }
}
