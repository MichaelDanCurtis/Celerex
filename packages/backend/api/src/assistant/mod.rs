use axum::{Json, extract::State, http::StatusCode};
use celerex_orchestrator::llm::ProviderRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

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
