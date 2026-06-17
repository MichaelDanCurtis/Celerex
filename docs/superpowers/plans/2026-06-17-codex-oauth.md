# Codex / ChatGPT OAuth (slice 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Connect ChatGPT (Codex)" via a desktop-hosted OAuth loopback, store the tokens in the per-user vault, and use the ChatGPT Codex backend as a selectable LLM provider — the second OAuth-mode provider after GitHub Copilot.

**Architecture:** The daemon runs the PKCE loopback (`localhost:1455`) + browser-open + code exchange, derives `account_id` from the `id_token`, and uploads a JSON blob `{access_token, refresh_token, account_id, expires_at}` to the Agent, which vaults it under provider id `codex` scoped by `claims.engagement_id`. A `CodexProvider` refreshes the token when stale (writing the new blob back to the vault), translates chat-completions messages into the ChatGPT Responses contract (system→`instructions`, `stream:true`, `store:false`, no max-tokens, Codex slug, `chatgpt-account-id` header), POSTs to `chatgpt.com/backend-api/codex/responses`, and **de-streams** the mandatory SSE into a single `Completion`. Flow is web→daemon→Agent (the desktop does the capture), unlike Copilot's web→Agent-direct.

**Tech Stack:** Rust/Axum (`celerex-api`), `reqwest`, sqlx/Postgres, `wiremock` (tests); Next.js web (`apps/web`) + Express daemon (`apps/daemon`) + `@celerex/connect` seam, vitest/bun.

---

## Conventions

- Agent tests run from `cd "packages/backend/api"` via `tests/common/mod.rs::start_test_db()` + `axum_test::TestServer::new(celerex_api::build_app(db.pool.clone()))`. Mint a per-engagement session token with the helper used across tests:
```rust
#[derive(serde::Serialize)] struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: uuid::Uuid) -> String {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); } // SAFETY: single-process test
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None, exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}
```
  (`common::start_test_db()` already sets `CELEREX_CRED_KEY` + `CELEREX_JWT_SECRET`.)
- **Provider trait** (`agent-orchestrator/src/llm/provider.rs`): `async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Completion,String>` (+ a defaulted `complete_stream`). `Completion { raw: Value }`. Import via `use celerex_orchestrator::llm::provider::{Completion, Provider};` (the path the Copilot/Anthropic providers use).
- **Vault:** `state.provider_keys.set(engagement_id: Uuid, provider_id: &str, plaintext: &str) -> Result<ProviderKeyMeta, String>` and `.get(engagement_id: Uuid, provider_id: &str) -> Result<Option<String>, String>`. Store the Codex blob as a JSON **string** under provider id `codex`. No new table.
- **Registry is startup-immutable** — per-engagement providers are built at call time (mirror the `copilot` arm already in `assistant::build_engagement_provider` and `EngagementLlmHandler::build_provider`). Add `"codex"` to `SUPPORTED_PROVIDERS` (`src/providers/store.rs`) + both arms.
- **Auth extractor:** `AuthClaims(claims)` → `claims.engagement_id: Uuid`. New routes mount in `lib.rs` `route_chain()` beside the `/api/design/copilot/*` block.
- **Codex OAuth client_id:** OpenAI's first-party Codex client `app_EMoamEEZ73f0CkXaXp7hrann` (the value the Codex CLI uses). **Redirect:** `http://localhost:1455/auth/callback`. **Scope:** `openid profile email offline_access`.
- **The Responses contract** (skill `codex-cli-oauth-token-not-accepted-by-openai-api`): `POST https://chatgpt.com/backend-api/codex/responses`, headers `Authorization: Bearer <access_token>` + `chatgpt-account-id: <account_id>` + `Accept: text/event-stream`; body requires top-level `instructions` (non-empty), `stream:true`, `store:false`, NO max-tokens, a Codex slug. Refresh: `POST https://auth.openai.com/oauth/token` (`grant_type=refresh_token`).
- **Default Codex model slug for slice 1:** `gpt-5.4-mini` (with a tiny fallback list `["gpt-5.4-mini","gpt-5.4","gpt-5.3-codex"]` — the model passed by the caller is used as-is; the default is only for the registration/smoke surface). Run Agent from `cd "packages/backend/api"`; web/daemon from `cd "apps/web"` / `cd "apps/daemon"` in `celerex-design`.

---

# Stage 1 — Agent

### Task 1: Codex auth client + provider (`src/llm/codex.rs`)

**Files:** Create `packages/backend/api/src/llm/codex.rs`; Modify `src/llm/mod.rs` (add `pub mod codex;`)

- [ ] **Step 1: Recon.** Read `src/llm/copilot.rs` (the sibling — match its structure, the `#[allow(dead_code)]` convention for not-yet-wired items, and the wiremock test layout) and confirm the `Provider`/`Completion` import path. Confirm how `Completion.raw` is consumed downstream (grep for `.raw` / `choices` in `assistant/` + `llm/handler.rs`) so the de-streamed `raw` matches the chat-completions shape the other providers return (`{"choices":[{"message":{"role":"assistant","content": "...", "tool_calls": [...]}}], "usage": {...}}`).

- [ ] **Step 2: Write the module with the tests below.**

`src/llm/codex.rs`:
```rust
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Mutex;
use celerex_orchestrator::llm::provider::{Completion, Provider};

pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.4-mini";

/// The vaulted Codex credential (stored as a JSON string under provider id `codex`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodexCredential {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
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

/// Token-refresh client. Base URL injectable for tests.
pub struct CodexAuthClient {
    /// e.g. https://auth.openai.com
    pub auth_base: String,
    http: reqwest::Client,
}

impl CodexAuthClient {
    pub fn new() -> Self { Self::with_base("https://auth.openai.com".into()) }
    pub fn with_base(auth_base: String) -> Self {
        Self { auth_base: auth_base.trim_end_matches('/').to_string(), http: reqwest::Client::new() }
    }

    /// Exchange a refresh_token for a fresh access_token (+ maybe a rotated refresh_token).
    pub async fn refresh(&self, refresh_token: &str) -> Result<(String, Option<String>, i64), String> {
        let res = self.http
            .post(format!("{}/oauth/token", self.auth_base))
            .header("accept", "application/json")
            .json(&json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": CODEX_CLIENT_ID
            }))
            .send().await.map_err(|e| format!("codex refresh failed: {e}"))?;
        if !res.status().is_success() { return Err(format!("codex refresh status {}", res.status())); }
        let r: RefreshResp = res.json().await.map_err(|e| format!("codex refresh parse: {e}"))?;
        let exp = chrono::Utc::now().timestamp() + r.expires_in.unwrap_or(864_000);
        Ok((r.access_token, r.refresh_token, exp))
    }
}

/// A per-engagement Codex provider built from the vaulted credential blob.
/// `persist` is invoked after a refresh so the new blob is written back to the vault.
pub struct CodexProvider {
    cred: Mutex<CodexCredential>,
    auth: CodexAuthClient,
    /// override the chat host in tests; otherwise chatgpt.com
    chat_base: String,
    /// optional sink to persist a refreshed credential (engagement-scoped); None in unit tests
    persist: Option<Box<dyn Fn(CodexCredential) + Send + Sync>>,
}

impl CodexProvider {
    /// Production: parse the vaulted JSON blob.
    pub fn from_blob(blob: &str) -> Result<Self, String> {
        let cred: CodexCredential = serde_json::from_str(blob).map_err(|e| format!("codex cred parse: {e}"))?;
        Ok(Self {
            cred: Mutex::new(cred),
            auth: CodexAuthClient::new(),
            chat_base: "https://chatgpt.com".into(),
            persist: None,
        })
    }
    /// Test factory: point refresh + chat at stubs.
    pub fn with_bases(cred: CodexCredential, auth_base: String, chat_base: String) -> Self {
        Self {
            cred: Mutex::new(cred),
            auth: CodexAuthClient::with_base(auth_base),
            chat_base: chat_base.trim_end_matches('/').to_string(),
            persist: None,
        }
    }
    pub fn with_persist(mut self, f: Box<dyn Fn(CodexCredential) + Send + Sync>) -> Self {
        self.persist = Some(f); self
    }

    /// Return a valid access token + account_id, refreshing if within 60s of expiry.
    async fn access(&self) -> Result<(String, String), String> {
        let now = chrono::Utc::now().timestamp();
        let (access, refresh, account, exp) = {
            let c = self.cred.lock().unwrap();
            (c.access_token.clone(), c.refresh_token.clone(), c.account_id.clone(), c.expires_at)
        };
        if exp - 60 > now {
            return Ok((access, account));
        }
        let (new_access, new_refresh, new_exp) = self.auth.refresh(&refresh).await?;
        let updated = {
            let mut c = self.cred.lock().unwrap();
            c.access_token = new_access.clone();
            if let Some(rt) = new_refresh { c.refresh_token = rt; }
            c.expires_at = new_exp;
            c.clone()
        };
        if let Some(p) = &self.persist { p(updated); }
        Ok((new_access, account))
    }

    /// Translate chat-completions messages → (instructions, input items).
    fn translate(messages: &Value) -> (String, Vec<Value>) {
        let mut instructions = String::new();
        let mut input = Vec::new();
        if let Some(arr) = messages.as_array() {
            for m in arr {
                let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if role == "system" {
                    if !instructions.is_empty() { instructions.push_str("\n\n"); }
                    instructions.push_str(content);
                } else {
                    input.push(json!({
                        "type": "message", "role": role,
                        "content": [{ "type": "input_text", "text": content }]
                    }));
                }
            }
        }
        if instructions.is_empty() { instructions = "You are a helpful assistant.".into(); }
        (instructions, input)
    }

    /// Accumulate a Responses-API SSE body into a chat-completions-shaped `raw`.
    fn destream(body: &str) -> Value {
        let mut text = String::new();
        let mut usage = Value::Null;
        for line in body.lines() {
            let line = line.trim();
            let data = match line.strip_prefix("data:") { Some(d) => d.trim(), None => continue };
            if data == "[DONE]" { continue; }
            let ev: Value = match serde_json::from_str(data) { Ok(v) => v, Err(_) => continue };
            match ev.get("type").and_then(|v| v.as_str()) {
                Some("response.output_text.delta") => {
                    if let Some(d) = ev.get("delta").and_then(|v| v.as_str()) { text.push_str(d); }
                }
                Some("response.completed") => {
                    usage = ev.get("response").and_then(|r| r.get("usage")).cloned().unwrap_or(Value::Null);
                }
                _ => {}
            }
        }
        json!({
            "choices": [{ "message": { "role": "assistant", "content": text } }],
            "usage": usage
        })
    }
}

#[async_trait]
impl Provider for CodexProvider {
    async fn complete(&self, model: &str, messages: Value, _tools: Value) -> Result<Completion, String> {
        let (access, account) = self.access().await?;
        let (instructions, input) = Self::translate(&messages);
        let body = json!({
            "model": if model.is_empty() { DEFAULT_CODEX_MODEL } else { model },
            "instructions": instructions,
            "input": input,
            "stream": true,
            "store": false
        });
        let res = reqwest::Client::new()
            .post(format!("{}/backend-api/codex/responses", self.chat_base.trim_end_matches('/')))
            .header("authorization", format!("Bearer {access}"))
            .header("chatgpt-account-id", account)
            .header("accept", "text/event-stream")
            .json(&body)
            .send().await.map_err(|e| format!("codex responses failed: {e}"))?;
        let status = res.status();
        let text = res.text().await.map_err(|e| format!("codex responses read: {e}"))?;
        if !status.is_success() {
            return Err(format!("codex responses status {status}: {text}"));
        }
        Ok(Completion { raw: Self::destream(&text) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, header, body_partial_json};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cred(exp_offset: i64) -> CodexCredential {
        CodexCredential {
            access_token: "at_old".into(), refresh_token: "rt_1".into(),
            account_id: "acc_1".into(), expires_at: chrono::Utc::now().timestamp() + exp_offset,
        }
    }

    #[tokio::test]
    async fn refreshes_when_stale_then_completes_destreamed() {
        let auth = MockServer::start().await;  // auth.openai.com
        let chat = MockServer::start().await;  // chatgpt.com
        Mock::given(method("POST")).and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token":"at_new","refresh_token":"rt_2","expires_in":864000
            }))).mount(&auth).await;
        let sse = "event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"pong\"}\n\n\
event: response.completed\n\
data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n";
        Mock::given(method("POST")).and(path("/backend-api/codex/responses"))
            .and(header("authorization", "Bearer at_new"))      // used the refreshed token
            .and(header("chatgpt-account-id", "acc_1"))
            .and(body_partial_json(json!({ "stream": true, "store": false, "instructions": "You are terse." })))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse))
            .mount(&chat).await;
        // expires_at in the past → must refresh
        let p = CodexProvider::with_bases(cred(-10), auth.uri(), chat.uri());
        let out = p.complete("gpt-5.4-mini",
            json!([{"role":"system","content":"You are terse."},{"role":"user","content":"reply pong"}]),
            json!([])).await.unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "pong");
        assert_eq!(out.raw["usage"]["output_tokens"], 1);
    }

    #[tokio::test]
    async fn skips_refresh_when_fresh_and_promotes_default_instructions() {
        let auth = MockServer::start().await; // must NOT be called
        let chat = MockServer::start().await;
        Mock::given(method("POST")).and(path("/backend-api/codex/responses"))
            .and(header("authorization", "Bearer at_old"))                 // original token, no refresh
            .and(body_partial_json(json!({ "instructions": "You are a helpful assistant." })))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{}}\n\n"))
            .mount(&chat).await;
        let p = CodexProvider::with_bases(cred(3600), auth.uri(), chat.uri());
        let out = p.complete("", json!([{"role":"user","content":"hi"}]), json!([])).await.unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "hi");
    }
}
```
**Note for the implementer:** `body_partial_json` matches a subset, so the assertions above don't over-constrain. The `access()` method clones values out of the `Mutex` guard and drops the guard **before** the `.await` on refresh — do NOT hold the `std::sync::Mutex` guard across the await (compile + correctness). Add `pub mod codex;` to `src/llm/mod.rs`. Confirm `body_partial_json` is available in the project's `wiremock` (it is in 0.5+); if not, assert via a custom matcher or drop to `body_string_contains`.

- [ ] **Step 3: Run** `cd "packages/backend/api" && cargo test --lib llm::codex 2>&1 | tail -25` → FAIL→implement→PASS (2 tests). Then `cargo build --lib 2>&1 | tail -5`. Use `#[allow(dead_code)]` on items not yet wired (`CodexProvider::from_blob`, `with_persist`, `CODEX_CLIENT_ID` if unused until T2/T3); remove them as wired in T2-T3. Run `rustfmt --edition 2024 src/llm/codex.rs` (that file only).

- [ ] **Step 4: Commit**
```bash
git add packages/backend/api/src/llm/codex.rs packages/backend/api/src/llm/mod.rs
git commit -m "feat(agent): Codex auth client (refresh) + CodexProvider (Responses translate + de-stream)"
```

### Task 2: Credential upload + status endpoints (`src/design/codex.rs`)

**Files:** Create `packages/backend/api/src/design/codex.rs`, `packages/backend/api/tests/codex_credential.rs`; Modify `src/design/mod.rs` (add `pub mod codex;`), `src/lib.rs` (routes)

- [ ] **Step 1: Recon.** Read `src/design/copilot.rs` (the sibling — `AuthClaims` destructure, `State(state)`, vault `.set`/`.get` usage, error mapping) and how its routes mount in `src/lib.rs`. Confirm whether the credential upload should require `AuthClaims` (WorkOS session) or also accept the daemon's `design.device` pairing token. **Decision:** accept `AuthClaims` (the daemon uploads using a token whose `engagement_id` is the caller's engagement — matches how the daemon calls other `/api/design/*` endpoints). Verify the daemon's pairing token is accepted by `AuthClaims`; if the daemon uses a `design.device`-scoped token validated by a different extractor (e.g. `DeviceAuth`), mirror exactly what `design/deploy.rs` uses for daemon-originated calls and use that extractor here. Note which extractor you used in your report.

- [ ] **Step 2: Endpoints `src/design/codex.rs`:**
```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::auth::extractors::AuthClaims;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CodexCredentialBody {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
    #[serde(default)]
    pub expires_at: i64,
}

/// POST /api/design/codex/credential — store the captured Codex tokens in the vault.
pub async fn put_credential(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Json(body): Json<CodexCredentialBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if body.access_token.is_empty() || body.refresh_token.is_empty() || body.account_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing token fields".into()));
    }
    let blob = serde_json::to_string(&json!({
        "access_token": body.access_token,
        "refresh_token": body.refresh_token,
        "account_id": body.account_id,
        "expires_at": body.expires_at
    })).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state.provider_keys.set(claims.engagement_id, "codex", &blob).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

/// GET /api/design/codex/status — { connected }.
pub async fn status(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let connected = state.provider_keys.get(claims.engagement_id, "codex").await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?.is_some();
    Ok(Json(json!({ "connected": connected })))
}
```
Adapt the vault `.set`/`.get` signatures + the extractor to whatever Step 1 confirmed. Add `pub mod codex;` to `src/design/mod.rs`. Mount in `src/lib.rs` beside the copilot routes:
```rust
.route("/api/design/codex/credential", post(design::codex::put_credential))
.route("/api/design/codex/status", get(design::codex::status))
```

- [ ] **Step 3: Integration test** `tests/codex_credential.rs` (paste the `token()` helper from Conventions):
1. `start_test_db()` + `TestServer::new(celerex_api::build_app(db.pool.clone()))`.
2. Seed engagement A (SQL insert like `copilot_login.rs` does) + `token(A)`.
3. `POST /api/design/codex/credential` (Bearer A) with `{access_token:"at",refresh_token:"rt",account_id:"acc",expires_at: <now+864000>}` → 200 `{ok:true}`.
4. `GET /api/design/codex/status` (Bearer A) → `{connected:true}`.
5. Cross-engagement isolation: seed engagement B + `token(B)`, `GET .../status` (Bearer B) → `{connected:false}`.
6. Unauth: `POST .../credential` with no Authorization → 401.
7. Validation: `POST .../credential` (Bearer A) with empty `access_token` → 400.

Read `tests/copilot_login.rs` for the exact seed-engagement SQL + server bootstrap; mirror it.

- [ ] **Step 4: Run** `cargo test --test codex_credential 2>&1 | tail -25 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt --edition 2024` the new files only.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/design/codex.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/codex_credential.rs
git commit -m "feat(agent): Codex credential upload + status endpoints (vault store, engagement-scoped)"
```

### Task 3: Register Codex as a selectable per-engagement provider

**Files:** Modify `src/providers/store.rs` (SUPPORTED_PROVIDERS), `src/assistant/mod.rs` (build_engagement_provider), `src/llm/handler.rs` (EngagementLlmHandler::build_provider); Test `tests/codex_provider.rs`

- [ ] **Step 1: Recon.** Read the `copilot` arm in both `assistant::build_engagement_provider` and `EngagementLlmHandler::build_provider` (they are byte-identical: `"copilot" => Some(Arc::new(crate::llm::copilot::CopilotProvider::new(key))),`). The Codex arm differs: `CodexProvider::from_blob(&key)` returns a `Result`, so handle the parse error by returning `None` (treat an unparseable blob as "not configured").

- [ ] **Step 2: Add `"codex"` to `SUPPORTED_PROVIDERS`** in `src/providers/store.rs` (append to the array, leaving `copilot` and the others intact).

- [ ] **Step 3: Add the `codex` arm** to BOTH matches (the `key` here is the vaulted blob string):
```rust
"codex" => crate::llm::codex::CodexProvider::from_blob(&key).ok().map(|p| std::sync::Arc::new(p) as std::sync::Arc<dyn celerex_orchestrator::llm::provider::Provider>),
```
Adapt the `Arc<dyn Provider>` cast/path to match the existing arms' exact return type (the existing arms return `Some(Arc::new(...))` where the `Arc<dyn Provider>` coercion is implicit — if the match arm's type is `Option<Arc<dyn Provider>>`, `.ok().map(|p| Arc::new(p) as Arc<dyn Provider>)` is the equivalent that also yields `None` on a bad blob). Apply in `assistant::build_engagement_provider` AND `EngagementLlmHandler::build_provider`.

- [ ] **Step 4: Test** `tests/codex_provider.rs` — mirror `tests/copilot_provider.rs`'s approach exactly (it asserts via the HTTP boundary because `resolve_engagement_provider`/`build_engagement_provider` are private). Set `CELEREX_HOUSE_MODE=false`. Seed engagement A and store a valid Codex blob via `state.provider_keys.set(a, "codex", &json_blob)`; drive `POST /api/assistant/chat` with `provider_id:"codex"` → expect a **502 BAD_GATEWAY** (provider built from the blob, then the real chatgpt.com call fails — proving `codex` passed `SUPPORTED_PROVIDERS` AND a provider was constructed). Engagement B with NO codex blob → **409 CONFLICT** (house mode off, nothing built). Reuse the exact `AppState`/router bootstrap + the same `PgProviderKeysStore` instance handed to both the test and the app state, as `copilot_provider.rs` does. If a stored-but-unparseable blob should also yield 409 (treated as not-configured), add that assertion.

- [ ] **Step 5: Run** `cargo test --test codex_provider 2>&1 | tail -15 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. Remove any now-satisfied `#[allow(dead_code)]` in `src/llm/codex.rs` (`from_blob` is now used); leave allows that still only have test users.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/providers/store.rs packages/backend/api/src/assistant/mod.rs packages/backend/api/src/llm/handler.rs packages/backend/api/tests/codex_provider.rs packages/backend/api/src/llm/codex.rs
git commit -m "feat(agent): register Codex as a selectable per-engagement provider"
```

**Stage 1 ships as one Celerex branch `feat/codex-oauth-agent` → PR → squash-merge.**

---

# Stage 2 — Web + daemon (`celerex-design`, branch `feat/codex-oauth-web`)

### Task 4: Codex seam helpers (`@celerex/connect`)

**Files:** Create `packages/celerex-connect/src/codex.ts`, `packages/celerex-connect/test/codex.test.ts`; Modify the seam's barrel export (`packages/celerex-connect/src/index.ts` or equivalent)

- [ ] **Step 1: Recon.** Read `packages/celerex-connect/src/deploy.ts` (or `rituals.ts`) for the seam's fetch/error/types convention + how it's exported from the barrel + how tests are written (bun). The daemon will call these from the desktop; they hit the **daemon's own** routes (added in Task 5), or directly the Agent — confirm which side the seam targets in the existing deploy/rituals helpers and match. **Decision:** the Codex seam helpers mirror deploy — they call the **Agent** (`startCodexLogin` is a daemon-local action, so it's NOT in the seam; the seam exposes `getCodexStatus(base, token)` against the Agent's `/api/design/codex/status`, used by both daemon and web). Keep the seam minimal: just `getCodexStatus`.

- [ ] **Step 2: Write the failing test** `packages/celerex-connect/test/codex.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest"; // or bun:test — match the repo
import { getCodexStatus } from "../src/codex";

it("getCodexStatus GETs status with Bearer", async () => {
  const f = vi.fn(async () => new Response(JSON.stringify({ connected: true }), { status: 200, headers: { "content-type": "application/json" } }));
  const res = await getCodexStatus("https://agent.example", "sess1", f as unknown as typeof fetch);
  expect(res.connected).toBe(true);
  const [url, init] = (f as any).mock.calls[0];
  expect(url).toBe("https://agent.example/api/design/codex/status");
  expect((init as RequestInit & { headers: Record<string,string> }).headers.Authorization).toBe("Bearer sess1");
});
```
(Match the repo's test runner — `bun:test` if `deploy.test.ts` uses it.)

- [ ] **Step 3: Run red:** `cd "packages/celerex-connect" && <test cmd, e.g. bun test test/codex.test.ts> 2>&1 | tail -15`

- [ ] **Step 4: Implement `packages/celerex-connect/src/codex.ts`:**
```ts
export interface CodexStatus { connected: boolean; }

export async function getCodexStatus(base: string, sessionToken: string, fetchFn: typeof fetch = fetch): Promise<CodexStatus> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/codex/status`, {
    method: "GET",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
  if (!res.ok) throw new Error("Could not check Codex status.");
  return res.json() as Promise<CodexStatus>;
}
```
Export `getCodexStatus` from the barrel. Rebuild the dist if the daemon imports the built package: `pnpm --filter @celerex/connect build` (the dist is gitignored — the daemon imports built output).

- [ ] **Step 5: Run green** (1/1) and `tsc`/build clean.

- [ ] **Step 6: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add packages/celerex-connect/src/codex.ts packages/celerex-connect/test/codex.test.ts packages/celerex-connect/src/index.ts
git commit -m "feat(seam): Codex status helper in @celerex/connect"
```

### Task 5: Daemon PKCE loopback + upload (`apps/daemon/src/routes/codex.ts`)

**Files:** Create `apps/daemon/src/routes/codex.ts`, `apps/daemon/src/codex/oauth.ts` (PKCE + loopback helpers), `apps/daemon/test/codex.test.ts`; Modify the daemon route registrar (where `registerCelerexDeployRoutes`/other routes are wired)

- [ ] **Step 1: Recon.** Read `apps/daemon/src/routes/celerex-deploy.ts` (the deploy proxy precedent — how it reads the app-config `celerex` pairing token + base URL, calls the Agent, maps errors) and how routes are registered + how the daemon opens a browser elsewhere (grep `open(` / `child_process` / any existing `openExternal`). Confirm the app-config accessor for the pairing token + Agent base URL.

- [ ] **Step 2: PKCE + token-exchange helpers** `apps/daemon/src/codex/oauth.ts` (pure, unit-testable — no network in the helpers themselves):
```ts
import { createHash, randomBytes } from "node:crypto";

export const CODEX_CLIENT_ID = "app_EMoamEEZ73f0CkXaXp7hrann";
export const CODEX_REDIRECT = "http://localhost:1455/auth/callback";
export const CODEX_AUTHORIZE = "https://auth.openai.com/oauth/authorize";
export const CODEX_TOKEN = "https://auth.openai.com/oauth/token";

export function b64url(buf: Buffer): string {
  return buf.toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
export function makePkce() {
  const verifier = b64url(randomBytes(32));
  const challenge = b64url(createHash("sha256").update(verifier).digest());
  const state = b64url(randomBytes(16));
  return { verifier, challenge, state };
}
export function authorizeUrl(challenge: string, state: string): string {
  const p = new URLSearchParams({
    response_type: "code",
    client_id: CODEX_CLIENT_ID,
    redirect_uri: CODEX_REDIRECT,
    scope: "openid profile email offline_access",
    code_challenge: challenge,
    code_challenge_method: "S256",
    state,
  });
  return `${CODEX_AUTHORIZE}?${p.toString()}`;
}

export interface CodexTokens { access_token: string; refresh_token: string; account_id: string; expires_at: number; }

/** Decode the account_id from an id_token JWT (claim `chatgpt_account_id`, possibly nested under the OpenAI auth namespace). */
export function accountIdFromIdToken(idToken: string): string {
  const parts = idToken.split(".");
  if (parts.length < 2) return "";
  const payload = JSON.parse(Buffer.from(parts[1].replace(/-/g, "+").replace(/_/g, "/"), "base64").toString("utf8"));
  return (
    payload["chatgpt_account_id"] ||
    payload["https://api.openai.com/auth"]?.chatgpt_account_id ||
    payload["https://api.openai.com/auth"]?.["chatgpt_account_id"] ||
    ""
  );
}

/** Exchange an auth code (PKCE) for tokens. fetchFn injectable for tests. */
export async function exchangeCode(code: string, verifier: string, fetchFn: typeof fetch = fetch): Promise<CodexTokens> {
  const res = await fetchFn(CODEX_TOKEN, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      grant_type: "authorization_code",
      code,
      code_verifier: verifier,
      redirect_uri: CODEX_REDIRECT,
      client_id: CODEX_CLIENT_ID,
    }),
  });
  if (!res.ok) throw new Error(`codex token exchange failed: ${res.status}`);
  const j = (await res.json()) as { access_token: string; refresh_token: string; id_token?: string; expires_in?: number };
  return {
    access_token: j.access_token,
    refresh_token: j.refresh_token,
    account_id: j.id_token ? accountIdFromIdToken(j.id_token) : "",
    expires_at: Math.floor(Date.now() / 1000) + (j.expires_in ?? 864000),
  };
}
```

- [ ] **Step 3: Failing unit test** `apps/daemon/test/codex.test.ts` (PKCE + exchange + account_id — the loopback HTTP server itself is integration-tested lightly; the pure helpers carry the coverage):
```ts
import { describe, it, expect, vi } from "vitest";
import { makePkce, authorizeUrl, accountIdFromIdToken, exchangeCode, b64url } from "../src/codex/oauth";
import { createHash } from "node:crypto";

it("makePkce produces an S256-consistent challenge", () => {
  const { verifier, challenge } = makePkce();
  expect(challenge).toBe(b64url(createHash("sha256").update(verifier).digest()));
});
it("authorizeUrl carries the required OAuth params", () => {
  const url = new URL(authorizeUrl("chal", "st8"));
  expect(url.searchParams.get("client_id")).toBe("app_EMoamEEZ73f0CkXaXp7hrann");
  expect(url.searchParams.get("redirect_uri")).toBe("http://localhost:1455/auth/callback");
  expect(url.searchParams.get("code_challenge_method")).toBe("S256");
});
it("accountIdFromIdToken reads chatgpt_account_id", () => {
  const payload = Buffer.from(JSON.stringify({ chatgpt_account_id: "acc_42" })).toString("base64").replace(/=+$/,"");
  expect(accountIdFromIdToken(`h.${payload}.s`)).toBe("acc_42");
});
it("exchangeCode posts PKCE and returns tokens + account_id", async () => {
  const idPayload = Buffer.from(JSON.stringify({ chatgpt_account_id: "acc_9" })).toString("base64").replace(/=+$/,"");
  const f = vi.fn(async () => new Response(JSON.stringify({ access_token:"at", refresh_token:"rt", id_token:`h.${idPayload}.s`, expires_in: 864000 }), { status: 200 }));
  const t = await exchangeCode("code123", "ver", f as unknown as typeof fetch);
  expect(t.access_token).toBe("at");
  expect(t.account_id).toBe("acc_9");
  const body = JSON.parse((f as any).mock.calls[0][1].body);
  expect(body.grant_type).toBe("authorization_code");
  expect(body.code_verifier).toBe("ver");
});
```

- [ ] **Step 4: Run red:** `cd "apps/daemon" && pnpm vitest run test/codex.test.ts 2>&1 | tail -20`. Implement the helpers (Step 2) → green (4/4).

- [ ] **Step 5: Route `apps/daemon/src/routes/codex.ts`** (`registerCodexRoutes(app, deps)`), wiring the loopback. Mirror `celerex-deploy.ts` for the app-config pairing token + Agent base + error mapping:
```ts
import type { Express, Request, Response } from "express";
import http from "node:http";
import { makePkce, authorizeUrl, exchangeCode, CODEX_REDIRECT } from "../codex/oauth";

interface Deps {
  agentBaseUrl: () => string | null;        // from app-config celerex pairing
  agentSessionToken: () => string | null;   // pairing token
  openBrowser: (url: string) => void;        // existing daemon browser-open
  fetchFn?: typeof fetch;
}

export function registerCodexRoutes(app: Express, deps: Deps) {
  const fetchFn = deps.fetchFn ?? fetch;

  // POST /api/codex/login/start — run the loopback, capture+exchange, upload to the Agent.
  app.post("/api/codex/login/start", async (_req: Request, res: Response) => {
    const base = deps.agentBaseUrl();
    const token = deps.agentSessionToken();
    if (!base || !token) { res.status(409).json({ error: "not paired to an Agent" }); return; }

    const { verifier, challenge, state } = makePkce();
    let settled = false;
    const server = http.createServer(async (r, w) => {
      try {
        const u = new URL(r.url ?? "/", "http://localhost:1455");
        if (u.pathname !== "/auth/callback") { w.writeHead(404).end(); return; }
        if (u.searchParams.get("state") !== state) { w.writeHead(400).end("bad state"); return; }
        const code = u.searchParams.get("code");
        if (!code) { w.writeHead(400).end("missing code"); return; }
        const tokens = await exchangeCode(code, verifier, fetchFn);
        const up = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/codex/credential`, {
          method: "POST",
          headers: { Authorization: `Bearer ${token}`, "content-type": "application/json" },
          body: JSON.stringify(tokens),
        });
        w.writeHead(up.ok ? 200 : 502, { "content-type": "text/html" })
          .end(up.ok ? "<p>ChatGPT connected. You can close this tab.</p>" : "<p>Connection failed.</p>");
      } catch {
        if (!w.headersSent) w.writeHead(500).end("error");
      } finally {
        settled = true;
        server.close();
      }
    });
    server.listen(1455, "127.0.0.1", () => {
      deps.openBrowser(authorizeUrl(challenge, state));
    });
    // Safety: stop the loopback after 5 minutes if the user never finishes.
    setTimeout(() => { if (!settled) server.close(); }, 5 * 60 * 1000);
    res.json({ started: true });
  });

  // GET /api/codex/status — proxy the Agent status.
  app.get("/api/codex/status", async (_req: Request, res: Response) => {
    const base = deps.agentBaseUrl();
    const token = deps.agentSessionToken();
    if (!base || !token) { res.json({ connected: false }); return; }
    try {
      const r = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/codex/status`, { headers: { Authorization: `Bearer ${token}` } });
      const j = await r.json();
      res.json({ connected: !!j.connected });
    } catch { res.status(502).json({ connected: false }); }
  });
}
```
Register it where the other daemon routes are wired, passing the real `agentBaseUrl`/`agentSessionToken` accessors (from the app-config `celerex` pairing, as `celerex-deploy.ts` reads them) and the existing browser-open helper. Adapt arg names to the real app-config accessors found in Step 1.

- [ ] **Step 6: Route test** add to `apps/daemon/test/codex.test.ts`: spin the registrar onto a throwaway Express app with an injected `fetchFn` (stub the Agent's `/api/design/codex/status`) and a no-op `openBrowser`; assert `GET /api/codex/status` proxies `{connected}` and that with no pairing it returns `{connected:false}`. (The full loopback round-trip — binding :1455 + real redirect — is out of unit scope; the PKCE/exchange helpers already cover the capture logic.)

- [ ] **Step 7: Run** `cd "apps/daemon" && pnpm vitest run test/codex.test.ts 2>&1 | tail -15 && pnpm tsc -b --noEmit 2>&1 | tail -8` → green/clean.

- [ ] **Step 8: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/daemon/src/codex/oauth.ts apps/daemon/src/routes/codex.ts apps/daemon/test/codex.test.ts <the route registrar file>
git commit -m "feat(daemon): Codex PKCE loopback + code exchange + upload to Agent vault"
```

### Task 6: Web state + `CodexOAuthPanel`

**Files:** Create `apps/web/src/state/codex.ts`, `apps/web/tests/state/codex.test.ts`, `apps/web/src/components/byok/CodexOAuthPanel.tsx`, `apps/web/tests/components/CodexOAuthPanel.test.tsx`; Modify `apps/web/src/components/SettingsDialog.tsx` (add the panel to the existing OAuth section)

- [ ] **Step 1: Recon.** Read `apps/web/src/state/copilot.ts` + `apps/web/src/components/byok/CopilotOAuthPanel.tsx` + the OAuth section in `SettingsDialog.tsx` (where `cfg.mode === 'oauth'` renders `<CopilotOAuthPanel />` inside `.oauth-section`). The Codex panel sits **beside** Copilot in the same OAuth section. Confirm `daemonLive` is in scope where the OAuth section renders (it gates the CLI segment); the Codex panel uses it to grey out in a browser.

- [ ] **Step 2: Failing state test** `apps/web/tests/state/codex.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest";
import { startCodexLogin, getCodexStatus } from "../../src/state/codex";

function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as unknown as typeof fetch;
}
describe("codex state", () => {
  it("startCodexLogin POSTs to the daemon", async () => {
    const f = mockFetch({ started: true });
    await startCodexLogin("http://localhost:7456", f);
    const [url, init] = (f as any).mock.calls[0];
    expect(url).toBe("http://localhost:7456/api/codex/login/start");
    expect((init as RequestInit).method).toBe("POST");
  });
  it("getCodexStatus GETs the daemon status", async () => {
    const f = mockFetch({ connected: true });
    const res = await getCodexStatus("http://localhost:7456", f);
    expect(res.connected).toBe(true);
    expect((f as any).mock.calls[0][0]).toBe("http://localhost:7456/api/codex/status");
  });
});
```

- [ ] **Step 3: Implement `apps/web/src/state/codex.ts`** (talks to the **daemon** — confirm the daemon base-URL accessor the web already uses for daemon calls in Step 1; the snippet uses a passed-in `daemonBase`):
```ts
export interface CodexStatus { connected: boolean; }

export async function startCodexLogin(daemonBase: string, fetchFn: typeof fetch = fetch): Promise<{ started: boolean }> {
  const res = await fetchFn(`${daemonBase.replace(/\/+$/, "")}/api/codex/login/start`, {
    method: "POST",
    headers: { "content-type": "application/json" },
  });
  if (!res.ok) throw new Error("Could not start ChatGPT login. Is the desktop app running?");
  return res.json() as Promise<{ started: boolean }>;
}
export async function getCodexStatus(daemonBase: string, fetchFn: typeof fetch = fetch): Promise<CodexStatus> {
  const res = await fetchFn(`${daemonBase.replace(/\/+$/, "")}/api/codex/status`, { method: "GET" });
  if (!res.ok) throw new Error("Could not check ChatGPT status.");
  return res.json() as Promise<CodexStatus>;
}
```
Run green (2/2).

- [ ] **Step 4: Failing component test** `apps/web/tests/components/CodexOAuthPanel.test.tsx`:
```tsx
// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../src/state/codex", () => ({
  startCodexLogin: vi.fn(async () => ({ started: true })),
  getCodexStatus: vi.fn(async () => ({ connected: true })),
}));
import { startCodexLogin } from "../../src/state/codex";
import { CodexOAuthPanel } from "../../src/components/byok/CodexOAuthPanel";

describe("CodexOAuthPanel", () => {
  afterEach(() => { cleanup(); vi.clearAllMocks(); });
  it("greys out when the desktop is not running", async () => {
    render(<CodexOAuthPanel daemonLive={false} daemonBase="http://localhost:7456" pollIntervalMs={0} />);
    expect(await screen.findByText(/open the desktop app/i)).toBeTruthy();
  });
  it("starts login on click and polls to connected", async () => {
    render(<CodexOAuthPanel daemonLive={true} daemonBase="http://localhost:7456" pollIntervalMs={0} />);
    fireEvent.click(await screen.findByRole("button", { name: /connect chatgpt/i }));
    await waitFor(() => expect(startCodexLogin).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText(/connected/i)).toBeTruthy());
  });
});
```

- [ ] **Step 5: Implement `apps/web/src/components/byok/CodexOAuthPanel.tsx`** (mirror `CopilotOAuthPanel`, incl. the mounted-ref unmount guard):
```tsx
import { useEffect, useRef, useState } from "react";
import { startCodexLogin, getCodexStatus } from "../../state/codex";

interface Props { daemonLive: boolean; daemonBase: string; pollIntervalMs?: number; }

export function CodexOAuthPanel({ daemonLive, daemonBase, pollIntervalMs = 2500 }: Props) {
  const [status, setStatus] = useState<"idle" | "pending" | "connected" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const polling = useRef(false);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);

  if (!daemonLive) {
    return <p className="codex-oauth__hint">Open the desktop app to connect ChatGPT (Codex).</p>;
  }

  async function begin() {
    setError(null); setStatus("pending");
    try {
      await startCodexLogin(daemonBase);
      if (polling.current) return;
      polling.current = true;
      for (let i = 0; i < 200; i++) {
        if (!mounted.current) return;
        const st = await getCodexStatus(daemonBase);
        if (!mounted.current) return;
        if (st.connected) { setStatus("connected"); break; }
        await new Promise((r) => setTimeout(r, pollIntervalMs));
      }
      polling.current = false;
    } catch (e) {
      if (mounted.current) { setStatus("error"); setError((e as Error).message); }
      polling.current = false;
    }
  }

  return (
    <div className="codex-oauth">
      <button type="button" onClick={() => void begin()} disabled={status === "pending"}>
        Connect ChatGPT (Codex)
      </button>
      {status === "pending" ? <p className="codex-oauth__code">A browser window opened — approve access, then come back.</p> : null}
      {status === "connected" ? <p className="codex-oauth__ok">ChatGPT (Codex) connected.</p> : null}
      {error ? <p className="codex-oauth__err">{error}</p> : null}
    </div>
  );
}
```
Run green (2/2).

- [ ] **Step 6: Wire into `SettingsDialog.tsx`** — in the existing `cfg.mode === 'oauth'` block (the `.oauth-section` div that renders `<CopilotOAuthPanel />`), add `<CodexOAuthPanel daemonLive={daemonLive} daemonBase={<the daemon base the dialog already uses>} />` beside it. Import it at the top next to the Copilot import. Use the same `daemonLive` value the CLI segment uses and the daemon base-URL accessor confirmed in Step 1. If a small heading per provider helps the two panels read clearly, add minimal labels ("GitHub Copilot" / "ChatGPT (Codex)").

- [ ] **Step 7: Typecheck + suite** `cd "apps/web" && pnpm tsc -b --noEmit 2>&1 | tail -12 && pnpm vitest run 2>&1 | tail -25` → clean; suite green except the known pre-existing `default-background` (3) failures. If a SettingsDialog test asserts the OAuth section contains exactly one panel, update it to allow both.

- [ ] **Step 8: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/codex.ts apps/web/tests/state/codex.test.ts apps/web/src/components/byok/CodexOAuthPanel.tsx apps/web/tests/components/CodexOAuthPanel.test.tsx apps/web/src/components/SettingsDialog.tsx
git commit -m "feat(web): Codex OAuth panel (desktop-gated) in the OAuth section"
```

**Stage 2 ships as one celerex-design branch `feat/codex-oauth-web` → PR → squash-merge.**

---

## Final verification
- [ ] Agent: `cd "packages/backend/api" && cargo test --lib llm::codex && cargo test --test codex_credential --test codex_provider 2>&1 | tail -15` → PASS.
- [ ] Seam: `cd "packages/celerex-connect" && <test cmd> 2>&1 | tail -8` → PASS; dist rebuilt.
- [ ] Daemon: `cd "apps/daemon" && pnpm vitest run test/codex.test.ts && pnpm tsc -b --noEmit` → green/clean.
- [ ] Web: `cd "apps/web" && pnpm vitest run tests/state/codex.test.ts tests/components/CodexOAuthPanel.test.tsx && pnpm tsc -b --noEmit` → green/clean.
- [ ] Stages shipped as branch + PR + squash-merge per repo (Agent = Celerex; web+daemon+seam = celerex-design).

## Self-review (spec coverage)
- Desktop-hosted OAuth loopback (PKCE on :1455, browser-open, code exchange, account_id from id_token) → Task 5. ✓
- Tokens stored as a JSON blob in the vault under `codex`, scoped by engagement; never returned to the web → Task 2 (upload/status only return `{ok}`/`{connected}`). ✓
- `CodexProvider`: refresh-on-stale writing back to the vault, system→`instructions`, `stream:true`/`store:false`, no max-tokens, `chatgpt-account-id` header, Codex slug, de-stream SSE → Task 1. ✓
- Codex a selectable per-engagement provider (built from the blob; bad blob → not configured) → Task 3. ✓
- Flow web→daemon→Agent (web state hits the daemon; daemon uploads to Agent via pairing token) → Tasks 5-6. ✓
- Panel in the OAuth section beside Copilot, greyed when `!daemonLive`, mounted-ref guard, polls to connected → Task 6. ✓
- All stubbed in tests (auth.openai.com + chatgpt.com wiremock; daemon injectable fetch; web mocked state) — no live ChatGPT account → Tasks 1,3,5,6. ✓
- Cross-engagement isolation + unauth 401 + validation 400 → Task 2. ✓
- Deferred (spec §8): per-account model browsing, `complete_stream`, CLI-import variant, disconnect UI. ✓
