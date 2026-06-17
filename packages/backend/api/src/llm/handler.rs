//! Engagement-aware LLM dispatch handler.
//!
//! This is what makes per-engagement provider keys REAL: when a flow's LLM node
//! names an explicit provider (`anthropic` / `openai` / `kimi` / `gemini`) and
//! the engagement has a stored key for it, the call uses that decrypted key —
//! built into a fresh provider client at call time. Otherwise it falls back to
//! the house registry (env-configured providers + `default`), preserving the
//! original behaviour and the `provider_fallback_ids` chain.
//!
//! The engagement id arrives via `config.engagement_id`, injected at run-launch
//! by `run_job::apply_engagement_id` (the id is non-secret; the key itself is
//! decrypted here, never placed in node config or persisted).

use crate::llm::{
    anthropic::AnthropicClient, gemini::GeminiProvider, kimi::KimiProvider, openai::OpenAiProvider,
};
use crate::providers::store::{ProviderKeysStore, SUPPORTED_PROVIDERS};
use async_trait::async_trait;
use celerex_orchestrator::llm::{Provider, ProviderRegistry};
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Per-provider API base URLs. Defaults to the real hosts; overridable so the
/// engagement-key path can be exercised against a mock in tests (and so a
/// self-hosted gateway can be pointed at in future).
#[derive(Debug, Clone)]
pub struct ProviderBases {
    pub anthropic: String,
    pub openai: String,
    pub kimi: String,
    pub gemini: String,
}

impl Default for ProviderBases {
    fn default() -> Self {
        Self {
            anthropic: "https://api.anthropic.com".into(),
            openai: "https://api.openai.com".into(),
            kimi: "https://api.moonshot.cn".into(),
            gemini: "https://generativelanguage.googleapis.com".into(),
        }
    }
}

pub struct EngagementLlmHandler {
    registry: Arc<ProviderRegistry>,
    keys: Arc<dyn ProviderKeysStore>,
    bases: ProviderBases,
}

impl EngagementLlmHandler {
    pub fn new(registry: Arc<ProviderRegistry>, keys: Arc<dyn ProviderKeysStore>) -> Self {
        Self {
            registry,
            keys,
            bases: ProviderBases::default(),
        }
    }

    pub fn with_bases(
        registry: Arc<ProviderRegistry>,
        keys: Arc<dyn ProviderKeysStore>,
        bases: ProviderBases,
    ) -> Self {
        Self {
            registry,
            keys,
            bases,
        }
    }

    fn build_provider(&self, provider_id: &str, key: String) -> Option<Arc<dyn Provider>> {
        match provider_id {
            "anthropic" => Some(Arc::new(AnthropicClient::with_base_url(
                self.bases.anthropic.clone(),
                key,
            ))),
            "openai" => Some(Arc::new(OpenAiProvider::with_base_url(
                self.bases.openai.clone(),
                key,
            ))),
            "kimi" => Some(Arc::new(KimiProvider::with_base_url(
                self.bases.kimi.clone(),
                key,
            ))),
            "gemini" => Some(Arc::new(GeminiProvider::with_base_url(
                self.bases.gemini.clone(),
                key,
            ))),
            "copilot" => Some(Arc::new(crate::llm::copilot::CopilotProvider::new(key))),
            _ => None,
        }
    }
}

#[async_trait]
impl Handler for EngagementLlmHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let provider_id = config
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs
            .get("messages")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        let mut last_err: Option<String> = None;

        // 1. Per-engagement key — only for an explicitly named real provider.
        if SUPPORTED_PROVIDERS.contains(&provider_id) {
            if let Some(eng) = config
                .get("engagement_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                match self.keys.get(eng, provider_id).await {
                    Ok(Some(key)) => {
                        if let Some(p) = self.build_provider(provider_id, key) {
                            match p.complete(model, messages.clone(), tools.clone()).await {
                                Ok(c) => return Ok(c.raw),
                                Err(e) => {
                                    last_err = Some(format!("engagement key '{provider_id}': {e}"))
                                }
                            }
                        }
                    }
                    Ok(None) => {} // no engagement key → house fallback
                    Err(e) => last_err = Some(format!("key store: {e}")),
                }
            }
        }

        // 2. House registry fallback + explicit fallback chain (original behaviour),
        // gated behind CELEREX_HOUSE_MODE (default OFF). With house mode off, a call
        // that found no per-engagement key fails closed instead of silently spending
        // the operator's shared env key (cross-tenant billing).
        let house_mode = std::env::var("CELEREX_HOUSE_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        if !house_mode {
            return Err(DispatchError::Handler(last_err.unwrap_or_else(|| {
                format!("no key configured for provider '{provider_id}'")
            })));
        }
        let fallback_ids: Vec<String> = config
            .get("provider_fallback_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let mut chain = vec![provider_id.to_string()];
        chain.extend(fallback_ids);
        for id in &chain {
            let Some(p) = self.registry.get(id) else {
                if last_err.is_none() {
                    last_err = Some(format!("no provider '{id}'"));
                }
                continue;
            };
            match p.complete(model, messages.clone(), tools.clone()).await {
                Ok(c) => return Ok(c.raw),
                Err(e) => last_err = Some(format!("provider '{id}': {e}")),
            }
        }
        Err(DispatchError::Handler(
            last_err.unwrap_or_else(|| "no providers attempted".into()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use celerex_orchestrator::llm::provider::Completion;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// In-memory keys store so the handler can be tested without Postgres.
    struct MemKeys(Mutex<HashMap<(Uuid, String), String>>);
    #[async_trait]
    impl ProviderKeysStore for MemKeys {
        async fn set(
            &self,
            e: Uuid,
            p: &str,
            k: &str,
        ) -> Result<crate::providers::store::ProviderKeyMeta, String> {
            self.0
                .lock()
                .unwrap()
                .insert((e, p.to_string()), k.to_string());
            Err("unused in tests".into())
        }
        async fn get(&self, e: Uuid, p: &str) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().get(&(e, p.to_string())).cloned())
        }
        async fn list_for_engagement(
            &self,
            _e: Uuid,
        ) -> Result<Vec<crate::providers::store::ProviderKeyMeta>, String> {
            Ok(vec![])
        }
        async fn delete(&self, _e: Uuid, _p: &str) -> Result<bool, String> {
            Ok(false)
        }
    }

    struct StubProvider;
    #[async_trait]
    impl Provider for StubProvider {
        async fn complete(&self, _m: &str, _msgs: Value, _t: Value) -> Result<Completion, String> {
            Ok(Completion {
                raw: json!({ "source": "house-registry" }),
            })
        }
    }

    #[tokio::test]
    async fn engagement_key_is_decrypted_and_sent_to_the_provider() {
        let server = MockServer::start().await;
        // The mock asserts the request carried the ENGAGEMENT's key, not an env one.
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-engagement-anthropic"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{ "type": "text", "text": "hi from the engagement key" }]
            })))
            .mount(&server)
            .await;

        let eng = Uuid::new_v4();
        let keys = MemKeys(Mutex::new(HashMap::from([(
            (eng, "anthropic".to_string()),
            "sk-engagement-anthropic".to_string(),
        )])));
        let bases = ProviderBases {
            anthropic: server.uri(),
            ..Default::default()
        };
        let handler = EngagementLlmHandler::with_bases(
            Arc::new(ProviderRegistry::new()),
            Arc::new(keys),
            bases,
        );

        let out = handler
            .invoke(
                json!({ "provider_id": "anthropic", "model": "claude-sonnet-4-6", "engagement_id": eng.to_string() }),
                json!({ "messages": [{ "role": "user", "content": "hi" }] }),
            )
            .await
            .expect("engagement-key completion");
        assert_eq!(out["content"][0]["text"], "hi from the engagement key");
    }

    #[tokio::test]
    async fn falls_back_to_house_registry_without_engagement_key() {
        // House fallback is gated behind CELEREX_HOUSE_MODE (default off).
        // SAFETY: set before the handler reads it; test asserts the house path.
        unsafe {
            std::env::set_var("CELEREX_HOUSE_MODE", "true");
        }
        let mut registry = ProviderRegistry::new();
        registry.register("default", Arc::new(StubProvider));
        let handler = EngagementLlmHandler::new(
            Arc::new(registry),
            Arc::new(MemKeys(Mutex::new(HashMap::new()))),
        );
        // provider_id "default" is never an engagement-key provider → house path.
        let out = handler
            .invoke(
                json!({ "provider_id": "default", "engagement_id": Uuid::new_v4().to_string() }),
                json!({ "messages": [] }),
            )
            .await
            .unwrap();
        assert_eq!(out["source"], "house-registry");
    }

    #[tokio::test]
    async fn named_provider_without_stored_key_falls_back_to_house() {
        // provider_id "anthropic" but no engagement key stored AND it's also
        // registered in the house registry → uses house, no error.
        // House fallback requires CELEREX_HOUSE_MODE on.
        // SAFETY: set before the handler reads it; test asserts the house path.
        unsafe {
            std::env::set_var("CELEREX_HOUSE_MODE", "true");
        }
        let mut registry = ProviderRegistry::new();
        registry.register("anthropic", Arc::new(StubProvider));
        let handler = EngagementLlmHandler::new(
            Arc::new(registry),
            Arc::new(MemKeys(Mutex::new(HashMap::new()))),
        );
        let out = handler
            .invoke(
                json!({ "provider_id": "anthropic", "engagement_id": Uuid::new_v4().to_string() }),
                json!({ "messages": [] }),
            )
            .await
            .unwrap();
        assert_eq!(out["source"], "house-registry");
    }
}
