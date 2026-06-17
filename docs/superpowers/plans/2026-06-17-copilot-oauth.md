# GitHub Copilot OAuth (slice 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Log in with GitHub Copilot" entirely server-side (device flow), store the token in the per-user vault, use Copilot as a selectable LLM provider, and reorganize the model picker into CLI / BYOK / OAuth sections.

**Architecture:** Agent runs the GitHub device flow + a background poll, stores the `ghu_` token in `engagement_provider_keys` (provider id `copilot`, scoped by `claims.engagement_id`). A `CopilotProvider` exchanges `ghu_` → a short-lived Copilot token (`copilot_internal/v2/token`, honoring `endpoints.api`) and calls `{api}/chat/completions` with editor headers — built per-engagement at call time (the registry is startup-immutable). The web calls the Agent **directly** with the session Bearer (the settings-slice pattern — no seam/daemon proxy). The picker gains a third `oauth` segment + a `CopilotOAuthPanel`; the existing `daemon` (Local CLI, auto-greyed when offline) and `api` (BYOK) tabs become the CLI and BYOK sections.

**Tech Stack:** Rust/Axum (`celerex-api`), `reqwest` (json+stream), sqlx/Postgres, `wiremock` (tests); Next.js web (`apps/web`), vitest+jsdom.

---

## Conventions

- Agent tests run from `cd "packages/backend/api"` via `tests/common/mod.rs::start_test_db()` + `axum_test::TestServer`. Mint a per-engagement session token with the local helper used across tests:
```rust
#[derive(serde::Serialize)] struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: uuid::Uuid) -> String {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); } // SAFETY: single-process test
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None, exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}
```
- **Provider trait** (`agent-orchestrator/src/llm/provider.rs`): `async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Completion,String>` (+ a defaulted `complete_stream`). `Completion { raw: Value }`.
- **Registry is startup-immutable** — per-engagement providers are built at call time from the vaulted key (mirror `assistant::build_engagement_provider` / `EngagementLlmHandler::build_provider`). Add `"copilot"` to `SUPPORTED_PROVIDERS` (`src/providers/store.rs:29`) + both match arms.
- **Vault:** `state.provider_keys.set/get(engagement_id, provider_id, …)` — store the `ghu_` token under provider id `copilot`. No new credential table.
- **Auth extractor:** `AuthClaims(claims)` → `claims.engagement_id`. New routes mount in `lib.rs` `route_chain()` beside the `/api/design/deploy*` block.
- **Editor client_id:** `Iv1.b507a08c87ecfe98`. **Run from `cd "packages/backend/api"`** for Agent; `cd "apps/web"` for web (in `celerex-design`).

---

# Stage 1 — Agent

### Task 1: Copilot OAuth client + provider (`src/llm/copilot.rs`)

**Files:** Create `packages/backend/api/src/llm/copilot.rs`; Modify `src/llm/mod.rs` (add `pub mod copilot;`)

- [ ] **Step 1: Write the module (client + provider) with the test below**

`src/llm/copilot.rs`:
```rust
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Mutex;
use celerex_orchestrator::llm::provider::{Completion, Provider};

pub const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Device-flow + token-exchange client. Base URLs are injectable for tests.
pub struct CopilotOAuthClient {
    /// e.g. https://github.com  (device/code + oauth/access_token)
    pub github_base: String,
    /// e.g. https://api.github.com  (copilot_internal/v2/token)
    pub api_github_base: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
}
fn default_interval() -> u64 { 5 }
fn default_expires() -> u64 { 900 }

#[derive(Debug, PartialEq)]
pub enum PollOutcome { Pending, SlowDown, Token(String), Denied, Expired, Err(String) }

#[derive(Debug, Deserialize)]
pub struct CopilotTokenResp {
    pub token: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub endpoints: CopilotEndpoints,
}
#[derive(Debug, Default, Deserialize)]
pub struct CopilotEndpoints {
    #[serde(default)]
    pub api: Option<String>,
}

impl CopilotOAuthClient {
    pub fn new() -> Self {
        Self::with_bases("https://github.com".into(), "https://api.github.com".into())
    }
    pub fn with_bases(github_base: String, api_github_base: String) -> Self {
        Self {
            github_base: github_base.trim_end_matches('/').to_string(),
            api_github_base: api_github_base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn start_device_flow(&self) -> Result<DeviceCode, String> {
        let res = self.http
            .post(format!("{}/login/device/code", self.github_base))
            .header("accept", "application/json")
            .json(&json!({ "client_id": COPILOT_CLIENT_ID, "scope": "read:user" }))
            .send().await.map_err(|e| format!("device/code request failed: {e}"))?;
        if !res.status().is_success() { return Err(format!("device/code status {}", res.status())); }
        res.json::<DeviceCode>().await.map_err(|e| format!("device/code parse failed: {e}"))
    }

    pub async fn poll_access_token(&self, device_code: &str) -> PollOutcome {
        let res = self.http
            .post(format!("{}/login/oauth/access_token", self.github_base))
            .header("accept", "application/json")
            .json(&json!({
                "client_id": COPILOT_CLIENT_ID,
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
            }))
            .send().await;
        let res = match res { Ok(r) => r, Err(e) => return PollOutcome::Err(format!("poll failed: {e}")) };
        let body: Value = match res.json().await { Ok(v) => v, Err(e) => return PollOutcome::Err(format!("poll parse: {e}")) };
        if let Some(tok) = body.get("access_token").and_then(|v| v.as_str()) {
            return PollOutcome::Token(tok.to_string());
        }
        match body.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => PollOutcome::Pending,
            Some("slow_down") => PollOutcome::SlowDown,
            Some("access_denied") => PollOutcome::Denied,
            Some("expired_token") => PollOutcome::Expired,
            other => PollOutcome::Err(format!("poll error: {:?}", other)),
        }
    }

    /// Exchange a `ghu_` token for a short-lived Copilot token + the API host.
    pub async fn exchange_copilot_token(&self, ghu: &str) -> Result<CopilotTokenResp, String> {
        let res = self.http
            .get(format!("{}/copilot_internal/v2/token", self.api_github_base))
            .header("authorization", format!("token {ghu}"))
            .header("accept", "application/json")
            .send().await.map_err(|e| format!("copilot token exchange failed: {e}"))?;
        if !res.status().is_success() { return Err(format!("copilot token exchange status {}", res.status())); }
        res.json::<CopilotTokenResp>().await.map_err(|e| format!("copilot token parse: {e}"))
    }
}

/// A per-engagement Copilot LLM provider built from the vaulted `ghu_` token.
pub struct CopilotProvider {
    ghu: String,
    client: CopilotOAuthClient,
    /// override the chat host in tests; otherwise use the exchange's endpoints.api
    chat_base_override: Option<String>,
    cache: Mutex<Option<(String, String, i64)>>, // (copilot_token, api_base, expires_at)
}

impl CopilotProvider {
    pub fn new(ghu: String) -> Self {
        Self { ghu, client: CopilotOAuthClient::new(), chat_base_override: None, cache: Mutex::new(None) }
    }
    /// Test factory: point both the exchange and the chat at a stub.
    pub fn with_bases(ghu: String, github_base: String, api_github_base: String, chat_base: String) -> Self {
        Self {
            ghu,
            client: CopilotOAuthClient::with_bases(github_base, api_github_base),
            chat_base_override: Some(chat_base.trim_end_matches('/').to_string()),
            cache: Mutex::new(None),
        }
    }

    async fn copilot_token(&self) -> Result<(String, String), String> {
        // now() — tests pass exp far in the future; production re-exchanges when stale.
        let now = chrono::Utc::now().timestamp();
        if let Some((tok, api, exp)) = self.cache.lock().unwrap().clone() {
            if exp - 60 > now { return Ok((tok, api)); }
        }
        let resp = self.client.exchange_copilot_token(&self.ghu).await?;
        let api = self.chat_base_override.clone()
            .or(resp.endpoints.api.clone())
            .unwrap_or_else(|| "https://api.githubcopilot.com".to_string());
        *self.cache.lock().unwrap() = Some((resp.token.clone(), api.clone(), resp.expires_at));
        Ok((resp.token, api))
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Completion, String> {
        let (copilot_token, api_base) = self.copilot_token().await?;
        let mut body = json!({ "model": model, "messages": messages });
        if tools.as_array().map(|a| !a.is_empty()).unwrap_or(false) { body["tools"] = tools; }
        let res = reqwest::Client::new()
            .post(format!("{}/chat/completions", api_base.trim_end_matches('/')))
            .header("authorization", format!("Bearer {copilot_token}"))
            .header("copilot-integration-id", "vscode-chat")
            .header("editor-version", "OmniVyx/1.0")
            .header("editor-plugin-version", "OmniVyx/1.0")
            .header("user-agent", "OmniVyx/1.0")
            .json(&body)
            .send().await.map_err(|e| format!("copilot chat failed: {e}"))?;
        let status = res.status();
        let raw: Value = res.json().await.map_err(|e| format!("copilot chat parse: {e}"))?;
        if !status.is_success() {
            return Err(format!("copilot chat status {status}: {}", raw));
        }
        Ok(Completion { raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, header};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn device_flow_start_and_poll() {
        let gh = MockServer::start().await;
        Mock::given(method("POST")).and(path("/login/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code":"dc","user_code":"WXYZ-1234","verification_uri":"https://github.com/login/device","interval":5,"expires_in":900
            }))).mount(&gh).await;
        // first poll pending, then a token
        Mock::given(method("POST")).and(path("/login/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token":"ghu_abc" })))
            .mount(&gh).await;
        let c = CopilotOAuthClient::with_bases(gh.uri(), gh.uri());
        let dc = c.start_device_flow().await.unwrap();
        assert_eq!(dc.user_code, "WXYZ-1234");
        assert_eq!(c.poll_access_token(&dc.device_code).await, PollOutcome::Token("ghu_abc".into()));
    }

    #[tokio::test]
    async fn provider_exchanges_then_completes() {
        let api = MockServer::start().await; // serves copilot_internal/v2/token
        let chat = MockServer::start().await; // serves /chat/completions
        Mock::given(method("GET")).and(path("/copilot_internal/v2/token"))
            .and(header("authorization", "token ghu_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "token":"cop_tok","expires_at": (chrono::Utc::now().timestamp()+1800),
                "endpoints": { "api": chat.uri() }
            }))).mount(&api).await;
        Mock::given(method("POST")).and(path("/chat/completions"))
            .and(header("authorization", "Bearer cop_tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices":[{"message":{"role":"assistant","content":"hi from copilot"}}]
            }))).mount(&chat).await;
        // chat_base_override is None so we use endpoints.api from the exchange
        let p = CopilotProvider::with_bases("ghu_abc".into(), "https://unused".into(), api.uri(), String::new());
        // with_bases sets chat_base_override to "" — treat empty as "use endpoints.api"
        let out = p.complete("gpt-4o", json!([{"role":"user","content":"hi"}]), json!([])).await.unwrap();
        assert_eq!(out.raw["choices"][0]["message"]["content"], "hi from copilot");
    }
}
```
**Note for the implementer:** in `copilot_token()`, treat an empty `chat_base_override` as "unset" (fall through to `endpoints.api`) so the second test exercises the real `endpoints.api` path — change the `.or(...)` to `self.chat_base_override.clone().filter(|s| !s.is_empty()).or(resp.endpoints.api.clone())`. (Adjust the snippet accordingly.)

Add `pub mod copilot;` to `src/llm/mod.rs`. Confirm `async_trait`, `chrono`, `wiremock` are available (they are — used elsewhere). If `celerex_orchestrator::llm::provider` is the wrong path, use the path the assistant/handler use to import `Provider`/`Completion`.

- [ ] **Step 2: Run** `cd "packages/backend/api" && cargo test --lib llm::copilot 2>&1 | tail -25` → FAIL→implement→PASS (2 tests). Then `cargo build --lib 2>&1 | tail -5`. (`#[allow(dead_code)]` on items not yet wired if the build denies warnings; removed in Tasks 2-3.)

- [ ] **Step 3: Commit**
```bash
git add packages/backend/api/src/llm/copilot.rs packages/backend/api/src/llm/mod.rs
git commit -m "feat(agent): Copilot OAuth client (device flow + token exchange) + CopilotProvider"
```

### Task 2: Device-flow endpoints + background poll + vault store

**Files:** Create `packages/backend/api/src/db/migrations/0020_copilot_logins.sql`, `src/design/copilot.rs`; Modify `src/db/mod.rs`, `src/design/mod.rs` (add `pub mod copilot;`), `src/lib.rs` (routes)

- [ ] **Step 1: Migration** `0020_copilot_logins.sql` (mirrors `design_pairings`, one in-flight login per engagement):
```sql
CREATE TABLE copilot_logins (
    engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE,
    device_code TEXT NOT NULL,
    user_code TEXT NOT NULL,
    verification_uri TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending', -- pending | connected | denied | expired | error
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 2: DB fns in `db/mod.rs`**
```rust
pub async fn upsert_copilot_login(pool: &PgPool, engagement_id: Uuid, device_code: &str, user_code: &str, verification_uri: &str, expires_at: chrono::DateTime<chrono::Utc>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO copilot_logins (engagement_id, device_code, user_code, verification_uri, status, expires_at)
         VALUES ($1,$2,$3,$4,'pending',$5)
         ON CONFLICT (engagement_id) DO UPDATE SET device_code=$2, user_code=$3, verification_uri=$4, status='pending', expires_at=$5, updated_at=NOW()",
    ).bind(engagement_id).bind(device_code).bind(user_code).bind(verification_uri).bind(expires_at).execute(pool).await?;
    Ok(())
}
pub async fn set_copilot_login_status(pool: &PgPool, engagement_id: Uuid, status: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE copilot_logins SET status=$2, updated_at=NOW() WHERE engagement_id=$1")
        .bind(engagement_id).bind(status).execute(pool).await?;
    Ok(())
}
pub async fn get_copilot_login(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<Option<(String, String, String)>> {
    // (user_code, verification_uri, status)
    sqlx::query_as::<_, (String, String, String)>("SELECT user_code, verification_uri, status FROM copilot_logins WHERE engagement_id=$1")
        .bind(engagement_id).fetch_optional(pool).await
}
```

- [ ] **Step 3: Endpoints `src/design/copilot.rs`** (mirror `design/deploy.rs` handler style; `AuthClaims`):
```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use crate::auth::extractors::AuthClaims;
use crate::{db, AppState};
use crate::llm::copilot::{CopilotOAuthClient, PollOutcome};

/// Allow tests to point the device flow at a stub.
fn oauth_client() -> CopilotOAuthClient {
    match (std::env::var("GITHUB_OAUTH_BASE"), std::env::var("GITHUB_API_BASE")) {
        (Ok(g), Ok(a)) => CopilotOAuthClient::with_bases(g, a),
        _ => CopilotOAuthClient::new(),
    }
}

/// POST /api/design/copilot/login/start — begin device flow, spawn the poll, return the code.
pub async fn start(AuthClaims(claims): AuthClaims, State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let client = oauth_client();
    let dc = client.start_device_flow().await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(dc.expires_in as i64);
    db::upsert_copilot_login(&state.pool, claims.engagement_id, &dc.device_code, &dc.user_code, &dc.verification_uri, expires_at)
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Fire-and-forget bounded poll.
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
                let _ = db::set_copilot_login_status(&pool, eng, "expired").await; break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            match client.poll_access_token(&device_code).await {
                PollOutcome::Token(ghu) => {
                    if keys.set(eng, "copilot", &ghu).await.is_ok() {
                        let _ = db::set_copilot_login_status(&pool, eng, "connected").await;
                    } else {
                        let _ = db::set_copilot_login_status(&pool, eng, "error").await;
                    }
                    break;
                }
                PollOutcome::SlowDown => { wait += 5; }
                PollOutcome::Pending => {}
                PollOutcome::Denied => { let _ = db::set_copilot_login_status(&pool, eng, "denied").await; break; }
                PollOutcome::Expired => { let _ = db::set_copilot_login_status(&pool, eng, "expired").await; break; }
                PollOutcome::Err(_) => { let _ = db::set_copilot_login_status(&pool, eng, "error").await; break; }
            }
        }
    });

    Ok(Json(json!({ "user_code": dc.user_code, "verification_uri": dc.verification_uri, "interval": dc.interval })))
}

/// GET /api/design/copilot/login/status — { status }.
pub async fn status(AuthClaims(claims): AuthClaims, State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let row = db::get_copilot_login(&state.pool, claims.engagement_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let status = row.map(|(_, _, s)| s).unwrap_or_else(|| "none".to_string());
    // Also report whether a vaulted token exists (connected across restarts).
    let connected = state.provider_keys.get(claims.engagement_id, "copilot").await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?.is_some();
    Ok(Json(json!({ "status": status, "connected": connected })))
}
```
Add `pub mod copilot;` to `src/design/mod.rs`; mount in `lib.rs`:
```rust
.route("/api/design/copilot/login/start", post(design::copilot::start))
.route("/api/design/copilot/login/status", get(design::copilot::status))
```

- [ ] **Step 4: Integration test** `tests/copilot_login.rs` (paste `token()`): stub GitHub device/code + access_token via wiremock, set `GITHUB_OAUTH_BASE`/`GITHUB_API_BASE` to the stub, `POST /api/design/copilot/login/start` (Bearer) → 200 with `user_code`; the access_token mock returns `ghu_abc` so the background poll stores it; `waitFor`-style: poll `GET /api/design/copilot/login/status` until `{ connected: true }` (loop with a short sleep, bounded ~20 tries); assert the vaulted token exists for A and NOT for a second engagement B (`get_settings`-style cross check via a second token). Unauth start → 401.

- [ ] **Step 5: Run** `cargo test --test copilot_login 2>&1 | tail -20 && cargo build --lib 2>&1 | tail -5` → PASS.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/db/migrations/0020_copilot_logins.sql packages/backend/api/src/db/mod.rs packages/backend/api/src/design/copilot.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/copilot_login.rs
git commit -m "feat(agent): Copilot device-flow login endpoints + background poll + vault store"
```

### Task 3: Register Copilot as a selectable per-engagement provider

**Files:** Modify `src/providers/store.rs` (SUPPORTED_PROVIDERS), `src/assistant/mod.rs` (build_engagement_provider), `src/llm/handler.rs` (EngagementLlmHandler::build_provider); Test `tests/copilot_provider.rs`

- [ ] **Step 1: Add `"copilot"` to `SUPPORTED_PROVIDERS`** in `src/providers/store.rs:29` (the array of allowed provider ids).

- [ ] **Step 2: Add the `copilot` arm** to `assistant::build_engagement_provider` (`src/assistant/mod.rs`):
```rust
"copilot" => Some(std::sync::Arc::new(crate::llm::copilot::CopilotProvider::new(key))),
```
and the equivalent arm in `EngagementLlmHandler::build_provider` (`src/llm/handler.rs`):
```rust
"copilot" => Some(std::sync::Arc::new(crate::llm::copilot::CopilotProvider::new(key))),
```
(`CopilotProvider::new(ghu)` uses real GitHub hosts; for tests, the provider's `with_bases` factory + the endpoints.api flow are already covered in Task 1, so this task's test can stub at the assistant boundary — see Step 3.)

- [ ] **Step 3: Test** `tests/copilot_provider.rs` — seed an engagement, store a `copilot` token via `state.provider_keys.set(eng, "copilot", "ghu_x")` (or the start-flow), then assert `resolve_engagement_provider(state, eng, "copilot")` returns `Some` (provider built) and that an engagement WITHOUT a copilot token returns `None`. (Full network completion is covered by Task 1's wiremock test; here assert the wiring/selection + that `"copilot"` is accepted by SUPPORTED_PROVIDERS.) If `resolve_engagement_provider` is private, test via the `POST /api/assistant/chat` path with `provider_id:"copilot"` against a stub — OR expose a thin `pub(crate)` test hook; pick the lowest-friction option and note it.

- [ ] **Step 4: Run** `cargo test --test copilot_provider 2>&1 | tail -15 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. Remove any leftover `#[allow(dead_code)]` now that the provider is wired.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/providers/store.rs packages/backend/api/src/assistant/mod.rs packages/backend/api/src/llm/handler.rs packages/backend/api/tests/copilot_provider.rs
git commit -m "feat(agent): register Copilot as a selectable per-engagement provider"
```

---

# Stage 2 — Web (`apps/web`, `celerex-design`)

### Task 4: Copilot login state helpers (direct Agent calls)

**Files:** Create `apps/web/src/state/copilot.ts`; Create `apps/web/tests/state/copilot.test.ts`

- [ ] **Step 1: Write the failing test** `apps/web/tests/state/copilot.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest";
import { startCopilotLogin, getCopilotLoginStatus } from "../../src/state/copilot";

function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as unknown as typeof fetch;
}

describe("copilot state", () => {
  it("startCopilotLogin POSTs with Bearer and returns user_code", async () => {
    const f = mockFetch({ user_code: "WXYZ-1234", verification_uri: "https://github.com/login/device", interval: 5 });
    const res = await startCopilotLogin("https://agent.example", "sess1", f);
    expect(res.user_code).toBe("WXYZ-1234");
    const [url, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe("https://agent.example/api/design/copilot/login/start");
    expect((init as RequestInit).method).toBe("POST");
    expect((init as RequestInit & { headers: Record<string,string> }).headers.Authorization).toBe("Bearer sess1");
  });
  it("getCopilotLoginStatus GETs status", async () => {
    const f = mockFetch({ status: "connected", connected: true });
    const res = await getCopilotLoginStatus("https://agent.example", "sess1", f);
    expect(res.connected).toBe(true);
    expect((f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![0]).toBe("https://agent.example/api/design/copilot/login/status");
  });
});
```

- [ ] **Step 2: Run (fails)** `cd "apps/web" && pnpm vitest run tests/state/copilot.test.ts 2>&1 | tail -20`

- [ ] **Step 3: Implement `apps/web/src/state/copilot.ts`**
```ts
export interface CopilotLoginStart { user_code: string; verification_uri: string; interval: number; }
export interface CopilotLoginStatus { status: string; connected: boolean; }

export async function startCopilotLogin(base: string, sessionToken: string, fetchFn: typeof fetch = fetch): Promise<CopilotLoginStart> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/copilot/login/start`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}`, "content-type": "application/json" },
  });
  if (!res.ok) throw new Error("Could not start GitHub Copilot login.");
  return res.json() as Promise<CopilotLoginStart>;
}

export async function getCopilotLoginStatus(base: string, sessionToken: string, fetchFn: typeof fetch = fetch): Promise<CopilotLoginStatus> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/copilot/login/status`, {
    method: "GET",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
  if (!res.ok) throw new Error("Could not check Copilot status.");
  return res.json() as Promise<CopilotLoginStatus>;
}
```

- [ ] **Step 4: Run (passes 2/2)** `pnpm vitest run tests/state/copilot.test.ts 2>&1 | tail -12`

- [ ] **Step 5: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/copilot.ts apps/web/tests/state/copilot.test.ts
git commit -m "feat(web): Copilot login state helpers (direct Agent calls)"
```

### Task 5: CopilotOAuthPanel + the CLI/BYOK/OAuth picker sections

**Files:** Create `apps/web/src/components/byok/CopilotOAuthPanel.tsx`, `apps/web/tests/components/CopilotOAuthPanel.test.tsx`; Modify `apps/web/src/components/SettingsDialog.tsx`

- [ ] **Step 1: Write the failing test** `apps/web/tests/components/CopilotOAuthPanel.test.tsx` (jsdom):
```tsx
// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../src/state/copilot", () => ({
  startCopilotLogin: vi.fn(async () => ({ user_code: "WXYZ-1234", verification_uri: "https://github.com/login/device", interval: 0 })),
  getCopilotLoginStatus: vi.fn(async () => ({ status: "connected", connected: true })),
}));
vi.mock("../../src/state/user-settings", () => ({
  readAgentSession: vi.fn(() => ({ token: "sess1", engagement: { id: "e", name: "w" } })),
  agentBaseUrl: vi.fn(() => "https://agent.example"),
}));
import { startCopilotLogin } from "../../src/state/copilot";
import { CopilotOAuthPanel } from "../../src/components/byok/CopilotOAuthPanel";

describe("CopilotOAuthPanel", () => {
  afterEach(() => { cleanup(); vi.clearAllMocks(); });
  it("starts login on click and shows the user code, then connected", async () => {
    render(<CopilotOAuthPanel pollIntervalMs={0} />);
    fireEvent.click(await screen.findByRole("button", { name: /log in with github copilot/i }));
    await waitFor(() => expect(startCopilotLogin).toHaveBeenCalled());
    expect(await screen.findByText(/WXYZ-1234/)).toBeTruthy();
    await waitFor(() => expect(screen.getByText(/connected/i)).toBeTruthy());
  });
  it("shows a signed-out hint when there is no Agent session", async () => {
    const us = await import("../../src/state/user-settings");
    (us.readAgentSession as unknown as ReturnType<typeof vi.fn>).mockReturnValueOnce(null);
    render(<CopilotOAuthPanel pollIntervalMs={0} />);
    expect(await screen.findByText(/sign in/i)).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run (fails)** `cd "apps/web" && pnpm vitest run tests/components/CopilotOAuthPanel.test.tsx 2>&1 | tail -20`

- [ ] **Step 3: Implement `apps/web/src/components/byok/CopilotOAuthPanel.tsx`**
```tsx
import { useRef, useState } from "react";
import { readAgentSession, agentBaseUrl } from "../../state/user-settings";
import { startCopilotLogin, getCopilotLoginStatus } from "../../state/copilot";

interface Props { pollIntervalMs?: number; }

export function CopilotOAuthPanel({ pollIntervalMs = 2500 }: Props) {
  const session = readAgentSession();
  const [userCode, setUserCode] = useState<string | null>(null);
  const [verifyUrl, setVerifyUrl] = useState<string | null>(null);
  const [status, setStatus] = useState<"idle" | "pending" | "connected" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const polling = useRef(false);

  if (!session) {
    return <p className="copilot-oauth__hint">Sign in to connect GitHub Copilot.</p>;
  }

  async function begin() {
    setError(null);
    setStatus("pending");
    try {
      const base = agentBaseUrl();
      const start = await startCopilotLogin(base, session!.token);
      setUserCode(start.user_code);
      setVerifyUrl(start.verification_uri);
      if (polling.current) return;
      polling.current = true;
      for (let i = 0; i < 200; i++) {
        const st = await getCopilotLoginStatus(base, session!.token);
        if (st.connected || st.status === "connected") { setStatus("connected"); break; }
        if (st.status === "denied" || st.status === "expired" || st.status === "error") { setStatus("error"); setError("Login " + st.status + "."); break; }
        await new Promise((r) => setTimeout(r, pollIntervalMs));
      }
      polling.current = false;
    } catch (e) {
      setStatus("error"); setError((e as Error).message); polling.current = false;
    }
  }

  return (
    <div className="copilot-oauth">
      <button type="button" onClick={() => void begin()} disabled={status === "pending"}>
        Log in with GitHub Copilot
      </button>
      {userCode ? (
        <p className="copilot-oauth__code">
          Go to <a href={verifyUrl ?? "https://github.com/login/device"} target="_blank" rel="noreferrer">github.com/login/device</a> and enter <strong>{userCode}</strong>
        </p>
      ) : null}
      {status === "connected" ? <p className="copilot-oauth__ok">GitHub Copilot connected.</p> : null}
      {error ? <p className="copilot-oauth__err">{error}</p> : null}
    </div>
  );
}
```

- [ ] **Step 4: Run (passes 2/2)** `pnpm vitest run tests/components/CopilotOAuthPanel.test.tsx 2>&1 | tail -15`

- [ ] **Step 5: Wire the third section into `SettingsDialog.tsx`** (read around lines 3283-3368 first). Two edits:
  1. Add a third segment button after the `mode === 'api'` button (~line 3323), mirroring the existing seg-btn markup:
     ```tsx
     <button
       type="button"
       className={`seg-btn${cfg.mode === 'oauth' ? ' is-active' : ''}`}
       role="tab"
       aria-selected={cfg.mode === 'oauth'}
       onClick={() => setCfg({ ...cfg, mode: 'oauth' })}
       data-testid="settings-mode-oauth"
     >
       OAuth
     </button>
     ```
     (Match the exact className/active-state convention of the two existing seg-btns — copy their structure. Confirm `cfg.mode`'s type accepts `'oauth'`; if it's a union in `types.ts`, add `'oauth'` to it.)
  2. Add the panel after the `cfg.mode === 'api'` block (~line 3366):
     ```tsx
     {cfg.mode === 'oauth' ? (
       <div className="oauth-section">
         <CopilotOAuthPanel />
       </div>
     ) : null}
     ```
  Import `CopilotOAuthPanel` at the top. Also re-label the existing two segments so the trio reads **CLI / BYOK / OAuth** (the `daemon` button → "CLI", the `api` button → "BYOK") — adjust their visible labels (and/or i18n keys) minimally; the `daemon` (CLI) button keeps its existing `disabled={!daemonLive}` (greyed-out in the browser, exactly the requirement).

- [ ] **Step 6: Add the `'oauth'` mode to the `AppConfig` mode union** if `types.ts` types `mode` as `'api' | 'daemon'` — extend to `'api' | 'daemon' | 'oauth'`. Run `pnpm tsc -b --noEmit` to confirm no type errors from the new mode.

- [ ] **Step 7: Typecheck + full suite** `cd "apps/web" && pnpm tsc -b --noEmit 2>&1 | tail -12 && pnpm vitest run 2>&1 | tail -25` → clean; suite green except the known pre-existing `default-background` (3) failures. If any SettingsDialog test asserts only two segments, update it to allow the third.

- [ ] **Step 8: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/components/byok/CopilotOAuthPanel.tsx apps/web/tests/components/CopilotOAuthPanel.test.tsx apps/web/src/components/SettingsDialog.tsx apps/web/src/state/config.ts apps/web/src/types.ts
git commit -m "feat(web): GitHub Copilot OAuth panel + CLI/BYOK/OAuth picker sections"
```

---

## Final verification
- [ ] Agent: `cd "packages/backend/api" && cargo test --lib llm::copilot && cargo test --test copilot_login --test copilot_provider 2>&1 | tail -15` → PASS.
- [ ] Web: `cd "apps/web" && pnpm vitest run tests/state/copilot.test.ts tests/components/CopilotOAuthPanel.test.tsx 2>&1 | tail -10 && pnpm tsc -b --noEmit` → green/clean.
- [ ] Stages shipped as branch + PR + merge per repo (Agent = Celerex; web = celerex-design).

## Self-review (spec coverage)
- Server-side device flow + store ghu_ in vault scoped by engagement → Tasks 1-2. ✓
- Two-step token exchange (copilot_internal/v2/token → endpoints.api → chat/completions + editor headers) → Task 1 (`CopilotProvider`). ✓
- Editor client_id `Iv1.b507a08c87ecfe98` → Task 1. ✓
- Copilot a selectable per-engagement provider → Task 3. ✓
- Per-user/server-held; creds never leave the vault → vault store (Task 2); web only sees user_code/status, never the token. ✓
- Web "Log in with GitHub Copilot" panel (shows user_code, polls to connected) → Tasks 4-5. ✓
- Picker reorganized into CLI / BYOK / OAuth (CLI greyed-out in browser via daemonLive) → Task 5. ✓
- wiremock-stubbed tests (device-flow, exchange+completion, cross-engagement isolation) → Tasks 1-3. ✓
- **No seam/daemon proxy** (recon: web calls the Agent directly with the session Bearer, like settings) — a deliberate simplification vs the spec's 4-layer spine; a daemon proxy is an optional future for desktop-only credential storage. ✓
- Deferred (spec §6): model-list browsing, refresh/disconnect polish, business-vs-individual beyond endpoints.api, Codex OAuth. ✓
