use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use celerex_orchestrator::llm::ProviderRegistry;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Value,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub provider_id: String,
    pub model: String,
    pub raw: Value,
}

pub async fn chat(
    State(state): State<crate::AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    let registry: Arc<ProviderRegistry> = state.providers_registry.clone();
    let provider_id = body.provider_id.unwrap_or_else(|| "default".to_string());
    let model = body.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let tools = body.tools.unwrap_or(Value::Array(vec![]));
    let provider = registry
        .get(&provider_id)
        .ok_or((StatusCode::BAD_REQUEST, format!("no provider '{provider_id}'")))?;
    let completion = provider
        .complete(&model, body.messages, tools)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(ChatResponse {
        provider_id,
        model,
        raw: completion.raw,
    }))
}

/// SSE streaming variant of `chat`. Yields named events:
/// - `delta`: text fragments as they arrive from the provider
/// - `error`: provider error (terminal — stream ends after this)
/// - `done`: terminal sentinel emitted when the provider stream completes
///
/// Browser KeepAlive ping at 15s to keep proxies/CDNs from closing idle conns.
pub async fn chat_stream(
    State(state): State<crate::AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let registry: Arc<ProviderRegistry> = state.providers_registry.clone();
    let provider_id = body.provider_id.unwrap_or_else(|| "default".to_string());
    let model = body.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let tools = body.tools.unwrap_or(Value::Array(vec![]));
    let provider = registry
        .get(&provider_id)
        .ok_or((StatusCode::BAD_REQUEST, format!("no provider '{provider_id}'")))?;

    // Drive the provider stream on a tokio task and forward chunks through a
    // bounded mpsc channel. This sidesteps the lifetime gymnastics that would
    // otherwise be needed (since `complete_stream` borrows from &self/&model)
    // and keeps the SSE response type a clean `impl Stream + 'static`.
    let messages = body.messages;
    let tx_provider = provider.clone();
    let tx_model = model.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let mut delta_stream =
            tx_provider.complete_stream(&tx_model, messages, tools);
        while let Some(chunk) = delta_stream.next().await {
            let evt = match chunk {
                Ok(text) => Event::default().event("delta").data(text),
                Err(e) => {
                    let evt = Event::default().event("error").data(e);
                    let _ = tx.send(Ok(evt)).await;
                    return;
                }
            };
            if tx.send(Ok(evt)).await.is_err() {
                // Receiver dropped (client disconnected).
                return;
            }
        }
        let done = Event::default().event("done").data("");
        let _ = tx.send(Ok(done)).await;
    });

    let sse_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
