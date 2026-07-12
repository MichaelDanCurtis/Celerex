use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use super::tools::{
    ToolResult, ToolSource, ToolSpec, namespace_prefix, namespace_tool_name, owns_tool_name,
};

/// Aggregates multiple [`ToolSource`]s into a single flat, namespaced tool
/// catalogue and routes dispatch calls back to the owning source. Port of
/// daemon `tool-registry.ts`.
///
/// Routing is registered-prefix-aware and name-verified — NOT a generic
/// "does the name contain '__'" test:
///
///   1. For each registered NON-file source, check whether `name` starts
///      with that source's *actual* registered namespace prefix (what
///      `owns_tool_name(source.id, name)` means for a prefixed source). If
///      so, de-namespace back to the bare tool name and dispatch there.
///   2. Otherwise, if a `file` source is registered AND it actually
///      declares (via `list_tools()`) a tool matching `name` (bare, as-is —
///      the file source is unprefixed), dispatch there.
///   3. Otherwise the name is unknown: return an error `ToolResult`, never
///      panic.
///
/// This guarantees a bare file-tool name that happens to contain `__` (e.g.
/// `write__file`) is routed to `file` rather than being misclassified as
/// "prefixed" by a naive `contains("__")` check. Critically, the file route
/// is decided in step 2 by asking the file source what it actually
/// declares — NEVER via `owns_tool_name("file", name)`. That helper's
/// file-side definition (`!tool_name.contains("__")`) would itself return
/// `false` for a bare name containing `__`, which would wrongly report a
/// real, registered file tool as not owned by `file` and misroute or reject
/// it (see the regression test below).
pub struct ToolRegistry {
    sources: Vec<Arc<dyn ToolSource>>,
}

impl ToolRegistry {
    pub fn new(sources: Vec<Arc<dyn ToolSource>>) -> ToolRegistry {
        ToolRegistry { sources }
    }

    /// Flattened, namespaced catalogue across all sources. When two sources
    /// would namespace to the same name, the first-registered source wins
    /// and later duplicates are dropped silently (matches daemon behavior).
    pub async fn schemas(&self) -> Vec<ToolSpec> {
        let mut out: Vec<ToolSpec> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for source in &self.sources {
            for spec in source.list_tools().await {
                let name = namespace_tool_name(source.id(), &spec.name);
                if !seen.insert(name.clone()) {
                    continue; // first source wins; later duplicate dropped
                }
                out.push(ToolSpec {
                    name,
                    description: spec.description,
                    parameters: spec.parameters,
                });
            }
        }
        out
    }

    /// Routes a namespaced (or bare, for `file`) tool name back to its
    /// owning source and executes it there with the de-namespaced bare
    /// name.
    pub async fn dispatch(&self, name: &str, args: &Value) -> ToolResult {
        // 1. Non-file (prefixed) sources: match by their own registered
        //    namespace prefix — not a generic contains("__") test.
        for source in self.sources.iter().filter(|s| s.id() != "file") {
            if !owns_tool_name(source.id(), name) {
                continue;
            }
            let bare = &name[namespace_prefix(source.id()).len()..];
            return source.execute(bare, args).await;
        }
        // 2. No prefixed source matched: the name can only belong to the
        //    unprefixed `file` source. Verify it against the file source's
        //    ACTUAL list_tools() declarations before dispatching — never
        //    via owns_tool_name("file", name), which would misreport a
        //    genuinely-registered file tool name containing "__" as
        //    "not owned by file".
        if let Some(file) = self.sources.iter().find(|s| s.id() == "file") {
            let declares = file.list_tools().await.iter().any(|s| s.name == name);
            if declares {
                return file.execute(name, args).await;
            }
        }
        // 3. Unknown to every source: report, never panic.
        ToolResult::err(format!("unknown tool '{name}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::tools::{ToolResult, ToolSource, ToolSpec};
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::Arc;

    struct Stub {
        id: String,
        names: Vec<String>,
        tag: String,
    }
    #[async_trait]
    impl ToolSource for Stub {
        fn id(&self) -> &str {
            &self.id
        }
        async fn list_tools(&self) -> Vec<ToolSpec> {
            self.names
                .iter()
                .map(|n| ToolSpec {
                    name: n.clone(),
                    description: n.clone(),
                    parameters: json!({"type":"object","properties":{}}),
                })
                .collect()
        }
        async fn execute(&self, name: &str, _args: &Value) -> ToolResult {
            ToolResult::ok(format!("{}:{}", self.tag, name))
        }
    }

    fn stub(id: &str, names: &[&str], tag: &str) -> Arc<dyn ToolSource> {
        Arc::new(Stub {
            id: id.into(),
            names: names.iter().map(|s| s.to_string()).collect(),
            tag: tag.into(),
        })
    }

    #[tokio::test]
    async fn schemas_namespace_file_bare_mcp_prefixed() {
        let reg = ToolRegistry::new(vec![
            stub("file", &["write_file"], "f"),
            stub("mcp:figma", &["get_file"], "m"),
        ]);
        let mut names: Vec<String> = reg.schemas().await.into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["mcp__figma__get_file".to_string(), "write_file".to_string()]
        );
    }

    #[tokio::test]
    async fn dispatch_routes_namespaced_call_to_owner_with_bare_name() {
        let reg = ToolRegistry::new(vec![
            stub("file", &["write_file"], "f"),
            stub("mcp:figma", &["get_file"], "m"),
        ]);
        let res = reg.dispatch("mcp__figma__get_file", &json!({})).await;
        assert_eq!(res.content, "m:get_file"); // bare name passed to the source
    }

    /// Regression test ported from the daemon reference
    /// (`apps/daemon/tests/api-generation-tool-registry.test.ts:44-67`),
    /// flagged by Task 1's review as the branch's biggest latent trap: a
    /// naive ownership check like `owns_tool_name("file", name)` returns
    /// `false` for a bare file-tool name containing "__" (its file-side
    /// definition is `!name.contains("__")`), which would wrongly misroute
    /// or reject a real, registered file tool literally named `write__file`.
    /// Routing must instead verify the bare name against the file source's
    /// actual `list_tools()` declarations, never against `owns_tool_name`.
    #[tokio::test]
    async fn dispatch_bare_file_tool_with_double_underscore_not_misrouted() {
        let reg = ToolRegistry::new(vec![
            stub("file", &["write__file"], "f"),
            stub("mcp:figma", &["get_file"], "m"),
        ]);
        let res = reg.dispatch("write__file", &json!({})).await;
        assert_eq!(res.content, "f:write__file"); // routed to file, bare name as-is
        assert!(!res.is_error);
    }

    #[tokio::test]
    async fn dispatch_unknown_name_is_error_never_panics() {
        let reg = ToolRegistry::new(vec![stub("file", &["write_file"], "f")]);
        let res = reg.dispatch("nope", &json!({})).await;
        assert!(res.is_error);
    }
}
