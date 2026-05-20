# Providers Introspection + Workspace Assistant Chat

Shipped: 2026-05-19 — `e98cda0` + `8d54fb4`

## Problem

After `a29df05` landed multi-provider routing for graph steps, the frontend still had no way to (a) discover which providers were actually configured on the backend (i.e. which env keys were populated at startup), or (b) talk to an LLM directly without paying the cost of standing up a Project Member, a graph, and a run. The workspace-assistant chat in the right-hand AIPanel needs a plain HTTP path to the same `ProviderRegistry` the executor uses, so an unloaded user with just a token can ask Claude/GPT/Kimi/Gemini a question and get a single response back.

## Design

Two endpoints, both sharing the registry that already feeds `LlmHandler`:

| Method | Path | Body | Response |
|---|---|---|---|
| `GET` | `/api/providers` | — | `{ "providers": ["anthropic", "default", "openai", ...] }` |
| `POST` | `/api/assistant/chat` | `{ messages, provider_id?, model?, tools? }` | `{ provider_id, model, raw: <provider body> }` |

### `AppState` wiring

`AppState` gained two new fields in `packages/backend/api/src/lib.rs`:

```rust
pub providers: Arc<Vec<String>>,
pub providers_registry: Arc<ProviderRegistry>,
```

`providers` is the ordered list of IDs that successfully registered at startup (each `if let Ok(p) = FooProvider::from_env()` block pushes its ID). `providers_registry` is the same `Arc<ProviderRegistry>` that `LlmHandler` holds — both the graph executor and the assistant chat handler resolve provider IDs through it, so there's a single source of truth for which providers exist and what they're called.

`build_app_with_dispatcher_inner` was extended to take `provider_ids: Vec<String>` and `providers_registry: Arc<ProviderRegistry>` so the test path (`build_app_with_dispatcher`) can supply an empty list + empty registry without forcing real keys into the environment.

### `/api/providers` handler

Trivial — just `Json(json!({ "providers": &*state.providers }))`. Order matches the registration order in `build_app_with_store`, so both `"anthropic"` and the duplicate `"default"` alias appear when an Anthropic key is set.

### `/api/assistant/chat` handler

Lives in `packages/backend/api/src/assistant/mod.rs`. Pulls the shared registry off `AppState`, defaults `provider_id` to `"default"` and `model` to `"claude-sonnet-4-6"`, defaults `tools` to `[]`, looks up the provider, and calls `Provider::complete(model, messages, tools)`.

Errors map cleanly:
- Unknown `provider_id` → `400 BAD_REQUEST` with body `"no provider '<id>'"`
- Provider returns an error → `502 BAD_GATEWAY` with the provider's error string

The response is the *raw* provider body (Anthropic shape, OpenAI shape, Gemini shape) wrapped with the resolved `provider_id` + `model`. No normalisation on the backend — the caller (currently the AIPanel) does shape detection in `extractReply()`.

### Frontend clients

Added to `packages/frontend/app/src/api/client.ts`:

```ts
assistantChat(token, messages, providerId?, model?) → /api/assistant/chat
listProviders(token) → /api/providers          // returns string[]
```

`listProviders` defensively reads `body.providers ?? []`. The legacy `sendChatMessage` (which pointed at the never-implemented `/api/chat`) was left in place — a follow-up slice swaps AIPanel over to `assistantChat`.

## Test coverage

The two endpoints are exercised by `tests/scheduler_routes.rs` setup paths (the routes wire into `build_app_with_store`, so the existing testcontainer harness hits them indirectly). No dedicated `tests/assistant_chat.rs` yet — see "what's not done."

## What's not done yet

- No streaming responses — `chat()` awaits the full provider body before returning.
- No conversation history persistence — every `POST /api/assistant/chat` is single-turn from the backend's perspective; history is held entirely in the frontend's `chatMessagesAtom`.
- No rate-limit / 429 handling on the assistant path (same gap as the executor LLM path).
- No per-engagement provider scoping — every authenticated user sees the same provider list and can call any registered provider.
- No dedicated integration test exercising the assistant endpoint against a wiremocked provider.
- AIPanel's `extractReply` covers Anthropic / OpenAI / Gemini shapes; Kimi works because it returns OpenAI-compatible JSON but isn't independently shape-tested.
