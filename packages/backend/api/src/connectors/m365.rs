//! Microsoft 365 connector — publish Celerex content into Microsoft 365 Copilot
//! (via Graph connectors / `externalItem`s) and read Microsoft Graph data.
//!
//! Implements the flow from the "Build your first connector" guide:
//!   - `copilot_setup`   → create an external connection + register its schema
//!   - `copilot_publish` → ingest a document as an `externalItem` so Copilot can
//!                          reason over it (PUT …/items/{id})
//!   - `graph_get`       → generic authenticated Graph GET (mail / calendar /
//!                          SharePoint / Teams), for inbound flows
//!
//! Auth is the app-only client-credentials flow. Credentials come from the node
//! config or, failing that, the environment: `AZURE_TENANT_ID`,
//! `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`. The login/graph hosts are
//! overridable (config `login_base` / `graph_base`) so the OAuth + Graph calls
//! can be exercised against a mock in tests.

use async_trait::async_trait;
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::{json, Value};

const DEFAULT_LOGIN_BASE: &str = "https://login.microsoftonline.com";
const DEFAULT_GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default";

pub struct M365Handler {
    client: reqwest::Client,
}

impl M365Handler {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

impl Default for M365Handler {
    fn default() -> Self {
        Self::new()
    }
}

fn err(m: impl Into<String>) -> DispatchError {
    DispatchError::Handler(m.into())
}

/// Pull a credential from config, falling back to an env var.
fn cred(config: &Value, key: &str, env: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| std::env::var(env).ok().filter(|s| !s.is_empty()))
}

fn base(config: &Value, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn extract_text(inputs: &Value, config: &Value) -> String {
    if let Some(s) = config.get("content").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    crate::docs::handler::extract_text(inputs)
}

impl M365Handler {
    async fn token(&self, config: &Value) -> Result<String, DispatchError> {
        let tenant = cred(config, "tenant", "AZURE_TENANT_ID")
            .ok_or_else(|| err("m365: missing tenant (config.tenant or AZURE_TENANT_ID)"))?;
        let client_id = cred(config, "client_id", "AZURE_CLIENT_ID")
            .ok_or_else(|| err("m365: missing client_id (config.client_id or AZURE_CLIENT_ID)"))?;
        let client_secret = cred(config, "client_secret", "AZURE_CLIENT_SECRET")
            .ok_or_else(|| err("m365: missing client_secret (config.client_secret or AZURE_CLIENT_SECRET)"))?;
        let login_base = base(config, "login_base", DEFAULT_LOGIN_BASE);

        let url = format!("{login_base}/{tenant}/oauth2/v2.0/token");
        let resp = self
            .client
            .post(&url)
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("grant_type", "client_credentials"),
                ("scope", GRAPH_SCOPE),
            ])
            .send()
            .await
            .map_err(|e| err(format!("m365 token: {e}")))?;
        let status = resp.status();
        let v: Value = resp
            .json()
            .await
            .map_err(|e| err(format!("m365 token decode: {e}")))?;
        if !status.is_success() {
            return Err(err(format!("m365 token {status}: {v}")));
        }
        v.get("access_token")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .ok_or_else(|| err("m365 token: no access_token in response"))
    }
}

#[async_trait]
impl Handler for M365Handler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let op = config.get("op").and_then(|v| v.as_str()).unwrap_or("copilot_publish");
        let graph_base = base(&config, "graph_base", DEFAULT_GRAPH_BASE);
        let token = self.token(&config).await?;

        match op {
            "copilot_setup" => {
                let conn_id = config
                    .get("connection_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("m365 copilot_setup: missing `connection_id`"))?;
                let name = config.get("name").and_then(|v| v.as_str()).unwrap_or("Celerex");
                let description = config
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Celerex documents published to Microsoft 365 Copilot");

                // 1. Create the external connection.
                let create = self
                    .client
                    .post(format!("{graph_base}/external/connections"))
                    .bearer_auth(&token)
                    .json(&json!({ "id": conn_id, "name": name, "description": description }))
                    .send()
                    .await
                    .map_err(|e| err(format!("m365 create connection: {e}")))?;
                let create_status = create.status();
                let create_body: Value = create.json().await.unwrap_or(Value::Null);
                // 201 created or 409 already-exists are both fine.
                if !create_status.is_success() && create_status.as_u16() != 409 {
                    return Err(err(format!("m365 create connection {create_status}: {create_body}")));
                }

                // 2. Register the schema (title + url, searchable/retrievable).
                let schema = self
                    .client
                    .post(format!("{graph_base}/external/connections/{conn_id}/schema"))
                    .bearer_auth(&token)
                    .json(&json!({
                        "baseType": "microsoft.graph.externalItem",
                        "properties": [
                            { "name": "title", "type": "String", "isSearchable": true, "isQueryable": true, "isRetrievable": true, "labels": ["title"] },
                            { "name": "url", "type": "String", "isRetrievable": true, "labels": ["url"] }
                        ]
                    }))
                    .send()
                    .await
                    .map_err(|e| err(format!("m365 register schema: {e}")))?;
                let schema_status = schema.status();
                if !schema_status.is_success() && schema_status.as_u16() != 202 {
                    let b: Value = schema.json().await.unwrap_or(Value::Null);
                    return Err(err(format!("m365 register schema {schema_status}: {b}")));
                }
                Ok(json!({ "connection_id": conn_id, "schema_registration": "accepted" }))
            }
            "copilot_publish" => {
                let conn_id = config
                    .get("connection_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("m365 copilot_publish: missing `connection_id`"))?;
                let item_id = config
                    .get("item_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("m365 copilot_publish: missing `item_id`"))?;
                let title = config
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Celerex document");
                let url_prop = config.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let content = extract_text(&inputs, &config);

                let resp = self
                    .client
                    .put(format!("{graph_base}/external/connections/{conn_id}/items/{item_id}"))
                    .bearer_auth(&token)
                    .json(&json!({
                        "acl": [{ "type": "everyone", "value": "everyone", "accessType": "grant" }],
                        "properties": { "title": title, "url": url_prop },
                        "content": { "value": content, "type": "text" }
                    }))
                    .send()
                    .await
                    .map_err(|e| err(format!("m365 publish item: {e}")))?;
                let status = resp.status();
                if !status.is_success() {
                    let b: Value = resp.json().await.unwrap_or(Value::Null);
                    return Err(err(format!("m365 publish item {status}: {b}")));
                }
                Ok(json!({
                    "connection_id": conn_id,
                    "item_id": item_id,
                    "published": true,
                    "chars": content.chars().count(),
                }))
            }
            "graph_get" => {
                let path = config
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err("m365 graph_get: missing `path`"))?;
                let path = path.trim_start_matches('/');
                let resp = self
                    .client
                    .get(format!("{graph_base}/{path}"))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .map_err(|e| err(format!("m365 graph_get: {e}")))?;
                let status = resp.status();
                let v: Value = resp
                    .json()
                    .await
                    .map_err(|e| err(format!("m365 graph_get decode: {e}")))?;
                if !status.is_success() {
                    return Err(err(format!("m365 graph_get {status}: {v}")));
                }
                Ok(v)
            }
            other => Err(err(format!("m365: unknown op `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celerex_orchestrator::tool_dispatch::Handler;
    use wiremock::matchers::{body_partial_json, header, method, path as mpath};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn creds(server_uri: &str) -> Value {
        json!({
            "tenant": "tenant-123",
            "client_id": "client-abc",
            "client_secret": "secret-xyz",
            "login_base": server_uri,
            "graph_base": format!("{server_uri}/v1.0"),
        })
    }

    async fn mock_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(mpath("/tenant-123/oauth2/v2.0/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "access_token": "graph-token", "token_type": "Bearer" })),
            )
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn copilot_publish_gets_token_then_puts_item() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("PUT"))
            .and(mpath("/v1.0/external/connections/celerexdocs/items/doc1"))
            .and(header("authorization", "Bearer graph-token"))
            .and(body_partial_json(json!({ "properties": { "title": "Q3 Report" } })))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let mut cfg = creds(&server.uri());
        cfg["op"] = json!("copilot_publish");
        cfg["connection_id"] = json!("celerexdocs");
        cfg["item_id"] = json!("doc1");
        cfg["title"] = json!("Q3 Report");

        let h = M365Handler::new();
        let out = h
            .invoke(cfg, json!({ "markdown": "# Q3 Report\nNumbers up." }))
            .await
            .unwrap();
        assert_eq!(out["published"], true);
        assert_eq!(out["item_id"], "doc1");
    }

    #[tokio::test]
    async fn graph_get_authenticates_and_returns_json() {
        let server = MockServer::start().await;
        mock_token(&server).await;
        Mock::given(method("GET"))
            .and(mpath("/v1.0/users/me@example.com/messages"))
            .and(header("authorization", "Bearer graph-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [{ "subject": "Hi" }] })))
            .mount(&server)
            .await;

        let mut cfg = creds(&server.uri());
        cfg["op"] = json!("graph_get");
        cfg["path"] = json!("/users/me@example.com/messages");

        let h = M365Handler::new();
        let out = h.invoke(cfg, Value::Null).await.unwrap();
        assert_eq!(out["value"][0]["subject"], "Hi");
    }

    #[tokio::test]
    async fn missing_credentials_is_a_clear_error() {
        let h = M365Handler::new();
        let res = h.invoke(json!({ "op": "graph_get", "path": "/me" }), Value::Null).await;
        assert!(res.is_err());
    }
}
