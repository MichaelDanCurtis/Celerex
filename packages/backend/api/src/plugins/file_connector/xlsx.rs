use super::{ConnectorError, FileConnector, ToolDef};
use async_trait::async_trait;
use calamine::{open_workbook, Data, Reader, Xlsx, XlsxError};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Default)]
pub struct XlsxConnector;

impl XlsxConnector {
    fn sheet_to_markdown(rows: &[Vec<Data>]) -> String {
        if rows.is_empty() {
            return String::new();
        }
        let cell = |d: &Data| match d {
            Data::Empty => String::new(),
            Data::String(s) => s.clone(),
            Data::Float(f) => {
                if f.fract() == 0.0 {
                    (*f as i64).to_string()
                } else {
                    f.to_string()
                }
            }
            Data::Int(i) => i.to_string(),
            Data::Bool(b) => b.to_string(),
            Data::DateTime(d) => d.to_string(),
            Data::Error(e) => format!("#{e:?}"),
            Data::DurationIso(s) | Data::DateTimeIso(s) => s.clone(),
        };
        let header = &rows[0];
        let mut out = String::new();
        out.push_str("| ");
        out.push_str(&header.iter().map(cell).collect::<Vec<_>>().join(" | "));
        out.push_str(" |\n| ");
        out.push_str(&vec!["---"; header.len()].join(" | "));
        out.push_str(" |\n");
        for r in &rows[1..] {
            out.push_str("| ");
            out.push_str(&r.iter().map(cell).collect::<Vec<_>>().join(" | "));
            out.push_str(" |\n");
        }
        out
    }
}

#[async_trait]
impl FileConnector for XlsxConnector {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError> {
        let path = file.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<String, ConnectorError> {
            let mut wb: Xlsx<BufReader<File>> =
                open_workbook(&path).map_err(|e: XlsxError| ConnectorError::Io(e.to_string()))?;
            let name = wb
                .sheet_names()
                .first()
                .cloned()
                .ok_or(ConnectorError::Parse("no sheets".into()))?;
            let range = wb
                .worksheet_range(&name)
                .map_err(|e| ConnectorError::Parse(e.to_string()))?;
            let rows: Vec<Vec<Data>> = range.rows().map(|r: &[Data]| r.to_vec()).collect();
            Ok(Self::sheet_to_markdown(&rows))
        })
        .await
        .map_err(|e| ConnectorError::Io(e.to_string()))?
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
