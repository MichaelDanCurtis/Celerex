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
        Ok(vec![
            ToolDef {
                name: "list_sheets".into(),
                description: "Return the sheet names in the workbook.".into(),
                input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            },
            ToolDef {
                name: "get_cell".into(),
                description: "Return a single cell value (1-indexed).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sheet": { "type": ["string", "null"] },
                        "row": { "type": "integer", "minimum": 1 },
                        "col": { "type": "integer", "minimum": 1 }
                    },
                    "required": ["row", "col"]
                }),
            },
        ])
    }

    async fn invoke_tool(
        &self,
        file: &Path,
        name: &str,
        args: Value,
    ) -> Result<Value, ConnectorError> {
        let path = file.to_path_buf();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || -> Result<Value, ConnectorError> {
            let mut wb: Xlsx<BufReader<File>> = open_workbook(&path)
                .map_err(|e: XlsxError| ConnectorError::Io(e.to_string()))?;
            match name.as_str() {
                "list_sheets" => Ok(Value::Array(
                    wb.sheet_names()
                        .iter()
                        .map(|s| Value::String(s.clone()))
                        .collect(),
                )),
                "get_cell" => {
                    let sheet = args["sheet"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| wb.sheet_names().first().cloned())
                        .ok_or(ConnectorError::Parse("no sheet".into()))?;
                    let row = args["row"]
                        .as_u64()
                        .ok_or(ConnectorError::Parse("row".into()))?
                        as usize;
                    let col = args["col"]
                        .as_u64()
                        .ok_or(ConnectorError::Parse("col".into()))?
                        as usize;
                    let range = wb
                        .worksheet_range(&sheet)
                        .map_err(|e| ConnectorError::Parse(e.to_string()))?;
                    let rows: Vec<Vec<Data>> =
                        range.rows().map(|r: &[Data]| r.to_vec()).collect();
                    let cell = rows
                        .get(row.saturating_sub(1))
                        .and_then(|r| r.get(col.saturating_sub(1)))
                        .ok_or(ConnectorError::Parse("out of range".into()))?;
                    let as_str = match cell {
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
                        Data::Empty => String::new(),
                        Data::Error(e) => format!("#{e:?}"),
                        Data::DateTime(d) => d.to_string(),
                        Data::DurationIso(s) | Data::DateTimeIso(s) => s.clone(),
                    };
                    Ok(Value::String(as_str))
                }
                other => Err(ConnectorError::UnknownTool(other.into())),
            }
        })
        .await
        .map_err(|e| ConnectorError::Io(e.to_string()))?
    }
}
