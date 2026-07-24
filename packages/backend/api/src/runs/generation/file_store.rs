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

/// Daemon parity: `apps/daemon/src/projects.ts` `validateProjectPath`. A run
/// is single-shot and the file tool is the *only* write path into a design's
/// artifact, so a path the model got wrong must be a loud tool error it can
/// see and retry — never a silent write under a key nothing will ever read
/// back (the bug this function exists to close: `write_file("./index.html",
/// …)` used to succeed silently while `read_index_html()` — hardcoded to the
/// exact key `"index.html"` — returned `None`).
///
/// Mirrors the daemon's rejections: empty/whitespace-only input, embedded
/// NUL, a Windows drive letter or leading `/` (absolute), and any `/`
/// segment that is empty, `.`, or `..` (the daemon's
/// `FORBIDDEN_SEGMENT = /^$|^\.\.?$/`). Backslashes are normalized to `/`
/// first, matching the daemon's own `raw.replace(/\\/g, '/')`.
///
/// One deliberate divergence: the daemon computes
/// `normalized.split('/').filter(Boolean)` — dropping empty segments
/// *before* testing `FORBIDDEN_SEGMENT` — so a raw double slash like
/// `"a//b"` silently collapses to the accepted path `"a/b"` there; the
/// empty-string arm of `FORBIDDEN_SEGMENT` never actually fires in the
/// daemon's own runtime. We test segments *before* dropping empties, so
/// `"a//b"` (and a trailing/leading `/` between otherwise-valid segments)
/// is rejected loudly instead of silently collapsed. This is strictly
/// stricter, never looser, than the daemon: it cannot accept anything the
/// daemon would reject, and closing an unintended silent-normalization
/// quirk is consistent with this fix's whole point — loud rejection beats
/// silent divergence from what the model asked for.
///
/// The daemon's separate `RESERVED_PROJECT_FILE_SEGMENTS` check (rejecting
/// the single internal segment name `.live-artifacts` with a *different*
/// error, `'reserved project path'`) is intentionally not ported here: it
/// guards a daemon-only on-disk sentinel unrelated to the in-memory
/// `FileStore`, and porting it is out of this fix's narrow scope.
fn validate_project_path(raw: &str) -> Result<String, &'static str> {
    const ERR: &str = "invalid file name";
    if raw.trim().is_empty() {
        return Err(ERR);
    }
    if raw.contains('\0') {
        return Err(ERR);
    }
    let normalized = raw.replace('\\', "/");
    let starts_with_drive_letter = {
        let bytes = normalized.as_bytes();
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
    };
    if starts_with_drive_letter || normalized.starts_with('/') {
        return Err(ERR);
    }
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.is_empty()
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err(ERR);
    }
    Ok(parts.join("/"))
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
                let key = match validate_project_path(path) {
                    Ok(key) => key,
                    Err(msg) => return ToolResult::err(format!("file tool error: {msg}")),
                };
                self.store
                    .files
                    .lock()
                    .unwrap()
                    .insert(key, content.to_string());
                ToolResult::ok(format!("wrote {path} ({} bytes)", content.len()))
            }
            "read_file" => {
                let Some(path) = arg_str(args, "path") else {
                    return ToolResult::err(
                        "invalid arguments: read_file requires string \"path\"",
                    );
                };
                let key = match validate_project_path(path) {
                    Ok(key) => key,
                    Err(msg) => return ToolResult::err(format!("file tool error: {msg}")),
                };
                match self.store.files.lock().unwrap().get(&key) {
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
                let key = match validate_project_path(path) {
                    Ok(key) => key,
                    Err(msg) => return ToolResult::err(format!("file tool error: {msg}")),
                };
                let mut guard = self.store.files.lock().unwrap();
                let Some(text) = guard.get(&key).cloned() else {
                    return ToolResult::err(format!("edit_file: {path} not found"));
                };
                let Some(idx) = text.find(find) else {
                    return ToolResult::err(format!("edit_file: substring not found in {path}"));
                };
                let next = format!("{}{}{}", &text[..idx], replace, &text[idx + find.len()..]);
                guard.insert(key, next);
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

    // --- daemon validateProjectPath parity: reject bad paths loudly instead
    // of silently accepting them under a divergent key (see
    // apps/daemon/src/projects.ts validateProjectPath / sanitizePath). ---

    #[tokio::test]
    async fn write_file_rejects_leading_dot_slash_and_produces_no_artifact() {
        // Exact repro of the silent-artifact-loss bug: a model writing to
        // "./index.html" must be rejected loudly, not silently stored under
        // a key that read_index_html() will never find.
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        let w = src
            .execute(
                "write_file",
                &json!({ "path": "./index.html", "content": "<h1>hi</h1>" }),
            )
            .await;
        assert!(w.is_error);
        assert_eq!(w.content, "file tool error: invalid file name");
        assert!(store.read_index_html().is_none());
    }

    #[tokio::test]
    async fn write_file_rejects_the_forbidden_path_matrix() {
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        for bad in ["../x", "/abs", "", "a//b", "bad\0name"] {
            let w = src
                .execute("write_file", &json!({ "path": bad, "content": "x" }))
                .await;
            assert!(
                w.is_error,
                "expected {:?} to be rejected, got: {:?}",
                bad, w.content
            );
        }
    }

    #[tokio::test]
    async fn write_file_accepts_the_happy_path_control() {
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        for good in ["index.html", "assets/style.css", "notes..final.html"] {
            let w = src
                .execute("write_file", &json!({ "path": good, "content": "ok" }))
                .await;
            assert!(
                !w.is_error,
                "expected {:?} to be accepted, got: {:?}",
                good, w.content
            );
        }
        assert_eq!(store.read_index_html().as_deref(), Some("ok"));
    }

    #[tokio::test]
    async fn read_file_rejects_invalid_path() {
        // Assert the *specific* validation message, not just is_error: a
        // nonexistent-but-well-shaped path also returns is_error=true via
        // the pre-existing "not found" branch, which would let this test
        // pass for the wrong reason (missing file, not rejected path).
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        let r = src.execute("read_file", &json!({ "path": "../x" })).await;
        assert!(r.is_error);
        assert_eq!(r.content, "file tool error: invalid file name");
    }

    #[tokio::test]
    async fn edit_file_rejects_invalid_path() {
        // Same rationale as read_file_rejects_invalid_path above: assert the
        // validation-specific message so a coincidental "not found" error
        // can't make this test pass without real validation.
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        let e = src
            .execute(
                "edit_file",
                &json!({ "path": "../x", "find": "a", "replace": "b" }),
            )
            .await;
        assert!(e.is_error);
        assert_eq!(e.content, "file tool error: invalid file name");
    }
}
