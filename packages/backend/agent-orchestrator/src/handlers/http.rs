use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;

pub struct HttpHandler {
    client: reqwest::Client,
}

impl HttpHandler {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HttpHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Handler for HttpHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let url = config
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing url".into()))?;
        let method = config
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");
        let parsed_method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        let mut req = self.client.request(parsed_method, url);
        if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
        }
        if method != "GET" {
            req = req.json(&inputs);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        let json: Value = resp
            .json()
            .await
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json)
    }
}
