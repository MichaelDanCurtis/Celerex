# GitHub Copilot OAuth (the "OAuth" LLM mode) — slice 1 design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`) + celerex-design (seam + daemon + web)
**Builds on:** the 3-mode LLM picker (CLI / BYOK / **OAuth**) from memory `omnivyx-architecture`; the per-user provider-key vault (2a / `engagement_provider_keys`); WorkOS login (B) which establishes the session that scopes everything. This is the **OAuth mode = GitHub Copilot**, server-side device flow.

## 1. Purpose

Let a user **"Log in with GitHub Copilot"** entirely server-side (device flow, the client is just a shell), so the Agent holds the credential and can use **Copilot as an LLM provider** — the third LLM-auth mode after CLI (desktop-only) and BYOK. Slice 1 = **device-flow login → token stored in the per-user vault → Copilot works as a selectable model.**

## 2. Locked decisions (from brainstorming + research)

- **Server-side device flow.** The Agent runs the GitHub device flow; the user approves a code in their browser; the Agent polls and stores the resulting **GitHub OAuth token (`ghu_…`)** in the per-user vault (`engagement_provider_keys`, provider id `github-copilot`), keyed to `claims.engagement_id`. The desktop/browser is just a shell that shows the code + polls status.
- **Editor client_id `Iv1.b507a08c87ecfe98`** (the well-known editor client used by Copilot CLI/editors) — exposes the full model catalog. Device flow uses it without a secret.
- **Two-step token at call time (the non-obvious part):** the stored `ghu_` token is NOT usable directly against Copilot. At call time the Agent exchanges it via `GET https://api.github.com/copilot_internal/v2/token` (header `Authorization: token <ghu_…>`) for a **short-lived (~30 min) Copilot bearer token**, caches it until ~expiry, and calls the Copilot chat endpoint. **Use the `endpoints.api` field from the exchange response** to pick the host (individual vs business) rather than hardcoding — kills the "404 for individual subscribers" class of bug. Required headers on the chat call: `Authorization: Bearer <copilot_token>`, `Copilot-Integration-Id` (e.g. `vscode-chat`), `Editor-Version`, `Editor-Plugin-Version`, matching `User-Agent`. The chat API is OpenAI-shaped (`POST {endpoints.api}/chat/completions`).
- **Copilot is a new provider** selectable in the picker's OAuth slot; it slots into the existing LLM-call path as another `Provider`-like backend. Its "credential" is the vaulted `ghu_` token (not an API key).
- **Per-user, server-held, separate layer.** Copilot creds live in our vault, scoped by engagement; never in WorkOS (identity ≠ provider credential). "Sign in with GitHub" (if ever offered as WorkOS identity) is a DIFFERENT integration and does NOT grant Copilot access.
- **Scope guardrail (slice 1):** device-flow login + store + **one working model** through Copilot (e.g. `gpt-4o`), surfaced as the OAuth provider. Model-list browsing, refresh/disconnect UI polish, business-vs-individual nuance beyond `endpoints.api`, and the conversational "pull from X" flows are later.

## 3. Key facts / research (current as of 2026-06)

- GitHub device flow: `POST https://github.com/login/device/code` (client_id + scope) → `{ device_code, user_code, verification_uri, interval, expires_in }`; poll `POST https://github.com/login/oauth/access_token` (client_id + device_code + `grant_type=urn:ietf:params:oauth:grant-type:device_code`) → `access_token` (`ghu_…`) or `{ error: authorization_pending | slow_down }`.
- Copilot token exchange: `GET https://api.github.com/copilot_internal/v2/token` with `Authorization: token <ghu_…>` → `{ token, expires_at, endpoints: { api, … }, … }`. The Copilot bearer is HMAC-signed + short-lived.
- Relevant gotchas (skills): `copilot-utility-models-shared-rate-limit` (Copilot utility models share a rate limit); the Codex analog `codex-cli-oauth-token-not-accepted-by-openai-api` (a similar "OAuth token rejected by the standard API, only works against the vendor's own endpoint" pattern — Copilot is the GitHub equivalent, hence the `copilot_internal/v2/token` exchange).
- Agent: provider keys via `state.provider_keys` (`engagement_provider_keys`, ChaCha20-Poly1305 AEAD); `reqwest` available; the LLM-call path is `EngagementLlmHandler` + the `Provider` trait (the per-engagement-key success path mirrored by the assistant in Stage-A T10). `AuthClaims` extractor → `claims.engagement_id`. Routes mounted in `lib.rs`.

## 4. Components (the established spine: Agent → seam → daemon → web)

### 4.1 Agent
1. **Device-flow endpoints** (DeviceAuth/AuthClaims, scoped to `claims.engagement_id`):
   - `POST /api/design/copilot/login/start` → calls GitHub device/code; returns `{ user_code, verification_uri, interval, expires_in }`; the Agent spawns a background task that polls GitHub's token endpoint (respecting `interval`/`slow_down`, bounded by `expires_in`) and, on success, stores the `ghu_` token in the vault (provider `github-copilot`) for the engagement. A per-engagement login-state map (or a small `copilot_logins` row) tracks `pending | connected | expired | denied`.
   - `GET /api/design/copilot/login/status` → `{ status }` for the caller's engagement.
   - `GET /api/design/copilot/status` → `{ connected: bool }` (does a vaulted `github-copilot` token exist).
   - (optional) `DELETE /api/design/copilot/credential`.
2. **`CopilotProvider`** (e.g. `src/llm/copilot.rs`): given the vaulted `ghu_` token, exchange → cache the Copilot token (with `expires_at`) → `POST {endpoints.api}/chat/completions` with the required headers; map to the existing `Provider`/completion shape. Register it so the LLM path + the workspace assistant can select `copilot` for an engagement that has a stored token.
3. JWKS-style caching: cache the exchanged Copilot token per engagement until ~`expires_at`; re-exchange when stale. (Per-call exchange is acceptable for the very first cut; cache is a fast follow.)
4. Mount routes in `lib.rs`.

### 4.2 Seam → daemon → web
- `@celerex/connect` `copilot.ts`: `startCopilotLogin`, `getCopilotLoginStatus`, `getCopilotStatus` (mirror the rituals/automations/deploy pattern).
- Daemon proxy `routes/copilot.ts` (`registerCopilotRoutes`) over the device pairing (403/409/502), beside the others.
- Web: in the model picker's **OAuth** section, a **"Log in with GitHub Copilot"** panel that shows the `user_code` + a link to `verification_uri`, polls status until **connected**, then lets the user pick **Copilot** as the model. Plain, non-technical copy + an "Ask AI"-style affordance is out of scope for slice 1.

## 5. Testing

- **Agent:** unit/integration with **stubbed GitHub + Copilot endpoints** (wiremock): device/code → user_code; token poll returns `authorization_pending` then `access_token`; the background poll stores the vaulted token; status flips to `connected`. `CopilotProvider`: stub `copilot_internal/v2/token` (returns a token + `endpoints.api` pointing at the stub) + `{api}/chat/completions` → assert the exchange + the headers + a completion. All endpoints stubbed (no real GitHub). Cross-engagement: A's stored token never used for B.
- **Seam:** bun tests mirroring `deploy.ts`/`automations.ts`.
- **Daemon:** vitest proxy tests (pairing read, 403/409/502).
- **Web:** component tests for the login panel (shows code, polls to connected) with mocked fetch.

## 6. Non-goals / later

- Model-list browsing / per-model selection beyond one default; token refresh/disconnect UI polish.
- Business-vs-individual handling beyond honoring `endpoints.api`.
- The conversational "pull this from <connector>" flows (the 2c/2h connector vision) — Copilot here is just the OAuth *model* provider.
- Codex/ChatGPT server-side OAuth (the sibling, harder case — separate slice; see `codex-cli-oauth-token-not-accepted-by-openai-api`).

## 7. Build order

Subagent-driven, branch + PR + merge per stage, each task spec-and-quality reviewed. Agent first (device-flow endpoints + background poll + CopilotProvider + stubbed-endpoint tests), then seam → daemon → web. Honors the desktop-shell + server-holds-keys model: the Copilot credential never leaves the server vault.

See memory `omnivyx-architecture` (2e provider-auth model + 3-mode picker), `celerex_multi_provider`, `celerex-multitenancy-audit` (vault scoping), and skills `copilot-utility-models-shared-rate-limit`, `codex-cli-oauth-token-not-accepted-by-openai-api`.
