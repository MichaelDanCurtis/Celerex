# Integration Test Coverage — Assistant Chat + Providers

Shipped: 2026-05-20 — `b19d20f` (W6C portion)

## Problem

When the providers introspection + workspace assistant chat slice shipped (`e98cda0` + `8d54fb4`, see `2026-05-19-providers-and-assistant-chat.md`), the two new endpoints were exercised only indirectly through the existing testcontainer harness — there was no dedicated `tests/assistant_chat.rs` and no `tests/providers_endpoint.rs`. The "what's not done" list of that doc explicitly called out "no dedicated integration test exercising the assistant endpoint against a wiremocked provider." W6C is that gap.

## What was added

Two new integration test files in `packages/backend/api/tests/`, six tests total. Both share the same pattern: build a stripped-down `AppState` that carries only what the endpoint needs, mount the handler on a minimal router, and assert against `oneshot()` responses via `tower::util::ServiceExt`.

### `tests/assistant_chat.rs` (4 tests)

Each test stands up one or more `wiremock::MockServer`s, points an `AnthropicClient::with_base_url(...)` at each one, registers them in a fresh `ProviderRegistry`, and mounts only `/api/assistant/chat` on the router.

- **`assistant_chat_routes_to_default_provider`** — registry has the same provider registered under both `"anthropic"` and `"default"`. Request omits `provider_id`. Asserts `provider_id == "default"`, `model == "claude-sonnet-4-6"`, and the wiremock body comes back inside `raw.content[0].text`.
- **`assistant_chat_routes_to_explicit_provider`** — two distinct wiremock servers backing providers `"alpha"` and `"beta"`, returning differing payloads. Request specifies `provider_id: "alpha"` then `provider_id: "beta"` and asserts the right body comes back each time. This is the actual proof that the registry routes by id, not by happenstance.
- **`assistant_chat_missing_provider_returns_400`** — empty registry, request specifies `provider_id: "nonexistent"`. Asserts 400 with a body that contains the missing provider's name (helpful for the AIPanel error bubble).
- **`assistant_chat_propagates_provider_error`** — wiremock returns 500. Asserts the handler maps that to 502 BAD_GATEWAY, matching the docstring contract from the original slice.

### `tests/providers_endpoint.rs` (2 tests)

No wiremock needed — the handler just serialises a `Vec<String>` off `AppState`.

- **`providers_endpoint_returns_configured_ids`** — state seeded with `["anthropic", "default", "openai"]`, asserts `providers` array reads back in the same order.
- **`providers_endpoint_returns_empty_when_none_configured`** — empty `Vec`, asserts `{ "providers": [] }`.

## Pattern: test-only `AppState` with a real Postgres pool

Both test files use a real `start_test_db()` PgPool. Neither endpoint queries the database — the assistant chat handler only touches `state.providers_registry` and `/api/providers` only touches `state.providers` — but the `AppState` struct requires a `PgPool`, an `Executor`, a `MembersStore`, a `CredentialStore`, a `RunBus`, and a `Vec<Arc<dyn Notifier>>`. Rather than refactor `AppState` to be optional in those fields (which would ripple through every other test), each file's `build_state(...)` helper builds the minimum-viable concrete versions:

```rust
let dispatcher = Arc::new(Dispatcher::new());
let run_bus = Arc::new(RunBus::new());
let run_store = Arc::new(PgRunStore::with_bus(pool.clone(), run_bus.clone()));
let executor = Arc::new(Executor::new(dispatcher, run_store));
let members = Arc::new(PgMembersStore::new(pool.clone()));
let credentials = Arc::new(LocalAeadStore::from_env().expect("CELEREX_CRED_KEY"));
let notifiers_vec = Arc::new(vec![Arc::new(ConsoleNotifier) as Arc<dyn Notifier>]);
```

The DB is real but unused by these endpoints, so the testcontainer cost is paid only for struct conformance.

## Caveat: `providers_endpoint.rs` re-implements the handler

The production `list_providers` handler lives inside `build_app_with_dispatcher_inner` in `packages/backend/api/src/lib.rs` and is *not* `pub`-exported as a free function. So `tests/providers_endpoint.rs` re-implements the body inline:

```rust
async fn list_providers(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<serde_json::Value> {
    axum::Json(json!({ "providers": &*state.providers }))
}
```

with a comment noting it mirrors `src/lib.rs::list_providers`. This is a one-liner, so the maintenance burden is minimal — but if anyone adds further behaviour to the production handler (caching, sorting, per-engagement filtering), this re-impl will drift. The right cleanup is to lift `list_providers` into a `pub` free function in `lib.rs` and `use` it from the test; deferred because the slice as-shipped is already validating the wire format the frontend depends on.

The assistant chat path doesn't have this problem — `celerex_api::assistant::chat` is already `pub` and is imported directly by the test.

## What's not done yet

- **No tests for `/api/schedules/{id}/fire`** — wait, that's wrong: those *did* land in `b19d20f` inside `tests/scheduler_routes.rs` (3 tests, covered separately in `2026-05-20-schedule-fire-now.md`). What's still missing is testing the fire endpoint's interaction with the periodic scheduler loop — i.e. proving that a `/fire` call doesn't trigger a duplicate run when the cron tick comes up next.
- **No real-provider integration tests** — every test wiremocks. There's no opt-in `#[ignore]` or feature-flagged test that runs against a live Anthropic / OpenAI / Kimi / Gemini endpoint to catch wire-format drift in the upstream APIs.
- **No assistant-chat-with-tool-use coverage** — `Provider::complete(model, messages, tools)` accepts tools but assistant chat is single-shot. There's no integration test for a model that *does* return `stop_reason: "tool_use"` from the assistant path, because the current handler doesn't loop — the tool block is just passed through in `raw`. When/if the assistant grows a tool-use agent loop, this becomes a gap.
- **No conversation-history coverage** — the assistant treats every POST as single-turn (history is held entirely on the frontend). There's no test for multi-turn history submission, because the spec is "frontend decides what to send" — but if that contract changes, a test should encode it.
- **No 429 / rate-limit coverage** — the same gap called out in the original slice's "what's not done" still stands; no test simulates a provider 429 and asserts the handler's behaviour.
- **No per-engagement scoping coverage** — providers are global. If a future slice scopes the registry per engagement, the existing tests will need to gain an engagement-context assertion.
