//! Galvo federation connector — invoke a Galvo agent from a Celerex flow.
//!
//! Targets the Galvo Forge *control plane* (the shipping Pillar-1 surface),
//! not the heavyweight mTLS Hub. A Galvo instance serves its control plane at
//! `http://localhost:8787` by default (override via `GALVO_CONTROL_PLANE_URL`)
//! with a Bearer token (`GALVO_CONTROL_PLANE_TOKEN`).
//!
//! Op `invoke` → `POST /agents/{agent}/run` with `{ "input": … }` and returns
//! the run result (`output` / `events` / `cost` …) verbatim for downstream
//! nodes to template against.

use async_trait::async_trait;
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::{json, Value};

pub fn control_plane_url() -> String {
    std::env::var("GALVO_CONTROL_PLANE_URL").unwrap_or_else(|_| "http://localhost:8787".to_string())
}

pub fn control_plane_token() -> Option<String> {
    std::env::var("GALVO_CONTROL_PLANE_TOKEN").ok().filter(|s| !s.is_empty())
}

pub struct GalvoHandler {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl GalvoHandler {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: control_plane_url(),
            token: control_plane_token(),
        }
    }

    pub fn with(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            token,
        }
    }
}

impl Default for GalvoHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn err(m: impl Into<String>) -> DispatchError {
    DispatchError::Handler(m.into())
}

#[async_trait]
impl Handler for GalvoHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let op = config.get("op").and_then(|v| v.as_str()).unwrap_or("invoke");
        match op {
            "invoke" => {
                let agent = config
                    .get("agent")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("galvo invoke: missing `agent`"))?;
                // Input precedence: explicit config.input → upstream inputs.input
                // → the whole inputs payload (so a flow can just pipe data in).
                let input = config
                    .get("input")
                    .cloned()
                    .or_else(|| inputs.get("input").cloned())
                    .unwrap_or(inputs);
                let token = config
                    .get("token")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or_else(|| self.token.clone());

                let url = format!("{}/agents/{}/run", self.base_url, agent);
                let mut req = self.client.post(&url).json(&json!({ "input": input }));
                if let Some(t) = token {
                    req = req.bearer_auth(t);
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| err(format!("galvo invoke: {e} (is a Galvo control plane running at {}?)", self.base_url)))?;
                let status = resp.status();
                let v: Value = resp
                    .json()
                    .await
                    .map_err(|e| err(format!("galvo invoke decode: {e}")))?;
                if !status.is_success() {
                    return Err(err(format!("galvo invoke {status}: {v}")));
                }
                Ok(v)
            }
            other => Err(err(format!("galvo: unknown op `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celerex_orchestrator::tool_dispatch::Handler;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn invoke_posts_to_agent_run_with_bearer_and_input() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/agents/data_analyzer/run"))
            .and(header("authorization", "Bearer tok123"))
            .and(body_partial_json(json!({ "input": { "csv": "a,b" } })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "output": "2 columns", "cost": 0.01 })),
            )
            .mount(&server)
            .await;

        let h = GalvoHandler::with(server.uri(), Some("tok123".to_string()));
        let out = h
            .invoke(
                json!({ "op": "invoke", "agent": "data_analyzer", "input": { "csv": "a,b" } }),
                Value::Null,
            )
            .await
            .unwrap();
        assert_eq!(out["output"], "2 columns");
    }

    #[tokio::test]
    async fn invoke_pipes_upstream_inputs_when_no_explicit_input() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/agents/summarizer/run"))
            .and(body_partial_json(json!({ "input": { "text": "upstream" } })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "output": "ok" })))
            .mount(&server)
            .await;

        let h = GalvoHandler::with(server.uri(), None);
        let out = h
            .invoke(
                json!({ "op": "invoke", "agent": "summarizer" }),
                json!({ "text": "upstream" }),
            )
            .await
            .unwrap();
        assert_eq!(out["output"], "ok");
    }
}
