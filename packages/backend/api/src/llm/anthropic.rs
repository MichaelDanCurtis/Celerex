use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

#[async_trait]
pub trait ToolDispatch: Send + Sync {
    async fn invoke(&self, name: &str, input: Value) -> Result<Value, String>;
}

pub struct AnthropicClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        Ok(Self::with_base_url("https://api.anthropic.com".into(), key))
    }

    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self { base_url, api_key, http: reqwest::Client::new() }
    }

    pub async fn create_message(
        &self,
        model: &str,
        messages: Vec<Value>,
        tools: Vec<Value>,
    ) -> anyhow::Result<Value> {
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": messages,
            "tools": tools,
        });
        let resp = self.http.post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        Ok(resp.json::<Value>().await?)
    }

    pub async fn run_with_tools(
        &self,
        model: &str,
        mut messages: Vec<Value>,
        tools: Vec<Value>,
        dispatch: Arc<dyn ToolDispatch>,
    ) -> anyhow::Result<String> {
        for _ in 0..16 {
            let resp = self.create_message(model, messages.clone(), tools.clone()).await?;
            let stop = resp["stop_reason"].as_str().unwrap_or("");
            let content = resp["content"].as_array().cloned().unwrap_or_default();

            if stop == "end_turn" {
                let text = content.iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(text);
            }

            if stop == "tool_use" {
                messages.push(json!({ "role": "assistant", "content": content }));
                let mut results = Vec::new();
                for block in resp["content"].as_array().cloned().unwrap_or_default() {
                    if block["type"] == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input = block["input"].clone();
                        match dispatch.invoke(&name, input).await {
                            Ok(out) => results.push(json!({
                                "type": "tool_result",
                                "tool_use_id": id,
                                "content": out.to_string()
                            })),
                            Err(e) => results.push(json!({
                                "type": "tool_result",
                                "tool_use_id": id,
                                "is_error": true,
                                "content": e
                            })),
                        }
                    }
                }
                messages.push(json!({ "role": "user", "content": results }));
                continue;
            }

            anyhow::bail!("unexpected stop_reason: {stop}");
        }
        anyhow::bail!("tool loop exceeded 16 iterations")
    }
}

use celerex_orchestrator::llm::provider::{Completion, Provider};
use futures_util::stream::{self, BoxStream, StreamExt};

#[async_trait]
impl Provider for AnthropicClient {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let msgs = messages.as_array().cloned().unwrap_or_default();
        let tools = tools.as_array().cloned().unwrap_or_default();
        let raw = self
            .create_message(model, msgs, tools)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Completion { raw })
    }

    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: Value,
        tools: Value,
    ) -> BoxStream<'a, Result<String, String>> {
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": messages,
            "tools": tools,
            "stream": true,
        });
        let url = format!("{}/v1/messages", self.base_url);
        let api_key = self.api_key.clone();
        let http = self.http.clone();

        // Stream that performs the POST, then yields text deltas as SSE data
        // lines arrive. Built with `stream::unfold` so we don't have to spawn
        // a separate task and don't need the `async-stream` crate.
        //
        // State: `Option<UnfoldState>` — `None` means we're done.
        enum Phase {
            // Not yet connected; need to send the request.
            Pending {
                http: reqwest::Client,
                url: String,
                api_key: String,
                body: Value,
            },
            // Streaming chunks from the response body.
            Streaming {
                bytes: BoxStream<'static, reqwest::Result<bytes::Bytes>>,
                buf: String,
                pending_out: std::collections::VecDeque<String>,
            },
        }

        let initial = Some(Phase::Pending {
            http,
            url,
            api_key,
            body,
        });

        let s = stream::unfold(initial, |state| async move {
            let mut phase = state?;
            loop {
                match phase {
                    Phase::Pending {
                        http,
                        url,
                        api_key,
                        body,
                    } => {
                        let resp = match http
                            .post(&url)
                            .header("x-api-key", &api_key)
                            .header("anthropic-version", "2023-06-01")
                            .json(&body)
                            .send()
                            .await
                        {
                            Ok(r) => r,
                            Err(e) => {
                                return Some((Err(e.to_string()), None));
                            }
                        };
                        phase = Phase::Streaming {
                            bytes: resp.bytes_stream().boxed(),
                            buf: String::new(),
                            pending_out: std::collections::VecDeque::new(),
                        };
                    }
                    Phase::Streaming {
                        mut bytes,
                        mut buf,
                        mut pending_out,
                    } => {
                        // Drain any already-parsed deltas first.
                        if let Some(out) = pending_out.pop_front() {
                            return Some((
                                Ok(out),
                                Some(Phase::Streaming {
                                    bytes,
                                    buf,
                                    pending_out,
                                }),
                            ));
                        }
                        // Otherwise read more bytes and parse SSE lines.
                        match bytes.next().await {
                            None => return None,
                            Some(Err(e)) => return Some((Err(e.to_string()), None)),
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                                while let Some(idx) = buf.find('\n') {
                                    let line = buf[..idx].to_string();
                                    buf.drain(..=idx);
                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if let Ok(v) =
                                            serde_json::from_str::<Value>(data.trim())
                                        {
                                            if v.get("type").and_then(|t| t.as_str())
                                                == Some("content_block_delta")
                                            {
                                                if let Some(text) = v
                                                    .get("delta")
                                                    .and_then(|d| d.get("text"))
                                                    .and_then(|t| t.as_str())
                                                {
                                                    pending_out.push_back(text.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                                phase = Phase::Streaming {
                                    bytes,
                                    buf,
                                    pending_out,
                                };
                            }
                        }
                    }
                }
            }
        });

        s.boxed()
    }
}
