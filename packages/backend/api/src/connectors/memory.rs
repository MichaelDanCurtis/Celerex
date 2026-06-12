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
        let scope = config
            .get("scope")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.default_scope)
            .to_string();

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
    let scope = default_scope();
    let outcome = outcome.to_string();
    let body = json!({
        "scope": scope,
        "title": format!("Flow run: {flow_name}"),
        "agent_tool": "celerex-flow",
        "task_description": format!("engagement={engagement_id} run_id={run_id}"),
        "outcome": outcome,
    });
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
