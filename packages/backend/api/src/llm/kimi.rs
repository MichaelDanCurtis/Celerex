// Kimi K2 (Moonshot AI) provider — OpenAI-compatible chat completions API.
// Uses /v1/chat/completions with Bearer auth; base_url is overrideable for wiremock in tests.

use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde_json::{Value, json};

use celerex_orchestrator::llm::provider::{Completion, Provider};

pub struct KimiProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl KimiProvider {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("MOONSHOT_API_KEY").or_else(|_| std::env::var("KIMI_API_KEY"))?;
        Ok(Self::with_base_url("https://api.moonshot.cn".into(), key))
    }

    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for KimiProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let mut body = json!({ "model": model, "messages": messages });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = tools;
        }
        let raw = self
            .http
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        tools: Value,
    ) -> BoxStream<'a, Result<String, String>> {
        // Omit `tools` entirely when empty — some OpenAI-compatible providers
        // reject `"tools": []` (mirrors the `complete()` gate above).
        let mut body = json!({ "model": model, "messages": messages, "stream": true });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = tools;
        }
        let url = format!("{}/v1/chat/completions", self.base_url);
        let api_key = self.api_key.clone();
        let http = self.http.clone();

        // Kimi is OpenAI-compatible: same SSE shape with
        // `choices[0].delta.content` and a literal `data: [DONE]` terminator.
        // Mirrors the AnthropicClient/OpenAiProvider streaming pattern.
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
                            .header("Authorization", format!("Bearer {}", api_key))
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
                                        if trimmed == "[DONE]" {
                                            continue;
                                        }
                                        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                                            if let Some(text) = v
                                                .get("choices")
                                                .and_then(|c| c.get(0))
                                                .and_then(|c| c.get("delta"))
                                                .and_then(|d| d.get("content"))
                                                .and_then(|t| t.as_str())
                                            {
                                                if !text.is_empty() {
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
