//! galvo-memory connector — read/write the Neo4j-backed agent memory.
//!
//! Talks to the galvo-memory FastAPI sidecar (default `http://localhost:7575`).
//! Two ops drive the `ExecutionType::Memory` node:
//!   - `store`  → `POST /api/{label}`            (write a typed memory node)
//!   - `search` → `GET  /api/search/{label}?q=…` (semantic recall)
//!
//! The sidecar's per-label models use `extra="forbid"`, so `store` sends
//! exactly `scope` + the caller-supplied `fields` for that label. `scope`
//! defaults to `project:celerex` (override via `CELEREX_MEMORY_SCOPE`).
//!
//! Beyond the flow node, [`log_run_session`] lets Celerex itself build memory:
//! every flow run is recorded as a `:Session` node (best-effort, never blocks).

use async_trait::async_trait;
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::{json, Value};
use uuid::Uuid;

pub fn sidecar_base_url() -> String {
    std::env::var("GALVO_MEMORY_URL").unwrap_or_else(|_| "http://localhost:7575".to_string())
}

pub fn default_scope() -> String {
    std::env::var("CELEREX_MEMORY_SCOPE").unwrap_or_else(|_| "project:celerex".to_string())
}

/// Per-engagement memory scope. Honors `CELEREX_MEMORY_SCOPE` as an explicit
/// override (deliberately-shared single-tenant deployments); otherwise scopes
/// to the engagement so tenants' memory never pools in one global scope.
pub fn engagement_scope(engagement_id: Uuid) -> String {
    engagement_scope_with(
        engagement_id,
        std::env::var("CELEREX_MEMORY_SCOPE").ok().as_deref(),
    )
}

/// Pure core of [`engagement_scope`] with the override threaded explicitly, so
/// tests don't mutate `CELEREX_MEMORY_SCOPE` (no env race under the parallel
/// harness, no `unsafe`). A blank override is ignored.
pub fn engagement_scope_with(engagement_id: Uuid, override_scope: Option<&str>) -> String {
    match override_scope {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => format!("engagement:{engagement_id}"),
    }
}

/// Build the `:Session` node body posted by [`log_run_session`]. Extracted as a
/// pure helper so the per-engagement `scope` is unit-testable without exercising
/// the fire-and-forget `tokio::spawn` POST.
pub fn session_body(flow_name: &str, engagement_id: Uuid, run_id: Uuid, outcome: &str) -> Value {
    json!({
        "scope": engagement_scope(engagement_id),
        "title": format!("Flow run: {flow_name}"),
        "agent_tool": "celerex-flow",
        "task_description": format!("engagement={engagement_id} run_id={run_id}"),
        "outcome": outcome,
    })
}

pub struct MemoryHandler {
    client: reqwest::Client,
    base_url: String,
    default_scope: String,
}

impl MemoryHandler {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: sidecar_base_url(),
            default_scope: default_scope(),
        }
    }

    /// Construct against an explicit base URL + scope (used by tests).
    pub fn with(base_url: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            default_scope: scope.into(),
        }
    }
}

impl Default for MemoryHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn err(m: impl Into<String>) -> DispatchError {
    DispatchError::Handler(m.into())
}

#[async_trait]
impl Handler for MemoryHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let op = config.get("op").and_then(|v| v.as_str()).unwrap_or("search");
        // Scope precedence: an explicit `scope` on the node config wins; otherwise
        // derive a per-engagement scope from the run's injected `engagement_id`
        // (stamped by `apply_doc_engagement` in the run path) so a memory node
        // never recalls/writes across tenants; fall back to the handler default.
        let scope = match config
            .get("scope")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => config
                .get("engagement_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .map(engagement_scope)
                .unwrap_or_else(|| self.default_scope.clone()),
        };

        match op {
            "store" => {
                // `mem_label` (not `label`) — `label` is the node's display name
                // on the canvas, so the memory node type needs its own key.
                let label = config
                    .get("mem_label")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("memory store: missing `mem_label`"))?;
                let mut body = serde_json::Map::new();
                body.insert("scope".to_string(), json!(scope));
                // The sidecar forbids unknown fields, so pass through only the
                // caller-supplied per-label `fields` object (title/rationale/…).
                if let Some(Value::Object(fields)) = config.get("fields") {
                    for (k, v) in fields {
                        body.insert(k.clone(), v.clone());
                    }
                }
                let url = format!("{}/api/{}", self.base_url, label);
                let resp = self
                    .client
                    .post(&url)
                    .json(&Value::Object(body))
                    .send()
                    .await
                    .map_err(|e| err(format!("memory store: {e}")))?;
                let status = resp.status();
                let v: Value = resp
                    .json()
                    .await
                    .map_err(|e| err(format!("memory store decode: {e}")))?;
                if !status.is_success() {
                    return Err(err(format!("memory store {status}: {v}")));
                }
                Ok(v)
            }
            "search" => {
                let label = config.get("mem_label").and_then(|v| v.as_str()).unwrap_or("Entity");
                let q = config
                    .get("query")
                    .and_then(|v| v.as_str())
                    .or_else(|| inputs.get("query").and_then(|v| v.as_str()))
                    .unwrap_or("");
                let limit = config.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
                let threshold = config.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.7);
                let url = format!("{}/api/search/{}", self.base_url, label);
                let resp = self
                    .client
                    .get(&url)
                    .query(&[
                        ("q", q),
                        ("scope", scope.as_str()),
                        ("limit", &limit.to_string()),
                        ("threshold", &threshold.to_string()),
                    ])
                    .send()
                    .await
                    .map_err(|e| err(format!("memory search: {e}")))?;
                let status = resp.status();
                let hits: Value = resp
                    .json()
                    .await
                    .map_err(|e| err(format!("memory search decode: {e}")))?;
                if !status.is_success() {
                    return Err(err(format!("memory search {status}: {hits}")));
                }
                let count = hits.as_array().map(|a| a.len()).unwrap_or(0);
                Ok(json!({ "hits": hits, "count": count }))
            }
            other => Err(err(format!("memory: unknown op `{other}`"))),
        }
    }
}

/// Best-effort: record a completed flow run as a `:Session` memory node so the
/// Celerex workspace accrues memory of what it has run. Fire-and-forget — a
/// down sidecar never blocks or fails the run.
pub fn log_run_session(flow_name: String, engagement_id: Uuid, run_id: Uuid, outcome: &str) {
    let base = sidecar_base_url();
    let body = session_body(&flow_name, engagement_id, run_id, outcome);
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        if let Err(e) = client
            .post(format!("{base}/api/Session"))
            .json(&body)
            .send()
            .await
        {
            tracing::debug!(error=%e, "galvo-memory session log skipped (sidecar unreachable)");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use celerex_orchestrator::tool_dispatch::Handler;
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn engagement_scope_formats_and_overrides() {
        // No env mutation — the override is threaded explicitly, so this is
        // parallel-safe under the default harness.
        let e = Uuid::nil();
        assert_eq!(engagement_scope_with(e, None), format!("engagement:{e}"));

        // Explicit override wins (shared single-tenant deployments).
        assert_eq!(
            engagement_scope_with(e, Some("project:shared")),
            "project:shared"
        );

        // Blank/whitespace override is ignored — falls back to the engagement form.
        assert_eq!(
            engagement_scope_with(e, Some("   ")),
            format!("engagement:{e}")
        );
    }

    #[test]
    fn session_body_scopes_to_distinct_engagements() {
        let eng_a = Uuid::new_v4();
        let eng_b = Uuid::new_v4();
        let run = Uuid::new_v4();

        // Distinct engagements must never share a scope (the run-history leak
        // this fixes). Build via engagement_scope_with so the assertion holds
        // regardless of any ambient CELEREX_MEMORY_SCOPE in the test process.
        let scope_a = engagement_scope_with(eng_a, None);
        let scope_b = engagement_scope_with(eng_b, None);
        assert_eq!(scope_a, format!("engagement:{eng_a}"));
        assert_eq!(scope_b, format!("engagement:{eng_b}"));
        assert_ne!(scope_a, scope_b);

        // And session_body threads that scope into the posted node body.
        let body = session_body("flow", eng_a, run, "success");
        assert_eq!(body["scope"], scope_a);
    }

    #[tokio::test]
    async fn store_scopes_to_injected_engagement_id() {
        // A memory `store` node with the run's injected engagement_id (no explicit
        // `scope`) must post `scope = engagement:{id}` — not the handler default.
        let server = MockServer::start().await;
        let eng = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path("/api/Decision"))
            .and(body_partial_json(
                json!({ "scope": format!("engagement:{eng}") }),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "n1" })))
            .mount(&server)
            .await;

        let h = MemoryHandler::with(server.uri(), "project:fallback");
        let out = h
            .invoke(
                json!({
                    "op": "store",
                    "mem_label": "Decision",
                    "engagement_id": eng.to_string(),
                    "fields": { "title": "x" }
                }),
                Value::Null,
            )
            .await
            .unwrap();
        assert_eq!(out["id"], "n1");
    }

    #[tokio::test]
    async fn search_falls_back_to_default_scope_without_engagement() {
        // No injected engagement_id and no explicit scope → handler default.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/Entity"))
            .and(query_param("scope", "project:fallback"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let h = MemoryHandler::with(server.uri(), "project:fallback");
        let out = h
            .invoke(json!({ "op": "search", "query": "q" }), Value::Null)
            .await
            .unwrap();
        assert_eq!(out["count"], 0);
    }

    #[tokio::test]
    async fn store_posts_label_with_scope_and_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/Decision"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(json!({ "id": "node_abc", "scope": "project:celerex", "title": "Use Axum" })),
            )
            .mount(&server)
            .await;

        let h = MemoryHandler::with(server.uri(), "project:celerex");
        let out = h
            .invoke(
                json!({
                    "op": "store",
                    "mem_label": "Decision",
                    "fields": { "title": "Use Axum", "rationale": "ergonomics", "confidence": 0.9 }
                }),
                Value::Null,
            )
            .await
            .unwrap();
        assert_eq!(out["id"], "node_abc");
    }

    #[tokio::test]
    async fn search_hits_search_endpoint_and_wraps_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/Entity"))
            .and(query_param("scope", "project:celerex"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": "node_1", "_score": 0.92 },
                { "id": "node_2", "_score": 0.81 }
            ])))
            .mount(&server)
            .await;

        let h = MemoryHandler::with(server.uri(), "project:celerex");
        let out = h
            .invoke(json!({ "op": "search", "query": "axum patterns" }), Value::Null)
            .await
            .unwrap();
        assert_eq!(out["count"], 2);
        assert_eq!(out["hits"][0]["id"], "node_1");
    }

    #[tokio::test]
    async fn search_query_falls_back_to_inputs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/Pattern"))
            .and(query_param("q", "from inputs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;

        let h = MemoryHandler::with(server.uri(), "project:celerex");
        let out = h
            .invoke(
                json!({ "op": "search", "mem_label": "Pattern" }),
                json!({ "query": "from inputs" }),
            )
            .await
            .unwrap();
        assert_eq!(out["count"], 0);
    }
}
