use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use super::tools::{ToolResult, ToolSource, ToolSpec};
#[cfg(test)]
use super::tools::{namespace_tool_name, owns_tool_name};

/// In-memory per-run working set of project files. Replaces the daemon's
/// on-disk project dir; a design run is single-shot and ephemeral, so the
/// files only need to survive the loop, after which `index.html` is validated
/// and persisted as one artifact row.
pub struct FileStore {
    files: Mutex<HashMap<String, String>>,
}

impl FileStore {
    pub fn new() -> Arc<FileStore> {
        Arc::new(FileStore {
            files: Mutex::new(HashMap::new()),
        })
    }

    pub fn read_index_html(&self) -> Option<String> {
        self.files.lock().unwrap().get("index.html").cloned()
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

pub struct FileToolSource {
    store: Arc<FileStore>,
}

impl FileToolSource {
    pub fn new(store: Arc<FileStore>) -> FileToolSource {
        FileToolSource { store }
    }

    fn specs() -> Vec<ToolSpec> {
        use serde_json::json;
        vec![
            ToolSpec {
                name: "read_file".into(),
                description: "Read a UTF-8 text file from the project by project-relative path."
                    .into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Project-relative file path." } }, "required": ["path"] }),
            },
            ToolSpec {
                name: "write_file".into(),
                description: "Create or overwrite a project file with the given UTF-8 content."
                    .into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"] }),
            },
            ToolSpec {
                name: "edit_file".into(),
                description:
                    "Replace the first occurrence of an exact substring in a project file.".into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string" }, "find": { "type": "string" }, "replace": { "type": "string" } }, "required": ["path", "find", "replace"] }),
            },
            ToolSpec {
                name: "list_files".into(),
                description: "List the project files (project-relative paths).".into(),
                parameters: json!({ "type": "object", "properties": {}, "required": [] }),
            },
        ]
    }
}

#[async_trait]
impl ToolSource for FileToolSource {
    fn id(&self) -> &str {
        "file"
    }

    async fn list_tools(&self) -> Vec<ToolSpec> {
        FileToolSource::specs()
    }

    async fn execute(&self, name: &str, args: &Value) -> ToolResult {
        match name {
            "write_file" => {
                let (Some(path), Some(content)) = (arg_str(args, "path"), arg_str(args, "content"))
                else {
                    return ToolResult::err(
                        "invalid arguments: write_file requires string \"path\" and \"content\"",
                    );
                };
                self.store
                    .files
                    .lock()
                    .unwrap()
                    .insert(path.to_string(), content.to_string());
                ToolResult::ok(format!("wrote {path} ({} bytes)", content.len()))
            }
            "read_file" => {
                let Some(path) = arg_str(args, "path") else {
                    return ToolResult::err(
                        "invalid arguments: read_file requires string \"path\"",
                    );
                };
                match self.store.files.lock().unwrap().get(path) {
                    Some(c) => ToolResult::ok(c.clone()),
                    None => ToolResult::err(format!("read_file: {path} not found")),
                }
            }
            "edit_file" => {
                let (Some(path), Some(find), Some(replace)) = (
                    arg_str(args, "path"),
                    arg_str(args, "find"),
                    arg_str(args, "replace"),
                ) else {
                    return ToolResult::err(
                        "invalid arguments: edit_file requires string \"path\", \"find\", \"replace\"",
                    );
                };
                let mut guard = self.store.files.lock().unwrap();
                let Some(text) = guard.get(path).cloned() else {
                    return ToolResult::err(format!("edit_file: {path} not found"));
                };
                let Some(idx) = text.find(find) else {
                    return ToolResult::err(format!("edit_file: substring not found in {path}"));
                };
                let next = format!("{}{}{}", &text[..idx], replace, &text[idx + find.len()..]);
                guard.insert(path.to_string(), next);
                ToolResult::ok(format!("edited {path}"))
            }
            "list_files" => {
                let guard = self.store.files.lock().unwrap();
                let mut names: Vec<String> = guard.keys().cloned().collect();
                names.sort();
                let joined = names.join("\n");
                ToolResult::ok(if joined.is_empty() {
                    "(no files)".to_string()
                } else {
                    joined
                })
            }
            other => ToolResult::err(format!("unknown file tool: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn write_then_read_roundtrips_and_reports_bytes() {
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        let w = src
            .execute(
                "write_file",
                &json!({ "path": "index.html", "content": "<h1>hi</h1>" }),
            )
            .await;
        assert!(!w.is_error);
        assert_eq!(w.content, "wrote index.html (11 bytes)");
        let r = src
            .execute("read_file", &json!({ "path": "index.html" }))
            .await;
        assert_eq!(r.content, "<h1>hi</h1>");
        assert_eq!(store.read_index_html().as_deref(), Some("<h1>hi</h1>"));
    }

    #[tokio::test]
    async fn edit_replaces_first_occurrence_only() {
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        src.execute("write_file", &json!({ "path": "a.txt", "content": "x x" }))
            .await;
        let e = src
            .execute(
                "edit_file",
                &json!({ "path": "a.txt", "find": "x", "replace": "y" }),
            )
            .await;
        assert!(!e.is_error);
        let r = src.execute("read_file", &json!({ "path": "a.txt" })).await;
        assert_eq!(r.content, "y x");
    }

    #[tokio::test]
    async fn edit_missing_substring_is_error() {
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        src.execute("write_file", &json!({ "path": "a.txt", "content": "abc" }))
            .await;
        let e = src
            .execute(
                "edit_file",
                &json!({ "path": "a.txt", "find": "zzz", "replace": "y" }),
            )
            .await;
        assert!(e.is_error);
    }

    #[tokio::test]
    async fn read_missing_file_is_error_and_list_reports_files() {
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        let r = src.execute("read_file", &json!({ "path": "nope" })).await;
        assert!(r.is_error);
        src.execute(
            "write_file",
            &json!({ "path": "index.html", "content": "x" }),
        )
        .await;
        let l = src.execute("list_files", &json!({})).await;
        assert!(!l.is_error);
        assert_eq!(l.content, "index.html");
    }

    #[test]
    fn namespacing_is_bare_for_file_prefixed_for_mcp() {
        assert_eq!(namespace_tool_name("file", "write_file"), "write_file");
        assert_eq!(
            namespace_tool_name("mcp:figma", "get_file"),
            "mcp__figma__get_file"
        );
        assert!(owns_tool_name("file", "write_file"));
        assert!(!owns_tool_name("file", "mcp__figma__get_file"));
        assert!(owns_tool_name("mcp:figma", "mcp__figma__get_file"));
    }
}
