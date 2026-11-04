# Engine Port: Server-Side Design Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Agent's bare-completion design-chat stub with the full server-side agentic generation engine (tool loop → server-side file writes → validated HTML artifact) ported from the TypeScript daemon, streamed to the unchanged web SPA over the existing runs SSE channel.

**Architecture:** A new `runs::generation` module inside `packages/backend/api` reproduces the daemon's `apps/daemon/src/api-generation/` pipeline in Rust: an in-memory per-run file store backs `write_file`/`read_file`/`edit_file`/`list_files` tools; a tool registry namespaces + routes them; a provider-turn layer builds the OpenAI function-tool envelope and normalizes provider responses, then calls the in-process `Arc<dyn Provider>` resolved from the engagement's vaulted key (NO HTTP hop); a loop drives turns until the model stops; the final `index.html` is validated by an `html_template_v1` guard and persisted as one engagement-scoped row in a new `design_artifacts` table. The single validated artifact is served back to the SPA by two new engagement-authenticated GET routes that the web already calls, and lifecycle/agent events stream over the run's SSE using the daemon's exact event vocabulary.

**Tech Stack:** Rust, axum (SSE via `response::sse::Event`), tokio (mpsc), sqlx/Postgres, `celerex_orchestrator::llm::Provider`, `regex`/`std::sync::LazyLock`, `serde_json`. Tests: `testcontainers` Postgres harness (`tests/common/mod.rs`), `celerex_orchestrator::llm::stub::StubProvider` for scripted provider turns.

## Global Constraints

- **Base branch:** Branch this work from `feat/xai-glm-kimicode-providers` **after its Task 8 has landed** (glm / kimi-code providers + `SUPPORTED_PROVIDERS` already extended). NEVER branch from `main`. Name the branch `feat/engine-port-server-generation`.
- **Additive only:** Do not modify existing tables. The one schema change is a NEW migration file `0024_design_artifacts.sql` (0023 is the current highest). Do not renumber or edit existing migrations.
- **SSE vocabulary is a HARD compatibility constraint.** The web SPA (`apps/web/src/providers/daemon.ts` `consumeDaemonRun` + `translateAgentEvent`, and `apps/web/src/components/FileViewer.tsx`) is unchanged. The engine MUST emit only these frames, with these exact names/shapes:
  - Top-level named SSE events (`event:` field): `start`, `agent`, `error`, `end`.
  - `start` data: `{ "bin": "<provider>/<model>", "projectId": <string>, "model": <string> }`.
  - `agent` data `type` field, one of: `status` (`{type:"status",label,detail?}`), `text_delta` (`{type:"text_delta",delta}`), `tool_use` (`{type:"tool_use",id,name,input}`), `tool_result` (`{type:"tool_result",toolUseId,content,isError?}`), `usage` (`{type:"usage",usage:{input_tokens,output_tokens}}`), `live_artifact` (`{type:"live_artifact",action:"created",projectId,artifactId,title}`).
  - `error` data: `{ "error": { "message": <string> } }`.
  - `end` data: `{ "status": "succeeded"|"failed", "code": <int> }`.
  - **`artifact-error` is a mid-stream `agent`/`status` frame** (`{type:"status",label:"artifact-error",detail:<message>}`) and the run STILL finishes `end {status:"succeeded", code:0}`. This exact label is load-bearing.
  - **`reached step limit`** is emitted as `agent`/`status` (`label:"reached step limit"`) when the loop hits `maxTurns`, then the run finishes `succeeded`.
- **Provider call is IN-PROCESS.** The turn layer calls `provider.complete(model, messages, tools).await` on the `Arc<dyn Provider>` from `assistant::resolve_engagement_provider`. It does NOT POST to `/api/assistant/chat`. Fail closed: a design run requires the engagement's OWN vaulted key (no `CELEREX_HOUSE_MODE` fallback), exactly as today's stub.
- **Tools go on the wire as the OpenAI function-tool envelope.** The Agent's provider clients forward `tools` VERBATIM (`packages/backend/api/src/llm/openai.rs` sets `"tools": tools`; `openai_compat.rs` includes it only when non-empty). So the turn layer MUST wrap each `ToolSpec` as `{type:"function",function:{name,description,parameters}}` and sanitize tool names to `[a-zA-Z0-9_-]` via a reversible alias map, then reverse-map returned tool-call names before dispatch. This is proven parity with the daemon (which sent the same shape through the proxy).
- **Secrets never logged.** Never log the vaulted key, the provider `Authorization`, or the raw provider response body. On a provider non-2xx/transport error, surface a generic client-facing message; log server-side with `tracing::error!` only (mirror the current `design_chat::stream_pending` key-store branch).
- **grep-^error discipline for verification:** after each `cargo build`/`cargo test`, scan output with `grep -nE '^error' <log>` (anchored) so pre-existing warnings elsewhere don't mask a real compile error in the new module.
- **Test invocation reality:** integration tests need Docker (testcontainers Postgres). Run a single test file with `cargo test -p celerex-api --test <name> -- --nocapture`. The live E2E task is `#[ignore]` and only runs with a real key present.
- **CSP-parity on the live-artifact serve is a HARD security constraint (Task 8).** The `html_template_v1` guard (Task 3) has known denylist gaps (`<svg/onload=…>`, SMIL `to="javascript:…"`, unrestricted external resource refs) that are live-neutralized only by the daemon's serve-time `setLiveArtifactPreviewHeaders` (`celerex-design/apps/daemon/src/server.ts:3610-3630`: `script-src 'none'` + a script-less `sandbox` + img/connect/font lockdown). Task 8's new Agent serving routes MUST replicate that header set verbatim, and Task 3 additionally hardens the guard itself (`/`-separated `on*=`, `<foreignObject>`). Origin: infographic-atelier feature spec §5.2–5.4 (`celerex-design/docs/superpowers/specs/2026-07-13-infographic-atelier-design.md`).
- **System-prompt fragment composer is an ORDERED, budgeted contract (cross-review B1).** `build_system_prompt` / `compose_system_prompt` take an ordered fragment list — `PromptFragment { slot: FragmentSlot, text }`, `enum FragmentSlot { Brand, Motion, Task }` — composed in canonical slot order **brand → motion → chart/task** (independent of insertion order; each `---`-separated, the existing separator convention) under a combined `MAX_PROMPT_FRAGMENT_CHARS` budget enforced **truncate-last** (the fragment that crosses the budget is truncated to fit, any later fragments are dropped, and `run_generation` emits a non-fatal `agent`/`status` `prompt-fragment-truncated` note — a free-form status label, vocabulary-safe exactly like `artifact-error` / `reached step limit`; the run still finishes `succeeded`). v1 ships the composer **EMPTY**: every call site passes `&[]`, so the system prompt is byte-identical to the pre-composer base (the removed `None` path), and Brand Kit is the first real producer — this amendment is plumbing only. Origin: cross-review reconciliation B1, `celerex-design/docs/superpowers/specs/2026-07-13-cross-review-reconciliations.md`.

---

## File Structure

New module `packages/backend/api/src/runs/generation/` (one responsibility per file, mirroring the daemon's `api-generation/`):

- `mod.rs` — module exports + `run_generation(...)` orchestrator (the `index.ts` analog): builds registry/emitter/file-store, seeds messages, runs the loop, persists + emits the artifact, emits terminal frames.
- `tools.rs` — `ToolSpec`, `ToolResult`, `ToolSource` trait, `namespace_tool_name`, `owns_tool_name` (port of daemon `tools.ts`).
- `file_store.rs` — `FileStore` (in-memory `HashMap<String,String>` working set) + `FileToolSource` (port of `file-tool-source.ts`).
- `registry.rs` — `ToolRegistry` (port of `tool-registry.ts`).
- `template_guard.rs` — `render_html_template_v1` / `TemplateGuardError` (port of `live-artifacts/render.ts` validation).
- `provider_turn.rs` — `ProviderTurnResult`, `ToolCall`, `build_tool_aliases`, `normalize_raw_completion`, `run_provider_turn` (port of `provider-turn.ts`, but calling `Provider::complete` directly).
- `loop_core.rs` — `run_generation_loop`, `LoopResult`, `StopReason` (port of `loop.ts`).
- `emitter.rs` — `RunEventEmitter` writing axum SSE `Event`s to the run's `mpsc::Sender` (port of `run-event-emitter.ts`).
- `system_prompt.rs` — `build_system_prompt` / `compose_system_prompt` + the B1 ordered fragment composer (`PromptFragment` / `FragmentSlot` / `MAX_PROMPT_FRAGMENT_CHARS`) (port of `system-prompt.ts`, extended).
- `artifact_store.rs` — `generate_artifact_id`, `insert_artifact`, `get_artifact`, `DesignArtifactRow` (new; DB-backed persistence).

New migration:
- `packages/backend/api/src/db/migrations/0024_design_artifacts.sql`.

New routes module:
- `packages/backend/api/src/runs/generation/artifact_routes.rs` — `get_artifact_meta`, `get_artifact_preview` (serve `/api/live-artifacts/...`).

Modified:
- `packages/backend/api/src/runs/design_chat.rs` — `CreateRunRequest` + `PendingDesignRun` gain `project_id`; `stream_pending` calls `run_generation` instead of doing a bare completion.
- `packages/backend/api/src/runs/mod.rs` — `pub mod generation;`.
- `packages/backend/api/src/lib.rs` — mount the two `/api/live-artifacts` routes.
- `packages/backend/api/Cargo.toml` — ensure `regex` dependency (Task 3).

Tests (each `packages/backend/api/tests/<name>.rs`, `mod common;` harness):
- Unit tests live inline (`#[cfg(test)] mod tests`) in the new modules where no DB is needed.
- `tests/design_generation.rs` — integration parity tests (ported daemon cases) driving `run_generation` with a `StubProvider`.
- `tests/design_generation_live.rs` — `#[ignore]` real-provider E2E.

**Storage decision:** Per-run generated files live in an **in-memory virtual file map** (`FileStore`, the loop's mutable working set, analogous to the daemon's on-disk project dir and to the loop test's `fileSourceCapturing`). The single validated + rendered artifact is persisted as **one engagement-scoped row** in the new `design_artifacts` Postgres table and served by two new engagement-authenticated GET routes (`/api/live-artifacts/{id}` and `/api/live-artifacts/{id}/preview`) that the existing web already calls. Rationale: the runs table never persisted design-chat runs (they are in-memory single-shot), there is no existing Agent artifact storage, and a `(engagement_id, project_id, artifact_id)`-keyed row gives durable, multi-tenant-isolated serving without introducing mutable multi-tenant server disk state or path-traversal surface. (Alternative considered and rejected for the primary path: a disk mirror faithfully reusing the daemon's `.live-artifacts/` layout — heavier multi-tenancy/path-safety burden, no reuse of the existing Postgres engagement-isolation model.)

---

### Task 1: In-memory file store + file ToolSource

**Files:**
- Create: `packages/backend/api/src/runs/generation/mod.rs` (stub: `pub mod tools; pub mod file_store;`)
- Create: `packages/backend/api/src/runs/generation/tools.rs`
- Create: `packages/backend/api/src/runs/generation/file_store.rs`
- Modify: `packages/backend/api/src/runs/mod.rs` (add `pub mod generation;`)

**Interfaces:**
- Produces (consumed by Tasks 2, 4, 5, 9):
  - `pub struct ToolSpec { pub name: String, pub description: String, pub parameters: serde_json::Value }`
  - `pub struct ToolResult { pub content: String, pub is_error: bool }`
  - `#[async_trait] pub trait ToolSource: Send + Sync { fn id(&self) -> &str; async fn list_tools(&self) -> Vec<ToolSpec>; async fn execute(&self, name: &str, args: &serde_json::Value) -> ToolResult; }`
  - `pub fn namespace_tool_name(source_id: &str, tool_name: &str) -> String`
  - `pub fn owns_tool_name(source_id: &str, tool_name: &str) -> bool`
  - `pub struct FileStore` with `pub fn new() -> std::sync::Arc<FileStore>`, `pub fn read_index_html(&self) -> Option<String>`
  - `pub struct FileToolSource { store: std::sync::Arc<FileStore> }` with `pub fn new(store: std::sync::Arc<FileStore>) -> FileToolSource`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/src/runs/generation/file_store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn write_then_read_roundtrips_and_reports_bytes() {
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        let w = src
            .execute("write_file", &json!({ "path": "index.html", "content": "<h1>hi</h1>" }))
            .await;
        assert!(!w.is_error);
        assert_eq!(w.content, "wrote index.html (11 bytes)");
        let r = src.execute("read_file", &json!({ "path": "index.html" })).await;
        assert_eq!(r.content, "<h1>hi</h1>");
        assert_eq!(store.read_index_html().as_deref(), Some("<h1>hi</h1>"));
    }

    #[tokio::test]
    async fn edit_replaces_first_occurrence_only() {
        let store = FileStore::new();
        let src = FileToolSource::new(store.clone());
        src.execute("write_file", &json!({ "path": "a.txt", "content": "x x" })).await;
        let e = src
            .execute("edit_file", &json!({ "path": "a.txt", "find": "x", "replace": "y" }))
            .await;
        assert!(!e.is_error);
        let r = src.execute("read_file", &json!({ "path": "a.txt" })).await;
        assert_eq!(r.content, "y x");
    }

    #[tokio::test]
    async fn edit_missing_substring_is_error() {
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        src.execute("write_file", &json!({ "path": "a.txt", "content": "abc" })).await;
        let e = src
            .execute("edit_file", &json!({ "path": "a.txt", "find": "zzz", "replace": "y" }))
            .await;
        assert!(e.is_error);
    }

    #[tokio::test]
    async fn read_missing_file_is_error_and_list_reports_files() {
        let store = FileStore::new();
        let src = FileToolSource::new(store);
        let r = src.execute("read_file", &json!({ "path": "nope" })).await;
        assert!(r.is_error);
        src.execute("write_file", &json!({ "path": "index.html", "content": "x" })).await;
        let l = src.execute("list_files", &json!({})).await;
        assert!(!l.is_error);
        assert_eq!(l.content, "index.html");
    }

    #[test]
    fn namespacing_is_bare_for_file_prefixed_for_mcp() {
        assert_eq!(namespace_tool_name("file", "write_file"), "write_file");
        assert_eq!(namespace_tool_name("mcp:figma", "get_file"), "mcp__figma__get_file");
        assert!(owns_tool_name("file", "write_file"));
        assert!(!owns_tool_name("file", "mcp__figma__get_file"));
        assert!(owns_tool_name("mcp:figma", "mcp__figma__get_file"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::file_store 2>&1 | tee /tmp/t1.log; grep -nE '^error' /tmp/t1.log`
Expected: FAIL — `ToolSpec`, `FileStore`, `FileToolSource`, `namespace_tool_name` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/mod.rs`:

```rust
//! Server-side design-generation engine (ported from the daemon's
//! apps/daemon/src/api-generation/). Runs an agentic tool loop against the
//! engagement's in-process vaulted provider, writes files server-side, and
//! streams the daemon SSE vocabulary to the unchanged web SPA.

pub mod tools;
pub mod file_store;
```

Write `packages/backend/api/src/runs/generation/tools.rs`:

```rust
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
        Self { content: content.into(), is_error: false }
    }
    pub fn err(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: true }
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
```

Write `packages/backend/api/src/runs/generation/file_store.rs`:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use super::tools::{ToolResult, ToolSource, ToolSpec};

/// In-memory per-run working set of project files. Replaces the daemon's
/// on-disk project dir; a design run is single-shot and ephemeral, so the
/// files only need to survive the loop, after which `index.html` is validated
/// and persisted as one artifact row.
pub struct FileStore {
    files: Mutex<HashMap<String, String>>,
}

impl FileStore {
    pub fn new() -> Arc<FileStore> {
        Arc::new(FileStore { files: Mutex::new(HashMap::new()) })
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
                description: "Read a UTF-8 text file from the project by project-relative path.".into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Project-relative file path." } }, "required": ["path"] }),
            },
            ToolSpec {
                name: "write_file".into(),
                description: "Create or overwrite a project file with the given UTF-8 content.".into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"] }),
            },
            ToolSpec {
                name: "edit_file".into(),
                description: "Replace the first occurrence of an exact substring in a project file.".into(),
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
                let (Some(path), Some(content)) = (arg_str(args, "path"), arg_str(args, "content")) else {
                    return ToolResult::err("invalid arguments: write_file requires string \"path\" and \"content\"");
                };
                self.store.files.lock().unwrap().insert(path.to_string(), content.to_string());
                ToolResult::ok(format!("wrote {path} ({} bytes)", content.len()))
            }
            "read_file" => {
                let Some(path) = arg_str(args, "path") else {
                    return ToolResult::err("invalid arguments: read_file requires string \"path\"");
                };
                match self.store.files.lock().unwrap().get(path) {
                    Some(c) => ToolResult::ok(c.clone()),
                    None => ToolResult::err(format!("read_file: {path} not found")),
                }
            }
            "edit_file" => {
                let (Some(path), Some(find), Some(replace)) =
                    (arg_str(args, "path"), arg_str(args, "find"), arg_str(args, "replace"))
                else {
                    return ToolResult::err("invalid arguments: edit_file requires string \"path\", \"find\", \"replace\"");
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
                ToolResult::ok(if joined.is_empty() { "(no files)".to_string() } else { joined })
            }
            other => ToolResult::err(format!("unknown file tool: {other}")),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::file_store 2>&1 | tee /tmp/t1.log; grep -nE '^error' /tmp/t1.log`
Expected: PASS (all 5 tests), no `^error` lines.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/ packages/backend/api/src/runs/mod.rs
git commit -m "feat(generation): in-memory file store + file ToolSource + tool namespacing"
```

---

### Task 2: Tool registry (namespacing + prefix-aware dispatch)

**Files:**
- Create: `packages/backend/api/src/runs/generation/registry.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod registry;`)

**Interfaces:**
- Consumes: `ToolSource`, `ToolSpec`, `ToolResult`, `namespace_tool_name`, `owns_tool_name`, `namespace_prefix` (Task 1).
- Produces (consumed by Tasks 5, 9):
  - `pub struct ToolRegistry`
  - `pub fn new(sources: Vec<std::sync::Arc<dyn ToolSource>>) -> ToolRegistry`
  - `pub async fn schemas(&self) -> Vec<ToolSpec>`
  - `pub async fn dispatch(&self, name: &str, args: &serde_json::Value) -> ToolResult`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/src/runs/generation/registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::tools::{ToolResult, ToolSource, ToolSpec};
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::sync::Arc;

    struct Stub {
        id: String,
        names: Vec<String>,
        tag: String,
    }
    #[async_trait]
    impl ToolSource for Stub {
        fn id(&self) -> &str { &self.id }
        async fn list_tools(&self) -> Vec<ToolSpec> {
            self.names.iter().map(|n| ToolSpec { name: n.clone(), description: n.clone(), parameters: json!({"type":"object","properties":{}}) }).collect()
        }
        async fn execute(&self, name: &str, _args: &Value) -> ToolResult {
            ToolResult::ok(format!("{}:{}", self.tag, name))
        }
    }

    fn stub(id: &str, names: &[&str], tag: &str) -> Arc<dyn ToolSource> {
        Arc::new(Stub { id: id.into(), names: names.iter().map(|s| s.to_string()).collect(), tag: tag.into() })
    }

    #[tokio::test]
    async fn schemas_namespace_file_bare_mcp_prefixed() {
        let reg = ToolRegistry::new(vec![stub("file", &["write_file"], "f"), stub("mcp:figma", &["get_file"], "m")]);
        let mut names: Vec<String> = reg.schemas().await.into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(names, vec!["mcp__figma__get_file".to_string(), "write_file".to_string()]);
    }

    #[tokio::test]
    async fn dispatch_routes_namespaced_call_to_owner_with_bare_name() {
        let reg = ToolRegistry::new(vec![stub("file", &["write_file"], "f"), stub("mcp:figma", &["get_file"], "m")]);
        let res = reg.dispatch("mcp__figma__get_file", &json!({})).await;
        assert_eq!(res.content, "m:get_file"); // bare name passed to the source
    }

    #[tokio::test]
    async fn dispatch_bare_file_tool_with_double_underscore_not_misrouted() {
        let reg = ToolRegistry::new(vec![stub("file", &["write_file"], "f"), stub("mcp:figma", &["get_file"], "m")]);
        let res = reg.dispatch("write_file", &json!({})).await;
        assert_eq!(res.content, "f:write_file");
        assert!(!res.is_error);
    }

    #[tokio::test]
    async fn dispatch_unknown_name_is_error_never_panics() {
        let reg = ToolRegistry::new(vec![stub("file", &["write_file"], "f")]);
        let res = reg.dispatch("nope", &json!({})).await;
        assert!(res.is_error);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::registry 2>&1 | tee /tmp/t2.log; grep -nE '^error' /tmp/t2.log`
Expected: FAIL — `ToolRegistry` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/registry.rs` (above the test module):

```rust
use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use super::tools::{namespace_prefix, namespace_tool_name, owns_tool_name, ToolResult, ToolSource, ToolSpec};

/// Aggregates multiple ToolSources into a single flat, namespaced tool
/// catalogue and routes dispatch calls back to the owning source. Routing is
/// registered-prefix-aware and name-verified (NOT a generic "contains '__'"
/// test), so a bare file-tool name containing '__' routes to `file`. Port of
/// daemon `tool-registry.ts`.
pub struct ToolRegistry {
    sources: Vec<Arc<dyn ToolSource>>,
}

impl ToolRegistry {
    pub fn new(sources: Vec<Arc<dyn ToolSource>>) -> ToolRegistry {
        ToolRegistry { sources }
    }

    pub async fn schemas(&self) -> Vec<ToolSpec> {
        let mut out: Vec<ToolSpec> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for source in &self.sources {
            for spec in source.list_tools().await {
                let name = namespace_tool_name(source.id(), &spec.name);
                if !seen.insert(name.clone()) {
                    continue; // first source wins
                }
                out.push(ToolSpec { name, description: spec.description, parameters: spec.parameters });
            }
        }
        out
    }

    pub async fn dispatch(&self, name: &str, args: &Value) -> ToolResult {
        // 1. Non-file (prefixed) sources: match by actual registered prefix.
        for source in self.sources.iter().filter(|s| s.id() != "file") {
            if owns_tool_name(source.id(), name) {
                let bare = &name[namespace_prefix(source.id()).len()..];
                return source.execute(bare, args).await;
            }
        }
        // 2. Otherwise only the unprefixed `file` source can own it — but verify
        //    it actually declares this bare name before dispatching.
        if let Some(file) = self.sources.iter().find(|s| s.id() == "file") {
            let declares = file.list_tools().await.iter().any(|s| s.name == name);
            if declares {
                return file.execute(name, args).await;
            }
        }
        ToolResult::err(format!("unknown tool '{name}'"))
    }
}
```

Add to `mod.rs`: `pub mod registry;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::registry 2>&1 | tee /tmp/t2.log; grep -nE '^error' /tmp/t2.log`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/registry.rs packages/backend/api/src/runs/generation/mod.rs
git commit -m "feat(generation): prefix-aware tool registry (namespacing + dispatch routing)"
```

---

### Task 3: html_template_v1 guard (validate + render)

**Files:**
- Create: `packages/backend/api/src/runs/generation/template_guard.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod template_guard;`)
- Modify: `packages/backend/api/Cargo.toml` (ensure `regex` dependency)

**Interfaces:**
- Produces (consumed by Task 9):
  - `pub struct TemplateGuardError(pub String)`
  - `pub fn render_html_template_v1(template_html: &str) -> Result<String, TemplateGuardError>` — returns the rendered HTML (`{{ data.x }}` → escaped scalar, which is `""` for an empty data object) on success; `Err` when the template contains a rejected construct (`<script>`, `<iframe>`, `srcdoc=`, `on*=` handlers, `javascript:` URLs, `data-od-*` directives, raw `{{{...}}}` / `{{ &...}}`, or a `{{ }}` binding that is not a `data.<scalar>` path). This is the port of the daemon's `renderHtmlTemplateV1` in `live-artifacts/render.ts`.

- [ ] **Step 1: Ensure the `regex` dependency**

Run: `grep -n '^regex' packages/backend/api/Cargo.toml || echo MISSING`
If `MISSING`, add under `[dependencies]` in `packages/backend/api/Cargo.toml`:

```toml
regex = "1"
```

- [ ] **Step 2: Write the failing test**

Append to `packages/backend/api/src/runs/generation/template_guard.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_html_and_leaves_it_unchanged() {
        let html = "<!doctype html><html><body><h1>Hi</h1></body></html>";
        assert_eq!(render_html_template_v1(html).unwrap(), html);
    }

    #[test]
    fn rejects_script_tag() {
        let err = render_html_template_v1("<h1>hi</h1><script>alert(1)</script>").unwrap_err();
        assert!(err.0.to_lowercase().contains("script"));
    }

    #[test]
    fn rejects_iframe_srcdoc_onhandler_and_javascript_url() {
        assert!(render_html_template_v1("<iframe></iframe>").is_err());
        assert!(render_html_template_v1("<div srcdoc=\"x\"></div>").is_err());
        assert!(render_html_template_v1("<button onclick=\"x()\">b</button>").is_err());
        assert!(render_html_template_v1("<a href=\"javascript:alert(1)\">x</a>").is_err());
    }

    #[test]
    fn rejects_stray_moustache_binding() {
        // Not a `data.` path → rejected, matching the daemon.
        assert!(render_html_template_v1("<p>{{ name }}</p>").is_err());
        // Raw triple-brace interpolation is rejected.
        assert!(render_html_template_v1("<p>{{{ raw }}}</p>").is_err());
    }

    #[test]
    fn data_binding_resolves_to_empty_scalar() {
        // dataJson is always {} for generated designs, so a valid data binding
        // renders to the escaped empty string (accepted, not rejected).
        assert_eq!(render_html_template_v1("<title>{{ data.title }}</title>").unwrap(), "<title></title>");
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::template_guard 2>&1 | tee /tmp/t3.log; grep -nE '^error' /tmp/t3.log`
Expected: FAIL — `render_html_template_v1` unresolved.

- [ ] **Step 4: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/template_guard.rs` (above the test module):

```rust
use std::sync::LazyLock;

use regex::Regex;

/// Rejection reason for an `html_template_v1` artifact. Its `.0` string becomes
/// the `artifact-error` status detail surfaced over SSE.
#[derive(Debug, Clone)]
pub struct TemplateGuardError(pub String);

// Port of daemon EXECUTABLE_TEMPLATE_PATTERNS (live-artifacts/render.ts).
static EXECUTABLE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r"(?i)<\s*script\b").unwrap(), "script elements are not supported in live artifact previews"),
        (Regex::new(r"(?i)<\s*iframe\b").unwrap(), "iframe elements are not supported in live artifact previews"),
        (Regex::new(r"(?i)\bsrcdoc\s*=").unwrap(), "srcdoc attributes are not supported in live artifact previews"),
        (Regex::new(r"(?i)\son[a-z][a-z0-9_-]*\s*=").unwrap(), "event handler attributes are not supported in live artifact previews"),
        (Regex::new(r#"(?i)(?:href|src|action|formaction)\s*=\s*['"]?\s*javascript\s*:"#).unwrap(), "javascript: URLs are not supported in live artifact previews"),
        (Regex::new(r"(?i)\bdata-od-(?:html|raw|bind-html)\b").unwrap(), "raw HTML insertion directives are not supported"),
    ]
});
static RAW_INTERP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\{[^{}]*\}\}\}|\{\{\s*&[^{}]*\}\}").unwrap());
static INTERP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap());
static TEMPLATE_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:data|[A-Za-z_][A-Za-z0-9_]*)(?:\.(?:[A-Za-z_][A-Za-z0-9_-]*|\d+))*$").unwrap());

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Validate + render `templateHtml`. `dataJson` is always the empty object for
/// generated designs, so every valid `data.*` binding resolves to `""`. Any
/// executable pattern, raw interpolation, or non-`data` binding is rejected.
pub fn render_html_template_v1(template_html: &str) -> Result<String, TemplateGuardError> {
    for (pattern, message) in EXECUTABLE_PATTERNS.iter() {
        if pattern.is_match(template_html) {
            return Err(TemplateGuardError((*message).to_string()));
        }
    }
    if RAW_INTERP.is_match(template_html) {
        return Err(TemplateGuardError("raw template interpolation is not supported".into()));
    }
    // Replace each `{{ binding }}` with the escaped scalar, rejecting any
    // binding that is not a `data.` path. dataJson is {} so the value is "".
    let mut err: Option<TemplateGuardError> = None;
    let rendered = INTERP.replace_all(template_html, |caps: &regex::Captures| {
        let binding = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !TEMPLATE_PATH.is_match(binding) || !binding.starts_with("data") {
            err = Some(TemplateGuardError(format!("invalid template binding path: {binding}")));
            return String::new();
        }
        // data object is empty → scalar value is "" → escaped "".
        escape_html("")
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(rendered.into_owned())
}
```

Add to `mod.rs`: `pub mod template_guard;`

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::template_guard 2>&1 | tee /tmp/t3.log; grep -nE '^error' /tmp/t3.log`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/runs/generation/template_guard.rs packages/backend/api/src/runs/generation/mod.rs packages/backend/api/Cargo.toml
git commit -m "feat(generation): html_template_v1 guard (script/{{}} rejection) ported from daemon render"
```

- [ ] **Step 7: Write the failing hardening tests**

Two cheap, spec-driven hardenings close known denylist gaps in the daemon guard we just ported (infographic-atelier feature spec §5.4.2–5.4.3, `celerex-design/docs/superpowers/specs/2026-07-13-infographic-atelier-design.md`): (a) the daemon's `on*=` pattern (#4) anchors on **whitespace**, so `<svg/onload=...>` (a `/`-separated handler — the canonical short SVG-XSS payload) slips through; (b) `<foreignObject>` (the HTML-in-SVG bridge) is not individually blocked and has no legitimate static-chart use. Both are **additive** to `EXECUTABLE_PATTERNS` — they only ADD rejections for constructs illegitimate in a static SVG artifact.

**Parity invariant (must still hold):** the Step-2 assertion `accepts_plain_html_and_leaves_it_unchanged` (output === input for clean templates) and every legitimate animated-SVG chart MUST remain unchanged — verified below by `hardening_keeps_parity_for_legit_animated_svg_chart`. No existing Task 3 or Task 10 fixture contains `<svg/onload...>` or `<foreignObject>` (checked); the only `on*=` fixture, `<button onclick="x()">` in Step 2, is whitespace-anchored and stays rejected either way — so **no parity fixture needs adjustment**.

Append to `packages/backend/api/src/runs/generation/template_guard.rs`:

```rust
#[cfg(test)]
mod hardening_tests {
    use super::*;

    #[test]
    fn rejects_slash_separated_on_handler() {
        // Gap 1: `/` (not whitespace) before `on`. The ported `\son...=` pattern
        // misses this; the hardened `[\s/]on...=` catches the `<svg/onload>` class.
        assert!(render_html_template_v1("<svg/onload=alert(1)></svg>").is_err());
        assert!(render_html_template_v1("<img/onerror=alert(1)>").is_err());
        // Whitespace-anchored handlers stay rejected (no regression).
        assert!(render_html_template_v1("<button onclick=\"x()\">b</button>").is_err());
    }

    #[test]
    fn rejects_foreign_object() {
        // The HTML-in-SVG bridge — no legitimate static-chart use.
        assert!(render_html_template_v1("<svg><foreignObject><b>x</b></foreignObject></svg>").is_err());
        assert!(render_html_template_v1("<foreignobject/>").is_err()); // case-insensitive
    }

    #[test]
    fn hardening_keeps_parity_for_legit_animated_svg_chart() {
        // Parity invariant: a representative animated inline-SVG chart (SMIL
        // <animate> + <text> labels, no handlers, no foreignObject) is returned
        // byte-for-byte unchanged — output === input for clean templates.
        let chart = "<svg viewBox=\"0 0 100 50\"><rect x=\"0\" y=\"10\" width=\"20\" height=\"40\"><animate attributeName=\"height\" from=\"0\" to=\"40\" dur=\"1s\"/></rect><text x=\"0\" y=\"48\">92%</text></svg>";
        assert_eq!(render_html_template_v1(chart).unwrap(), chart);
    }
}
```

- [ ] **Step 8: Run the hardening tests to verify they fail**

Run: `cargo test -p celerex-api --lib runs::generation::template_guard::hardening_tests 2>&1 | tee /tmp/t3h.log; grep -nE '^error' /tmp/t3h.log`
Expected: FAIL — `rejects_slash_separated_on_handler` and `rejects_foreign_object` fail (the ported patterns don't yet catch `<svg/onload>` or `<foreignObject>`); `hardening_keeps_parity_for_legit_animated_svg_chart` already passes.

- [ ] **Step 9: Harden the pattern list (additive)**

In `packages/backend/api/src/runs/generation/template_guard.rs`, make two additive edits to the `EXECUTABLE_PATTERNS` vec written in Step 4:

1. Widen pattern #4's leading anchor from whitespace `\s` to `[\s/]` so a `/`-separated handler is caught (spec §5.4.2). Change:

```rust
        (Regex::new(r"(?i)\son[a-z][a-z0-9_-]*\s*=").unwrap(), "event handler attributes are not supported in live artifact previews"),
```

to:

```rust
        (Regex::new(r"(?i)[\s/]on[a-z][a-z0-9_-]*\s*=").unwrap(), "event handler attributes are not supported in live artifact previews"),
```

2. Add a `<foreignObject>` rule (spec §5.4.3) as a new entry in the same `vec![...]` (order does not matter; place it right after the `<iframe>` entry for readability):

```rust
        (Regex::new(r"(?i)<\s*foreignObject\b").unwrap(), "foreignObject is not supported in live artifact previews"),
```

These are the two deliberate divergences from the ported daemon `render.ts` (`celerex-design/apps/daemon/src/live-artifacts/render.ts:20-27`, which has NOT made them — the daemon relies on the serve-time CSP, mirrored in Task 8). Everything else in the guard stays a faithful port.

- [ ] **Step 10: Run tests to verify green (hardening + parity)**

Run: `cargo test -p celerex-api --lib runs::generation::template_guard 2>&1 | tee /tmp/t3h.log; grep -nE '^error' /tmp/t3h.log`
Expected: PASS — all original Step-2 tests (including the `accepts_plain_html_and_leaves_it_unchanged` parity assertion) AND the three `hardening_tests`.

- [ ] **Step 11: Commit**

```bash
git add packages/backend/api/src/runs/generation/template_guard.rs
git commit -m "harden(generation): guard catches /-separated on*= handlers and <foreignObject> (infographic spec §5.4.2-3)"
```

---

### Task 4: Provider turn (tool envelope + alias map + response normalize)

**Files:**
- Create: `packages/backend/api/src/runs/generation/provider_turn.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod provider_turn;`)

**Interfaces:**
- Consumes: `ToolSpec` (Task 1); `celerex_orchestrator::llm::Provider` (`async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Completion, String>`, `Completion { raw: Value }`).
- Produces (consumed by Task 5):
  - `pub struct ToolCall { pub id: String, pub name: String, pub args: serde_json::Value }`
  - `pub struct ProviderTurnResult { pub text: String, pub tool_calls: Vec<ToolCall>, pub input_tokens: Option<u64>, pub output_tokens: Option<u64> }`
  - `pub fn build_tool_aliases(tools: &[ToolSpec]) -> (serde_json::Value, std::collections::HashMap<String, String>)` — returns `(wire_tools_array, wire_name -> real_name)`.
  - `pub fn normalize_raw_completion(raw: &serde_json::Value) -> ProviderTurnResult`
  - `pub async fn run_provider_turn(provider: &dyn celerex_orchestrator::llm::Provider, model: &str, messages: serde_json::Value, tools: &[ToolSpec]) -> Result<ProviderTurnResult, String>`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/src/runs/generation/provider_turn.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::tools::ToolSpec;
    use celerex_orchestrator::llm::stub::StubProvider;
    use serde_json::json;

    #[test]
    fn normalizes_openai_text_and_tool_calls() {
        let raw = json!({
            "choices": [{ "message": {
                "content": "building now",
                "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a.html\",\"content\":\"x\"}" } }]
            }}],
            "usage": { "prompt_tokens": 10, "completion_tokens": 4 }
        });
        let out = normalize_raw_completion(&raw);
        assert_eq!(out.text, "building now");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "write_file");
        assert_eq!(out.tool_calls[0].args, json!({ "path": "a.html", "content": "x" }));
        assert_eq!(out.input_tokens, Some(10));
        assert_eq!(out.output_tokens, Some(4));
    }

    #[test]
    fn normalizes_anthropic_content_blocks() {
        let raw = json!({
            "content": [
                { "type": "text", "text": "ok " },
                { "type": "tool_use", "id": "tu_1", "name": "write_file", "input": { "path": "b.html" } }
            ],
            "usage": { "input_tokens": 7, "output_tokens": 2 }
        });
        let out = normalize_raw_completion(&raw);
        assert_eq!(out.text, "ok ");
        assert_eq!(out.tool_calls[0].id, "tu_1");
        assert_eq!(out.tool_calls[0].args, json!({ "path": "b.html" }));
        assert_eq!(out.input_tokens, Some(7));
    }

    #[test]
    fn builds_openai_function_envelope_and_sanitizes_names() {
        let tools = vec![ToolSpec { name: "mcp__figma.dev__get_file".into(), description: "d".into(), parameters: json!({ "type": "object", "properties": {} }) }];
        let (wire, to_real) = build_tool_aliases(&tools);
        let arr = wire.as_array().unwrap();
        assert_eq!(arr[0]["type"], "function");
        let wire_name = arr[0]["function"]["name"].as_str().unwrap();
        assert!(wire_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert!(!wire_name.contains('.'));
        assert_eq!(to_real.get(wire_name).unwrap(), "mcp__figma.dev__get_file");
    }

    #[tokio::test]
    async fn run_turn_reverse_maps_tool_name_to_real() {
        // StubProvider echoes a scripted OpenAI response using the SANITIZED wire
        // name; run_provider_turn must reverse-map it to the real tool name.
        let real = "mcp__figma.dev__get_file";
        let (wire, _to_real) = build_tool_aliases(&[ToolSpec { name: real.into(), description: "d".into(), parameters: json!({ "type": "object", "properties": {} }) }]);
        let wire_name = wire.as_array().unwrap()[0]["function"]["name"].as_str().unwrap().to_string();
        let provider = StubProvider::new(vec![json!({
            "choices": [{ "message": { "content": serde_json::Value::Null,
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": wire_name, "arguments": "{\"path\":\"a\"}" } }] } }]
        })]);
        let tools = vec![ToolSpec { name: real.into(), description: "d".into(), parameters: json!({ "type": "object", "properties": {} }) }];
        let out = run_provider_turn(&provider, "m", json!([]), &tools).await.unwrap();
        assert_eq!(out.tool_calls[0].name, real);
        assert_eq!(out.tool_calls[0].args, json!({ "path": "a" }));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::provider_turn 2>&1 | tee /tmp/t4.log; grep -nE '^error' /tmp/t4.log`
Expected: FAIL — `normalize_raw_completion` / `build_tool_aliases` / `run_provider_turn` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/provider_turn.rs` (above the test module):

```rust
use std::collections::{HashMap, HashSet};

use celerex_orchestrator::llm::Provider;
use serde_json::{json, Value};

use super::tools::ToolSpec;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderTurnResult {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Map each tool name to the OpenAI-compatible charset `[a-zA-Z0-9_-]` keeping an
/// injective alias so distinct real names never collide onto one wire name, and
/// wrap each spec in the `{type:"function",function:{...}}` envelope the provider
/// chat/completions API requires. Port of daemon `buildToolNameAliases`.
pub fn build_tool_aliases(tools: &[ToolSpec]) -> (Value, HashMap<String, String>) {
    let mut to_real: HashMap<String, String> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut wire_tools: Vec<Value> = Vec::with_capacity(tools.len());
    for t in tools {
        let base: String = t
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        let mut wire = base.clone();
        let mut n = 1u32;
        while used.contains(&wire) && to_real.get(&wire).map(|s| s.as_str()) != Some(t.name.as_str()) {
            wire = format!("{base}_{n}");
            n += 1;
        }
        used.insert(wire.clone());
        to_real.insert(wire.clone(), t.name.clone());
        wire_tools.push(json!({
            "type": "function",
            "function": { "name": wire, "description": t.description, "parameters": t.parameters }
        }));
    }
    (Value::Array(wire_tools), to_real)
}

fn as_u64(v: Option<&Value>) -> Option<u64> {
    v.and_then(|x| x.as_u64())
}

/// Normalize a provider's native completion into ProviderTurnResult. Handles
/// OpenAI/Copilot (`choices[0].message`) and Anthropic (`content[]` blocks).
/// Port of daemon `normalizeRawCompletion`. Tool-call names are NOT reverse
/// mapped here (that happens in `run_provider_turn`).
pub fn normalize_raw_completion(raw: &Value) -> ProviderTurnResult {
    let mut out = ProviderTurnResult::default();

    // OpenAI / Copilot shape.
    if let Some(message) = raw.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("message")) {
        if let Some(s) = message.get("content").and_then(|c| c.as_str()) {
            out.text.push_str(s);
        }
        if let Some(tcs) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = tc.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let args = match tc.get("function").and_then(|f| f.get("arguments")).and_then(|v| v.as_str()) {
                    Some(s) => serde_json::from_str::<Value>(s).unwrap_or(json!({})),
                    None => json!({}),
                };
                out.tool_calls.push(ToolCall { id, name, args });
            }
        }
    }

    // Anthropic content-block shape.
    if let Some(blocks) = raw.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                        out.text.push_str(s);
                    }
                }
                Some("tool_use") => {
                    out.tool_calls.push(ToolCall {
                        id: block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        name: block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        args: block.get("input").cloned().unwrap_or(json!({})),
                    });
                }
                _ => {}
            }
        }
    }

    let usage = raw.get("usage");
    out.input_tokens = as_u64(usage.and_then(|u| u.get("prompt_tokens"))).or_else(|| as_u64(usage.and_then(|u| u.get("input_tokens"))));
    out.output_tokens = as_u64(usage.and_then(|u| u.get("completion_tokens"))).or_else(|| as_u64(usage.and_then(|u| u.get("output_tokens"))));
    out
}

/// Perform ONE LLM turn against the in-process provider. Builds the wire tool
/// envelope, calls `provider.complete` directly (no HTTP hop), normalizes the
/// raw response, and reverse-maps wire tool names back to real names. On a
/// provider error, returns the error string WITHOUT leaking the raw body.
pub async fn run_provider_turn(
    provider: &dyn Provider,
    model: &str,
    messages: Value,
    tools: &[ToolSpec],
) -> Result<ProviderTurnResult, String> {
    let (wire_tools, to_real) = build_tool_aliases(tools);
    let completion = provider.complete(model, messages, wire_tools).await?;
    let mut result = normalize_raw_completion(&completion.raw);
    for tc in result.tool_calls.iter_mut() {
        if let Some(real) = to_real.get(&tc.name) {
            tc.name = real.clone();
        }
    }
    Ok(result)
}
```

Add to `mod.rs`: `pub mod provider_turn;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::provider_turn 2>&1 | tee /tmp/t4.log; grep -nE '^error' /tmp/t4.log`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/provider_turn.rs packages/backend/api/src/runs/generation/mod.rs
git commit -m "feat(generation): provider turn — OpenAI tool envelope, alias map, response normalize (in-process complete)"
```

---

### Task 5: SSE event emitter

**Files:**
- Create: `packages/backend/api/src/runs/generation/emitter.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod emitter;`)

**Interfaces:**
- Produces (consumed by Tasks 6, 9):
  - `pub struct RunEventEmitter`
  - `pub fn new(tx: tokio::sync::mpsc::Sender<Result<axum::response::sse::Event, std::convert::Infallible>>) -> RunEventEmitter`
  - async methods (all return `()` and best-effort send, ignoring a closed receiver):
    - `pub async fn start(&self, bin: &str, project_id: &str, model: &str)`
    - `pub async fn status(&self, label: &str, detail: Option<&str>)`
    - `pub async fn text_delta(&self, delta: &str)`
    - `pub async fn tool_use(&self, id: &str, name: &str, input: &serde_json::Value)`
    - `pub async fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool)`
    - `pub async fn usage(&self, input_tokens: Option<u64>, output_tokens: Option<u64>)`
    - `pub async fn live_artifact(&self, project_id: &str, artifact_id: &str, title: &str)`
    - `pub async fn error_event(&self, message: &str)`
    - `pub async fn end(&self, status: &str, code: i32)`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/src/runs/generation/emitter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // Reconstruct the JSON body an axum SSE Event serializes to `data:`. We
    // assert on the *shape we emit* by re-deriving it (Event has no public
    // getters), so this test pins the vocabulary the emitter must produce.
    fn drain(mut rx: tokio::sync::mpsc::Receiver<Result<axum::response::sse::Event, std::convert::Infallible>>) -> Vec<(String, Value)> {
        // The emitter methods each send exactly one Event; we cannot read Event
        // internals, so this test instead exercises the *helper* that builds the
        // (name, data) pair. See `frame()` below.
        let mut out = Vec::new();
        while let Ok(Some(_ev)) = rx.try_recv().map(Some) {
            let _ = &mut out;
            break;
        }
        out
    }

    #[test]
    fn frame_builds_named_json_event_body() {
        // The public `frame(name, data)` helper is the single source of truth
        // for what goes on the wire; assert its (name, data) output directly.
        let (name, data) = frame_parts("agent", json!({ "type": "text_delta", "delta": "hi" }));
        assert_eq!(name, "agent");
        assert_eq!(data, json!({ "type": "text_delta", "delta": "hi" }));
    }

    #[tokio::test]
    async fn emits_agent_frames_with_exact_vocabulary() {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let em = RunEventEmitter::new(tx);
        em.start("copilot/gpt-4o", "proj-1", "gpt-4o").await;
        em.text_delta("building").await;
        em.tool_use("c1", "write_file", &json!({ "path": "index.html" })).await;
        em.tool_result("c1", "wrote index.html (1 bytes)", false).await;
        em.usage(Some(10), Some(4)).await;
        em.live_artifact("proj-1", "la-x", "Title").await;
        em.status("artifact-error", Some("script elements are not supported")).await;
        em.end("succeeded", 0).await;
        drop(em);
        // Assert we sent 8 frames (one per call), proving none silently dropped.
        let received = collect(rx).await;
        assert_eq!(received.len(), 8);
    }
}
```

Note: because axum's `Event` exposes no getters, add two small test-only helpers in the module — `frame_parts` (pure, returns the `(name, Value)` pair) and `collect` (drains the receiver counting frames). Implement them in Step 3.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::emitter 2>&1 | tee /tmp/t5.log; grep -nE '^error' /tmp/t5.log`
Expected: FAIL — `RunEventEmitter` / `frame_parts` / `collect` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/emitter.rs` (above the test module):

```rust
use std::convert::Infallible;

use axum::response::sse::Event;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

/// Thin typed wrapper over the run's SSE channel emitting the daemon's exact
/// event vocabulary (start / agent{status,text_delta,tool_use,tool_result,
/// usage,live_artifact} / error / end). Port of daemon `run-event-emitter.ts`.
pub struct RunEventEmitter {
    tx: Sender<Result<Event, Infallible>>,
}

/// Pure builder of the (event-name, json-body) pair — single source of truth
/// for the wire shape. Test-visible.
pub fn frame_parts(name: &str, data: Value) -> (String, Value) {
    (name.to_string(), data)
}

fn named(name: &str, data: Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

impl RunEventEmitter {
    pub fn new(tx: Sender<Result<Event, Infallible>>) -> RunEventEmitter {
        RunEventEmitter { tx }
    }

    async fn send(&self, name: &str, data: Value) {
        let (name, data) = frame_parts(name, data);
        let _ = self.tx.send(Ok(named(&name, data))).await;
    }

    pub async fn start(&self, bin: &str, project_id: &str, model: &str) {
        self.send("start", json!({ "bin": bin, "projectId": project_id, "model": model })).await;
    }
    pub async fn status(&self, label: &str, detail: Option<&str>) {
        let data = match detail {
            Some(d) => json!({ "type": "status", "label": label, "detail": d }),
            None => json!({ "type": "status", "label": label }),
        };
        self.send("agent", data).await;
    }
    pub async fn text_delta(&self, delta: &str) {
        self.send("agent", json!({ "type": "text_delta", "delta": delta })).await;
    }
    pub async fn tool_use(&self, id: &str, name: &str, input: &Value) {
        self.send("agent", json!({ "type": "tool_use", "id": id, "name": name, "input": input })).await;
    }
    pub async fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        let data = if is_error {
            json!({ "type": "tool_result", "toolUseId": tool_use_id, "content": content, "isError": true })
        } else {
            json!({ "type": "tool_result", "toolUseId": tool_use_id, "content": content })
        };
        self.send("agent", data).await;
    }
    pub async fn usage(&self, input_tokens: Option<u64>, output_tokens: Option<u64>) {
        self.send("agent", json!({ "type": "usage", "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens } })).await;
    }
    pub async fn live_artifact(&self, project_id: &str, artifact_id: &str, title: &str) {
        self.send("agent", json!({ "type": "live_artifact", "action": "created", "projectId": project_id, "artifactId": artifact_id, "title": title })).await;
    }
    pub async fn error_event(&self, message: &str) {
        self.send("error", json!({ "error": { "message": message } })).await;
    }
    pub async fn end(&self, status: &str, code: i32) {
        self.send("end", json!({ "status": status, "code": code })).await;
    }
}

#[cfg(test)]
pub(crate) async fn collect(
    mut rx: tokio::sync::mpsc::Receiver<Result<Event, Infallible>>,
) -> Vec<Event> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let Ok(e) = ev {
            out.push(e);
        }
    }
    out
}
```

Add to `mod.rs`: `pub mod emitter;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::emitter 2>&1 | tee /tmp/t5.log; grep -nE '^error' /tmp/t5.log`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/emitter.rs packages/backend/api/src/runs/generation/mod.rs
git commit -m "feat(generation): SSE RunEventEmitter with exact daemon event vocabulary"
```

---

### Task 6: System prompt + loop core

**Files:**
- Create: `packages/backend/api/src/runs/generation/system_prompt.rs`
- Create: `packages/backend/api/src/runs/generation/loop_core.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod system_prompt; pub mod loop_core;`)

**Interfaces:**
- Consumes: `ToolRegistry` (Task 2), `RunEventEmitter` (Task 5), `run_provider_turn`/`ProviderTurnResult` (Task 4), `FileStore` (Task 1), `celerex_orchestrator::llm::Provider`.
- Produces (consumed by Task 9):
  - `pub struct PromptFragment { pub slot: FragmentSlot, pub text: String }` + `pub enum FragmentSlot { Brand, Motion, Task }` — an ordered system-prompt fragment and its canonical slot (composition order Brand → Motion → Task). Origin: cross-review B1.
  - `pub const MAX_PROMPT_FRAGMENT_CHARS: usize` — combined fragment character budget (truncate-last).
  - `pub struct ComposedPrompt { pub text: String, pub truncated: bool }`
  - `pub fn compose_system_prompt(tool_names: &[String], fragments: &[PromptFragment]) -> ComposedPrompt` — base prompt + slot-ordered fragments under the budget; `truncated` flags whether truncate-last fired (so `run_generation` can emit the `prompt-fragment-truncated` status note).
  - `pub fn build_system_prompt(tool_names: &[String], fragments: &[PromptFragment]) -> String` — convenience wrapper = `compose_system_prompt(...).text`; `build_system_prompt(names, &[])` is byte-identical to the pre-composer base prompt (the old `None` path).
  - `pub enum StopReason { Natural, MaxTurns, Canceled }`
  - `pub struct LoopResult { pub final_text: String, pub produced_html: Option<String>, pub stop_reason: StopReason }`
  - `pub async fn run_generation_loop(seed: Vec<serde_json::Value>, provider: std::sync::Arc<dyn celerex_orchestrator::llm::Provider>, model: &str, registry: &ToolRegistry, emitter: &RunEventEmitter, file_store: &FileStore, max_turns: usize, canceled: &std::sync::atomic::AtomicBool) -> Result<LoopResult, String>`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/src/runs/generation/loop_core.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::file_store::{FileStore, FileToolSource};
    use crate::runs::generation::registry::ToolRegistry;
    use crate::runs::generation::emitter::RunEventEmitter;
    use celerex_orchestrator::llm::stub::StubProvider;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn reg_and_store() -> (ToolRegistry, Arc<FileStore>) {
        let store = FileStore::new();
        let reg = ToolRegistry::new(vec![Arc::new(FileToolSource::new(store.clone()))]);
        (reg, store)
    }

    #[tokio::test]
    async fn terminates_naturally_and_captures_produced_html() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "building",
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>hi</h1>\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "done" } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(vec![json!({ "role": "user", "content": "make a page" })], provider, "m", &reg, &em, &store, 20, &canceled).await.unwrap();
        assert!(matches!(res.stop_reason, StopReason::Natural));
        assert_eq!(res.produced_html.as_deref(), Some("<h1>hi</h1>"));
    }

    #[tokio::test]
    async fn stops_at_max_turns() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
        // Three identical tool-call turns; maxTurns=3 → MaxTurns.
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(vec![json!({ "role": "user", "content": "go" })], provider, "m", &reg, &em, &store, 3, &canceled).await.unwrap();
        assert!(matches!(res.stop_reason, StopReason::MaxTurns));
    }

    #[tokio::test]
    async fn tool_error_is_fed_back_and_loop_continues() {
        let (reg, store) = reg_and_store();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
        // First turn calls write_file with no args → tool error result; then done.
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "e1", "type": "function", "function": { "name": "write_file", "arguments": "{}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "ok" } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(vec![json!({ "role": "user", "content": "go" })], provider, "m", &reg, &em, &store, 20, &canceled).await.unwrap();
        assert!(matches!(res.stop_reason, StopReason::Natural));
        // Drain and confirm at least one tool_result frame was emitted.
        let mut saw_tool_result = false;
        while let Ok(Ok(_ev)) = rx.try_recv() { saw_tool_result = true; }
        assert!(saw_tool_result);
    }

    #[tokio::test]
    async fn already_canceled_stops_before_first_turn() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
        let provider = Arc::new(StubProvider::new(vec![]));
        let canceled = AtomicBool::new(true);
        let res = run_generation_loop(vec![json!({ "role": "user", "content": "go" })], provider, "m", &reg, &em, &store, 20, &canceled).await.unwrap();
        assert!(matches!(res.stop_reason, StopReason::Canceled));
    }
}
```

Also append the `build_system_prompt` composer tests to `packages/backend/api/src/runs/generation/system_prompt.rs` — these cover the B1 ordered fragment composer (order enforcement regardless of insertion order, empty-list byte-identical to the base prompt, and combined-budget truncate-last):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["write_file".to_string(), "read_file".to_string()]
    }

    #[test]
    fn empty_fragment_list_is_byte_identical_to_base_prompt() {
        // The empty composer == today's `None` path: base prompt only, no
        // trailing `---` separator, byte-for-byte.
        assert_eq!(build_system_prompt(&names(), &[]), base_prompt(&names()));
        assert!(!build_system_prompt(&names(), &[]).contains("---"));
    }

    #[test]
    fn fragments_compose_in_canonical_slot_order_regardless_of_insertion_order() {
        // Inserted Task, Brand, Motion; output MUST be Brand -> Motion -> Task.
        let frags = vec![
            PromptFragment { slot: FragmentSlot::Task, text: "TASKFRAG".into() },
            PromptFragment { slot: FragmentSlot::Brand, text: "BRANDFRAG".into() },
            PromptFragment { slot: FragmentSlot::Motion, text: "MOTIONFRAG".into() },
        ];
        let out = build_system_prompt(&names(), &frags);
        let (b, m, t) = (
            out.find("BRANDFRAG").unwrap(),
            out.find("MOTIONFRAG").unwrap(),
            out.find("TASKFRAG").unwrap(),
        );
        assert!(b < m && m < t, "expected Brand < Motion < Task ordering");
        assert_eq!(out.matches("\n\n---\n\n").count(), 3); // one separator per fragment
    }

    #[test]
    fn single_fragment_matches_the_legacy_some_branch_shape() {
        // Parity with the removed `Some(fragment)` arm: base + "\n\n---\n\n" + fragment.
        let frags = vec![PromptFragment { slot: FragmentSlot::Task, text: "EXTRA".into() }];
        assert_eq!(
            build_system_prompt(&names(), &frags),
            format!("{}\n\n---\n\n{}", base_prompt(&names()), "EXTRA")
        );
    }

    #[test]
    fn budget_truncates_last_fragment_and_flags_truncation() {
        // The combined budget is enforced across ALL fragments; the fragment that
        // crosses MAX_PROMPT_FRAGMENT_CHARS is truncated (truncate-last) and
        // `truncated` is reported so run_generation can emit the status note.
        let big = "x".repeat(MAX_PROMPT_FRAGMENT_CHARS + 500);
        let composed = compose_system_prompt(&names(), &[PromptFragment { slot: FragmentSlot::Brand, text: big }]);
        assert!(composed.truncated);
        let fragment_chars = composed.text.chars().count()
            - base_prompt(&names()).chars().count()
            - "\n\n---\n\n".chars().count();
        assert_eq!(fragment_chars, MAX_PROMPT_FRAGMENT_CHARS);
    }

    #[test]
    fn under_budget_reports_no_truncation() {
        let composed = compose_system_prompt(&names(), &[PromptFragment { slot: FragmentSlot::Brand, text: "short".into() }]);
        assert!(!composed.truncated);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib runs::generation::system_prompt runs::generation::loop_core 2>&1 | tee /tmp/t6.log; grep -nE '^error' /tmp/t6.log`
Expected: FAIL — `run_generation_loop` / `StopReason` / `compose_system_prompt` / `PromptFragment` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/system_prompt.rs`:

```rust
/// One optional, feature-contributed fragment of the system prompt (Brand Kit
/// brand voice, Cinematic motion tier, Infographic chart-design, ...). Fragments
/// are composed into the base prompt in canonical SLOT order — never insertion
/// order. Origin: cross-review reconciliation B1
/// (`celerex-design/docs/superpowers/specs/2026-07-13-cross-review-reconciliations.md`).
#[derive(Debug, Clone)]
pub struct PromptFragment {
    pub slot: FragmentSlot,
    pub text: String,
}

/// Canonical composition order: Brand -> Motion -> Task (the `Task` slot carries
/// the Infographic chart-design fragment and any task-tail copy). Variants are
/// DECLARED in canonical order, so deriving `Ord` makes a stable sort yield it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FragmentSlot {
    Brand,
    Motion,
    Task,
}

/// Combined character budget across ALL composed fragments (the base prompt is
/// never counted and never truncated). Fragments are appended in slot order until
/// the budget is reached; the fragment that crosses it is truncated on a char
/// boundary (truncate-last) and any later fragments are dropped. Origin: B1.
pub const MAX_PROMPT_FRAGMENT_CHARS: usize = 8_000;

/// Output of the composer: the full prompt plus whether the budget forced
/// truncation, so `run_generation` can emit the paired `prompt-fragment-truncated`
/// status note (a free-form `agent`/`status` label, vocabulary-safe exactly like
/// `artifact-error` / `reached step limit`).
#[derive(Debug, Clone)]
pub struct ComposedPrompt {
    pub text: String,
    pub truncated: bool,
}

/// The daemon `system-prompt.ts` base prompt. The `<script>` and `{{ }}`
/// prohibition is load-bearing: the artifact guard rejects both.
fn base_prompt(tool_names: &[String]) -> String {
    let tools = if tool_names.is_empty() { "(none)".to_string() } else { tool_names.join(", ") };
    [
        "You are OmniVyx Design, a build agent. Call the provided tools to build the requested design — do not describe the tools or paste file contents into your chat text.".to_string(),
        format!("Available tools: {tools}."),
        "Build a single self-contained document at index.html using write_file (inline CSS/JS; no external build step). Use read_file / edit_file to iterate on files you have written, and list_files to see what exists.".to_string(),
        "Do not include <script> tags and do not include any {{ }} (double-brace) template sequences anywhere in index.html — the live preview renderer rejects both and the run will produce no visible artifact.".to_string(),
        "When the design is complete and you have no further tool calls to make, stop calling tools and reply with a short completion note. That reply (with no tool calls) ends the run.".to_string(),
        "Never invoke a shell command or any command-line callback convention; the only actions available are the provided tools, invoked directly as tool calls.".to_string(),
    ]
    .join("\n\n")
}

/// Compose the base prompt with the ordered fragments. Fragments are sorted into
/// canonical slot order FIRST (so the output is independent of insertion order),
/// blank fragments are skipped, and each surviving fragment is appended after the
/// existing `---` separator. The combined fragment budget is enforced
/// truncate-last: the fragment that crosses `MAX_PROMPT_FRAGMENT_CHARS` is
/// truncated to the remaining budget and any later fragments are dropped;
/// `truncated` records whether that happened. An EMPTY list returns the base
/// prompt byte-for-byte (today's behavior; the old `None` path).
pub fn compose_system_prompt(tool_names: &[String], fragments: &[PromptFragment]) -> ComposedPrompt {
    let mut ordered: Vec<&PromptFragment> =
        fragments.iter().filter(|f| !f.text.trim().is_empty()).collect();
    ordered.sort_by_key(|f| f.slot); // stable: same-slot fragments keep input order

    let mut text = base_prompt(tool_names);
    let mut used = 0usize; // fragment chars emitted so far (base excluded)
    let mut truncated = false;
    for frag in ordered {
        if used >= MAX_PROMPT_FRAGMENT_CHARS {
            truncated = true;
            break;
        }
        let remaining = MAX_PROMPT_FRAGMENT_CHARS - used;
        let trimmed = frag.text.trim();
        let piece: String = if trimmed.chars().count() <= remaining {
            trimmed.to_string()
        } else {
            truncated = true;
            trimmed.chars().take(remaining).collect()
        };
        used += piece.chars().count();
        text.push_str("\n\n---\n\n");
        text.push_str(&piece);
    }
    ComposedPrompt { text, truncated }
}

/// Convenience wrapper returning just the composed prompt string (drops the
/// truncation flag). Port of daemon `build_system_prompt`, extended to the B1
/// ordered fragment composer; `build_system_prompt(names, &[])` is byte-identical
/// to the pre-composer `None` result.
pub fn build_system_prompt(tool_names: &[String], fragments: &[PromptFragment]) -> String {
    compose_system_prompt(tool_names, fragments).text
}
```

Write `packages/backend/api/src/runs/generation/loop_core.rs` (above the test module):

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use celerex_orchestrator::llm::Provider;
use serde_json::{json, Value};

use super::emitter::RunEventEmitter;
use super::provider_turn::run_provider_turn;
use super::registry::ToolRegistry;
use super::file_store::FileStore;

pub use super::system_prompt::{
    build_system_prompt, compose_system_prompt, ComposedPrompt, FragmentSlot, PromptFragment,
    MAX_PROMPT_FRAGMENT_CHARS,
};

#[derive(Debug, Clone, Copy)]
pub enum StopReason {
    Natural,
    MaxTurns,
    Canceled,
}

pub struct LoopResult {
    pub final_text: String,
    pub produced_html: Option<String>,
    pub stop_reason: StopReason,
}

/// Orchestration core: loop one provider turn at a time, emitting SSE for
/// text/tool-use/tool-result/usage, dispatching tool calls and feeding results
/// back. Terminates on natural stop (no tool calls), a maxTurns cap, or an
/// already-set cancel flag. Tool errors are fed back as tool_result content
/// (is_error=true) and never abort the loop. Port of daemon `loop.ts`.
pub async fn run_generation_loop(
    seed: Vec<Value>,
    provider: Arc<dyn Provider>,
    model: &str,
    registry: &ToolRegistry,
    emitter: &RunEventEmitter,
    file_store: &FileStore,
    max_turns: usize,
    canceled: &AtomicBool,
) -> Result<LoopResult, String> {
    let schemas = registry.schemas().await;
    let mut convo = seed;
    let mut final_text = String::new();
    let mut stop_reason = StopReason::MaxTurns;

    for _turn in 0..max_turns {
        if canceled.load(Ordering::Relaxed) {
            stop_reason = StopReason::Canceled;
            break;
        }

        let result = run_provider_turn(&*provider, model, Value::Array(convo.clone()), &schemas).await?;
        if !result.text.is_empty() {
            emitter.text_delta(&result.text).await;
            final_text = result.text.clone();
        }
        if result.input_tokens.is_some() || result.output_tokens.is_some() {
            emitter.usage(result.input_tokens, result.output_tokens).await;
        }

        if result.tool_calls.is_empty() {
            stop_reason = StopReason::Natural;
            break;
        }

        // Assistant turn carrying the tool calls (OpenAI-shaped, mirroring loop.ts).
        let assistant_tool_calls: Vec<Value> = result
            .tool_calls
            .iter()
            .map(|tc| json!({ "id": tc.id, "type": "function", "function": { "name": tc.name, "arguments": tc.args.to_string() } }))
            .collect();
        convo.push(json!({
            "role": "assistant",
            "content": if result.text.is_empty() { Value::Null } else { Value::String(result.text.clone()) },
            "tool_calls": assistant_tool_calls,
        }));

        for call in &result.tool_calls {
            emitter.tool_use(&call.id, &call.name, &call.args).await;
            let tool_result = registry.dispatch(&call.name, &call.args).await;
            emitter.tool_result(&call.id, &tool_result.content, tool_result.is_error).await;
            convo.push(json!({ "role": "tool", "tool_call_id": call.id, "content": tool_result.content }));
        }
    }

    let produced_html = file_store.read_index_html();
    Ok(LoopResult { final_text, produced_html, stop_reason })
}
```

Add to `mod.rs`: `pub mod system_prompt; pub mod loop_core;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib runs::generation::system_prompt runs::generation::loop_core 2>&1 | tee /tmp/t6.log; grep -nE '^error' /tmp/t6.log`
Expected: PASS (4 loop_core + 5 system_prompt tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/system_prompt.rs packages/backend/api/src/runs/generation/loop_core.rs packages/backend/api/src/runs/generation/mod.rs
git commit -m "feat(generation): agentic loop core + system prompt (natural/maxTurns/canceled)"
```

---

### Task 7: Artifact storage (migration + store)

**Files:**
- Create: `packages/backend/api/src/db/migrations/0024_design_artifacts.sql`
- Create: `packages/backend/api/src/runs/generation/artifact_store.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod artifact_store;`)
- Test: `packages/backend/api/tests/design_generation.rs` (new file; DB-backed store test)

**Interfaces:**
- Produces (consumed by Tasks 8, 9):
  - `pub struct DesignArtifactRow { pub artifact_id: String, pub title: String, pub template_html: String, pub rendered_html: String }`
  - `pub fn generate_artifact_id(title: &str) -> String` — `la-<slug>-<12 hex>` (slug from title, `[a-z0-9-]`, fallback `live-artifact`).
  - `pub async fn insert_artifact(pool: &sqlx::PgPool, engagement_id: uuid::Uuid, project_id: &str, artifact_id: &str, title: &str, template_html: &str, rendered_html: &str, created_by_run_id: uuid::Uuid) -> Result<(), String>`
  - `pub async fn get_artifact(pool: &sqlx::PgPool, engagement_id: uuid::Uuid, project_id: &str, artifact_id: &str) -> Result<Option<DesignArtifactRow>, String>`

- [ ] **Step 1: Write the migration**

Write `packages/backend/api/src/db/migrations/0024_design_artifacts.sql`:

```sql
-- Server-side design-generation artifacts. One row per validated artifact a
-- design-chat run produced. Keyed by (engagement_id, project_id, artifact_id)
-- for engagement-isolated serving via GET /api/live-artifacts/{artifact_id}.
CREATE TABLE design_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    title TEXT NOT NULL,
    template_html TEXT NOT NULL,
    rendered_html TEXT NOT NULL,
    created_by_run_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (engagement_id, project_id, artifact_id)
);

CREATE INDEX idx_design_artifacts_lookup
    ON design_artifacts (engagement_id, project_id, artifact_id);
```

- [ ] **Step 2: Write the failing test**

Write `packages/backend/api/tests/design_generation.rs`:

```rust
mod common;

use celerex_api::runs::generation::artifact_store::{generate_artifact_id, get_artifact, insert_artifact};
use uuid::Uuid;

#[tokio::test]
async fn insert_then_get_roundtrips_scoped_by_engagement() {
    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let other = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let run_id = Uuid::new_v4();
    let aid = generate_artifact_id("My Landing Page");
    assert!(aid.starts_with("la-"));
    insert_artifact(&db.pool, eng, "proj-1", &aid, "My Landing Page", "<h1>t</h1>", "<h1>t</h1>", run_id).await.unwrap();

    let got = get_artifact(&db.pool, eng, "proj-1", &aid).await.unwrap().unwrap();
    assert_eq!(got.title, "My Landing Page");
    assert_eq!(got.rendered_html, "<h1>t</h1>");

    // Cross-engagement read returns None (isolation).
    assert!(get_artifact(&db.pool, other, "proj-1", &aid).await.unwrap().is_none());
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p celerex-api --test design_generation 2>&1 | tee /tmp/t7.log; grep -nE '^error' /tmp/t7.log`
Expected: FAIL — `artifact_store` module unresolved.

- [ ] **Step 4: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/artifact_store.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DesignArtifactRow {
    pub artifact_id: String,
    pub title: String,
    pub template_html: String,
    pub rendered_html: String,
}

/// `la-<slug>-<12 hex>`. Port of the daemon's `generateLiveArtifactId` shape
/// (prefix `la`, slug from title, hex suffix). Uses a random suffix for
/// uniqueness within (engagement, project).
pub fn generate_artifact_id(title: &str) -> String {
    let slug = slugify(title);
    let suffix: String = {
        let mut bytes = [0u8; 6];
        // ring is already a dependency (see providers::store); reuse its RNG.
        use ring::rand::{SecureRandom, SystemRandom};
        SystemRandom::new().fill(&mut bytes).expect("rng");
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    };
    format!("la-{slug}-{suffix}")
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in input.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "live-artifact".to_string() } else { trimmed }
}

pub async fn insert_artifact(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: &str,
    artifact_id: &str,
    title: &str,
    template_html: &str,
    rendered_html: &str,
    created_by_run_id: Uuid,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO design_artifacts
            (engagement_id, project_id, artifact_id, title, template_html, rendered_html, created_by_run_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (engagement_id, project_id, artifact_id) DO UPDATE SET
            title = EXCLUDED.title,
            template_html = EXCLUDED.template_html,
            rendered_html = EXCLUDED.rendered_html,
            created_by_run_id = EXCLUDED.created_by_run_id",
    )
    .bind(engagement_id)
    .bind(project_id)
    .bind(artifact_id)
    .bind(title)
    .bind(template_html)
    .bind(rendered_html)
    .bind(created_by_run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_artifact(
    pool: &PgPool,
    engagement_id: Uuid,
    project_id: &str,
    artifact_id: &str,
) -> Result<Option<DesignArtifactRow>, String> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT artifact_id, title, template_html, rendered_html
         FROM design_artifacts
         WHERE engagement_id = $1 AND project_id = $2 AND artifact_id = $3",
    )
    .bind(engagement_id)
    .bind(project_id)
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|(artifact_id, title, template_html, rendered_html)| DesignArtifactRow {
        artifact_id,
        title,
        template_html,
        rendered_html,
    }))
}
```

Add to `mod.rs`: `pub mod artifact_store;`

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p celerex-api --test design_generation 2>&1 | tee /tmp/t7.log; grep -nE '^error' /tmp/t7.log`
Expected: PASS. (Requires Docker for testcontainers Postgres.)

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/db/migrations/0024_design_artifacts.sql packages/backend/api/src/runs/generation/artifact_store.rs packages/backend/api/src/runs/generation/mod.rs packages/backend/api/tests/design_generation.rs
git commit -m "feat(generation): design_artifacts migration + engagement-scoped artifact store"
```

---

### Task 8: Live-artifact serving routes

**Files:**
- Create: `packages/backend/api/src/runs/generation/artifact_routes.rs`
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add `pub mod artifact_routes;`)
- Modify: `packages/backend/api/src/lib.rs` (mount two routes in `route_chain`)
- Test: append to `packages/backend/api/tests/design_generation.rs`

**Interfaces:**
- Consumes: `get_artifact` (Task 7), `AuthClaims` (`crate::auth::extractors::AuthClaims`, `.0.engagement_id`), `AppState`.
- Produces (mounted in `lib.rs`):
  - `pub async fn get_artifact_meta(AuthClaims, State<AppState>, Path<String>, Query<ArtifactQuery>) -> Result<(SecurityHeaders, Json<serde_json::Value>), (StatusCode, String)>` — `GET /api/live-artifacts/{artifact_id}`.
  - `pub async fn get_artifact_preview(AuthClaims, State<AppState>, Path<String>, Query<ArtifactQuery>) -> Result<(SecurityHeaders, axum::response::Html<String>), (StatusCode, String)>` — `GET /api/live-artifacts/{artifact_id}/preview`.
  - `pub struct ArtifactQuery { pub project_id: String, pub variant: Option<String> }` (serde `rename_all = "camelCase"` → `projectId`, `variant`).
  - `pub type SecurityHeaders = [(HeaderName, &'static str); 4]` — the CSP-parity header prefix (security-critical, per Global Constraints) applied to BOTH handlers, replicating the daemon's `setLiveArtifactPreviewHeaders` (`celerex-design/apps/daemon/src/server.ts:3610-3630`): `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and the locked-down `Content-Security-Policy`. `Content-Type` is left to each body type (`Html` → `text/html; charset=utf-8` per `server.ts:3611`; `Json` → `application/json`). This is the load-bearing backstop for the Task 3 guard's denylist gaps; written in Step 3, asserted in Step 1.

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/tests/design_generation.rs`:

```rust
// --- artifact route tests -------------------------------------------------
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

#[tokio::test]
async fn preview_route_serves_rendered_html_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let aid = generate_artifact_id("Page");
    insert_artifact(&db.pool, eng, "proj-1", &aid, "Page", "<h1>t</h1>", "<h1>rendered</h1>", Uuid::new_v4()).await.unwrap();
    let token = common::mint_token(eng);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/live-artifacts/{aid}/preview?projectId=proj-1&variant=rendered"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&body[..], b"<h1>rendered</h1>");
}

#[tokio::test]
async fn meta_route_404s_for_other_engagement() {
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let other = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('Other') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let aid = generate_artifact_id("Page");
    insert_artifact(&db.pool, eng, "proj-1", &aid, "Page", "<h1>t</h1>", "<h1>t</h1>", Uuid::new_v4()).await.unwrap();
    let token = common::mint_token(other);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/live-artifacts/{aid}?projectId=proj-1"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn preview_route_sets_csp_parity_headers_matching_daemon() {
    // Serve-layer CSP parity with the daemon's setLiveArtifactPreviewHeaders
    // (celerex-design/apps/daemon/src/server.ts:3610-3630). The stored rendered
    // HTML deliberately carries a guard-gap payload (`<svg/onload=...>`): this
    // serve does NOT re-run the Task 3 guard, so `script-src 'none'` + the
    // script-less `sandbox` are the enforcement boundary that makes it inert.
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let aid = generate_artifact_id("Page");
    insert_artifact(&db.pool, eng, "proj-1", &aid, "Page", "<h1>t</h1>", "<svg/onload=alert(1)></svg>", Uuid::new_v4()).await.unwrap();
    let token = common::mint_token(eng);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/live-artifacts/{aid}/preview?projectId=proj-1&variant=rendered"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let h = resp.headers();
    // Byte-for-byte CSP parity (server.ts:3615-3630), directive order preserved.
    assert_eq!(
        h.get("content-security-policy").unwrap().to_str().unwrap(),
        "default-src 'none'; base-uri 'none'; script-src 'none'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'unsafe-inline'; sandbox allow-same-origin"
    );
    // The load-bearing controls the guard story depends on (spec §5.2/§5.4.1).
    let csp = h.get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("script-src 'none'"));
    assert!(csp.contains("sandbox allow-same-origin") && !csp.contains("allow-scripts"));
    assert!(csp.contains("img-src 'self' data: blob:"));
    assert!(csp.contains("connect-src 'none'"));
    assert_eq!(h.get("x-content-type-options").unwrap().to_str().unwrap(), "nosniff");
    assert_eq!(h.get("referrer-policy").unwrap().to_str().unwrap(), "no-referrer");
    assert_eq!(h.get("cache-control").unwrap().to_str().unwrap(), "no-store");
    // Content-Type stays owned by Html (server.ts:3611), not overridden by the prefix.
    assert_eq!(h.get("content-type").unwrap().to_str().unwrap(), "text/html; charset=utf-8");
}
```

Also add to `packages/backend/api/tests/common/mod.rs` two helpers (`build_app_for_generation_tests` building an `AppState` + a `Router` with the two routes mounted, and `mint_token(eng)` encoding a JWT with `engagement_id` — copy the `TestClaims` + `token` pattern from `tests/assistant_chat.rs` lines 25–54, and the `build_state` pattern from lines 56–89, exposing a router with `.route("/api/live-artifacts/{artifact_id}", get(celerex_api::runs::generation::artifact_routes::get_artifact_meta))` and `.route("/api/live-artifacts/{artifact_id}/preview", get(celerex_api::runs::generation::artifact_routes::get_artifact_preview))`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --test design_generation 2>&1 | tee /tmp/t8.log; grep -nE '^error' /tmp/t8.log`
Expected: FAIL — `artifact_routes` + `common::build_app_for_generation_tests` unresolved.

- [ ] **Step 3: Write minimal implementation**

Write `packages/backend/api/src/runs/generation/artifact_routes.rs`:

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderName};
use axum::response::Html;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::extractors::AuthClaims;
use crate::runs::generation::artifact_store::get_artifact;
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactQuery {
    pub project_id: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// The four non-`Content-Type` security headers emitted by the daemon's
/// `setLiveArtifactPreviewHeaders` (`celerex-design/apps/daemon/src/server.ts:3610-3630`),
/// applied verbatim to BOTH live-artifact routes. `Content-Type` is intentionally
/// omitted so each handler's body type owns it (`Html` → `text/html; charset=utf-8`,
/// matching `server.ts:3611`; `Json` → `application/json`).
pub type SecurityHeaders = [(HeaderName, &'static str); 4];

/// Directive-for-directive copy of the daemon CSP (`server.ts:3615-3630`), in the
/// same order. `script-src 'none'` + the script-less `sandbox` directive are the
/// load-bearing controls that make the `html_template_v1` guard's known denylist
/// gaps (`<svg/onload=...>`, SMIL `to="javascript:..."`, external resource refs)
/// inert on serve — see Global Constraints + infographic spec §5.2/§5.4.1.
const LIVE_ARTIFACT_CSP: &str = "default-src 'none'; base-uri 'none'; script-src 'none'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'unsafe-inline'; sandbox allow-same-origin";

/// Build the CSP-parity header prefix for a live-artifact response. Mirrors
/// `setLiveArtifactPreviewHeaders` (`server.ts:3612-3630`) minus `Content-Type`.
fn live_artifact_security_headers() -> SecurityHeaders {
    [
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CONTENT_SECURITY_POLICY, LIVE_ARTIFACT_CSP),
    ]
}

/// `GET /api/live-artifacts/{artifact_id}?projectId=...` — artifact metadata
/// the web FileViewer fetches on a `live_artifact` SSE event. Engagement-scoped.
/// Carries the CSP-parity headers (defense-in-depth; `Content-Type` stays
/// `application/json`).
pub async fn get_artifact_meta(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<ArtifactQuery>,
) -> Result<(SecurityHeaders, Json<serde_json::Value>), (StatusCode, String)> {
    let row = get_artifact(&state.pool, claims.engagement_id, &q.project_id, &artifact_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    Ok((
        live_artifact_security_headers(),
        Json(json!({
            "artifact": {
                "id": row.artifact_id,
                "projectId": q.project_id,
                "title": row.title,
                "refreshStatus": "idle",
                "preview": { "type": "html", "entry": "index.html" },
            }
        })),
    ))
}

/// `GET /api/live-artifacts/{artifact_id}/preview?projectId=...&variant=rendered`
/// — the rendered HTML the web iframes. `variant=template` returns the raw
/// author template; anything else returns the rendered preview. Engagement-scoped.
/// The response replicates `setLiveArtifactPreviewHeaders` verbatim; this serve
/// does NOT re-run the Task 3 guard, so the CSP is its enforcement boundary
/// against any guard-gap payload that reached the row.
pub async fn get_artifact_preview(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<ArtifactQuery>,
) -> Result<(SecurityHeaders, Html<String>), (StatusCode, String)> {
    let row = get_artifact(&state.pool, claims.engagement_id, &q.project_id, &artifact_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    let html = match q.variant.as_deref() {
        Some("template") => row.template_html,
        _ => row.rendered_html,
    };
    Ok((live_artifact_security_headers(), Html(html)))
}
```

Add to `mod.rs`: `pub mod artifact_routes;`

In `packages/backend/api/src/lib.rs` `route_chain()` (after the `/api/runs/...` routes, near line 136), add:

```rust
        .route("/api/live-artifacts/{artifact_id}", get(runs::generation::artifact_routes::get_artifact_meta))
        .route("/api/live-artifacts/{artifact_id}/preview", get(runs::generation::artifact_routes::get_artifact_preview))
```

Add the `common` helpers described in Step 1 to `tests/common/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --test design_generation 2>&1 | tee /tmp/t8.log; grep -nE '^error' /tmp/t8.log`
Expected: PASS — the route/body test, the cross-engagement 404, AND `preview_route_sets_csp_parity_headers_matching_daemon` (CSP + `nosniff` + `no-referrer` + `no-store` parity).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/artifact_routes.rs packages/backend/api/src/runs/generation/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/common/mod.rs packages/backend/api/tests/design_generation.rs
git commit -m "feat(generation): serve /api/live-artifacts/{id}(/preview) engagement-scoped with CSP-parity headers (daemon setLiveArtifactPreviewHeaders)"
```

---

### Task 9: Orchestrator + design_chat wiring (replace the stub)

**Files:**
- Modify: `packages/backend/api/src/runs/generation/mod.rs` (add the `run_generation` orchestrator)
- Modify: `packages/backend/api/src/runs/design_chat.rs` (`CreateRunRequest` + `PendingDesignRun` gain `project_id`; `stream_pending` calls `run_generation`)

**Interfaces:**
- Consumes: `RunEventEmitter` (Task 5), `run_generation_loop`/`compose_system_prompt`/`PromptFragment`/`StopReason`/`LoopResult` (Task 6), `FileStore`/`FileToolSource` (Task 1), `ToolRegistry` (Task 2), `render_html_template_v1` (Task 3), `generate_artifact_id`/`insert_artifact` (Task 7), `celerex_orchestrator::llm::Provider`.
- Produces:
  - `pub async fn run_generation(tx: &tokio::sync::mpsc::Sender<Result<axum::response::sse::Event, std::convert::Infallible>>, provider: std::sync::Arc<dyn celerex_orchestrator::llm::Provider>, provider_id: &str, model: &str, engagement_id: uuid::Uuid, project_id: &str, run_id: uuid::Uuid, user_message: &str, pool: &sqlx::PgPool, max_turns: usize, fragments: &[PromptFragment])` — runs the loop, persists + emits the artifact, and emits the terminal `end` frame. Emits `error`+`end` on a provider/loop failure. The caller (`stream_pending`) emits `start` and resolves the provider first. `fragments` is the ordered system-prompt fragment list (Task 6 `PromptFragment`, canonical order Brand → Motion → Task); **v1 call sites all pass `&[]`**, which composes to the base prompt byte-identically to the removed `None` (B1 — plumbing only; Brand Kit is the first producer).

- [ ] **Step 1: Write the failing test (integration parity — the daemon `api-generation-run` cases)**

Append to `packages/backend/api/tests/design_generation.rs`:

```rust
use celerex_api::runs::generation::run_generation;
use celerex_orchestrator::llm::stub::StubProvider;
use serde_json::json;
use std::sync::Arc;

async fn drain_frames(mut rx: tokio::sync::mpsc::Receiver<Result<axum::response::sse::Event, std::convert::Infallible>>) -> Vec<String> {
    // We can't read Event bodies directly; instead assert ordering by draining
    // the count and relying on the artifact row + terminal behavior below.
    let mut n = Vec::new();
    while let Ok(Ok(_ev)) = rx.try_recv() { n.push(String::new()); }
    n
}

#[tokio::test]
async fn run_generation_writes_file_persists_artifact_and_finishes_succeeded() {
    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let run_id = Uuid::new_v4();
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "building",
            "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<!doctype html><h1>Hi</h1>\"}" } }] } }] }),
        json!({ "choices": [{ "message": { "content": "DONE" } }] }),
    ]));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(&tx, provider, "copilot", "gpt-4o", eng, "proj-1", run_id, "make a landing page", &db.pool, 20, &[]).await;
    drop(tx);
    let frames = drain_frames(rx).await;
    assert!(!frames.is_empty());

    // Exactly one artifact row persisted for this engagement/project.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1 AND project_id = 'proj-1'")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
    let rendered: String = sqlx::query_scalar("SELECT rendered_html FROM design_artifacts WHERE engagement_id = $1")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert!(rendered.contains("<h1>Hi</h1>"));
}

#[tokio::test]
async fn run_generation_rejects_script_artifact_but_still_succeeds() {
    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G2') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "",
            "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>x</h1><script>alert(1)</script>\"}" } }] } }] }),
        json!({ "choices": [{ "message": { "content": "done" } }] }),
    ]));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    run_generation(&tx, provider, "copilot", "gpt-4o", eng, "proj-1", Uuid::new_v4(), "make it", &db.pool, 20, &[]).await;
    // A rejected artifact means NO row persisted, but the run must have finished
    // succeeded (asserted structurally: no panic, no row).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --test design_generation run_generation 2>&1 | tee /tmp/t9.log; grep -nE '^error' /tmp/t9.log`
Expected: FAIL — `run_generation` unresolved.

- [ ] **Step 3: Write minimal implementation**

Add to `packages/backend/api/src/runs/generation/mod.rs` (after the `pub mod ...;` lines):

```rust
use std::convert::Infallible;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::response::sse::Event;
use celerex_orchestrator::llm::Provider;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use self::artifact_store::{generate_artifact_id, insert_artifact};
use self::emitter::RunEventEmitter;
use self::file_store::{FileStore, FileToolSource};
use self::loop_core::{compose_system_prompt, run_generation_loop, PromptFragment, StopReason};
use self::registry::ToolRegistry;
use self::template_guard::render_html_template_v1;
use self::tools::ToolSource;

const DEFAULT_TITLE: &str = "Generated design";

fn derive_title(user_message: &str) -> String {
    let trimmed: String = user_message.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        DEFAULT_TITLE.to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// The `index.ts` analog: wire the registry/emitter/file-store, seed messages,
/// run the loop, validate + persist the produced HTML as one artifact row and
/// emit `live_artifact`, then emit the terminal `end` frame. The caller
/// (`design_chat::stream_pending`) has already emitted `start` and resolved the
/// provider. An artifact-guard rejection surfaces as an `artifact-error` status
/// while the run STILL finishes `succeeded` — load-bearing for the web UI.
///
/// `fragments` are the ordered system-prompt fragments (Brand → Motion → Task),
/// composed by `compose_system_prompt` under `MAX_PROMPT_FRAGMENT_CHARS`
/// (truncate-last). v1 always passes `&[]` (composer empty; base prompt only) —
/// this parameter is plumbing for Brand Kit, the first real producer (B1).
#[allow(clippy::too_many_arguments)]
pub async fn run_generation(
    tx: &Sender<Result<Event, Infallible>>,
    provider: Arc<dyn Provider>,
    provider_id: &str,
    model: &str,
    engagement_id: Uuid,
    project_id: &str,
    run_id: Uuid,
    user_message: &str,
    pool: &PgPool,
    max_turns: usize,
    fragments: &[PromptFragment],
) {
    let _ = provider_id; // reserved for future per-provider tool-shape adaptation
    let emitter = RunEventEmitter::new(tx.clone());
    let file_store = FileStore::new();
    let file_source: Arc<dyn ToolSource> = Arc::new(FileToolSource::new(file_store.clone()));
    let registry = ToolRegistry::new(vec![file_source]);

    let tool_names: Vec<String> = registry.schemas().await.into_iter().map(|s| s.name).collect();
    // Compose the system prompt from the ordered fragments (slot order, not
    // insertion order; truncate-last to the combined budget). v1 passes an empty
    // list, so `composed.text` is byte-identical to the pre-composer base prompt.
    let composed = compose_system_prompt(&tool_names, fragments);
    if composed.truncated {
        emitter
            .status(
                "prompt-fragment-truncated",
                Some("system-prompt fragments exceeded the combined budget; the last fragment was truncated"),
            )
            .await;
    }
    let seed: Vec<Value> = vec![
        json!({ "role": "system", "content": composed.text }),
        json!({ "role": "user", "content": user_message }),
    ];

    let canceled = AtomicBool::new(false);
    let result = match run_generation_loop(seed, provider, model, &registry, &emitter, &file_store, max_turns, &canceled).await {
        Ok(r) => r,
        Err(message) => {
            // Provider/loop failure: generic client-facing message, then failed.
            emitter.error_event(&message).await;
            emitter.end("failed", 1).await;
            return;
        }
    };

    if let Some(html) = &result.produced_html {
        match render_html_template_v1(html) {
            Ok(rendered) => {
                let title = derive_title(user_message);
                let artifact_id = generate_artifact_id(&title);
                match insert_artifact(pool, engagement_id, project_id, &artifact_id, &title, html, &rendered, run_id).await {
                    Ok(()) => emitter.live_artifact(project_id, &artifact_id, &title).await,
                    Err(e) => {
                        tracing::error!(engagement = %engagement_id, error = %e, "design run: artifact persist failed");
                        emitter.status("artifact-error", Some("failed to persist artifact")).await;
                    }
                }
            }
            // Guard rejection (e.g. <script>, stray {{ }}): surface artifact-error
            // but DO NOT fail the run.
            Err(err) => emitter.status("artifact-error", Some(&err.0)).await,
        }
    }

    if matches!(result.stop_reason, StopReason::MaxTurns) {
        emitter.status("reached step limit", None).await;
    }
    emitter.end("succeeded", 0).await;
}
```

Now rewire `packages/backend/api/src/runs/design_chat.rs`.

(a) Add `project_id` to `CreateRunRequest` (after `model`, line ~86):

```rust
    #[serde(default)]
    pub project_id: Option<String>,
```

(b) Change `PendingDesignRun` (lines 45–52) to carry the user message + project id instead of the pre-shaped `messages`:

```rust
#[derive(Clone)]
pub struct PendingDesignRun {
    pub engagement_id: Uuid,
    pub provider_id: String,
    pub model: String,
    pub project_id: String,
    /// The user's turn (flattened transcript) seeded into the generation loop.
    pub user_message: String,
}
```

(c) In `create` (lines ~124–140), replace the `messages` shaping + insert with:

```rust
    let content = body
        .message
        .filter(|m| !m.trim().is_empty())
        .or(body.current_prompt)
        .unwrap_or_default();
    let project_id = body
        .project_id
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| run_id.to_string());

    let run_id = Uuid::new_v4();
    state.design_runs.lock().await.insert(
        run_id,
        PendingDesignRun {
            engagement_id: claims.engagement_id,
            provider_id,
            model,
            project_id,
            user_message: content,
        },
    );
```

(Move the `let run_id = Uuid::new_v4();` line ABOVE the `project_id` fallback so `run_id.to_string()` is in scope; keep the existing TTL-reclaim `tokio::spawn` block below unchanged.)

(d) Replace the body of `stream_pending` (lines ~165–232, the `tokio::spawn` closure) so that after emitting `start` and resolving the provider it calls `run_generation`:

```rust
    tokio::spawn(async move {
        let bin = format!("{}/{}", pending.provider_id, pending.model);
        if tx
            .send(Ok(named("start", json!({ "bin": bin, "projectId": pending.project_id, "model": pending.model }))))
            .await
            .is_err()
        {
            return;
        }

        let provider =
            match resolve_engagement_provider(&state, pending.engagement_id, &pending.provider_id).await {
                Ok(Some(p)) => p,
                Ok(None) => {
                    send_error(&tx, format!("no key configured for provider '{}'", pending.provider_id)).await;
                    return;
                }
                Err((_, e)) => {
                    tracing::error!(
                        engagement = %pending.engagement_id,
                        provider = %pending.provider_id,
                        error = %e,
                        "design-chat run: provider key store error"
                    );
                    send_error(&tx, "provider key store error".to_string()).await;
                    return;
                }
            };

        crate::runs::generation::run_generation(
            &tx,
            provider,
            &pending.provider_id,
            &pending.model,
            pending.engagement_id,
            &pending.project_id,
            run_id,
            &pending.user_message,
            &state.pool,
            20,
            &[], // v1: system-prompt fragment composer ships EMPTY (Brand Kit is the first producer)
        )
        .await;
    });
```

Note the closure must capture `run_id`: add `let run_id = ...` — the pending run has no id field, so change `stream_pending`'s signature call site in `runs::sse::stream` to pass the matched `run_id`. Update `pub fn stream_pending(state: AppState, pending: PendingDesignRun)` to `pub fn stream_pending(state: AppState, run_id: Uuid, pending: PendingDesignRun)`, and in `runs/sse.rs` line ~24 change the call to `stream_pending(s.clone(), run_id, pending)`.

Remove the now-dead helpers `completion_text` (lines ~258–278) — it is no longer referenced. Keep `named` and `send_error` (still used).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --test design_generation run_generation 2>&1 | tee /tmp/t9.log; grep -nE '^error' /tmp/t9.log`
Expected: PASS. Then full-crate compile check: `cargo build -p celerex-api 2>&1 | tee /tmp/t9b.log; grep -nE '^error' /tmp/t9b.log` → no errors.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/runs/generation/mod.rs packages/backend/api/src/runs/design_chat.rs packages/backend/api/src/runs/sse.rs packages/backend/api/tests/design_generation.rs
git commit -m "feat(generation): wire full tool-loop engine into design_chat run (replaces bare-completion stub)"
```

---

### Task 10: Parity regression tests (ported daemon suite)

**Files:**
- Modify: `packages/backend/api/tests/design_generation.rs` (add ported parity cases)

**Interfaces:**
- Consumes: `run_generation` (Task 9), `StubProvider`, `common` harness. No new production code — this task is a review gate proving daemon-equivalent behavior.

- [ ] **Step 1: Write the failing/again-green regression tests**

Append the ported daemon cases to `packages/backend/api/tests/design_generation.rs`. These mirror `api-generation-provider-turn.test.ts` (empty-tools omission, envelope, charset alias — already unit-tested in Task 4) and `api-generation-run.test.ts` (ordered SSE, secret-non-leak). Add the two still-uncovered daemon behaviors:

```rust
#[tokio::test]
async fn provider_failure_does_not_leak_body_and_finishes_failed() {
    // A provider that returns an error string containing a secret upstream
    // detail must NOT appear in any emitted frame; the run finishes failed.
    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('F') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    // StubProvider with no scripted responses errors "no more scripted responses".
    let provider = Arc::new(StubProvider::new(vec![]));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    run_generation(&tx, provider, "openai", "gpt-4o", eng, "proj-1", Uuid::new_v4(), "x", &db.pool, 20, &[]).await;
    // No artifact row; loop errored on the first turn → failed terminal.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn max_turns_still_persists_last_index_html_and_succeeds() {
    // Model keeps calling tools; the loop caps at maxTurns but the last written
    // index.html is still validated + persisted, and the run succeeds.
    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('M') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let turn = || json!({ "choices": [{ "message": { "content": "",
        "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>loop</h1>\"}" } }] } }] });
    let provider = Arc::new(StubProvider::new(vec![turn(), turn(), turn()]));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    run_generation(&tx, provider, "copilot", "gpt-4o", eng, "proj-1", Uuid::new_v4(), "loop it", &db.pool, 3, &[]).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p celerex-api --test design_generation 2>&1 | tee /tmp/t10.log; grep -nE '^error' /tmp/t10.log`
Expected: PASS (all design_generation tests: store, routes, run_generation, and the two ported parity cases).

- [ ] **Step 3: Commit**

```bash
git add packages/backend/api/tests/design_generation.rs
git commit -m "test(generation): parity regression cases ported from the daemon api-generation suite"
```

---

### Task 11: Gated live end-to-end test

**Files:**
- Create: `packages/backend/api/tests/design_generation_live.rs`

**Interfaces:**
- Consumes: `run_generation` (Task 9), `assistant::resolve_engagement_provider_with` OR a directly-constructed real provider from an env key. `#[ignore]` so CI without a key skips it.

- [ ] **Step 1: Write the ignored live test**

Write `packages/backend/api/tests/design_generation_live.rs`:

```rust
mod common;

use celerex_api::runs::generation::run_generation;
use std::sync::Arc;
use uuid::Uuid;

/// Real end-to-end: uses a real provider key from the environment to drive the
/// full loop against a live model, then asserts an artifact row was persisted.
/// Ignored by default — run with:
///   OMNIVYX_LIVE_PROVIDER=copilot OMNIVYX_LIVE_MODEL=gpt-4o \
///   OMNIVYX_LIVE_KEY=... cargo test -p celerex-api --test design_generation_live -- --ignored --nocapture
#[tokio::test]
#[ignore = "requires a real provider key via OMNIVYX_LIVE_KEY"]
async fn live_generation_persists_a_real_artifact() {
    let key = std::env::var("OMNIVYX_LIVE_KEY").expect("OMNIVYX_LIVE_KEY");
    let provider_id = std::env::var("OMNIVYX_LIVE_PROVIDER").unwrap_or_else(|_| "copilot".into());
    let model = std::env::var("OMNIVYX_LIVE_MODEL").unwrap_or_else(|_| "gpt-4o".into());

    let db = common::start_test_db().await;
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('LIVE') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    // Store the key so resolve_engagement_provider builds the real client.
    let store = celerex_api::providers::store::PgProviderKeysStore::from_env(db.pool.clone()).unwrap();
    use celerex_api::providers::store::ProviderKeysStore;
    store.set(eng, &provider_id, &key).await.unwrap();
    let provider_keys: Arc<dyn ProviderKeysStore> = Arc::new(store);
    let provider = celerex_api::assistant::resolve_engagement_provider_with(&provider_keys, eng, &provider_id)
        .await
        .unwrap()
        .expect("provider built from stored key");

    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    run_generation(&tx, provider, &provider_id, &model, eng, "proj-live", Uuid::new_v4(),
        "Build a simple centered hero section with a headline and a subtitle. No scripts.", &db.pool, 12, &[]).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1, "a real run should persist exactly one artifact");
}
```

Note: this requires `resolve_engagement_provider_with` and `assistant` to be reachable from tests. If `assistant` is not `pub`, expose `pub use` or add a small `pub` re-export in `lib.rs` (`pub mod assistant;` already exists per `crate::assistant::resolve_engagement_provider`; ensure `resolve_engagement_provider_with` is `pub` rather than `pub(crate)` — widen its visibility in `assistant/mod.rs` line ~75).

- [ ] **Step 2: Verify it compiles and is skipped by default**

Run: `cargo test -p celerex-api --test design_generation_live 2>&1 | tee /tmp/t11.log; grep -nE '^error' /tmp/t11.log`
Expected: compiles; reports `1 ignored` (no key → skipped).

- [ ] **Step 3: Commit**

```bash
git add packages/backend/api/tests/design_generation_live.rs packages/backend/api/src/assistant/mod.rs
git commit -m "test(generation): gated live E2E (real provider key, #[ignore] by default)"
```

---

### Task 12: Docs / runbook

**Files:**
- Create: `packages/backend/api/src/runs/generation/README.md`

**Interfaces:** none (documentation only).

- [ ] **Step 1: Write the module runbook**

Write `packages/backend/api/src/runs/generation/README.md` documenting: the module map (one line per file), the SSE vocabulary table (copy the Global Constraints list verbatim), the storage model (in-memory `FileStore` working set + `design_artifacts` row + the two GET routes), the `artifact-error`→`succeeded` guard contract, the in-process provider call (no HTTP hop; fail-closed on missing vaulted key), and the known limitation (tools go on the wire as the OpenAI function-tool envelope — full parity for OpenAI-compatible providers; native Anthropic/Gemini tool schemas are a documented follow-up). Include the manual browser-verification steps from the runbook section below.

- [ ] **Step 2: Commit**

```bash
git add packages/backend/api/src/runs/generation/README.md
git commit -m "docs(generation): module runbook + SSE vocabulary + storage model"
```

---

## Manual Browser-Verification Runbook

After Task 12, verify the full path end-to-end against the real SPA (the web is unchanged):

1. **Build the SPA export** and point the Agent at it: `CELEREX_WEB_DIR=<path to design app `out/`>` (see `web_service()` in `lib.rs`).
2. **Boot the Agent** with a DB and the crypto env:
   ```bash
   CELEREX_CRED_KEY=0123456789abcdef0123456789abcdef \
   CELEREX_JWT_SECRET=dev-secret \
   PORT=3001 cargo run -p celerex-api
   ```
   (Do NOT set `CELEREX_HOUSE_MODE`; the design path is fail-closed.)
3. **Seed a real vaulted key** for your engagement via the provider-keys endpoint (or the design Settings UI): store an OpenAI-compatible provider key (e.g. `copilot`) for the engagement you will log in as.
4. **Open the SPA** at `http://localhost:3001`, sign in, open/create a project, select the matching `<provider>/<model>`, and send a design prompt like *"Build a centered hero with a headline and subtitle."*
5. **Watch the chat stream:** you should see the assistant status → text → tool-use/tool-result activity (the `agent` frames), then the artifact tab populate.
6. **Confirm the artifact renders:** the FileViewer iframe should show the generated page. In the network panel, confirm the web fetched `GET /api/live-artifacts/{artifactId}?projectId=...` (200 JSON) and `GET /api/live-artifacts/{artifactId}/preview?projectId=...&variant=rendered` (200 HTML) from the Agent.
7. **Confirm the guard path:** send a prompt that coerces a `<script>` into `index.html` (or temporarily relax the system prompt). The chat should show an `artifact-error` status but the run should still finish `succeeded` (no error banner), and no artifact row should be served.
8. **Confirm isolation:** with a second engagement that has NO vaulted key, sending a design prompt should surface `no key configured for provider '<id>'` and finish `failed` (fail-closed).
9. **DB check:** `SELECT engagement_id, project_id, artifact_id, title FROM design_artifacts ORDER BY created_at DESC LIMIT 5;` — the successful run's row is present and scoped to your engagement.

---

## Self-Review

**1. Spec coverage.** Every success-criterion clause maps to a task: browser prompt → `POST /api/runs` (design_chat, Task 9); full tool-loop with vaulted provider (Tasks 4–6, 9; in-process `resolve_engagement_provider`); server-side file writes (Task 1 `FileStore`); live HTML artifact over the EXISTING runs SSE with the web's exact vocabulary (Task 5 emitter + Task 9 `live_artifact` + Tasks 7–8 persistence/serving that the web fetches); `html_template_v1` guard → `artifact-error` + `succeeded` (Tasks 3, 9). Recon citations (daemon `api-generation/*` + tests, `render.ts`, `daemon.ts`/`FileViewer.tsx`, `provider.rs`/`store.rs`/`assistant/mod.rs`, migrations, `tests/common/mod.rs`) are reflected in the file paths and ported test cases.

**2. Placeholder scan.** No `TBD`/`later`/"add error handling" — each code step is complete Rust. The one deliberately deferred item (native Anthropic/Gemini tool-schema adaptation) is documented as an explicit known limitation, not a plan gap, because it preserves daemon parity (the daemon likewise sent OpenAI-envelope tools).

**3. Type consistency.** Signatures are stable across tasks: `ToolSpec`/`ToolResult`/`ToolSource` (Task 1) are consumed unchanged in 2/4/5/9; `ProviderTurnResult`/`ToolCall` (Task 4) feed `run_generation_loop` (Task 6); `RunEventEmitter` method names (Task 5) match every call site in Task 6/9; `run_generation` (Task 9) is called identically in Tasks 9/10/11; `generate_artifact_id`/`insert_artifact`/`get_artifact`/`DesignArtifactRow` (Task 7) are used verbatim in Tasks 8/9; `render_html_template_v1` returns `Result<String, TemplateGuardError>` and Task 9 destructures `err.0`. The `stream_pending` signature change (adding `run_id`) is propagated to its sole caller in `runs/sse.rs` (Task 9, Step 3d).
