# Multi-Provider LLM Registry

Shipped: 2026-05-19 — `a29df05` + `ae2861f`

## Problem

Sprint 0 hardcoded a single `Arc<dyn Provider>` on `LlmHandler`, with only `AnthropicClient` implementing it. That model conflicts with the product principle (per `celerex_multi_provider` memory) that Celerex is a multi-provider LLM platform from day one — each Project Member node picks its own provider/model so users can route reasoning work to Sonnet, long-context research to Kimi K2, OSW flows to OpenAI, etc.

## Design

```
config["provider_id"]  →  ProviderRegistry.get(id)  →  Provider::complete(...)
```

- **`ProviderRegistry`** (`celerex_orchestrator::llm::registry`) — `HashMap<String, Arc<dyn Provider>>` with `new()` / `register(id, provider)` / `get(id) -> Option<Arc<dyn Provider>>`.
- **`LlmHandler`** — holds `Arc<ProviderRegistry>`. On `invoke()`, reads `config["provider_id"]` (default `"default"`), looks up provider, errors with `"no provider '<id>'"` on miss. `LlmHandler::new(provider)` convenience constructor registers a single provider under `"default"` for backward-compat test call-sites.
- **Provider impls** — each provider lives in `api/src/llm/` and implements `celerex_orchestrator::llm::provider::Provider`. They share a constructor pattern: `from_env() -> anyhow::Result<Self>` (reads a per-provider env var) and `with_base_url(base_url, api_key) -> Self` (for wiremock).

## Wired providers

| ID | Crate file | API shape | Auth | Env var |
|---|---|---|---|---|
| `anthropic` / `default` | `api/src/llm/anthropic.rs` | `/v1/messages` | `x-api-key` + `anthropic-version` headers | `ANTHROPIC_API_KEY` |
| `openai` | `api/src/llm/openai.rs` | `/v1/chat/completions` | `Authorization: Bearer` | `OPENAI_API_KEY` |
| `kimi` | `api/src/llm/kimi.rs` | `/v1/chat/completions` (OpenAI-compatible) | `Authorization: Bearer` | `MOONSHOT_API_KEY` (fallback `KIMI_API_KEY`) |
| `gemini` | `api/src/llm/gemini.rs` | `/v1beta/models/{model}:generateContent?key=...` | URL param | `GEMINI_API_KEY` (fallback `GOOGLE_AI_API_KEY`) |

`build_app_with_store` populates the registry at startup. Each `if let Ok(client) = ...::from_env()` block is independently optional — missing keys are silently skipped, leaving the empty-registry case as a valid local-dev state (LLM steps then fail with `"no provider 'default'"` until at least one key is supplied).

## Test coverage

- **Orchestrator:** `tests/llm_handler.rs` — registry routing + convenience constructor
- **API (wiremock):** `tests/llm_anthropic.rs`, `tests/llm_openai.rs`, `tests/llm_kimi.rs`, `tests/llm_gemini.rs`
- **API (testcontainers):** `tests/provider.rs::registry_routes_by_provider_id`

## What's not done yet

- No streaming — all `complete()` impls are single POST/response.
- No retries or fallback chains.
- No real production traffic against any provider yet.
- No `/api/providers` introspection endpoint.
- `tools` arg ignored for Gemini.
- No 429 / rate-limit handling.

## Adding a new provider

1. Create `api/src/llm/<name>.rs` with `<Name>Provider { base_url, api_key, http }` plus `from_env()` and `with_base_url()`.
2. `impl Provider for <Name>Provider` — translate to the provider's request shape; return `Completion { raw }`.
3. Add `pub mod <name>;` to `api/src/llm/mod.rs`.
4. In `lib.rs::build_app_with_store`, `if let Ok(p) = <Name>Provider::from_env() { registry.register("<id>", Arc::new(p)); }`.
5. Add `tests/llm_<name>.rs` with wiremock coverage.
