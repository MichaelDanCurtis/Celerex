use crate::auth::extractors::DeviceAuth;
use crate::llm::{
    anthropic::AnthropicClient, gemini::GeminiProvider, kimi::KimiProvider, openai::OpenAiProvider,
};
use crate::providers::store::SUPPORTED_PROVIDERS;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use celerex_orchestrator::llm::{Provider, ProviderRegistry};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// True when the operator's shared "house" env key may be used as a fallback for
/// engagements that have no vaulted key. Default OFF — fail closed.
pub(crate) fn house_mode_enabled() -> bool {
    std::env::var("CELEREX_HOUSE_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Build a provider client from a per-engagement vaulted key, mirroring
/// `EngagementLlmHandler::build_provider` (real hosts at call time). Returns
/// None for a provider id that isn't a supported real provider.
fn build_engagement_provider(
    provider_keys: &Arc<dyn crate::providers::store::ProviderKeysStore>,
    engagement_id: Uuid,
    provider_id: &str,
    key: String,
) -> Option<Arc<dyn Provider>> {
    match provider_id {
        "anthropic" => Some(Arc::new(AnthropicClient::with_base_url(
            "https://api.anthropic.com".to_string(),
            key,
        ))),
        "openai" => Some(Arc::new(OpenAiProvider::with_base_url(
            "https://api.openai.com".to_string(),
            key,
        ))),
        "kimi" => Some(Arc::new(KimiProvider::with_base_url(
            "https://api.moonshot.cn".to_string(),
            key,
        ))),
        "gemini" => Some(Arc::new(GeminiProvider::with_base_url(
            "https://generativelanguage.googleapis.com".to_string(),
            key,
        ))),
        "copilot" => Some(Arc::new(crate::llm::copilot::CopilotProvider::new(key))),
        "codex" => crate::llm::codex::CodexProvider::from_blob(&key)
            .ok()
            .map(|p| {
                Arc::new(
                    p.with_persist(persist_blob::<crate::llm::codex::CodexCredential>(
                        provider_keys,
                        engagement_id,
                        "codex",
                    )),
                ) as Arc<dyn Provider>
            }),
        "xai" => crate::llm::xai::XaiProvider::from_blob(&key).ok().map(|p| {
            Arc::new(
                p.with_persist(persist_blob::<crate::llm::xai::XaiCredential>(
                    provider_keys,
                    engagement_id,
                    "xai",
                )),
            ) as Arc<dyn Provider>
        }),
        "kimi-code" => Some(Arc::new(
            crate::llm::openai_compat::OpenAiCompatProvider::new(
                "https://api.kimi.com/coding/v1".to_string(),
                key,
            ),
        )),
        "glm" => Some(Arc::new(
            crate::llm::openai_compat::OpenAiCompatProvider::new(
                "https://api.z.ai/api/coding/paas/v4".to_string(),
                key,
            ),
        )),
        _ => None,
    }
}

/// Build the write-back sink for an OAuth-blob provider (`xai`, `codex`). The
/// returned closure is handed to `XaiProvider::with_persist` /
/// `CodexProvider::with_persist`; on each token refresh it serializes the fresh
/// credential and writes it back to the per-engagement vault under `provider_id`,
/// so a rotated `refresh_token` survives to the next request instead of being
/// discarded.
///
/// The vault write (`ProviderKeysStore::set`) is async while the persist hook is
/// a sync `Fn`, so the write is spawned as a detached task (a tokio runtime is
/// always present — the hook fires inside an awaited `complete()`). A failed
/// write-back is non-fatal: the in-memory credential remains valid for this
/// instance and the next request re-refreshes from the vaulted token. No
/// credential material is ever logged.
fn persist_blob<C>(
    provider_keys: &Arc<dyn crate::providers::store::ProviderKeysStore>,
    engagement_id: Uuid,
    provider_id: &'static str,
) -> Box<dyn Fn(C) + Send + Sync>
where
    C: serde::Serialize + 'static,
{
    let store = provider_keys.clone();
    Box::new(move |cred: C| {
        let Ok(blob) = serde_json::to_string(&cred) else {
            return; // never log secret material
        };
        let store = store.clone();
        tokio::spawn(async move {
            let _ = store.set(engagement_id, provider_id, &blob).await;
        });
    })
}

/// Resolve the per-engagement vaulted key for `provider_id` (only for supported
/// real providers), returning the built provider client. Yields:
/// - `Ok(Some(provider))` — engagement key found and a client was built.
/// - `Ok(None)` — no engagement key (caller decides: house fallback or 409).
/// - `Err(..)` — key store failure.
pub(crate) async fn resolve_engagement_provider(
    state: &crate::AppState,
    engagement_id: Uuid,
    provider_id: &str,
) -> Result<Option<Arc<dyn Provider>>, (StatusCode, String)> {
    resolve_engagement_provider_with(&state.provider_keys, engagement_id, provider_id).await
}

/// AppState-free core of `resolve_engagement_provider`: resolve the per-engagement
/// vaulted key for `provider_id` against any `ProviderKeysStore`. Lets executors
/// (which only hold the key store + registry, not the full `AppState`) reuse the
/// exact engagement-key acquisition policy.
///
/// `pub` (not `pub(crate)`): the Task 11 gated live E2E test
/// (`tests/design_generation_live.rs`) is compiled as a separate crate and
/// needs to build a real provider from a real vaulted key without going
/// through a full `AppState`/HTTP route — this is the narrowest existing seam
/// that does that. `#[doc(hidden)]` keeps this test-only seam out of the
/// crate's rendered docs without narrowing the visibility the test needs
/// (`#[cfg(test)]` cannot reach a `tests/` integration binary).
#[doc(hidden)]
pub async fn resolve_engagement_provider_with(
    provider_keys: &Arc<dyn crate::providers::store::ProviderKeysStore>,
    engagement_id: Uuid,
    provider_id: &str,
) -> Result<Option<Arc<dyn Provider>>, (StatusCode, String)> {
    if !SUPPORTED_PROVIDERS.contains(&provider_id) {
        return Ok(None);
    }
    match provider_keys.get(engagement_id, provider_id).await {
        Ok(Some(key)) => Ok(build_engagement_provider(
            provider_keys,
            engagement_id,
            provider_id,
            key,
        )),
        Ok(None) => Ok(None),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

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
    a: DeviceAuth,
    State(state): State<crate::AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    let provider_id = body.provider_id.unwrap_or_else(|| "anthropic".to_string());
    let model = body
        .model
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let tools = body.tools.unwrap_or(Value::Array(vec![]));

    // Prefer the caller's engagement-scoped vaulted key. Only fall back to the
    // house registry when CELEREX_HOUSE_MODE is on; otherwise fail closed (409).
    let engagement_provider =
        resolve_engagement_provider(&state, a.0.engagement_id, &provider_id).await?;
    let completion = match engagement_provider {
        Some(provider) => provider
            .complete(&model, body.messages, tools)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e))?,
        None if house_mode_enabled() => {
            let registry: Arc<ProviderRegistry> = state.providers_registry.clone();
            let provider = registry.get(&provider_id).ok_or((
                StatusCode::BAD_REQUEST,
                format!("no provider '{provider_id}'"),
            ))?;
            provider
                .complete(&model, body.messages, tools)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, e))?
        }
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!("no key configured for provider '{provider_id}'"),
            ));
        }
    };
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
    a: DeviceAuth,
    State(state): State<crate::AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let provider_id = body.provider_id.unwrap_or_else(|| "anthropic".to_string());
    let model = body
        .model
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let tools = body.tools.unwrap_or(Value::Array(vec![]));

    // Same provider-acquisition policy as `chat`: engagement key first, house
    // registry only when CELEREX_HOUSE_MODE is on, else fail closed (409).
    let engagement_provider =
        resolve_engagement_provider(&state, a.0.engagement_id, &provider_id).await?;
    let provider: Arc<dyn Provider> = match engagement_provider {
        Some(provider) => provider,
        None if house_mode_enabled() => {
            let registry: Arc<ProviderRegistry> = state.providers_registry.clone();
            registry.get(&provider_id).ok_or((
                StatusCode::BAD_REQUEST,
                format!("no provider '{provider_id}'"),
            ))?
        }
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!("no key configured for provider '{provider_id}'"),
            ));
        }
    };

    // Drive the provider stream on a tokio task and forward chunks through a
    // bounded mpsc channel. This sidesteps the lifetime gymnastics that would
    // otherwise be needed (since `complete_stream` borrows from &self/&model)
    // and keeps the SSE response type a clean `impl Stream + 'static`.
    let messages = body.messages;
    let tx_provider = provider.clone();
    let tx_model = model.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let mut delta_stream = tx_provider.complete_stream(&tx_model, messages, tools);
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

#[cfg(test)]
mod persist_tests {
    use super::*;
    use crate::llm::codex::CodexCredential;
    use crate::llm::xai::XaiCredential;
    use crate::providers::store::{ProviderKeyMeta, ProviderKeysStore};
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    /// In-memory `ProviderKeysStore` that records `set` calls (plaintext) so a
    /// test can assert what the persist sink wrote back. No crypto, no DB.
    #[derive(Default)]
    struct RecordingStore {
        writes: StdMutex<Vec<(Uuid, String, String)>>,
    }

    #[async_trait]
    impl ProviderKeysStore for RecordingStore {
        async fn set(
            &self,
            engagement_id: Uuid,
            provider_id: &str,
            plaintext_key: &str,
        ) -> Result<ProviderKeyMeta, String> {
            self.writes.lock().unwrap().push((
                engagement_id,
                provider_id.to_string(),
                plaintext_key.to_string(),
            ));
            Ok(ProviderKeyMeta {
                provider_id: provider_id.to_string(),
                key_preview: "[rec]".into(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
        }
        async fn get(&self, _e: Uuid, _p: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        async fn list_for_engagement(&self, _e: Uuid) -> Result<Vec<ProviderKeyMeta>, String> {
            Ok(vec![])
        }
        async fn delete(&self, _e: Uuid, _p: &str) -> Result<bool, String> {
            Ok(false)
        }
    }

    // The persist sink serializes the fresh credential and writes it back to the
    // vault under the given provider id. Proves the exact seam that was missing.
    #[tokio::test]
    async fn persist_blob_writes_serialized_cred_under_provider_id() {
        let rec = Arc::new(RecordingStore::default());
        let store: Arc<dyn ProviderKeysStore> = rec.clone();
        let eng = Uuid::new_v4();
        let persist = persist_blob::<XaiCredential>(&store, eng, "xai");
        persist(XaiCredential {
            access_token: "at_new".into(),
            refresh_token: "rt_2".into(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
        });
        // The sink spawns a detached vault write; bounded-poll for it.
        let mut wrote = None;
        for _ in 0..40 {
            if let Some(w) = rec.writes.lock().unwrap().first().cloned() {
                wrote = Some(w);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let (e, pid, blob) = wrote.expect("persist_blob must write back to the vault");
        assert_eq!(e, eng);
        assert_eq!(pid, "xai");
        let round: XaiCredential = serde_json::from_str(&blob).unwrap();
        assert_eq!(round.refresh_token, "rt_2");
        assert_eq!(round.access_token, "at_new");
    }

    // Build-only proof that both blob arms accept the threaded store and still
    // build a provider (fresh blobs → from_blob succeeds, no refresh at build).
    #[tokio::test]
    async fn factory_arms_thread_store_and_build_blob_providers() {
        let rec = Arc::new(RecordingStore::default());
        let store: Arc<dyn ProviderKeysStore> = rec.clone();
        let eng = Uuid::new_v4();
        let fresh_xai = serde_json::to_string(&XaiCredential {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
        })
        .unwrap();
        assert!(
            build_engagement_provider(&store, eng, "xai", fresh_xai).is_some(),
            "xai arm must build a provider with the store threaded through"
        );
        let fresh_codex = serde_json::to_string(&CodexCredential {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            account_id: "acc".into(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
        })
        .unwrap();
        assert!(
            build_engagement_provider(&store, eng, "codex", fresh_codex).is_some(),
            "codex arm must build a provider with the store threaded through"
        );
    }
}
