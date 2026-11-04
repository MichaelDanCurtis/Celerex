# xAI / GLM / Kimi-Code Providers (Agent side) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add server-side support in the Rust Agent for three new model providers — Grok (`xai`, OAuth device-code), Kimi Code (`kimi-code`, API key), and GLM (`glm`, API key) — so the SPA's First-Run Provider Connect screen can connect them and server-side generation succeeds through the existing `/api/runs` and `/api/assistant/chat` paths.

**Architecture:** Mirror the shipped Copilot device-flow module (`design::copilot`) for `xai`: two endpoints (`start` + `status`) drive a background poller that seals an access+refresh+expiry token blob into the per-engagement encrypted vault under id `xai`. Add three arms to the provider factory `build_engagement_provider` in `assistant/mod.rs`. `xai` uses a new blob-backed provider (`llm/xai.rs`) that refreshes its token like `CodexProvider`, then delegates the chat call to a new generic OpenAI-compatible client (`llm/openai_compat.rs`) that posts `{base}/chat/completions` with the base URL carrying the full version path. `kimi-code` and `glm` reuse that same generic client with a static vaulted key. The vault allow-list (`SUPPORTED_PROVIDERS`) gains the three ids so the key routes and the resolver accept them.

**Tech Stack:** Rust 2024, axum, tokio, reqwest (rustls-tls), sqlx (runtime string queries + `sqlx::migrate!`), ring (ChaCha20-Poly1305 vault), chrono, serde_json. Tests: `wiremock` 0.6 (HTTP stubs), `axum-test` 17, `testcontainers` + `testcontainers-modules` (ephemeral Postgres), `jsonwebtoken`.

## Global Constraints

- Branch: create a new branch `feat/xai-glm-kimicode-providers` off `main` in the `Celerex` repo. Never commit to `main`.
- All changes additive; existing tests stay green. Do not modify `/api/runs` or `/api/assistant/chat` generation semantics — the new providers ride the existing paths.
- Rust 2024: wrap any `std::env::set_var` call in tests inside an `unsafe { }` block.
- Never print or commit secret values (vault contents, tokens, `CELEREX_CRED_KEY`, `CELEREX_JWT_SECRET`, WorkOS keys, provider keys). The xAI OAuth `client_id` is **public desktop-client metadata** and IS OK to hardcode.
- Pinned xAI OAuth `client_id` (public): `b1a00492-073a-47ea-816f-4c329264a828` (source: community flow `ysnock404/opencode-grok-auth`, `src/constants.ts`).
- xAI OIDC (verified live 2026-07-10): device endpoint `https://auth.x.ai/oauth2/device/code`, token endpoint `https://auth.x.ai/oauth2/token`, grant `urn:ietf:params:oauth:grant-type:device_code`, refresh grant `refresh_token`, scopes `openid profile email offline_access grok-cli:access api:access` (verified constant from opencode-grok-auth src/constants.ts — the string proven to work with this client_id).
- OpenAI-compatible chat bases (version path already included; client appends only `/chat/completions`): `xai` → `https://api.x.ai/v1`, `kimi-code` → `https://api.kimi.com/coding/v1`, `glm` → `https://api.z.ai/api/coding/paas/v4`.
- Test command: `cargo test -p celerex-api`. The suite spins up ephemeral Postgres via `testcontainers`, so a **running Docker daemon is required**; no `DATABASE_URL` is needed (all queries are runtime string queries; `sqlx::migrate!` only needs the migrations directory at compile time). Run a single integration test with `cargo test -p celerex-api --test <file_stem> -- --nocapture`; a single unit test with `cargo test -p celerex-api <name>`.
- Repo docs may be gitignored under `docs/superpowers/**`; that is fine — this plan file is not committed.

---

## File Structure

**New files:**
- `packages/backend/api/src/llm/openai_compat.rs` — generic OpenAI-compatible chat client. Posts `{base_url}/chat/completions` (base includes the version path), static bearer key, checks upstream status (returns `Err` on non-2xx). Used by the `kimi-code` and `glm` factory arms and, internally, by `XaiProvider`.
- `packages/backend/api/src/llm/xai.rs` — xAI OAuth device-flow client (`XaiOAuthClient`: `start_device_flow`, `poll_token`, `refresh`) + the vaulted-blob provider (`XaiProvider`: `from_blob`, token refresh-on-expiry, chat via `OpenAiCompatProvider`) + the `XaiCredential` blob type.
- `packages/backend/api/src/design/xai.rs` — HTTP handlers `POST /api/design/xai/login/start` and `GET /api/design/xai/login/status`, mirroring `design::copilot`. Spawns the poller that seals the `xai` blob.
- `packages/backend/api/src/db/migrations/0023_xai_logins.sql` — `xai_logins` device-flow status table (mirror of `copilot_logins`).
- `packages/backend/api/tests/xai_login.rs` — integration test for the device-flow endpoints (mirror of `tests/copilot_login.rs`).
- `packages/backend/api/tests/xai_provider.rs` — factory/allow-list integration test for all three new ids (mirror of `tests/copilot_provider.rs`).
- `packages/backend/api/tests/xai_live_e2e.rs` — `#[ignore]`d live end-to-end checks (manual, env-gated; never run in CI).

**Modified files:**
- `packages/backend/api/src/providers/store.rs:29-30` — add `"xai"`, `"kimi-code"`, `"glm"` to `SUPPORTED_PROVIDERS`.
- `packages/backend/api/src/llm/mod.rs:1-7` — add `pub mod openai_compat;` and `pub mod xai;`.
- `packages/backend/api/src/design/mod.rs:9-18` — add `pub mod xai;`.
- `packages/backend/api/src/db/mod.rs` (append after line 436) — add `upsert_xai_login`, `set_xai_login_status`, `get_xai_login` (mirror the copilot fns at lines 390-436).
- `packages/backend/api/src/lib.rs:169-170` — register the two `xai` login routes right after the codex routes.
- `packages/backend/api/src/assistant/mod.rs:32-56` — add `xai`, `kimi-code`, `glm` arms to `build_engagement_provider` (before the `_ => None` at line 54).

---

## Task 1: Vault allow-list adds `xai`, `kimi-code`, `glm`

**Files:**
- Modify: `packages/backend/api/src/providers/store.rs:29-30`
- Test: `packages/backend/api/src/providers/store.rs` (existing `#[cfg(test)] mod tests` at lines 246-267)

**Interfaces:**
- Produces: `pub const SUPPORTED_PROVIDERS: &[&str]` now contains `"xai"`, `"kimi-code"`, `"glm"`. This constant gates the `PUT/DELETE /api/providers/keys/{id}` routes (`providers/routes.rs:38`) and the engagement-key resolver (`assistant/mod.rs:80`). Consumed by Tasks 5 and 6.

Context: the current constant (store.rs:29-30) is
```rust
pub const SUPPORTED_PROVIDERS: &[&str] =
    &["anthropic", "openai", "kimi", "gemini", "copilot", "codex"];
```

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` block at the bottom of `packages/backend/api/src/providers/store.rs`:
```rust
    #[test]
    fn supported_providers_includes_new_first_run_ids() {
        for id in ["xai", "kimi-code", "glm"] {
            assert!(
                SUPPORTED_PROVIDERS.contains(&id),
                "{id} must be allow-listed for per-engagement keys"
            );
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api supported_providers_includes_new_first_run_ids`
Expected: FAIL — assertion `"xai must be allow-listed…"` panics (ids not yet in the array).

- [ ] **Step 3: Write minimal implementation**

Replace `packages/backend/api/src/providers/store.rs:29-30` with:
```rust
pub const SUPPORTED_PROVIDERS: &[&str] = &[
    "anthropic", "openai", "kimi", "gemini", "copilot", "codex", "xai", "kimi-code", "glm",
];
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api supported_providers_includes_new_first_run_ids`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/providers/store.rs
git commit -m "feat(providers): allow-list xai, kimi-code, glm provider keys"
```

---

## Task 2: Generic OpenAI-compatible client (`llm/openai_compat.rs`)

**Files:**
- Create: `packages/backend/api/src/llm/openai_compat.rs`
- Modify: `packages/backend/api/src/llm/mod.rs:1-7` (add `pub mod openai_compat;`)
- Test: `packages/backend/api/src/llm/openai_compat.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Produces: `pub struct OpenAiCompatProvider` with `pub fn new(base_url: String, api_key: String) -> Self`. Implements `celerex_orchestrator::llm::provider::Provider::complete`, posting to `{base_url}/chat/completions` with `Authorization: Bearer {api_key}`, returning `Err(String)` on non-2xx. `base_url` must already include the version path (e.g. `https://api.x.ai/v1`). Consumed by `XaiProvider` (Task 4) and the `kimi-code`/`glm` factory arms (Task 6).
- Consumes: the `Provider` trait (`celerex_orchestrator::llm::provider::{Completion, Provider}`). `complete_stream` has a default impl in the trait (`packages/backend/agent-orchestrator/src/llm/provider.rs:25`), so only `complete` is implemented — the same choice `CopilotProvider` and `CodexProvider` make.

- [ ] **Step 1: Write the failing test**

Create `packages/backend/api/src/llm/openai_compat.rs` containing ONLY the test module first (the impl is added in Step 3):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn posts_to_base_plus_chat_completions_and_returns_raw() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/coding/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&srv)
            .await;
        let base = format!("{}/coding/v1", srv.uri());
        let p = OpenAiCompatProvider::new(base, "sk-test".into());
        let out = p
            .complete("glm-4", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "ok");
    }

    #[tokio::test]
    async fn non_2xx_is_a_provider_error() {
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({ "error": "bad key" })),
            )
            .mount(&srv)
            .await;
        let base = format!("{}/v1", srv.uri());
        let p = OpenAiCompatProvider::new(base, "nope".into());
        let err = p
            .complete("grok", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap_err();
        assert!(err.contains("401"), "error should carry the upstream status: {err}");
    }
}
```

Add the module declaration. Edit `packages/backend/api/src/llm/mod.rs` — after `pub mod openai;` (line 7) add:
```rust
pub mod openai_compat;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib llm::openai_compat`
Expected: FAIL to compile — `cannot find type OpenAiCompatProvider`, `json!` unresolved.

- [ ] **Step 3: Write minimal implementation**

Prepend the implementation ABOVE the test module in `packages/backend/api/src/llm/openai_compat.rs`:
```rust
//! Generic OpenAI-compatible chat-completions provider.
//!
//! Unlike [`crate::llm::openai::OpenAiProvider`] (which hardcodes a
//! `/v1/chat/completions` suffix), this client posts to
//! `{base_url}/chat/completions` — the caller supplies the FULL base including
//! any version path segment. That lets one client serve endpoints whose version
//! path differs (`.../v1`, `.../coding/v1`, `.../paas/v4`). It also checks the
//! upstream status and returns `Err` on non-2xx, so a failed upstream surfaces
//! as a provider error (HTTP 502 at the handler) rather than a silent 200.

use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use serde_json::{json, Value};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl OpenAiCompatProvider {
    /// `base_url` must include the version path (e.g. `https://api.x.ai/v1`);
    /// the client appends `/chat/completions`.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let mut body = json!({ "model": model, "messages": messages });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            body["tools"] = tools;
        }
        let res = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("openai-compat request failed: {e}"))?;
        let status = res.status();
        let raw: Value = res
            .json()
            .await
            .map_err(|e| format!("openai-compat parse: {e}"))?;
        if !status.is_success() {
            return Err(format!("openai-compat status {status}: {raw}"));
        }
        Ok(Completion { raw })
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib llm::openai_compat`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/llm/openai_compat.rs packages/backend/api/src/llm/mod.rs
git commit -m "feat(llm): generic OpenAI-compatible client (base+/chat/completions, status-checked)"
```

---

## Task 3: xAI OAuth device-flow client (`llm/xai.rs` — client half)

**Files:**
- Create: `packages/backend/api/src/llm/xai.rs`
- Modify: `packages/backend/api/src/llm/mod.rs` (add `pub mod xai;`)
- Test: `packages/backend/api/src/llm/xai.rs` (inline `#[cfg(test)] mod device_tests`)

**Interfaces:**
- Produces:
  - `pub const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";`
  - `pub const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";`
  - `pub const XAI_API_BASE: &str = "https://api.x.ai/v1";`
  - `pub const DEFAULT_XAI_MODEL: &str = "grok-4.3";`
  - `pub struct DeviceCode { device_code, user_code, verification_uri, verification_uri_complete: Option<String>, interval: u64, expires_in: u64 }` (Deserialize).
  - `pub enum PollOutcome { Pending, SlowDown, Token(String, String, i64), Denied, Expired, Err(String) }` where `Token(access_token, refresh_token, expires_in_seconds)`.
  - `pub struct XaiOAuthClient` with `pub fn new()`, `pub fn with_base(auth_base: String)`, `pub async fn start_device_flow(&self) -> Result<DeviceCode, String>`, `pub async fn poll_token(&self, device_code: &str) -> PollOutcome`.
- Consumed by: `design::xai` (Task 5). The `refresh` method + `XaiProvider` are added to the same file in Task 4.

- [ ] **Step 1: Write the failing test**

Create `packages/backend/api/src/llm/xai.rs` containing ONLY the test module first:
```rust
#[cfg(test)]
mod device_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn device_flow_start_and_poll_success() {
        let auth = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dc",
                "user_code": "WXYZ-1234",
                "verification_uri": "https://x.ai/device",
                "verification_uri_complete": "https://x.ai/device?code=WXYZ-1234",
                "interval": 1,
                "expires_in": 900
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "xai_at",
                "refresh_token": "xai_rt",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        let c = XaiOAuthClient::with_base(auth.uri());
        let dc = c.start_device_flow().await.unwrap();
        assert_eq!(dc.user_code, "WXYZ-1234");
        assert_eq!(
            c.poll_token(&dc.device_code).await,
            PollOutcome::Token("xai_at".into(), "xai_rt".into(), 3600)
        );
    }

    #[tokio::test]
    async fn poll_maps_pending_and_denied() {
        let pending = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": "authorization_pending" })),
            )
            .mount(&pending)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(pending.uri()).poll_token("dc").await,
            PollOutcome::Pending
        );

        let denied = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({ "error": "access_denied" })),
            )
            .mount(&denied)
            .await;
        assert_eq!(
            XaiOAuthClient::with_base(denied.uri()).poll_token("dc").await,
            PollOutcome::Denied
        );
    }

    #[tokio::test]
    async fn client_id_is_the_pinned_public_value() {
        assert_eq!(XAI_CLIENT_ID, "b1a00492-073a-47ea-816f-4c329264a828");
    }
}
```

Add the module declaration. Edit `packages/backend/api/src/llm/mod.rs` — after the `pub mod openai_compat;` line added in Task 2, add:
```rust
pub mod xai;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib llm::xai::device_tests`
Expected: FAIL to compile — `cannot find XaiOAuthClient`, `PollOutcome`, `XAI_CLIENT_ID`, `json!`.

- [ ] **Step 3: Write minimal implementation**

Prepend ABOVE the test module in `packages/backend/api/src/llm/xai.rs`:
```rust
//! xAI (Grok) OAuth device-code flow + a vaulted-blob provider.
//!
//! Device flow mirrors `crate::llm::copilot::CopilotOAuthClient`; the blob-backed
//! `XaiProvider` (added in the provider half of this file) mirrors
//! `crate::llm::codex::CodexProvider` — refresh-on-expiry — but issues an
//! OpenAI-compatible chat call via `crate::llm::openai_compat::OpenAiCompatProvider`.
//!
//! The `client_id` below is PUBLIC desktop-client OAuth metadata (from the
//! community Grok flow `ysnock404/opencode-grok-auth`, `src/constants.ts`); it is
//! NOT a secret.

use serde::Deserialize;
use serde_json::{json, Value};

/// Public desktop-client OAuth client_id for the xAI (Grok) device flow.
pub const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
/// Device-flow scopes: coding + api access + a refresh token (offline_access).
pub const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
/// OpenAI-compatible chat base (version path included; client appends /chat/completions).
pub const XAI_API_BASE: &str = "https://api.x.ai/v1";
/// Default model when the caller sends an empty model string.
pub const DEFAULT_XAI_MODEL: &str = "grok-4.3";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
}
fn default_interval() -> u64 {
    5
}
fn default_expires() -> u64 {
    900
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    /// (access_token, refresh_token, expires_in seconds)
    Token(String, String, i64),
    Denied,
    Expired,
    Err(String),
}

/// Device-flow + refresh client. Base URL is injectable for tests.
pub struct XaiOAuthClient {
    /// e.g. https://auth.x.ai
    pub auth_base: String,
    http: reqwest::Client,
}

impl XaiOAuthClient {
    pub fn new() -> Self {
        Self::with_base("https://auth.x.ai".into())
    }
    pub fn with_base(auth_base: String) -> Self {
        Self {
            auth_base: auth_base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// RFC 8628 device authorization request (form-encoded).
    pub async fn start_device_flow(&self) -> Result<DeviceCode, String> {
        let res = self
            .http
            .post(format!("{}/oauth2/device/code", self.auth_base))
            .header("accept", "application/json")
            .form(&[("client_id", XAI_CLIENT_ID), ("scope", XAI_SCOPE)])
            .send()
            .await
            .map_err(|e| format!("device/code request failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("device/code status {}", res.status()));
        }
        res.json::<DeviceCode>()
            .await
            .map_err(|e| format!("device/code parse failed: {e}"))
    }

    /// Poll the token endpoint with the device_code grant. The token endpoint
    /// returns HTTP 400 with an `error` field while pending, so the body is
    /// parsed regardless of status (same approach as the Copilot poll).
    pub async fn poll_token(&self, device_code: &str) -> PollOutcome {
        let res = self
            .http
            .post(format!("{}/oauth2/token", self.auth_base))
            .header("accept", "application/json")
            .form(&[
                ("client_id", XAI_CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await;
        let res = match res {
            Ok(r) => r,
            Err(e) => return PollOutcome::Err(format!("poll failed: {e}")),
        };
        let body: Value = match res.json().await {
            Ok(v) => v,
            Err(e) => return PollOutcome::Err(format!("poll parse: {e}")),
        };
        if let Some(at) = body.get("access_token").and_then(|v| v.as_str()) {
            let rt = body
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let exp = body.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(3600);
            return PollOutcome::Token(at.to_string(), rt, exp);
        }
        match body.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => PollOutcome::Pending,
            Some("slow_down") => PollOutcome::SlowDown,
            Some("access_denied") => PollOutcome::Denied,
            Some("expired_token") => PollOutcome::Expired,
            other => PollOutcome::Err(format!("poll error: {other:?}")),
        }
    }
}

impl Default for XaiOAuthClient {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib llm::xai::device_tests`
Expected: PASS (all three tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/llm/xai.rs packages/backend/api/src/llm/mod.rs
git commit -m "feat(llm): xAI OAuth device-flow client (start + poll, pinned public client_id)"
```

---

## Task 4: xAI blob-backed provider (`llm/xai.rs` — provider half)

**Files:**
- Modify: `packages/backend/api/src/llm/xai.rs` (append `XaiCredential`, `RefreshResp`, `XaiOAuthClient::refresh`, `XaiProvider`, and a `#[cfg(test)] mod provider_tests`)
- Test: `packages/backend/api/src/llm/xai.rs` (inline `mod provider_tests`)

**Interfaces:**
- Consumes: `XaiOAuthClient`, `XAI_API_BASE`, `DEFAULT_XAI_MODEL` (Task 3); `crate::llm::openai_compat::OpenAiCompatProvider` (Task 2).
- Produces:
  - `pub struct XaiCredential { access_token: String, refresh_token: String, expires_at: i64 }` (Serialize + Deserialize) — the JSON blob sealed under vault id `xai`.
  - `XaiOAuthClient::refresh(&self, refresh_token: &str) -> Result<(String, Option<String>, i64), String>` returning `(access_token, rotated_refresh_token, expires_at_unix)`.
  - `pub struct XaiProvider` with `pub fn from_blob(blob: &str) -> Result<Self, String>`, `pub fn with_bases(cred: XaiCredential, auth_base: String, chat_base: String) -> Self`, `pub fn with_persist(self, f: Box<dyn Fn(XaiCredential) + Send + Sync>) -> Self`. Implements `Provider::complete`, refreshing the token when within 60s of expiry, then delegating to `OpenAiCompatProvider`. Consumed by the `xai` factory arm (Task 6) and the live E2E (Task 7).

- [ ] **Step 1: Write the failing test**

Append this test module to the END of `packages/backend/api/src/llm/xai.rs`:
```rust
#[cfg(test)]
mod provider_tests {
    use super::*;
    use celerex_orchestrator::llm::provider::Provider;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cred(exp_offset: i64) -> XaiCredential {
        XaiCredential {
            access_token: "at_old".into(),
            refresh_token: "rt_1".into(),
            expires_at: chrono::Utc::now().timestamp() + exp_offset,
        }
    }

    #[tokio::test]
    async fn refreshes_when_stale_then_completes() {
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "refresh_token": "rt_2",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer at_new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "pong" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        // expires_at in the past → must refresh before completing.
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base);
        let out = p
            .complete(
                "grok-4.3",
                json!([{"role":"user","content":"ping"}]),
                json!([]),
            )
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "pong");
    }

    #[tokio::test]
    async fn skips_refresh_when_fresh_and_defaults_model() {
        let auth = MockServer::start().await; // must NOT be called
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer at_old"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "hi" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let p = XaiProvider::with_bases(cred(3600), auth.uri(), chat_base);
        // empty model string → DEFAULT_XAI_MODEL is used (no assertion on it here,
        // just that the request succeeds without hitting the refresh endpoint).
        let out = p
            .complete("", json!([{"role":"user","content":"hi"}]), json!([]))
            .await
            .unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "hi");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --lib llm::xai::provider_tests`
Expected: FAIL to compile — `cannot find XaiCredential`, `XaiProvider`.

- [ ] **Step 3: Write minimal implementation**

Append this implementation to `packages/backend/api/src/llm/xai.rs` AFTER the `impl Default for XaiOAuthClient` block and BEFORE the `#[cfg(test)] mod device_tests` block (i.e. between the client half and the tests):
```rust
use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};
use std::sync::Mutex;

use crate::llm::openai_compat::OpenAiCompatProvider;

/// The vaulted xAI credential (stored as a JSON string under provider id `xai`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct XaiCredential {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at: i64, // unix seconds
}

#[derive(Debug, Deserialize)]
struct RefreshResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl XaiOAuthClient {
    /// Exchange a refresh_token for a fresh access_token (+ maybe a rotated
    /// refresh_token). Returns (access_token, rotated_refresh, expires_at_unix).
    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<(String, Option<String>, i64), String> {
        let res = self
            .http
            .post(format!("{}/oauth2/token", self.auth_base))
            .header("accept", "application/json")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", XAI_CLIENT_ID),
            ])
            .send()
            .await
            .map_err(|e| format!("xai refresh failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("xai refresh status {}", res.status()));
        }
        let r: RefreshResp = res
            .json()
            .await
            .map_err(|e| format!("xai refresh parse: {e}"))?;
        let exp = chrono::Utc::now().timestamp() + r.expires_in.unwrap_or(3600);
        Ok((r.access_token, r.refresh_token, exp))
    }
}

/// A per-engagement xAI (Grok) provider built from the vaulted credential blob.
/// `persist` is invoked after a refresh so the new blob is written back to the vault.
pub struct XaiProvider {
    cred: Mutex<XaiCredential>,
    auth: XaiOAuthClient,
    /// OpenAI-compatible chat base (version path included); real = XAI_API_BASE.
    chat_base: String,
    #[allow(clippy::type_complexity)]
    persist: Option<Box<dyn Fn(XaiCredential) + Send + Sync>>,
}

impl XaiProvider {
    /// Production: parse the vaulted JSON blob; chat at the real xAI base.
    pub fn from_blob(blob: &str) -> Result<Self, String> {
        let cred: XaiCredential =
            serde_json::from_str(blob).map_err(|e| format!("xai cred parse: {e}"))?;
        Ok(Self {
            cred: Mutex::new(cred),
            auth: XaiOAuthClient::new(),
            chat_base: XAI_API_BASE.to_string(),
            persist: None,
        })
    }
    /// Test factory: point refresh + chat at stubs.
    #[allow(dead_code)]
    pub fn with_bases(cred: XaiCredential, auth_base: String, chat_base: String) -> Self {
        Self {
            cred: Mutex::new(cred),
            auth: XaiOAuthClient::with_base(auth_base),
            chat_base: chat_base.trim_end_matches('/').to_string(),
            persist: None,
        }
    }
    #[allow(dead_code)]
    pub fn with_persist(mut self, f: Box<dyn Fn(XaiCredential) + Send + Sync>) -> Self {
        self.persist = Some(f);
        self
    }

    /// Return a valid access token, refreshing if within 60s of expiry. The
    /// credential mutex is never held across the `.await` on refresh.
    async fn access(&self) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp();
        let (access, refresh, exp) = {
            let c = self.cred.lock().unwrap();
            (c.access_token.clone(), c.refresh_token.clone(), c.expires_at)
        };
        if exp - 60 > now {
            return Ok(access);
        }
        let (new_access, new_refresh, new_exp) = self.auth.refresh(&refresh).await?;
        let updated = {
            let mut c = self.cred.lock().unwrap();
            c.access_token = new_access.clone();
            if let Some(rt) = new_refresh {
                c.refresh_token = rt;
            }
            c.expires_at = new_exp;
            c.clone()
        };
        if let Some(p) = &self.persist {
            p(updated);
        }
        Ok(new_access)
    }
}

#[async_trait]
impl Provider for XaiProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Value,
        tools: Value,
    ) -> Result<Completion, String> {
        let access = self.access().await?;
        let model = if model.is_empty() { DEFAULT_XAI_MODEL } else { model };
        OpenAiCompatProvider::new(self.chat_base.clone(), access)
            .complete(model, messages, tools)
            .await
    }
}
```

Note: `use async_trait::async_trait;`, `use ...::{Completion, Provider};`, `use std::sync::Mutex;`, and `use crate::llm::openai_compat::OpenAiCompatProvider;` are added mid-file here; that is valid Rust (item-position `use`). If the compiler warns about `use` placement style, the four `use` lines may instead be moved up beside the existing `use serde::Deserialize;` at the top of the file — behaviour is identical.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --lib llm::xai`
Expected: PASS (device_tests + provider_tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/llm/xai.rs
git commit -m "feat(llm): XaiProvider — vaulted blob, refresh-on-expiry, OpenAI-compatible chat"
```

---

## Task 5: `design::xai` device-flow endpoints + `xai_logins` table

**Files:**
- Create: `packages/backend/api/src/design/xai.rs`
- Create: `packages/backend/api/src/db/migrations/0023_xai_logins.sql`
- Modify: `packages/backend/api/src/design/mod.rs` (add `pub mod xai;`)
- Modify: `packages/backend/api/src/db/mod.rs` (append 3 fns after line 436)
- Modify: `packages/backend/api/src/lib.rs:170` (register 2 routes after the codex routes)
- Test: `packages/backend/api/tests/xai_login.rs`

**Interfaces:**
- Consumes: `crate::llm::xai::{PollOutcome, XaiCredential, XaiOAuthClient}` (Tasks 3-4); `crate::auth::extractors::AuthClaims`; `state.provider_keys` (`ProviderKeysStore`); `state.pool`.
- Produces:
  - DB fns in `crate::db`: `upsert_xai_login(pool, engagement_id, device_code, user_code, verification_uri, expires_at) -> sqlx::Result<()>`, `set_xai_login_status(pool, engagement_id, status) -> sqlx::Result<()>`, `get_xai_login(pool, engagement_id) -> sqlx::Result<Option<(String,String,String)>>` (user_code, verification_uri, status).
  - HTTP handlers `design::xai::start` and `design::xai::status`. `start` responds `{ user_code, verification_uri, verification_uri_complete, interval }` and spawns the poller that seals the `xai` blob. `status` responds `{ status, connected }`.
- Auth: routes use `AuthClaims` (full session token; scoped tokens rejected) — identical to the Copilot login routes.

Reference the mirror source verbatim: `packages/backend/api/src/design/copilot.rs` (whole file), the copilot DB fns at `packages/backend/api/src/db/mod.rs:390-436`, and the copilot table at `packages/backend/api/src/db/migrations/0020_copilot_logins.sql`.

- [ ] **Step 1: Write the failing test**

Create `packages/backend/api/tests/xai_login.rs`:
```rust
mod common;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(serde::Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn token(eng: Uuid) -> String {
    // SAFETY: single-process test; JWT secret matches start_test_db().
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    }
    let c = Claims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

#[tokio::test]
async fn xai_device_flow_vaults_blob_and_is_isolated() {
    // xAI OIDC device-flow mock: device/code then a token success.
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dev-123",
            "user_code": "WXYZ-1234",
            "verification_uri": "https://x.ai/device",
            "verification_uri_complete": "https://x.ai/device?code=WXYZ-1234",
            "interval": 1,
            "expires_in": 900
        })))
        .mount(&auth)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "xai_at",
            "refresh_token": "xai_rt",
            "expires_in": 3600
        })))
        .mount(&auth)
        .await;

    // SAFETY: single-process test; set before any handler builds the oauth client.
    unsafe {
        std::env::set_var("XAI_OAUTH_BASE", auth.uri());
    }

    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();

    let start = server
        .post("/api/design/xai/login/start")
        .add_header("authorization", format!("Bearer {}", token(a)))
        .await;
    start.assert_status_ok();
    assert_eq!(
        start.json::<serde_json::Value>()["user_code"],
        serde_json::json!("WXYZ-1234")
    );

    // Background poll (interval 1s) should vault the xai blob shortly. Bounded poll.
    let mut connected = false;
    for _ in 0..20 {
        let s = server
            .get("/api/design/xai/login/status")
            .add_header("authorization", format!("Bearer {}", token(a)))
            .await;
        s.assert_status_ok();
        if s.json::<serde_json::Value>()["connected"] == serde_json::json!(true) {
            connected = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(connected, "expected xai token blob to be vaulted for engagement A");

    // Cross-engagement isolation: B has no login row and no vaulted blob.
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let sb = server
        .get("/api/design/xai/login/status")
        .add_header("authorization", format!("Bearer {}", token(b)))
        .await;
    sb.assert_status_ok();
    let sbody = sb.json::<serde_json::Value>();
    assert_eq!(sbody["connected"], serde_json::json!(false));
    assert_eq!(sbody["status"], serde_json::json!("none"));

    // Unauthenticated start → 401.
    let anon = server.post("/api/design/xai/login/start").await;
    assert_eq!(anon.status_code(), axum::http::StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --test xai_login`
Expected: FAIL to compile — the `/api/design/xai/login/*` routes and `design::xai` module don't exist yet.

- [ ] **Step 3: Write minimal implementation**

3a. Create the migration `packages/backend/api/src/db/migrations/0023_xai_logins.sql`:
```sql
CREATE TABLE xai_logins (
    engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE,
    device_code TEXT NOT NULL,
    user_code TEXT NOT NULL,
    verification_uri TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

3b. Append the DB fns to `packages/backend/api/src/db/mod.rs` (after `get_copilot_login`, which ends at line 436):
```rust
pub async fn upsert_xai_login(
    pool: &PgPool,
    engagement_id: Uuid,
    device_code: &str,
    user_code: &str,
    verification_uri: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO xai_logins (engagement_id, device_code, user_code, verification_uri, status, expires_at)
         VALUES ($1,$2,$3,$4,'pending',$5)
         ON CONFLICT (engagement_id) DO UPDATE SET device_code=$2, user_code=$3, verification_uri=$4, status='pending', expires_at=$5, updated_at=NOW()",
    )
    .bind(engagement_id)
    .bind(device_code)
    .bind(user_code)
    .bind(verification_uri)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_xai_login_status(
    pool: &PgPool,
    engagement_id: Uuid,
    status: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE xai_logins SET status=$2, updated_at=NOW() WHERE engagement_id=$1")
        .bind(engagement_id)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_xai_login(
    pool: &PgPool,
    engagement_id: Uuid,
) -> sqlx::Result<Option<(String, String, String)>> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT user_code, verification_uri, status FROM xai_logins WHERE engagement_id=$1",
    )
    .bind(engagement_id)
    .fetch_optional(pool)
    .await
}
```

3c. Create `packages/backend/api/src/design/xai.rs`:
```rust
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::auth::extractors::AuthClaims;
use crate::llm::xai::{PollOutcome, XaiCredential, XaiOAuthClient};
use crate::{AppState, db};

fn oauth_client() -> XaiOAuthClient {
    match std::env::var("XAI_OAUTH_BASE") {
        Ok(b) => XaiOAuthClient::with_base(b),
        _ => XaiOAuthClient::new(),
    }
}

/// `POST /api/design/xai/login/start` — kick off the xAI device flow and spawn a
/// background poller that seals the resulting token blob (access + refresh +
/// expiry) under the `xai` provider id for this engagement.
pub async fn start(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let client = oauth_client();
    let dc = client
        .start_device_flow()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(dc.expires_in as i64);
    db::upsert_xai_login(
        &state.pool,
        claims.engagement_id,
        &dc.device_code,
        &dc.user_code,
        &dc.verification_uri,
        expires_at,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pool = state.pool.clone();
    let keys = state.provider_keys.clone();
    let eng = claims.engagement_id;
    let device_code = dc.device_code.clone();
    let interval = dc.interval.max(1);
    let expires_in = dc.expires_in;
    tokio::spawn(async move {
        let client = oauth_client();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_in);
        let mut wait = interval;
        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = db::set_xai_login_status(&pool, eng, "expired").await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            match client.poll_token(&device_code).await {
                PollOutcome::Token(access, refresh, ttl) => {
                    let cred = XaiCredential {
                        access_token: access,
                        refresh_token: refresh,
                        expires_at: chrono::Utc::now().timestamp() + ttl,
                    };
                    match serde_json::to_string(&cred) {
                        Ok(blob) if keys.set(eng, "xai", &blob).await.is_ok() => {
                            let _ = db::set_xai_login_status(&pool, eng, "connected").await;
                        }
                        _ => {
                            let _ = db::set_xai_login_status(&pool, eng, "error").await;
                        }
                    }
                    break;
                }
                PollOutcome::SlowDown => wait += 5,
                PollOutcome::Pending => {}
                PollOutcome::Denied => {
                    let _ = db::set_xai_login_status(&pool, eng, "denied").await;
                    break;
                }
                PollOutcome::Expired => {
                    let _ = db::set_xai_login_status(&pool, eng, "expired").await;
                    break;
                }
                PollOutcome::Err(_) => {
                    let _ = db::set_xai_login_status(&pool, eng, "error").await;
                    break;
                }
            }
        }
    });

    Ok(Json(json!({
        "user_code": dc.user_code,
        "verification_uri": dc.verification_uri,
        "verification_uri_complete": dc.verification_uri_complete,
        "interval": dc.interval,
    })))
}

/// `GET /api/design/xai/login/status` — report the device-flow status row plus
/// whether an `xai` token blob is actually vaulted for this engagement.
pub async fn status(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row = db::get_xai_login(&state.pool, claims.engagement_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let status = row.map(|(_, _, s)| s).unwrap_or_else(|| "none".to_string());
    let connected = state
        .provider_keys
        .get(claims.engagement_id, "xai")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "status": status, "connected": connected })))
}
```

3d. Register the module. In `packages/backend/api/src/design/mod.rs`, add alongside the other `pub mod` lines (e.g. after `pub mod slot_fill_handler;`):
```rust
pub mod xai;
```

3e. Register the routes. In `packages/backend/api/src/lib.rs`, immediately after line 170 (`.route("/api/design/codex/status", get(design::codex::status))`) add:
```rust
        .route("/api/design/xai/login/start", post(design::xai::start))
        .route("/api/design/xai/login/status", get(design::xai::status))
```
(`post` and `get` are already imported in `lib.rs`, as used by the copilot routes above.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --test xai_login`
Expected: PASS. (Requires a running Docker daemon for the ephemeral Postgres.)

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/xai.rs \
        packages/backend/api/src/design/mod.rs \
        packages/backend/api/src/db/migrations/0023_xai_logins.sql \
        packages/backend/api/src/db/mod.rs \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/xai_login.rs
git commit -m "feat(design): xai OAuth device-flow endpoints + xai_logins table"
```

---

## Task 6: Provider factory arms (`xai`, `kimi-code`, `glm`)

**Files:**
- Modify: `packages/backend/api/src/assistant/mod.rs:33-55` (add three arms before `_ => None` at line 54)
- Test: `packages/backend/api/tests/xai_provider.rs`

**Interfaces:**
- Consumes: `crate::llm::xai::XaiProvider::from_blob` (Task 4); `crate::llm::openai_compat::OpenAiCompatProvider::new` (Task 2); `SUPPORTED_PROVIDERS` (Task 1).
- Produces: `build_engagement_provider` now returns a built `Arc<dyn Provider>` for `xai` (valid blob), `kimi-code`, and `glm`. This makes those ids selectable through `POST /api/assistant/chat` and `POST /api/runs`.

Behaviour discriminator used by the test (identical to `tests/copilot_provider.rs`): with house mode OFF, the chat handler returns **409** when no provider is built (unknown id, unconfigured key, or an `xai` blob that fails to parse) and **502** when a provider IS built but its upstream call fails. Reaching the real provider host with a bogus token returns non-2xx → `Err` → 502; if the host is unreachable, `send()` errors → `Err` → 502 as well. So 502 reliably proves "the factory arm built a client" without any live credentials and regardless of network state.

- [ ] **Step 1: Write the failing test**

Create `packages/backend/api/tests/xai_provider.rs`:
```rust
//! Proves `xai`, `kimi-code`, and `glm` are registered, selectable per-engagement
//! providers. Strategy mirrors `tests/copilot_provider.rs`:
//!   - House mode OFF → the chat handler fails closed (409) when nothing is built.
//!   - A vaulted key/blob for a supported id → the factory builds a client → the
//!     provider's real (unstubbed) upstream call fails → 502. A 502 (not 409/400)
//!     proves the arm exists. No live credentials are required.
//!   - An engagement with no key → nothing built → 409.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use celerex_api::AppState;
use celerex_api::notifiers::{self, Notifier};
use celerex_api::project_members::store::{MembersStore, PgMembersStore};
use celerex_api::providers::store::{PgProviderKeysStore, ProviderKeysStore};
use celerex_api::runs::bus::RunBus;
use celerex_orchestrator::{executor::Executor, llm::ProviderRegistry, tool_dispatch::Dispatcher};
use serde_json::json;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

#[derive(serde::Serialize)]
struct TestClaims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

// SAFETY: tests run single-threaded at setup; JWT secret matches start_test_db().
// House mode forced OFF so "built" (502) is distinguishable from "not built" (409).
fn token(eng: Uuid) -> String {
    unsafe {
        std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
        std::env::set_var("CELEREX_HOUSE_MODE", "false");
    }
    let c = TestClaims {
        sub: "u".into(),
        engagement_id: eng.to_string(),
        project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &c,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

fn build_state(pool: PgPool, provider_keys: Arc<dyn ProviderKeysStore>) -> AppState {
    let dispatcher = Arc::new(Dispatcher::new());
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(celerex_api::runs::PgRunStore::with_bus(
        pool.clone(),
        run_bus.clone(),
    ));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));
    let credentials: Arc<dyn celerex_api::plugins::credentials::CredentialStore> = Arc::new(
        celerex_api::plugins::credentials::LocalAeadStore::from_env()
            .expect("CELEREX_CRED_KEY must be set"),
    );
    let notifiers_vec: Arc<Vec<Arc<dyn Notifier>>> =
        Arc::new(vec![Arc::new(notifiers::console::ConsoleNotifier)]);
    AppState {
        pool,
        credentials,
        executor,
        members,
        run_bus,
        notifiers: notifiers_vec,
        providers: Arc::new(Vec::new()),
        providers_registry: Arc::new(ProviderRegistry::new()),
        provider_keys,
        // NOTE: `AppState` gained a `design_runs` field (lib.rs:62) AFTER the
        // `copilot_provider.rs` helper was last written, so that older helper is
        // missing this field. It MUST be set or the struct literal won't compile.
        design_runs: celerex_api::runs::design_chat::new_registry(),
        hub_admin_engagements: Default::default(),
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/assistant/chat", post(celerex_api::assistant::chat))
        .with_state(state)
}

async fn chat(app: &Router, eng: Uuid, provider_id: &str) -> StatusCode {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant/chat")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(eng)))
                .body(Body::from(
                    json!({
                        "provider_id": provider_id,
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

#[tokio::test]
async fn new_providers_built_only_when_key_vaulted() {
    let db = common::start_test_db().await;
    let store = Arc::new(
        PgProviderKeysStore::from_env(db.pool.clone()).expect("CELEREX_CRED_KEY must be set"),
    );
    let provider_keys: Arc<dyn ProviderKeysStore> = store.clone();
    let app = build_router(build_state(db.pool.clone(), provider_keys));

    // xai: vault a VALID JSON blob with a fresh (unexpired) token so from_blob
    // parses and access() skips refresh, then the real api.x.ai call fails → 502.
    let xa: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('xa') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let blob = json!({
        "access_token": "xai_live_at",
        "refresh_token": "xai_live_rt",
        "expires_at": chrono::Utc::now().timestamp() + 3600
    })
    .to_string();
    store.set(xa, "xai", &blob).await.expect("vault xai blob");
    assert_eq!(
        chat(&app, xa, "xai").await,
        StatusCode::BAD_GATEWAY,
        "vaulted xai blob should build a provider and reach the upstream (502)"
    );

    // kimi-code: any non-empty key → built → real api.kimi.com call fails → 502.
    let kc: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('kc') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    store.set(kc, "kimi-code", "sk-kimi-test").await.expect("vault kimi-code key");
    assert_eq!(
        chat(&app, kc, "kimi-code").await,
        StatusCode::BAD_GATEWAY,
        "vaulted kimi-code key should build a provider and reach the upstream (502)"
    );

    // glm: any non-empty key → built → real api.z.ai call fails → 502.
    let gl: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('gl') RETURNING id")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    store.set(gl, "glm", "glm-test-key").await.expect("vault glm key");
    assert_eq!(
        chat(&app, gl, "glm").await,
        StatusCode::BAD_GATEWAY,
        "vaulted glm key should build a provider and reach the upstream (502)"
    );

    // no key for any id → nothing built, house mode off → fail closed (409).
    let none: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('none') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        chat(&app, none, "xai").await,
        StatusCode::CONFLICT,
        "engagement without an xai blob should build nothing and fail closed (409)"
    );
    assert_eq!(chat(&app, none, "glm").await, StatusCode::CONFLICT);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p celerex-api --test xai_provider`
Expected: FAIL — the xai/kimi-code/glm arms don't exist, so `build_engagement_provider` returns `None` for them → all three "built" assertions get 409 instead of 502.

- [ ] **Step 3: Write minimal implementation**

In `packages/backend/api/src/assistant/mod.rs`, insert these three arms immediately before `_ => None,` (currently line 54):
```rust
        "xai" => crate::llm::xai::XaiProvider::from_blob(&key)
            .ok()
            .map(|p| Arc::new(p) as Arc<dyn Provider>),
        "kimi-code" => Some(Arc::new(crate::llm::openai_compat::OpenAiCompatProvider::new(
            "https://api.kimi.com/coding/v1".to_string(),
            key,
        ))),
        "glm" => Some(Arc::new(crate::llm::openai_compat::OpenAiCompatProvider::new(
            "https://api.z.ai/api/coding/paas/v4".to_string(),
            key,
        ))),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p celerex-api --test xai_provider`
Expected: PASS. (Requires Docker for the ephemeral Postgres, plus outbound network to reach the real provider hosts; a bogus-token 401 OR an unreachable host both yield 502, so no credentials and no specific network state are required.)

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/assistant/mod.rs packages/backend/api/tests/xai_provider.rs
git commit -m "feat(assistant): provider factory arms for xai, kimi-code, glm"
```

---

## Task 7: Gated live E2E (manual, env-flagged; never in CI)

**Files:**
- Create: `packages/backend/api/tests/xai_live_e2e.rs`

**Interfaces:**
- Consumes: `celerex_api::llm::xai::XaiProvider::from_blob` (Task 4), `celerex_api::llm::openai_compat::OpenAiCompatProvider::new` (Task 2), the `Provider` trait.
- Produces: three `#[ignore]`d tests that hit the real provider endpoints with real credentials supplied via env. `#[ignore]` keeps them out of the default `cargo test` run, so CI (which has no credentials) never executes them.

- [ ] **Step 1: Write the (ignored) live tests**

Create `packages/backend/api/tests/xai_live_e2e.rs`:
```rust
//! Gated LIVE end-to-end checks against the REAL provider endpoints. These make
//! real network calls with real credentials and are `#[ignore]`d, so the default
//! `cargo test` (and CI) never runs them. Run manually, one provider at a time:
//!
//!   XAI_LIVE_BLOB='{"access_token":"...","refresh_token":"...","expires_at":0}' \
//!     cargo test -p celerex-api --test xai_live_e2e xai_live_completes -- --ignored --nocapture
//!   KIMI_CODE_API_KEY=sk-... \
//!     cargo test -p celerex-api --test xai_live_e2e kimi_code_live_completes -- --ignored --nocapture
//!   GLM_API_KEY=... \
//!     cargo test -p celerex-api --test xai_live_e2e glm_live_completes -- --ignored --nocapture
//!
//! Never commit real credentials. Model ids below may drift; adjust as needed.

use celerex_orchestrator::llm::provider::Provider;
use serde_json::json;

#[tokio::test]
#[ignore = "live: requires a real vaulted xAI token blob in XAI_LIVE_BLOB"]
async fn xai_live_completes() {
    let blob = std::env::var("XAI_LIVE_BLOB").expect("set XAI_LIVE_BLOB");
    let p = celerex_api::llm::xai::XaiProvider::from_blob(&blob).expect("blob parses");
    let out = p
        .complete(
            "grok-4.3",
            json!([{"role":"user","content":"reply with the single word pong"}]),
            json!([]),
        )
        .await
        .expect("live xai completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
#[ignore = "live: requires KIMI_CODE_API_KEY (Kimi for Coding plan)"]
async fn kimi_code_live_completes() {
    let key = std::env::var("KIMI_CODE_API_KEY").expect("set KIMI_CODE_API_KEY");
    let p = celerex_api::llm::openai_compat::OpenAiCompatProvider::new(
        "https://api.kimi.com/coding/v1".into(),
        key,
    );
    let out = p
        .complete(
            "kimi-k2.5",
            json!([{"role":"user","content":"say pong"}]),
            json!([]),
        )
        .await
        .expect("live kimi-code completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
#[ignore = "live: requires GLM_API_KEY (GLM Coding Plan key)"]
async fn glm_live_completes() {
    let key = std::env::var("GLM_API_KEY").expect("set GLM_API_KEY");
    let p = celerex_api::llm::openai_compat::OpenAiCompatProvider::new(
        "https://api.z.ai/api/coding/paas/v4".into(),
        key,
    );
    let out = p
        .complete(
            "glm-5.2",
            json!([{"role":"user","content":"say pong"}]),
            json!([]),
        )
        .await
        .expect("live glm completion");
    assert!(out.raw["choices"][0]["message"]["content"].is_string());
}
```

- [ ] **Step 2: Verify the file compiles and the tests are skipped by default**

Run: `cargo test -p celerex-api --test xai_live_e2e`
Expected: compiles; output shows `3 ignored` and `0 passed; 0 failed`. Do NOT run with `--ignored` in CI.

- [ ] **Step 3: (Optional, manual) Run one live check locally**

Only if you hold real credentials, e.g.:
```bash
GLM_API_KEY=... cargo test -p celerex-api --test xai_live_e2e glm_live_completes -- --ignored --nocapture
```
Expected: PASS against the real endpoint. If it fails with an auth error, confirm the key is a Coding-Plan key (GLM) / Kimi-for-Coding key, not a general-plan key.

- [ ] **Step 4: Commit**

```bash
git add packages/backend/api/tests/xai_live_e2e.rs
git commit -m "test(providers): gated live E2E for xai, kimi-code, glm (ignored by default)"
```

---

## Task 8: Persist rotated OAuth blobs to the vault (xai + codex)

**Runs after Task 6** (it edits the `xai` factory arm added there). Same branch `feat/xai-glm-kimicode-providers`.

**Files:**
- Modify: `packages/backend/api/src/assistant/mod.rs` — `build_engagement_provider` gains `provider_keys` + `engagement_id` params; the `xai` and `codex` arms attach `.with_persist(...)`; add a `persist_blob` helper; update the single caller (`resolve_engagement_provider_with`, currently line 84).
- Modify (tests only): `packages/backend/api/src/llm/xai.rs` — extend the existing `#[cfg(test)] mod provider_tests`.
- Modify (tests only): `packages/backend/api/src/llm/codex.rs` — extend the existing `#[cfg(test)] mod tests`.
- Test: `packages/backend/api/src/assistant/mod.rs` — new inline `#[cfg(test)] mod persist_tests`.

**Interfaces:**
- Consumes: `crate::llm::xai::XaiProvider::with_persist` (llm/xai.rs:228, shipped e9dfbaf) and `crate::llm::codex::CodexProvider::with_persist` (llm/codex.rs:116, already shipped); `XaiCredential`/`CodexCredential` (both `Serialize`, both fields `pub`); `crate::providers::store::ProviderKeysStore::set` (providers/store.rs:47 — the same async writer `design::xai`/`design::copilot` use to seal blobs); the store handle is `Arc<dyn ProviderKeysStore>` (cloneable, `Send + Sync`).
- Produces: `fn persist_blob<C: serde::Serialize + 'static>(&Arc<dyn ProviderKeysStore>, Uuid, &'static str) -> Box<dyn Fn(C) + Send + Sync>` — the vault-writing sink; and a `build_engagement_provider` that now attaches that sink to the `xai` and `codex` providers, so a refreshed/rotated blob is written back under the engagement's provider id instead of being discarded.

**The gap this closes:** `XaiProvider`/`CodexProvider` refresh their access token within 60s of expiry and expose `with_persist(...)`, but nothing wires the callback (`grep '\.with_persist(' packages/backend` → zero call-sites), so a refreshed/rotated blob is never written back. Every subsequent request re-refreshes from the *original* vaulted `refresh_token`; because `RefreshResp.refresh_token` is `Option<String>` (xai.rs:158-159, codex.rs:23-24 — rotation anticipated), once the provider rotates and retires the old token the stale vaulted token fails and the user must re-login.

**Constraints (grounded — comply with Global Constraints; do not restate them):**
- **Codex needs no new hook.** `CodexProvider::with_persist` already exists (llm/codex.rs:116-120) and is already fired after refresh (llm/codex.rs:147-149) — identical to xai (llm/xai.rs:228-232 / 255-257). Task 8 adds **no** production code to `codex.rs` (only a test); it wires the existing hook in the factory.
- **Wiring choice (grounded in the signatures):** thread `provider_keys` + `engagement_id` into `build_engagement_provider` so the `xai`/`codex` arms can attach the sink **before** the `Arc<dyn Provider>` type-erasure — that erasure lives *inside* the factory (`.map(|p| Arc::new(p) as Arc<dyn Provider>)`), and `with_persist` (a concrete method taking `self` by value) cannot be called on an already-erased `Arc<dyn Provider>`. A "special-case the caller" variant would have to duplicate the `from_blob` arms, so extending the factory is the smaller change. Only caller is assistant/mod.rs:84; both `provider_keys` and `engagement_id` are already in scope there.
- **No mutex held across an await.** The persist hook is a **sync** `Fn`, invoked in `access()` *after* the credential `Mutex` guard is dropped (xai.rs:246-257, codex.rs:138-149). `ProviderKeysStore::set` is `async`, so the write is performed on a detached `tokio::spawn` (a runtime is always present — the hook fires inside an awaited `complete()`), never inline under the lock. This preserves the file's existing lock-scope discipline.
- **Fire-and-forget, non-fatal, no secrets logged.** A failed write-back is swallowed (`let _ = store.set(...).await`): the in-memory credential stays valid for this provider instance and the next request/instance simply re-refreshes from the vaulted `refresh_token`. On a serialize failure the sink drops silently. No credential material is ever logged.
- **Coverage split (grounded limitation).** `XaiProvider::from_blob`/`CodexProvider::from_blob` bind the *real* auth base (`XaiOAuthClient::new()`/`CodexAuthClient::new()` — no injection seam), so a mocked refresh cannot be driven *through* `build_engagement_provider`; adding such a seam would violate "additive/minimal". Coverage is therefore split into two grounded halves: (1) the provider fires persist with the correct **NEW** blob — rotated when present, retained when absent — proven at the `llm/*` unit level via `with_bases` + wiremock (extends the shipped `provider_tests` idiom); (2) `persist_blob` writes that blob to the store under the right `(engagement_id, provider_id)` — proven at the `assistant` unit level via a fake `ProviderKeysStore`. Their junction is the two one-line arms, verified by a build-only assertion that the arms accept the store and return `Some`.

- [ ] **Step 1: Write the failing tests**

1a. Extend `#[cfg(test)] mod provider_tests` in `packages/backend/api/src/llm/xai.rs` (append these two tests inside the module; `cred`, `json!`, the wiremock imports, and `XaiProvider`/`XaiCredential` are already in scope there):
```rust
    #[tokio::test]
    async fn persists_rotated_refresh_token() {
        use std::sync::{Arc, Mutex};
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "refresh_token": "rt_2",
                "expires_in": 3600
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let seen: Arc<Mutex<Option<XaiCredential>>> = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        // stale cred (cred(-10)) → access() must refresh before completing.
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base)
            .with_persist(Box::new(move |c: XaiCredential| {
                *sink.lock().unwrap() = Some(c);
            }));
        p.complete("grok-4.3", json!([{"role":"user","content":"ping"}]), json!([]))
            .await
            .unwrap();
        let got = seen.lock().unwrap().clone().expect("persist must fire after refresh");
        assert_eq!(got.access_token, "at_new");
        assert_eq!(got.refresh_token, "rt_2", "rotated refresh_token must be persisted");
        assert!(
            got.expires_at > chrono::Utc::now().timestamp(),
            "expiry must be pushed forward on refresh"
        );
    }

    #[tokio::test]
    async fn persist_retains_refresh_token_when_response_omits_it() {
        use std::sync::{Arc, Mutex};
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "expires_in": 3600
                // no refresh_token → rotation absent; the prior rt_1 must be retained.
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&chat)
            .await;
        let chat_base = format!("{}/v1", chat.uri());
        let seen: Arc<Mutex<Option<XaiCredential>>> = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        let p = XaiProvider::with_bases(cred(-10), auth.uri(), chat_base)
            .with_persist(Box::new(move |c: XaiCredential| {
                *sink.lock().unwrap() = Some(c);
            }));
        p.complete("grok-4.3", json!([{"role":"user","content":"ping"}]), json!([]))
            .await
            .unwrap();
        let got = seen.lock().unwrap().clone().expect("persist must fire after refresh");
        assert_eq!(got.access_token, "at_new");
        assert_eq!(
            got.refresh_token, "rt_1",
            "an absent refresh_token must retain the prior vaulted token"
        );
    }
```

1b. Extend `#[cfg(test)] mod tests` in `packages/backend/api/src/llm/codex.rs` (append inside the module; `cred`, `json!`, wiremock imports, `CodexProvider`/`CodexCredential` are already in scope). This proves the identical persist behaviour for the shipped codex provider:
```rust
    #[tokio::test]
    async fn persists_rotated_refresh_token() {
        use std::sync::{Arc, Mutex};
        let auth = MockServer::start().await;
        let chat = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "at_new",
                "refresh_token": "rt_2",
                "expires_in": 864000
            })))
            .mount(&auth)
            .await;
        Mock::given(method("POST"))
            .and(path("/backend-api/codex/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
            ))
            .mount(&chat)
            .await;
        let seen: Arc<Mutex<Option<CodexCredential>>> = Arc::new(Mutex::new(None));
        let sink = seen.clone();
        // stale cred → must refresh before completing.
        let p = CodexProvider::with_bases(cred(-10), auth.uri(), chat.uri())
            .with_persist(Box::new(move |c: CodexCredential| {
                *sink.lock().unwrap() = Some(c);
            }));
        p.complete("gpt-5.4-mini", json!([{"role":"user","content":"ping"}]), json!([]))
            .await
            .unwrap();
        let got = seen.lock().unwrap().clone().expect("persist must fire after refresh");
        assert_eq!(got.access_token, "at_new");
        assert_eq!(got.refresh_token, "rt_2", "rotated refresh_token must be persisted");
        assert_eq!(got.account_id, "acc_1", "account_id must be retained across refresh");
        assert!(got.expires_at > chrono::Utc::now().timestamp());
    }
```
(Codex's retain-when-absent path is the identical `if let Some(rt) = new_refresh` branch as xai — codex.rs:141-143 vs xai.rs:249-251 — and is covered by the xai `persist_retains_refresh_token_when_response_omits_it` test; a duplicate codex case is intentionally omitted.)

1c. Add a new test module at the END of `packages/backend/api/src/assistant/mod.rs`. It references the not-yet-existing `persist_blob` and the not-yet-widened `build_engagement_provider`, so it is the RED driver:
```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p celerex-api --lib`
Expected: FAIL to compile — `cannot find function persist_blob in this scope`, and `build_engagement_provider` called with 4 arguments but defined with 2 (`this function takes 2 arguments but 4 arguments were supplied`). The whole `--lib` test build is red until Step 3 lands the helper + widened signature.

- [ ] **Step 3: Write minimal implementation**

3a. In `packages/backend/api/src/assistant/mod.rs`, widen the `build_engagement_provider` signature. Replace:
```rust
fn build_engagement_provider(provider_id: &str, key: String) -> Option<Arc<dyn Provider>> {
```
with:
```rust
fn build_engagement_provider(
    provider_keys: &Arc<dyn crate::providers::store::ProviderKeysStore>,
    engagement_id: Uuid,
    provider_id: &str,
    key: String,
) -> Option<Arc<dyn Provider>> {
```

3b. In the same `match`, replace the `codex` arm:
```rust
        "codex" => crate::llm::codex::CodexProvider::from_blob(&key)
            .ok()
            .map(|p| Arc::new(p) as Arc<dyn Provider>),
```
with:
```rust
        "codex" => crate::llm::codex::CodexProvider::from_blob(&key).ok().map(|p| {
            Arc::new(p.with_persist(persist_blob::<crate::llm::codex::CodexCredential>(
                provider_keys,
                engagement_id,
                "codex",
            ))) as Arc<dyn Provider>
        }),
```
and replace the `xai` arm added in Task 6:
```rust
        "xai" => crate::llm::xai::XaiProvider::from_blob(&key)
            .ok()
            .map(|p| Arc::new(p) as Arc<dyn Provider>),
```
with:
```rust
        "xai" => crate::llm::xai::XaiProvider::from_blob(&key).ok().map(|p| {
            Arc::new(p.with_persist(persist_blob::<crate::llm::xai::XaiCredential>(
                provider_keys,
                engagement_id,
                "xai",
            ))) as Arc<dyn Provider>
        }),
```

3c. Insert the `persist_blob` helper immediately after the closing brace of `build_engagement_provider` (before `resolve_engagement_provider`):
```rust
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
```

3d. Update the single caller inside `resolve_engagement_provider_with`. Replace:
```rust
        Ok(Some(key)) => Ok(build_engagement_provider(provider_id, key)),
```
with:
```rust
        Ok(Some(key)) => Ok(build_engagement_provider(
            provider_keys,
            engagement_id,
            provider_id,
            key,
        )),
```
(`provider_keys` and `engagement_id` are the enclosing fn's own parameters — already in scope.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p celerex-api --lib`
Expected: PASS — including `assistant::persist_tests` (2 tests), `llm::xai::provider_tests::persists_rotated_refresh_token`, `llm::xai::provider_tests::persist_retains_refresh_token_when_response_omits_it`, and `llm::codex::tests::persists_rotated_refresh_token`. No Docker needed (all wiremock/in-memory; no Postgres).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/assistant/mod.rs \
        packages/backend/api/src/llm/xai.rs \
        packages/backend/api/src/llm/codex.rs
git commit -m "feat(assistant): persist rotated xai/codex OAuth blobs back to the vault"
```

---

## Final: full-suite green check

- [ ] Run the whole crate suite (Docker daemon must be running):

Run: `cargo test -p celerex-api`
Expected: all existing tests plus the new unit tests (`llm::openai_compat`, `llm::xai`), `xai_login`, and `xai_provider` PASS; the three `xai_live_e2e` tests report as ignored. If `xai_provider` is flaky because of a sandbox with no outbound network, note that both "reached host / 401" and "unreachable host" resolve to 502 — a persistent non-502 there indicates a real regression, not a network issue.

**Preflight / known baseline hazard:** Before starting Task 1, run `cargo test -p celerex-api --no-run` to establish the compile baseline. The `AppState` struct gained a `design_runs` field (`src/lib.rs:62`) in commit `46029a6`, but the sibling test helpers `tests/copilot_provider.rs` (build_state at line 80) and `tests/assistant_chat.rs` (build_state at line 76) were last touched by the older commit `c277a29` and do NOT set `design_runs` — so they may already fail to compile on `main` with `missing field 'design_runs'`. If the baseline build is red for that reason, add `design_runs: celerex_api::runs::design_chat::new_registry(),` to those two helpers as part of this branch (an additive fix consistent with "existing tests stay green"); Task 6's new helper already includes the field.

- [ ] Open the PR (do not merge to main):

```bash
git push -u origin feat/xai-glm-kimicode-providers
gh pr create --base main --title "feat: xai (OAuth), kimi-code, glm providers (Agent side)" \
  --body "Server-side half of First-Run Provider Connect: design::xai device-code flow, provider factory arms for xai/kimi-code/glm, vault allow-list. Additive; existing tests green. Web side lands separately in celerex-design."
```

---

## Self-Review

**Spec coverage (Agent changes only):**
- `design::xai` module with `POST /api/design/xai/login/start` + `GET /api/design/xai/login/status`, OIDC device-code flow against the pinned endpoints, blob sealed under `xai`, same auth extractor as Copilot → **Task 5** (+ `XaiOAuthClient` in Task 3, `XaiCredential` in Task 4).
- `assistant/mod.rs` factory arms: `xai` (OpenAI-compatible at `https://api.x.ai/v1`, blob token, refresh-on-expiry/401), `kimi-code` (`https://api.kimi.com/coding/v1`), `glm` (`https://api.z.ai/api/coding/paas/v4`) → **Task 6** (provider impls in Tasks 2 & 4).
- `providers/store.rs` allow-list adds `xai`, `kimi-code`, `glm` → **Task 1**.
- `/api/runs` + `/api/assistant/chat` untouched → confirmed: no edits to those handlers; new providers ride `build_engagement_provider`.
- Testing: device-flow handlers vs mocked upstream (start→pending→success/denied/expired) → Task 3 (`poll_maps_pending_and_denied`) + Task 5 (success path end-to-end); factory tests pinning each id → Task 6; allow-list → Task 1; live manual env-gated → Task 7.

**Refresh-on-401 note:** the spec asks for refresh on expiry *or* 401. `XaiProvider::access` refreshes on expiry (within 60s), matching `CodexProvider` exactly. A hard 401 from an unexpired-but-revoked token surfaces as a provider `Err` (502) that clears back through the run; a dedicated retry-on-401 loop is NOT added because the mirror (`CodexProvider`) doesn't implement one either — keeping parity avoids inventing untested behaviour. Flagged as a deliberate scope decision (see Open Questions).

**Type consistency:** `XaiCredential` fields (`access_token`, `refresh_token`, `expires_at`) are identical across `llm/xai.rs`, `design/xai.rs` (poller seals it), and the `xai_provider`/live tests. `PollOutcome::Token(String, String, i64)` is bound as `(access, refresh, ttl)` in the poller. `OpenAiCompatProvider::new(base_url, api_key)` signature is used identically in Task 4 (via `XaiProvider::complete`), Task 6 (factory), and Task 7 (live). `SUPPORTED_PROVIDERS` is the single allow-list touched in Task 1 and relied on in Tasks 5-6.

**Placeholder scan:** no TBD/TODO; every code step is complete and verbatim; no "similar to Task N" references (code repeated where needed, e.g. the `build_state` helper in Task 6 is written out in full rather than referencing `copilot_provider.rs`).
