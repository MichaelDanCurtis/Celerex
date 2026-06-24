use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Completion {
    pub raw: Value,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String>;

    /// Stream text deltas as they arrive from the LLM.
    ///
    /// Default implementation calls `complete` once and yields the full text
    /// as a single chunk, so any existing provider works without changes.
    /// Providers that support real streaming (e.g. Anthropic SSE) should
    /// override this method.
    fn complete_stream<'a>(
        &'a self,
        model: &'a str,
        messages: Value,
        tools: Value,
    ) -> BoxStream<'a, Result<String, String>> {
        let fut = async move {
            let c = self.complete(model, messages, tools).await?;
            // Extract text across provider response shapes. Providers that don't
            // override complete_stream (Copilot, Codex, OpenAI, Kimi, …) reach
            // here, so we must handle BOTH the Anthropic shape
            // ({ content: [{ text }] }) AND the OpenAI/Copilot shape
            // ({ choices: [{ message: { content } }] }) — otherwise the streamed
            // delta is empty and the chat UI shows a blank reply.
            let anthropic = c
                .raw
                .get("content")
                .and_then(|a| a.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .filter_map(|b| {
                            b.get("text")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string())
                        })
                        .reduce(|a, b| a + &b)
                });
            let openai = c
                .raw
                .get("choices")
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c0| c0.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
            let text = anthropic.or(openai).unwrap_or_default();
            Ok::<String, String>(text)
        };
        stream::once(fut).boxed()
    }
}
