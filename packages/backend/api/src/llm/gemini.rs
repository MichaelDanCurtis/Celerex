// Google Gemini provider — generateContent API.
// Auth is via `?key=<api_key>` URL param (no Bearer header).
// OpenAI-style `messages` are translated to Gemini's `contents` shape.

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde_json::{json, Value};

use celerex_orchestrator::llm::provider::{Completion, Provider};

pub struct GeminiProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl GeminiProvider {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_AI_API_KEY"))?;
        Ok(Self::with_base_url(
            "https://generativelanguage.googleapis.com".into(),
            key,
        ))
    }

    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self { base_url, api_key, http: reqwest::Client::new() }
    }
}

/// Translate OpenAI-shape messages (`[{"role":"user","content":"..."}, ...]`) to
/// Gemini's `contents` array. Gemini has no system role — system messages are
/// flattened into the first user turn.
fn messages_to_contents(messages: &Value) -> Vec<Value> {
    let arr = messages.as_array().cloned().unwrap_or_default();
    let mut system_prefix = String::new();
    let mut contents: Vec<Value> = Vec::new();

    for msg in arr {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        match role {
            "system" => {
                if !system_prefix.is_empty() {
                    system_prefix.push('\n');
                }
                system_prefix.push_str(&content);
            }
            "assistant" => {
                contents.push(json!({
                    "role": "model",
                    "parts": [{ "text": content }],
                }));
            }
            _ => {
                // "user" or any unknown role → user
                let text = if !system_prefix.is_empty() {
                    let combined = format!("{}\n{}", system_prefix, content);
                    system_prefix.clear();
                    combined
                } else {
                    content
                };
                contents.push(json!({
                    "role": "user",
                    "parts": [{ "text": text }],
                }));
            }
        }
    }

    // If we only had system messages with no user turn, flush them as a user turn.
    if !system_prefix.is_empty() {
        contents.push(json!({
            "role": "user",
            "parts": [{ "text": system_prefix }],
        }));
    }

    contents
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        _tools: Value,
    ) -> Result<Completion, String> {
        // TODO: translate `_tools` (OpenAI function-call schema) to Gemini's
        // `tools: [{functionDeclarations: [...]}]` format. Skipped for now.
        let contents = messages_to_contents(&messages);
        let body = json!({ "contents": contents });

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, model
        );
        let raw = self
            .http
            .post(url)
            .query(&[("key", &self.api_key)])
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<Value>()
            .await
            .map_err(|e| e.to_string())?;
        Ok(Completion { raw })
    }

    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: Value,
        _tools: Value,
    ) -> BoxStream<'a, Result<String, String>> {
        // Translate OpenAI-shape messages to Gemini's `contents` (same as
        // non-streaming `complete`). Tool translation is still skipped.
        let contents = messages_to_contents(&messages);
        let body = json!({ "contents": contents });
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent",
            self.base_url, model
        );
        let api_key = self.api_key.clone();
        let http = self.http.clone();

        // Stream that performs the POST, then yields text deltas as SSE data
        // lines arrive. Built with `stream::unfold` to mirror the Anthropic
        // and OpenAI patterns — no separate task, no `async-stream` crate.
        //
        // Gemini SSE has no terminal marker (unlike OpenAI's `[DONE]`); the
        // server simply closes the body when generation ends.
        enum Phase {
            Pending {
                http: reqwest::Client,
                url: String,
                api_key: String,
                body: Value,
            },
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
                            .query(&[("key", &api_key), ("alt", &"sse".to_string())])
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
                        match bytes.next().await {
                            None => return None,
                            Some(Err(e)) => return Some((Err(e.to_string()), None)),
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                                while let Some(idx) = buf.find('\n') {
                                    let line = buf[..idx].to_string();
                                    buf.drain(..=idx);
                                    if let Some(data) = line.strip_prefix("data: ") {
                                        let trimmed = data.trim();
                                        if trimmed.is_empty() {
                                            continue;
                                        }
                                        if let Ok(v) =
                                            serde_json::from_str::<Value>(trimmed)
                                        {
                                            if let Some(text) = v
                                                .get("candidates")
                                                .and_then(|c| c.get(0))
                                                .and_then(|c| c.get("content"))
                                                .and_then(|c| c.get("parts"))
                                                .and_then(|p| p.get(0))
                                                .and_then(|p| p.get("text"))
                                                .and_then(|t| t.as_str())
                                            {
                                                if !text.is_empty() {
                                                    pending_out
                                                        .push_back(text.to_string());
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
