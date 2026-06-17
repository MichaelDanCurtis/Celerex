# Codex / ChatGPT OAuth (the second "OAuth" LLM provider) — slice 1 design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`) + celerex-design (seam + daemon + web)
**Builds on:** the just-shipped GitHub Copilot OAuth slice (the OAuth mode of the CLI/BYOK/OAuth picker); the per-user provider-key vault (`engagement_provider_keys`); WorkOS login (the session that scopes everything). This is the **second OAuth-mode provider = OpenAI Codex / ChatGPT**, captured via a desktop-hosted OAuth loopback and used server-side thereafter.

## 1. Purpose

Let a user **"Connect ChatGPT (Codex)"** so the Agent can use their ChatGPT subscription as an LLM provider — the second OAuth-mode provider after GitHub Copilot. Slice 1 = **desktop-hosted OAuth login → tokens stored in the per-user vault → the Agent refreshes + calls the ChatGPT Codex backend → Codex works as a selectable model.**

## 2. The two ways Codex differs from Copilot (the crux)

1. **Login is NOT a server-side device flow.** ChatGPT/Codex OAuth is **PKCE authorization-code** against OpenAI's **first-party Codex OAuth client** (`app_EMo…`, the client_id the Codex CLI uses), whose redirect is locked to `http://localhost:1455/auth/callback`. Only that first-party client can reach the ChatGPT Codex backend, and it only redirects to localhost — so **the initial capture cannot be browser-only/server-side the way Copilot's device flow is. It must run on the desktop shell.** After capture, the Agent holds the `refresh_token` and stays logged in server-side on its own (refresh needs no redirect). **Decision (user, 2026-06-17): desktop-hosted OAuth loopback** — the daemon runs the full PKCE flow itself (no dependency on the Codex CLI being installed).

2. **The token only works against a non-standard endpoint with a non-obvious contract.** The Codex OAuth access token is a JWT with `aud: https://api.openai.com/v1` but is **rejected by `api.openai.com`** (`Missing scopes: model.request` / `api.responses.write`). It IS accepted by **`https://chatgpt.com/backend-api/codex/responses`** — a *Responses-API* surface (NOT chat/completions) — but only with: top-level **`instructions`** required (system items inside `input` are ignored), **`stream:true`** required, **`store:false`** required, **no** `max_tokens`/`max_output_tokens`, **Codex-flavored model slugs only** (`gpt-5.4-mini` etc., never `gpt-4o`/`gpt-5`), and a **`chatgpt-account-id`** header. See skill `codex-cli-oauth-token-not-accepted-by-openai-api` for the exact contract. This is documented and live-verified — the work is mechanical, just not a clone of `CopilotProvider`.

## 3. Key facts / research (current as of 2026-06)

- **Authorize:** `GET https://auth.openai.com/oauth/authorize?response_type=code&client_id=<codex client_id>&redirect_uri=http://localhost:1455/auth/callback&scope=openid%20profile%20email%20offline_access&code_challenge=<S256>&code_challenge_method=S256&state=<state>` (the Codex CLI adds a couple of product params; the core OAuth params above are what matter).
- **Token exchange:** `POST https://auth.openai.com/oauth/token` (`grant_type=authorization_code`, `code`, `code_verifier`, `redirect_uri`, `client_id`) → `{ access_token, refresh_token, id_token, … }`. The **`account_id`** is derived from the `id_token` claims (`chatgpt_account_id`).
- **Refresh:** `POST https://auth.openai.com/oauth/token` (`grant_type=refresh_token`, `refresh_token`, `client_id`) → a fresh `access_token` (+ rotated `refresh_token`). Access tokens live ~10 days (`exp - iat == 864000s`).
- **Call:** `POST https://chatgpt.com/backend-api/codex/responses`, headers `Authorization: Bearer <access_token>` + `chatgpt-account-id: <account_id>` + `Accept: text/event-stream`. Body = Responses shape with the quirks in §2.2. Response = standard Responses-API SSE (`response.output_text.delta` … `response.completed` with `usage`); tool calls emit `response.function_call_arguments.delta` then a `response.output_item.done` with the full `function_call`.
- **Agent primitives reused from the Copilot slice:** `state.provider_keys` vault (ChaCha20-Poly1305 AEAD), `AuthClaims`/DeviceAuth extractors → `claims.engagement_id`, the `Provider` trait (`async fn complete(&self, model, messages: Value, tools: Value) -> Result<Completion,String>` + defaulted `complete_stream`; `Completion { raw }`), `SUPPORTED_PROVIDERS` + the two per-engagement `build_provider` match arms, wiremock test pattern, base-URL-injectable client. The daemon's deploy/connectors proxy is the precedent for the daemon→Agent (pairing-token) upload.

## 4. Locked decisions

- **Desktop-hosted OAuth loopback** (the daemon runs PKCE + loopback + browser-open + exchange). Not the Codex-CLI-import variant (no CLI dependency); not a fully server-side flow (impossible — see §2.1).
- **Credential = a JSON blob in the existing vault** under provider id **`codex`**: `{ access_token, refresh_token, account_id, expires_at }`. The vault is an encrypted string store; a JSON blob drops in with no new table. (Copilot stored a single `ghu_` string; Codex needs three values.)
- **The daemon exchanges the code and uploads the finished blob** to the Agent (clean boundary: the redirect lands on the desktop's loopback, so the daemon already has the code + verifier; PKCE is a public client, no secret). The Agent stores + refreshes + calls thereafter.
- **The Agent owns refresh** — server holds the credential; ongoing use needs no desktop, only the one-time login.
- **`CodexProvider` de-streams.** The endpoint requires `stream:true`, but slice 1 implements the non-streaming `complete()`: consume the SSE internally and assemble one `Completion { raw }`. True streaming passthrough (`complete_stream`) is later.
- **Flow is web → daemon → Agent** (like deploy/connectors), NOT web → Agent-direct (like Copilot), because the desktop does the capture.
- **Scope guardrail (slice 1):** loopback login + vault store + Agent refresh + **one working Codex model** through the responses endpoint, surfaced as the `codex` OAuth provider. We can't read the user's `~/.codex/models_cache.json` (no CLI in the loopback path), so ship a sensible default slug + a tiny hardcoded fallback list. Per-account model browsing, `complete_stream`, and disconnect/refresh UI polish are later.

## 5. Components (Agent → seam → daemon → web)

### 5.1 Agent (`packages/backend/api`)
- **`src/llm/codex.rs`** — `CodexAuthClient` (token exchange + refresh, base-URL-injectable) + `CodexProvider`:
  1. Parse the vault blob; if `access_token` within ~60s of `expires_at`, refresh and write the new blob back to the vault (persist across calls); cache in-process (mirror `CopilotProvider`).
  2. Translate `complete(model, messages, tools)`: **promote system messages → top-level `instructions`** (non-empty default if none); map the rest → `input: [{type:"message", role, content:[{type:"input_text", text}]}]`; pass `tools` through; force `stream:true`, `store:false`; **omit** any max-tokens field; model = a Codex slug.
  3. `POST {chatgpt_base}/backend-api/codex/responses` with `Authorization: Bearer` + `chatgpt-account-id` + `Accept: text/event-stream`.
  4. **De-stream** the SSE: accumulate `response.output_text.delta`, capture `function_call` items + final `usage` from `response.completed`, assemble `Completion { raw }`.
- **`src/design/codex.rs`** — `POST /api/design/codex/credential` (DeviceAuth/pairing token: store the blob in the vault under `codex` for `claims.engagement_id`) + `GET /api/design/codex/status` → `{ connected }` (a vaulted `codex` blob exists). No background poll — the daemon captures synchronously then uploads once. Mount in `lib.rs` beside the copilot/deploy routes.
- **Register `codex`** in `SUPPORTED_PROVIDERS` + both `build_provider` arms (`assistant::build_engagement_provider`, `EngagementLlmHandler::build_provider`) — exactly like `copilot`.

### 5.2 Seam + daemon (celerex-design)
- `@celerex/connect` `codex.ts` — `startCodexLogin`, `getCodexStatus` (mirror the rituals/deploy seam helpers).
- Daemon `routes/codex.ts` (`registerCodexRoutes`) — generates PKCE (`code_verifier` + S256 `code_challenge`) + `state`, runs the loopback server on `localhost:1455/auth/callback`, opens the browser to `auth.openai.com/oauth/authorize`, captures `code` (validates `state`), exchanges at `auth.openai.com/oauth/token`, derives `account_id` from the `id_token`, uploads `{access_token, refresh_token, account_id, expires_at}` to `POST /api/design/codex/credential` via the app-config pairing token. The genuinely new piece is the loopback + PKCE; the upload mirrors the deploy proxy. Map errors plainly (403/409/502).

### 5.3 Web (`apps/web`)
- `state/codex.ts` — talks to the **daemon** (the desktop owns this flow): `startCodexLogin`, `getCodexStatus`.
- `CodexOAuthPanel` (in `components/byok/`, beside `CopilotOAuthPanel`) in the OAuth section — "Connect ChatGPT (Codex)", **greyed when `!daemonLive`** (reuse the gate the CLI segment uses), with a plain "Open the desktop app to connect ChatGPT" hint when in a browser. On click → `startCodexLogin` → poll `getCodexStatus` until `connected`. Mounted-ref unmount guard (as in `CopilotOAuthPanel`).

## 6. Data contracts (cross-layer)
- `POST /api/design/codex/credential` body `{ access_token, refresh_token, account_id, expires_at }` → `{ ok: true }` (Agent vaults under `codex`).
- `GET /api/design/codex/status` → `{ connected: bool }`.
- Daemon `POST /api/codex/login/start` → kicks off the loopback; returns `{ started: true }` (or a verification hint); daemon `GET /api/codex/status` proxies the Agent status.
- Web `state/codex.ts` calls the daemon endpoints above.

## 7. Testing
- **Agent:** wiremock-stubbed `auth.openai.com` (refresh) + `chatgpt.com` (responses SSE). `CodexProvider`: refresh-then-respond honoring the contract — assert the captured request has top-level `instructions`, `stream:true`, `store:false`, no max-tokens, the `chatgpt-account-id` header, and a Codex slug; assert the de-streamed `Completion`. `src/design/codex.rs`: upload→`connected`; cross-engagement isolation (A's blob never visible to B); unauth 401. Provider registration: `codex` built only when a blob is vaulted.
- **Seam:** bun tests mirroring `deploy.ts`/`rituals.ts`.
- **Daemon:** vitest with an injectable fetch + a fake loopback (no real browser); assert PKCE params, the `state` check, the exchange call shape, and the upload to the Agent.
- **Web:** component tests for `CodexOAuthPanel` (greyed without `daemonLive`; click → polls → connected) with mocked `state/codex.ts`.
- No live ChatGPT account required — everything stubbed, as Copilot was.

## 8. Non-goals / later
- Per-account model browsing (reading `models_cache.json` / the account's slug list) beyond one default + a tiny fallback list.
- `complete_stream` true streaming passthrough (slice 1 de-streams).
- The Codex-CLI-import capture variant (read an existing `~/.codex/auth.json`) — could be an alternate "Import" affordance later; the loopback is self-contained and was chosen for slice 1.
- Disconnect/refresh UI polish; business/Team/Enterprise model-set nuance.
- Claude Code as an OAuth provider — Anthropic supports only its own CLI for login, so that stays a desktop-CLI (CLI-mode) concern, not an OAuth-mode provider.

## 9. Build order
Subagent-driven, branch + PR + merge per stage, each task spec-and-quality reviewed (rust-reviewer for Agent — auth-critical; typescript-reviewer for seam/daemon/web). **Stage 1 (Agent):** `CodexAuthClient` + `CodexProvider` (+ de-stream + wiremock tests) → `design/codex.rs` credential/status endpoints → register `codex` provider; one Celerex branch+PR. **Stage 2 (celerex-design):** seam `codex.ts` → daemon `routes/codex.ts` (loopback+PKCE+upload) → web `state/codex.ts` + `CodexOAuthPanel`; one celerex-design branch+PR. The Codex credential never leaves the server vault after capture; the daemon holds it only transiently during the loopback exchange.

See memory `omnivyx-architecture` (2e provider-auth model + 3-mode picker), the Copilot slice (the sibling this mirrors), and skill `codex-cli-oauth-token-not-accepted-by-openai-api` (the exact request/refresh contract).
