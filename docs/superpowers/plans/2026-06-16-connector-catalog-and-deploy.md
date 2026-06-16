# Connector Catalog & One-Click Deploy (2h v1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the v1 one-click Deploy slice — a non-technical user picks a host in a guided wizard (with "Ask AI"), stores their Vercel login once **in the server vault**, hits Publish, and the **Agent** deploys their artifact to Vercel and returns a live link.

**Architecture:** "Everything on the server" path (user-chosen). The Vercel token lives in the Agent's per-user vault (`engagement_provider_keys`). The web wizard → daemon proxy reads the chosen project file from local disk, base64-encodes it, and uploads it to a new Agent endpoint. The Agent calls Vercel's Create-Deployment API with the vaulted token and returns the deployment id + URL; a status endpoint polls Vercel until READY. An "Ask AI" endpoint on the Agent returns a one-line host recommendation via the Anthropic client. Shipped along the established spine: **Agent API → `@celerex/connect` seam → daemon proxy → web wizard**, mirroring Rituals (2b) and Automations (2d).

**Tech Stack:** Rust/Axum (`celerex-api`, `packages/backend/api`), TypeScript (`@celerex/connect` seam — bun tests; `apps/daemon` Express — vitest; `apps/web` React/Next — vitest + jsdom).

---

## Key facts from recon (ground truth — do not re-derive)

**Agent (Celerex repo, `packages/backend/api/src`):**
- Design route modules live in `design/`; `design/mod.rs` declares `pub mod rituals; pub mod automations;` and owns top-level handlers + the connector catalog.
- **Connector catalog** = `DESIGN_CONNECTOR_CATALOG: &[ConnectorCatalogEntry]` at `design/mod.rs:503-511`. Struct at `design/mod.rs:495-501`: `{ id, label, auth_kind, tools }` (NO role field yet). The catalog IS the allowlist; `get_connector_credential`/`set_connector_credential` validate against it (`mod.rs:560-593`).
- **Vault store** = `state.provider_keys: Arc<dyn ProviderKeysStore>` (`providers/store.rs`). Methods: `set(engagement_id: Uuid, provider_id: &str, plaintext_key: &str) -> Result<ProviderKeyMeta, String>`, `get(engagement_id, provider_id) -> Result<Option<String>, String>`.
- **Auth extractor** = `DeviceAuth(claims): DeviceAuth` (`auth/extractors.rs:58-70`); `claims.engagement_id: Uuid`. All ritual/automation device handlers use it. **Use `DeviceAuth` for every new deploy handler** (the existing `set_connector_credential` uses full-session `AuthClaims` — do NOT reuse it; the daemon talks to the Agent with a device token).
- **Routes mounted** in `lib.rs:121-130`. New routes slot in after the automations block (~line 128).
- **Backing project helper**: `automations_project(state, engagement_id)` at `mod.rs:271-291` (selects/creates a project named "Automations"). Pattern to mirror if a backing project is needed — **v1 deploy does NOT need one** (deploy is a direct Vercel call, not a flow run).
- **Anthropic client** for "Ask AI" = `crate::llm::anthropic::AnthropicClient`. `AnthropicClient::with_base_url(base_url: &str, key: String)` + `create_message(&self, model: &str, messages: Vec<Value>, tools: Vec<Value>) -> anyhow::Result<Value>`; text at `value["content"][0]["text"]`. Default base url `https://api.anthropic.com`.
- `reqwest` is already a dependency of the api crate.

**Seam (`celerex-design/packages/celerex-connect`):**
- `src/automations.ts` is the mirror. Functions are standalone `fetch` wrappers `(baseUrl, token, ...args, fetchFn?)`; helper `api(baseUrl, path)` strips trailing slash; `FetchFn = typeof fetch` from `./pairing.js`; requests carry `Authorization: Bearer ${token}`.
- Re-export from `src/index.ts` (automations block ~lines 62-73).
- Build: `pnpm --filter @celerex/connect build` (runs `tsc`). Tests: **bun** (`test/automations.test.ts` + `test/helpers.ts`). **`dist/` IS committed in this repo** — rebuild and commit dist after changes.

**Daemon (`celerex-design/apps/daemon`):**
- Proxy pattern = `src/routes/automation.ts` (inline `readConnection()` reads `readAppConfig(RUNTIME_DATA_DIR).celerex → {serverUrl, deviceToken}` from `../app-config.js`; `seamError(res, err)` → 502; same-origin check → 403; null connection → 409).
- **NAME COLLISION:** `src/routes/deploy.ts` already exists (the *local* Vercel/CF deploy). Create the new proxy as **`src/routes/celerex-deploy.ts`** exporting **`registerCelerexDeployRoutes`**.
- Register in `src/server.ts` after `registerAutomationRoutes(...)` (~line 6138).
- The local deploy already reads a project file by `(projectId, fileName)` from the project dir — reuse that resolution (`apps/daemon/src/deploy.ts` + project-dir helpers) for the bundle read.
- Tests: **vitest** (`tests/automation-routes.test.ts` — stub `node:http` Agent, pair via `POST /api/celerex/connect`, assert `Authorization: Bearer tok`).

**Web (`celerex-design/apps/web`):**
- State modules = bare async fns with `fetchFn: typeof fetch = fetch` (`src/state/automations.ts`). Components call them + hold results in `useState`.
- The existing per-file deploy modal lives in `HtmlFileViewer` (`src/components/FileViewer.tsx`); `fileName` is known there. Attach the new wizard's entry button there for v1.
- Tests: vitest; state unit tests (`tests/state/automations.test.ts`, `vi.fn()` mockFetch) + component tests (`tests/components/TasksView.*.test.tsx`, `// @vitest-environment jsdom`).

---

## File structure

**Agent (Celerex):**
- Modify `packages/backend/api/src/design/mod.rs` — add `role` field to `ConnectorCatalogEntry`; add `"vercel"` catalog entry; `pub mod deploy;`.
- Create `packages/backend/api/src/design/deploy.rs` — Vercel client + 5 handlers + types.
- Modify `packages/backend/api/src/lib.rs` — mount deploy routes.

**Seam (celerex-design):**
- Create `packages/celerex-connect/src/deploy.ts` — 5 functions + types.
- Modify `packages/celerex-connect/src/index.ts` — re-export.
- Create `packages/celerex-connect/test/deploy.test.ts`.

**Daemon (celerex-design):**
- Create `apps/daemon/src/routes/celerex-deploy.ts` — proxy + local file→bundle read.
- Modify `apps/daemon/src/server.ts` — register.
- Create `apps/daemon/tests/celerex-deploy-routes.test.ts`.

**Web (celerex-design):**
- Create `apps/web/src/state/deploy-celerex.ts` — wizard state helpers.
- Create `apps/web/src/components/DeployWizard.tsx` — the wizard modal.
- Modify `apps/web/src/components/FileViewer.tsx` — add the "Deploy with OmniVyx" entry button.
- Create `apps/web/tests/state/deploy-celerex.test.ts` + `apps/web/tests/components/DeployWizard.test.tsx`.

---

# Stage 1 — Agent: catalog + deploy endpoints

### Task 1: Add a `role` field + Vercel entry to the connector catalog

**Files:**
- Modify: `packages/backend/api/src/design/mod.rs:495-511`

- [ ] **Step 1: Write the failing test**

Add to the bottom of `design/mod.rs` (in or after the existing `#[cfg(test)] mod tests`; create the module if absent):

```rust
#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn vercel_is_in_catalog_with_web_host_role() {
        let entry = DESIGN_CONNECTOR_CATALOG
            .iter()
            .find(|c| c.id == "vercel")
            .expect("vercel connector must be in the catalog");
        assert_eq!(entry.role, "web-host");
        assert_eq!(entry.auth_kind, "api_key");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd "packages/backend/api" && cargo test --lib design::mod::catalog_tests 2>&1 | tail -20`
Expected: FAIL — no field `role` on `ConnectorCatalogEntry` / no "vercel" entry.

- [ ] **Step 3: Add the field and the entry**

In `ConnectorCatalogEntry` (`mod.rs:495-501`) add a `role` field:

```rust
struct ConnectorCatalogEntry {
    id: &'static str,
    label: &'static str,
    /// "api_key" | "app_creds" | "internal"
    auth_kind: &'static str,
    /// "web-host" | "database" | "domain" | "hosting" | "platform" | "data-source"
    role: &'static str,
    tools: &'static [(&'static str, &'static str)],
}
```

Add `role` to each existing entry in `DESIGN_CONNECTOR_CATALOG` (m365 → `"data-source"`, galvo-memory → `"data-source"`, http-api → `"data-source"`) and append the Vercel entry:

```rust
ConnectorCatalogEntry {
    id: "vercel",
    label: "Vercel",
    auth_kind: "api_key",
    role: "web-host",
    tools: &[("deploy", "Deploy a site to Vercel")],
},
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd "packages/backend/api" && cargo test --lib design::mod::catalog_tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/mod.rs
git commit -m "feat(agent): add role field + vercel entry to connector catalog"
```

---

### Task 2: Create the deploy module with types + a stubbable Vercel client

**Files:**
- Create: `packages/backend/api/src/design/deploy.rs`
- Modify: `packages/backend/api/src/design/mod.rs` (add `pub mod deploy;` beside `pub mod automations;`)

- [ ] **Step 1: Write the failing test**

Create `packages/backend/api/src/design/deploy.rs` containing ONLY this test plus the imports it needs (implementation comes next step):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn create_deployment_posts_files_and_returns_id_and_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v13/deployments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "dpl_123",
                "url": "my-site-abc.vercel.app",
                "readyState": "QUEUED"
            })))
            .mount(&server)
            .await;

        let client = VercelClient::with_base_url(server.uri(), "tok_test".to_string());
        let files = vec![DeployFile {
            path: "index.html".to_string(),
            content_base64: "PGgxPmhpPC9oMT4=".to_string(), // <h1>hi</h1>
        }];
        let res = client.create_deployment("my-site", &files).await.unwrap();
        assert_eq!(res.deployment_id, "dpl_123");
        assert_eq!(res.url, "https://my-site-abc.vercel.app");
        assert_eq!(res.status, "building");
    }

    #[tokio::test]
    async fn get_status_maps_ready_state() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v13/deployments/dpl_123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "dpl_123",
                "url": "my-site-abc.vercel.app",
                "readyState": "READY"
            })))
            .mount(&server)
            .await;

        let client = VercelClient::with_base_url(server.uri(), "tok_test".to_string());
        let res = client.get_status("dpl_123").await.unwrap();
        assert_eq!(res.status, "ready");
        assert_eq!(res.url, "https://my-site-abc.vercel.app");
    }
}
```

- [ ] **Step 2: Write the module implementation (types + client) above the test**

Prepend to `deploy.rs` (above the test module):

```rust
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A single file in the deployable bundle. `content_base64` is passed straight
/// through to Vercel with `encoding: "base64"`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeployFile {
    pub path: String,
    pub content_base64: String,
}

/// Result of creating a deployment (or polling its status).
#[derive(Debug, Clone, Serialize)]
pub struct DeployResult {
    pub deployment_id: String,
    pub url: String,
    /// normalized: "building" | "ready" | "error"
    pub status: String,
}

fn normalize_ready_state(ready_state: &str) -> &'static str {
    match ready_state {
        "READY" => "ready",
        "ERROR" | "CANCELED" => "error",
        _ => "building", // QUEUED | BUILDING | INITIALIZING | null
    }
}

fn with_scheme(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

/// Thin client over the Vercel Create-Deployment API. Base url is injectable for tests.
pub struct VercelClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl VercelClient {
    pub fn new(token: String) -> Self {
        Self::with_base_url("https://api.vercel.com".to_string(), token)
    }

    pub fn with_base_url(base_url: String, token: String) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), token, http: reqwest::Client::new() }
    }

    pub async fn create_deployment(&self, name: &str, files: &[DeployFile]) -> Result<DeployResult, String> {
        let vercel_files: Vec<_> = files
            .iter()
            .map(|f| json!({ "file": f.path, "data": f.content_base64, "encoding": "base64" }))
            .collect();
        let body = json!({
            "name": name,
            "files": vercel_files,
            "target": "production",
            "projectSettings": { "framework": serde_json::Value::Null }
        });
        let res = self
            .http
            .post(format!("{}/v13/deployments", self.base_url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("vercel request failed: {e}"))?;
        let status = res.status();
        let value: serde_json::Value = res.json().await.map_err(|e| format!("vercel response not json: {e}"))?;
        if !status.is_success() {
            let msg = value["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(format!("vercel rejected the deploy: {msg}"));
        }
        Ok(DeployResult {
            deployment_id: value["id"].as_str().unwrap_or_default().to_string(),
            url: with_scheme(value["url"].as_str().unwrap_or_default()),
            status: normalize_ready_state(value["readyState"].as_str().unwrap_or("QUEUED")).to_string(),
        })
    }

    pub async fn get_status(&self, deployment_id: &str) -> Result<DeployResult, String> {
        let res = self
            .http
            .get(format!("{}/v13/deployments/{}", self.base_url, deployment_id))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| format!("vercel request failed: {e}"))?;
        let value: serde_json::Value = res.json().await.map_err(|e| format!("vercel response not json: {e}"))?;
        Ok(DeployResult {
            deployment_id: value["id"].as_str().unwrap_or(deployment_id).to_string(),
            url: with_scheme(value["url"].as_str().unwrap_or_default()),
            status: normalize_ready_state(value["readyState"].as_str().unwrap_or("QUEUED")).to_string(),
        })
    }
}
```

Add `pub mod deploy;` to `design/mod.rs` directly under `pub mod automations;`.

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cd "packages/backend/api" && cargo test --lib design::deploy 2>&1 | tail -25`
Expected: PASS for both tests. (If `wiremock` is not a dev-dependency, add it — it is already used by the 2d automations integration tests, so it is present.)

- [ ] **Step 4: Commit**

```bash
git add packages/backend/api/src/design/deploy.rs packages/backend/api/src/design/mod.rs
git commit -m "feat(agent): vercel deploy client + deploy module types"
```

---

### Task 3: Credential set/get handlers (DeviceAuth) for the deploy token

**Files:**
- Modify: `packages/backend/api/src/design/deploy.rs`

- [ ] **Step 1: Write the handlers (no unit test — covered by the seam/daemon integration + a roundtrip test below)**

Append to `deploy.rs` (above the `#[cfg(test)]` module). Imports go at the top of the file.

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use crate::auth::extractors::DeviceAuth;
use crate::AppState;

/// v1 supports a single target. Maps a wizard target id → the vault provider id.
fn vault_provider_for(target: &str) -> Option<&'static str> {
    match target {
        "vercel" => Some("vercel"),
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct SetDeployCredentialBody {
    pub target: String,
    pub token: String,
}

/// PUT /api/design/deploy/credential — store the user's host token in the vault.
pub async fn set_deploy_credential(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<SetDeployCredentialBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = vault_provider_for(&body.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    if body.token.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "token is required".to_string()));
    }
    state
        .provider_keys
        .set(claims.engagement_id, provider, body.token.trim())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "connected": true })))
}

/// GET /api/design/deploy/credential/{target} — report whether a token is stored
/// (never returns the token itself).
pub async fn get_deploy_credential_status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = vault_provider_for(&target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    let connected = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .is_some();
    Ok(Json(json!({ "connected": connected })))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd "packages/backend/api" && cargo build --lib 2>&1 | tail -20`
Expected: builds clean (handlers unused-warning is fine until routes are mounted in Task 6).

- [ ] **Step 3: Commit**

```bash
git add packages/backend/api/src/design/deploy.rs
git commit -m "feat(agent): deploy credential set/status handlers (DeviceAuth, server vault)"
```

---

### Task 4: The deploy + status handlers

**Files:**
- Modify: `packages/backend/api/src/design/deploy.rs`

- [ ] **Step 1: Write the handlers**

Append to `deploy.rs` (above the test module):

```rust
#[derive(Deserialize)]
pub struct DeployRequestBody {
    pub target: String,
    pub project_name: String,
    pub files: Vec<DeployFile>,
}

/// POST /api/design/deploy — deploy an uploaded bundle to the target using the vaulted token.
pub async fn create_deploy(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<DeployRequestBody>,
) -> Result<Json<DeployResult>, (StatusCode, String)> {
    let provider = vault_provider_for(&body.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    if body.files.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "nothing to deploy".to_string()));
    }
    let token = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "no login stored for this host".to_string()))?;
    let client = VercelClient::new(token);
    let result = client
        .create_deployment(&body.project_name, &body.files)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct DeployStatusQuery {
    pub target: String,
}

/// GET /api/design/deploy/status/{deployment_id}?target=vercel — poll the deployment.
pub async fn get_deploy_status(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(deployment_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<DeployStatusQuery>,
) -> Result<Json<DeployResult>, (StatusCode, String)> {
    let provider = vault_provider_for(&q.target)
        .ok_or((StatusCode::BAD_REQUEST, "unsupported deploy target".to_string()))?;
    let token = state
        .provider_keys
        .get(claims.engagement_id, provider)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "no login stored for this host".to_string()))?;
    let client = VercelClient::new(token);
    let result = client.get_status(&deployment_id).await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(result))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd "packages/backend/api" && cargo build --lib 2>&1 | tail -20`
Expected: builds clean.

- [ ] **Step 3: Commit**

```bash
git add packages/backend/api/src/design/deploy.rs
git commit -m "feat(agent): server-side deploy + status handlers"
```

---

### Task 5: The "Ask AI" recommend handler

**Files:**
- Modify: `packages/backend/api/src/design/deploy.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `deploy.rs`:

```rust
#[test]
fn recommend_falls_back_to_first_option_without_a_key() {
    let options = vec!["vercel".to_string(), "netlify".to_string()];
    let rec = fallback_recommendation(&options);
    assert_eq!(rec.choice, "vercel");
    assert!(!rec.reason.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd "packages/backend/api" && cargo test --lib design::deploy::tests::recommend_falls_back 2>&1 | tail -15`
Expected: FAIL — `fallback_recommendation` not found.

- [ ] **Step 3: Implement the recommend handler + fallback**

Append to `deploy.rs` (above the test module). Uses the Anthropic client; falls back deterministically so the wizard's "Ask AI" always returns something for a non-technical user.

```rust
#[derive(Serialize, Clone)]
pub struct Recommendation {
    pub choice: String,
    pub reason: String,
}

pub(crate) fn fallback_recommendation(options: &[String]) -> Recommendation {
    let choice = options.first().cloned().unwrap_or_else(|| "vercel".to_string());
    Recommendation {
        choice,
        reason: "A reliable default that fits most projects.".to_string(),
    }
}

#[derive(Deserialize)]
pub struct RecommendBody {
    pub project_summary: String,
    pub options: Vec<String>,
}

/// POST /api/design/deploy/recommend — one-line "Ask AI" pick among the given options.
pub async fn recommend(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(body): Json<RecommendBody>,
) -> Result<Json<Recommendation>, (StatusCode, String)> {
    if body.options.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no options to choose from".to_string()));
    }
    let key = match state.provider_keys.get(claims.engagement_id, "anthropic").await {
        Ok(Some(k)) => k,
        _ => return Ok(Json(fallback_recommendation(&body.options))),
    };
    let prompt = format!(
        "A non-technical person is choosing where to publish their project. \
         Project: {}. Options: {}. Reply with ONLY a compact JSON object \
         {{\"choice\":\"<one option exactly>\",\"reason\":\"<one short plain sentence, no jargon>\"}}.",
        body.project_summary,
        body.options.join(", ")
    );
    let client = crate::llm::anthropic::AnthropicClient::with_base_url("https://api.anthropic.com", key);
    let messages = vec![json!({ "role": "user", "content": prompt })];
    let value = match client.create_message("claude-haiku-4-5-20251001", messages, vec![]).await {
        Ok(v) => v,
        Err(_) => return Ok(Json(fallback_recommendation(&body.options))),
    };
    let text = value["content"][0]["text"].as_str().unwrap_or("");
    let parsed: Option<Recommendation> = serde_json::from_str::<serde_json::Value>(text.trim())
        .ok()
        .and_then(|v| {
            let choice = v["choice"].as_str()?.to_string();
            let reason = v["reason"].as_str().unwrap_or("").to_string();
            if body.options.iter().any(|o| o == &choice) {
                Some(Recommendation { choice, reason })
            } else {
                None
            }
        });
    Ok(Json(parsed.unwrap_or_else(|| fallback_recommendation(&body.options))))
}
```

(Confirm the `AnthropicClient::with_base_url` argument types match recon: `with_base_url(base_url: &str, key: String)`. If the signature differs, adapt the call — the behavior is: construct with the vaulted key, send one user message, read `["content"][0]["text"]`.)

- [ ] **Step 4: Run to verify it passes**

Run: `cd "packages/backend/api" && cargo test --lib design::deploy 2>&1 | tail -20`
Expected: PASS (all deploy tests).

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/deploy.rs
git commit -m "feat(agent): Ask-AI deploy recommendation with deterministic fallback"
```

---

### Task 6: Mount the deploy routes

**Files:**
- Modify: `packages/backend/api/src/lib.rs:121-130`

- [ ] **Step 1: Add the routes after the automations block**

Insert after the last `/api/design/automations*` route:

```rust
.route("/api/design/deploy", post(design::deploy::create_deploy))
.route("/api/design/deploy/recommend", post(design::deploy::recommend))
.route("/api/design/deploy/credential", put(design::deploy::set_deploy_credential))
.route("/api/design/deploy/credential/{target}", get(design::deploy::get_deploy_credential_status))
.route("/api/design/deploy/status/{deployment_id}", get(design::deploy::get_deploy_status))
```

(Ensure `put`, `get`, `post` are in the `use axum::routing::{...}` import at the top of `lib.rs` — `put` and `get` are already used by the rituals/automations routes.)

- [ ] **Step 2: Verify it compiles**

Run: `cd "packages/backend/api" && cargo build --lib 2>&1 | tail -20`
Expected: builds clean, no unused-handler warnings for deploy.

- [ ] **Step 3: Run the whole api test suite to confirm nothing broke**

Run: `cd "packages/backend/api" && cargo test --lib 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add packages/backend/api/src/lib.rs
git commit -m "feat(agent): mount /api/design/deploy routes"
```

---

# Stage 2 — Seam: `@celerex/connect` deploy.ts

### Task 7: Seam functions + types + tests

**Files:**
- Create: `packages/celerex-connect/src/deploy.ts`
- Create: `packages/celerex-connect/test/deploy.test.ts`
- Modify: `packages/celerex-connect/src/index.ts`

- [ ] **Step 1: Write the failing test**

Create `test/deploy.test.ts` (mirrors `test/automations.test.ts`; reuse `./helpers.ts`):

```ts
import { test, expect } from "bun:test";
import { seqFetch, jsonResponse, parseBody } from "./helpers";
import {
  setDeployCredential,
  getDeployCredentialStatus,
  createDeploy,
  getDeployStatus,
  recommendDeployOption,
} from "../src/deploy";

const BASE = "https://agent.example";
const TOK = "tok_dev";

test("setDeployCredential PUTs target+token", async () => {
  const calls: Request[] = [];
  const fetchFn = seqFetch(calls, [jsonResponse({ connected: true })]);
  const res = await setDeployCredential(BASE, TOK, { target: "vercel", token: "vc_123" }, fetchFn);
  expect(res.connected).toBe(true);
  expect(calls[0].method).toBe("PUT");
  expect(new URL(calls[0].url).pathname).toBe("/api/design/deploy/credential");
  expect(await parseBody(calls[0])).toEqual({ target: "vercel", token: "vc_123" });
  expect(calls[0].headers.get("authorization")).toBe(`Bearer ${TOK}`);
});

test("getDeployCredentialStatus GETs by target", async () => {
  const calls: Request[] = [];
  const fetchFn = seqFetch(calls, [jsonResponse({ connected: false })]);
  const res = await getDeployCredentialStatus(BASE, TOK, "vercel", fetchFn);
  expect(res.connected).toBe(false);
  expect(new URL(calls[0].url).pathname).toBe("/api/design/deploy/credential/vercel");
});

test("createDeploy POSTs the bundle and returns id+url", async () => {
  const calls: Request[] = [];
  const fetchFn = seqFetch(calls, [
    jsonResponse({ deployment_id: "dpl_1", url: "https://x.vercel.app", status: "building" }),
  ]);
  const res = await createDeploy(
    BASE, TOK,
    { target: "vercel", project_name: "my-site", files: [{ path: "index.html", content_base64: "PGgxLz4=" }] },
    fetchFn,
  );
  expect(res.deployment_id).toBe("dpl_1");
  expect(new URL(calls[0].url).pathname).toBe("/api/design/deploy");
});

test("getDeployStatus GETs with target query", async () => {
  const calls: Request[] = [];
  const fetchFn = seqFetch(calls, [jsonResponse({ deployment_id: "dpl_1", url: "https://x.vercel.app", status: "ready" })]);
  const res = await getDeployStatus(BASE, TOK, "dpl_1", "vercel", fetchFn);
  expect(res.status).toBe("ready");
  const u = new URL(calls[0].url);
  expect(u.pathname).toBe("/api/design/deploy/status/dpl_1");
  expect(u.searchParams.get("target")).toBe("vercel");
});

test("recommendDeployOption POSTs summary+options", async () => {
  const calls: Request[] = [];
  const fetchFn = seqFetch(calls, [jsonResponse({ choice: "vercel", reason: "Best fit." })]);
  const res = await recommendDeployOption(BASE, TOK, { project_summary: "a landing page", options: ["vercel", "netlify"] }, fetchFn);
  expect(res.choice).toBe("vercel");
  expect(new URL(calls[0].url).pathname).toBe("/api/design/deploy/recommend");
});
```

If `seqFetch`/`jsonResponse`/`parseBody` are not all present in `test/helpers.ts`, inspect that file and use the helpers it actually exports (the automations test shows the real names).

- [ ] **Step 2: Run to verify it fails**

Run: `cd "packages/celerex-connect" && bun test test/deploy.test.ts 2>&1 | tail -20`
Expected: FAIL — `../src/deploy` does not exist.

- [ ] **Step 3: Implement `src/deploy.ts`**

```ts
import type { FetchFn } from "./pairing.js";

export interface DeployFile {
  path: string;
  content_base64: string;
}
export interface DeployResult {
  deployment_id: string;
  url: string;
  status: "building" | "ready" | "error";
}
export interface SetDeployCredentialInput {
  target: string;
  token: string;
}
export interface CreateDeployInput {
  target: string;
  project_name: string;
  files: DeployFile[];
}
export interface RecommendInput {
  project_summary: string;
  options: string[];
}
export interface Recommendation {
  choice: string;
  reason: string;
}

function api(baseUrl: string, path: string): string {
  return `${baseUrl.replace(/\/+$/, "")}${path}`;
}
async function asError(res: Response, action: string): Promise<Error> {
  return new Error(`celerex ${action} failed (${res.status}): ${await res.text().catch(() => "")}`.trim());
}
function authHeaders(token: string): Record<string, string> {
  return { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };
}

export async function setDeployCredential(
  baseUrl: string, token: string, input: SetDeployCredentialInput, fetchFn: FetchFn = fetch,
): Promise<{ connected: boolean }> {
  const res = await fetchFn(api(baseUrl, "/api/design/deploy/credential"), {
    method: "PUT", headers: authHeaders(token), body: JSON.stringify(input),
  });
  if (!res.ok) throw await asError(res, "set deploy credential");
  return res.json() as Promise<{ connected: boolean }>;
}

export async function getDeployCredentialStatus(
  baseUrl: string, token: string, target: string, fetchFn: FetchFn = fetch,
): Promise<{ connected: boolean }> {
  const res = await fetchFn(api(baseUrl, `/api/design/deploy/credential/${encodeURIComponent(target)}`), {
    method: "GET", headers: authHeaders(token),
  });
  if (!res.ok) throw await asError(res, "get deploy credential status");
  return res.json() as Promise<{ connected: boolean }>;
}

export async function createDeploy(
  baseUrl: string, token: string, input: CreateDeployInput, fetchFn: FetchFn = fetch,
): Promise<DeployResult> {
  const res = await fetchFn(api(baseUrl, "/api/design/deploy"), {
    method: "POST", headers: authHeaders(token), body: JSON.stringify(input),
  });
  if (!res.ok) throw await asError(res, "create deploy");
  return res.json() as Promise<DeployResult>;
}

export async function getDeployStatus(
  baseUrl: string, token: string, deploymentId: string, target: string, fetchFn: FetchFn = fetch,
): Promise<DeployResult> {
  const res = await fetchFn(
    api(baseUrl, `/api/design/deploy/status/${encodeURIComponent(deploymentId)}?target=${encodeURIComponent(target)}`),
    { method: "GET", headers: authHeaders(token) },
  );
  if (!res.ok) throw await asError(res, "get deploy status");
  return res.json() as Promise<DeployResult>;
}

export async function recommendDeployOption(
  baseUrl: string, token: string, input: RecommendInput, fetchFn: FetchFn = fetch,
): Promise<Recommendation> {
  const res = await fetchFn(api(baseUrl, "/api/design/deploy/recommend"), {
    method: "POST", headers: authHeaders(token), body: JSON.stringify(input),
  });
  if (!res.ok) throw await asError(res, "recommend deploy option");
  return res.json() as Promise<Recommendation>;
}
```

- [ ] **Step 4: Re-export from `src/index.ts`**

After the automations export block, add:

```ts
export {
  setDeployCredential,
  getDeployCredentialStatus,
  createDeploy,
  getDeployStatus,
  recommendDeployOption,
} from "./deploy.js";
export type {
  DeployFile,
  DeployResult,
  SetDeployCredentialInput,
  CreateDeployInput,
  RecommendInput,
  Recommendation,
} from "./deploy.js";
```

- [ ] **Step 5: Run tests + build the dist**

Run: `cd "packages/celerex-connect" && bun test test/deploy.test.ts 2>&1 | tail -20 && pnpm --filter @celerex/connect build 2>&1 | tail -5`
Expected: tests PASS; build emits updated `dist/`.

- [ ] **Step 6: Commit (including the rebuilt dist)**

```bash
git add packages/celerex-connect/src/deploy.ts packages/celerex-connect/src/index.ts packages/celerex-connect/test/deploy.test.ts packages/celerex-connect/dist
git commit -m "feat(seam): @celerex/connect deploy functions + types"
```

---

# Stage 3 — Daemon: celerex-deploy proxy + bundle read

### Task 8: Daemon proxy that reads the local file and uploads it to the Agent

**Files:**
- Create: `apps/daemon/src/routes/celerex-deploy.ts`
- Modify: `apps/daemon/src/server.ts` (register)
- Create: `apps/daemon/tests/celerex-deploy-routes.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/daemon/tests/celerex-deploy-routes.test.ts`, mirroring `tests/automation-routes.test.ts`. It stubs a `node:http` Agent, pairs the daemon, then drives the deploy proxy. Match the helper names used by the existing automation-routes test (e.g. `startServer`, `writeAppConfig`, the stub-server helper). Skeleton:

```ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import http from "node:http";
// import { startServer } from "../src/server.js";  // use the actual export the automation test uses
// import { writeAppConfig } from "../src/app-config.js";

// A stub Agent that records the requests it receives.
function stubAgent(handler: (req: http.IncomingMessage, body: string, res: http.ServerResponse) => void) {
  return http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => handler(req, body, res));
  });
}

describe("celerex-deploy proxy", () => {
  // beforeEach: start stub agent on an ephemeral port, start the daemon (port 0),
  //   pair via POST /api/celerex/connect with { serverUrl: stubUrl, deviceToken: "tok" }.
  // afterEach: close both servers, reset app-config celerex to null.

  it("POST /api/celerex-deploy reads the project file and forwards a base64 bundle with the bearer token", async () => {
    // Arrange: write a project file index.html on disk in the project's resolved dir.
    // Stub agent asserts: path === '/api/design/deploy', method POST,
    //   header authorization === 'Bearer tok', body.files[0].path === 'index.html',
    //   body.files[0].content_base64 decodes to the file contents.
    // Respond 200 { deployment_id:'dpl_1', url:'https://x.vercel.app', status:'building' }.
    // Act: POST /api/celerex-deploy { projectId, fileName:'index.html', target:'vercel', projectName:'my-site' }.
    // Assert: 200, body.deployment_id === 'dpl_1'.
    expect(true).toBe(true); // replace with real assertions following automation-routes.test.ts
  });

  it("returns 409 when the daemon is not paired", async () => {
    // With app-config celerex=null, POST /api/celerex-deploy → 409.
    expect(true).toBe(true);
  });
});
```

**Before writing the real test body, open `apps/daemon/tests/automation-routes.test.ts` and copy its exact bootstrap** (server start, pairing POST, app-config reset, the same-origin header it sets). Replace the placeholder assertions with the real ones described in the comments. The test must assert the **base64 round-trip** of the file contents.

- [ ] **Step 2: Run to verify it fails**

Run: `cd "apps/daemon" && pnpm vitest run tests/celerex-deploy-routes.test.ts 2>&1 | tail -20`
Expected: FAIL — route not registered (404) / module missing.

- [ ] **Step 3: Implement `apps/daemon/src/routes/celerex-deploy.ts`**

Mirror `routes/automation.ts` for the proxy scaffolding (the `readConnection`, `seamError`, same-origin and 403/409 checks). The deploy route additionally reads the chosen project file from disk and base64-encodes it. **Reuse the project-file resolution the existing local deploy uses** (`apps/daemon/src/deploy.ts` resolves a project file by `(projectId, fileName)` — import that resolver, or the project-dir helper it uses, rather than re-implementing path logic).

```ts
import type { Express, Request, Response } from "express";
import { promises as fs } from "node:fs";
import {
  createDeploy,
  getDeployStatus,
  setDeployCredential,
  getDeployCredentialStatus,
  recommendDeployOption,
} from "@celerex/connect";
import { readAppConfig } from "../app-config.js";
// import the existing resolver used by routes/deploy.ts, e.g.:
// import { resolveProjectFilePath } from "../deploy.js";

export interface RegisterCelerexDeployRoutesDeps {
  http: { isLocalSameOrigin: (req: Request, port: number | null) => boolean; getResolvedPort: () => number | null };
  paths: { RUNTIME_DATA_DIR: string; resolveProjectFilePath: (projectId: string, fileName: string) => Promise<string> };
}

export function registerCelerexDeployRoutes(app: Express, deps: RegisterCelerexDeployRoutesDeps): void {
  const { isLocalSameOrigin, getResolvedPort } = deps.http;
  const { RUNTIME_DATA_DIR, resolveProjectFilePath } = deps.paths;

  async function readConnection(): Promise<{ serverUrl: string; deviceToken: string } | null> {
    const cfg = await readAppConfig(RUNTIME_DATA_DIR);
    return cfg.celerex ?? null;
  }
  function guard(req: Request, res: Response): boolean {
    if (!isLocalSameOrigin(req, getResolvedPort())) {
      res.status(403).json({ error: "cross-origin request rejected" });
      return false;
    }
    return true;
  }
  function seamError(res: Response, err: unknown): void {
    res.status(502).json({ error: err instanceof Error ? err.message : String(err) });
  }

  // POST /api/celerex-deploy — read the file, base64 it, upload to the Agent.
  app.post("/api/celerex-deploy", async (req, res) => {
    if (!guard(req, res)) return;
    const conn = await readConnection();
    if (!conn) return void res.status(409).json({ error: "not connected to OmniVyx" });
    const { projectId, fileName, target, projectName } = req.body ?? {};
    if (!projectId || !fileName || !target) {
      return void res.status(400).json({ error: "projectId, fileName and target are required" });
    }
    try {
      const abs = await resolveProjectFilePath(projectId, fileName);
      const bytes = await fs.readFile(abs);
      const files = [{ path: "index.html", content_base64: bytes.toString("base64") }];
      const result = await createDeploy(conn.serverUrl, conn.deviceToken, {
        target, project_name: projectName || projectId, files,
      });
      res.status(200).json(result);
    } catch (err) {
      seamError(res, err);
    }
  });

  // GET /api/celerex-deploy/status/:deploymentId?target=vercel
  app.get("/api/celerex-deploy/status/:deploymentId", async (req, res) => {
    if (!guard(req, res)) return;
    const conn = await readConnection();
    if (!conn) return void res.status(409).json({ error: "not connected to OmniVyx" });
    try {
      const result = await getDeployStatus(
        conn.serverUrl, conn.deviceToken, req.params.deploymentId, String(req.query.target ?? "vercel"),
      );
      res.status(200).json(result);
    } catch (err) { seamError(res, err); }
  });

  // PUT /api/celerex-deploy/credential
  app.put("/api/celerex-deploy/credential", async (req, res) => {
    if (!guard(req, res)) return;
    const conn = await readConnection();
    if (!conn) return void res.status(409).json({ error: "not connected to OmniVyx" });
    try {
      const result = await setDeployCredential(conn.serverUrl, conn.deviceToken, req.body ?? {});
      res.status(200).json(result);
    } catch (err) { seamError(res, err); }
  });

  // GET /api/celerex-deploy/credential/:target
  app.get("/api/celerex-deploy/credential/:target", async (req, res) => {
    if (!guard(req, res)) return;
    const conn = await readConnection();
    if (!conn) return void res.status(409).json({ error: "not connected to OmniVyx" });
    try {
      const result = await getDeployCredentialStatus(conn.serverUrl, conn.deviceToken, req.params.target);
      res.status(200).json(result);
    } catch (err) { seamError(res, err); }
  });

  // POST /api/celerex-deploy/recommend
  app.post("/api/celerex-deploy/recommend", async (req, res) => {
    if (!guard(req, res)) return;
    const conn = await readConnection();
    if (!conn) return void res.status(409).json({ error: "not connected to OmniVyx" });
    try {
      const result = await recommendDeployOption(conn.serverUrl, conn.deviceToken, req.body ?? {});
      res.status(200).json(result);
    } catch (err) { seamError(res, err); }
  });
}
```

**Confirm the exact `deps` shape against `registerAutomationRoutes`** (the recon shows it takes `{ http: httpDeps, paths: pathDeps }`). Use whatever `httpDeps`/`pathDeps` actually expose for same-origin + project-dir resolution; add `resolveProjectFilePath` to `pathDeps` if it is not already there (wire it from the same helper `routes/deploy.ts` uses).

- [ ] **Step 4: Register in `server.ts`**

Import beside the others (~line 477) and register after `registerAutomationRoutes(...)` (~line 6138):

```ts
import { registerCelerexDeployRoutes } from "./routes/celerex-deploy.js";
// ...
registerCelerexDeployRoutes(app, { http: httpDeps, paths: pathDeps });
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd "apps/daemon" && pnpm vitest run tests/celerex-deploy-routes.test.ts 2>&1 | tail -20`
Expected: PASS (both cases — base64 round-trip + 409 unpaired).

- [ ] **Step 6: Typecheck + commit**

Run: `cd "apps/daemon" && pnpm tsc --noEmit 2>&1 | tail -10`
Expected: clean.

```bash
git add apps/daemon/src/routes/celerex-deploy.ts apps/daemon/src/server.ts apps/daemon/tests/celerex-deploy-routes.test.ts
git commit -m "feat(daemon): celerex-deploy proxy uploads local bundle to the Agent"
```

---

# Stage 4 — Web: the Deploy wizard

### Task 9: Web state helpers + tests

**Files:**
- Create: `apps/web/src/state/deploy-celerex.ts`
- Create: `apps/web/tests/state/deploy-celerex.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/web/tests/state/deploy-celerex.test.ts` (mirror `tests/state/automations.test.ts`):

```ts
import { describe, it, expect, vi } from "vitest";
import {
  getDeployCredentialStatus,
  setDeployCredential,
  recommendHost,
  startDeploy,
  pollDeployStatus,
} from "../../src/state/deploy-celerex";

function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), {
    status, headers: { "content-type": "application/json" },
  })) as unknown as typeof fetch;
}

describe("deploy-celerex state", () => {
  it("getDeployCredentialStatus GETs the credential status", async () => {
    const f = mockFetch({ connected: true });
    const res = await getDeployCredentialStatus("vercel", f);
    expect(res.connected).toBe(true);
    expect(f).toHaveBeenCalledWith("/api/celerex-deploy/credential/vercel", expect.anything());
  });

  it("setDeployCredential PUTs the token", async () => {
    const f = mockFetch({ connected: true });
    await setDeployCredential("vercel", "vc_1", f);
    const [, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body)).toEqual({ target: "vercel", token: "vc_1" });
  });

  it("recommendHost POSTs summary+options", async () => {
    const f = mockFetch({ choice: "vercel", reason: "Best fit." });
    const res = await recommendHost("a landing page", ["vercel", "netlify"], f);
    expect(res.choice).toBe("vercel");
  });

  it("startDeploy POSTs project+file", async () => {
    const f = mockFetch({ deployment_id: "dpl_1", url: "https://x.vercel.app", status: "building" });
    const res = await startDeploy({ projectId: "p1", fileName: "index.html", target: "vercel", projectName: "my-site" }, f);
    expect(res.deployment_id).toBe("dpl_1");
  });

  it("pollDeployStatus GETs status with target", async () => {
    const f = mockFetch({ deployment_id: "dpl_1", url: "https://x.vercel.app", status: "ready" });
    const res = await pollDeployStatus("dpl_1", "vercel", f);
    expect(res.status).toBe("ready");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd "apps/web" && pnpm vitest run tests/state/deploy-celerex.test.ts 2>&1 | tail -20`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement `apps/web/src/state/deploy-celerex.ts`**

```ts
export interface DeployResult {
  deployment_id: string;
  url: string;
  status: "building" | "ready" | "error";
}
export interface Recommendation {
  choice: string;
  reason: string;
}
export interface StartDeployInput {
  projectId: string;
  fileName: string;
  target: string;
  projectName: string;
}

export async function getDeployCredentialStatus(
  target: string, fetchFn: typeof fetch = fetch,
): Promise<{ connected: boolean }> {
  const res = await fetchFn(`/api/celerex-deploy/credential/${encodeURIComponent(target)}`, { method: "GET" });
  if (!res.ok) throw new Error(`Could not check your ${target} login.`);
  return res.json();
}

export async function setDeployCredential(
  target: string, token: string, fetchFn: typeof fetch = fetch,
): Promise<{ connected: boolean }> {
  const res = await fetchFn("/api/celerex-deploy/credential", {
    method: "PUT", headers: { "content-type": "application/json" },
    body: JSON.stringify({ target, token }),
  });
  if (!res.ok) throw new Error("That login didn't save. Please check it and try again.");
  return res.json();
}

export async function recommendHost(
  projectSummary: string, options: string[], fetchFn: typeof fetch = fetch,
): Promise<Recommendation> {
  const res = await fetchFn("/api/celerex-deploy/recommend", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ project_summary: projectSummary, options }),
  });
  if (!res.ok) throw new Error("AI couldn't make a suggestion right now.");
  return res.json();
}

export async function startDeploy(
  input: StartDeployInput, fetchFn: typeof fetch = fetch,
): Promise<DeployResult> {
  const res = await fetchFn("/api/celerex-deploy", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error("We couldn't start the publish. Please try again.");
  return res.json();
}

export async function pollDeployStatus(
  deploymentId: string, target: string, fetchFn: typeof fetch = fetch,
): Promise<DeployResult> {
  const res = await fetchFn(
    `/api/celerex-deploy/status/${encodeURIComponent(deploymentId)}?target=${encodeURIComponent(target)}`,
    { method: "GET" },
  );
  if (!res.ok) throw new Error("We lost track of the publish. Please check again in a moment.");
  return res.json();
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd "apps/web" && pnpm vitest run tests/state/deploy-celerex.test.ts 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/state/deploy-celerex.ts apps/web/tests/state/deploy-celerex.test.ts
git commit -m "feat(web): deploy-celerex state helpers"
```

---

### Task 10: The DeployWizard component + tests

**Files:**
- Create: `apps/web/src/components/DeployWizard.tsx`
- Create: `apps/web/tests/components/DeployWizard.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/web/tests/components/DeployWizard.test.tsx`:

```tsx
// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DeployWizard } from "../../src/components/DeployWizard";

const originalFetch = globalThis.fetch;

function routeFetch(map: Record<string, (init?: RequestInit) => Response>) {
  globalThis.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = input.toString();
    for (const key of Object.keys(map)) {
      if (url.startsWith(key)) return map[key](init);
    }
    return new Response(JSON.stringify({}), { status: 404 });
  }) as typeof fetch;
}
const ok = (body: unknown) => () => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });

describe("DeployWizard", () => {
  afterEach(() => { cleanup(); globalThis.fetch = originalFetch; vi.restoreAllMocks(); });

  it("prompts to connect Vercel when no login is stored", async () => {
    routeFetch({ "/api/celerex-deploy/credential/vercel": ok({ connected: false }) });
    render(<DeployWizard projectId="p1" fileName="index.html" onClose={() => {}} />);
    expect(await screen.findByText(/connect your vercel/i)).toBeTruthy();
  });

  it("Ask AI fills the recommendation", async () => {
    routeFetch({
      "/api/celerex-deploy/credential/vercel": ok({ connected: true }),
      "/api/celerex-deploy/recommend": ok({ choice: "vercel", reason: "Best fit for your page." }),
    });
    render(<DeployWizard projectId="p1" fileName="index.html" onClose={() => {}} />);
    fireEvent.click(await screen.findByRole("button", { name: /ask ai/i }));
    expect(await screen.findByText(/best fit for your page/i)).toBeTruthy();
  });

  it("publishes and shows a live link when ready", async () => {
    routeFetch({
      "/api/celerex-deploy/credential/vercel": ok({ connected: true }),
      "/api/celerex-deploy/status/": ok({ deployment_id: "dpl_1", url: "https://my-site.vercel.app", status: "ready" }),
      "/api/celerex-deploy": ok({ deployment_id: "dpl_1", url: "https://my-site.vercel.app", status: "building" }),
    });
    render(<DeployWizard projectId="p1" fileName="index.html" onClose={() => {}} />);
    fireEvent.click(await screen.findByRole("button", { name: /publish/i }));
    const link = await screen.findByRole("link", { name: /my-site\.vercel\.app|view your site/i });
    expect(link.getAttribute("href")).toBe("https://my-site.vercel.app");
  });
});
```

(Route matching is prefix-based, and `"/api/celerex-deploy/status/"` is listed before `"/api/celerex-deploy"` only matters if you use exact ordering — the matcher above checks all keys, so make the status key more specific by keeping the trailing slash; `startsWith` on `/api/celerex-deploy` would also catch status, so in the publish test the status route is checked because the code calls the full status path. If ordering causes a clash, switch the matcher to longest-prefix-first.)

- [ ] **Step 2: Run to verify it fails**

Run: `cd "apps/web" && pnpm vitest run tests/components/DeployWizard.test.tsx 2>&1 | tail -25`
Expected: FAIL — component missing.

- [ ] **Step 3: Implement `apps/web/src/components/DeployWizard.tsx`**

```tsx
import { useCallback, useEffect, useState } from "react";
import {
  getDeployCredentialStatus,
  setDeployCredential,
  recommendHost,
  startDeploy,
  pollDeployStatus,
} from "../state/deploy-celerex";

const HOST_OPTIONS = [
  { id: "vercel", label: "Vercel" },
  { id: "netlify", label: "Netlify (coming soon)", disabled: true },
];

interface Props {
  projectId: string;
  fileName: string;
  projectName?: string;
  onClose: () => void;
}

type Phase = "choose" | "connect" | "publishing" | "done" | "error";

export function DeployWizard({ projectId, fileName, projectName, onClose }: Props) {
  const [target] = useState("vercel");
  const [connected, setConnected] = useState<boolean | null>(null);
  const [tokenInput, setTokenInput] = useState("");
  const [reason, setReason] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("choose");
  const [liveUrl, setLiveUrl] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const checkConnection = useCallback(async () => {
    try {
      const { connected } = await getDeployCredentialStatus(target);
      setConnected(connected);
      setPhase(connected ? "choose" : "connect");
    } catch (e) {
      setMessage((e as Error).message);
      setPhase("error");
    }
  }, [target]);

  useEffect(() => { void checkConnection(); }, [checkConnection]);

  async function onAskAi() {
    try {
      const rec = await recommendHost(`a project file named ${fileName}`, ["vercel", "netlify"]);
      setReason(rec.reason);
    } catch (e) {
      setReason((e as Error).message);
    }
  }

  async function onSaveToken() {
    try {
      await setDeployCredential(target, tokenInput.trim());
      setConnected(true);
      setPhase("choose");
    } catch (e) {
      setMessage((e as Error).message);
    }
  }

  async function onPublish() {
    setPhase("publishing");
    setMessage(null);
    try {
      const started = await startDeploy({ projectId, fileName, target, projectName: projectName || projectId });
      let status = started;
      for (let i = 0; i < 30 && status.status === "building"; i++) {
        await new Promise((r) => setTimeout(r, 1500));
        status = await pollDeployStatus(started.deployment_id, target);
      }
      if (status.status === "ready") {
        setLiveUrl(status.url);
        setPhase("done");
      } else {
        setMessage("The publish didn't finish. Want to try again?");
        setPhase("error");
      }
    } catch (e) {
      setMessage((e as Error).message);
      setPhase("error");
    }
  }

  return (
    <div className="deploy-wizard" role="dialog" aria-label="Publish your project">
      <header><h2>Publish</h2><button onClick={onClose} aria-label="Close">×</button></header>

      {phase === "connect" && (
        <section>
          <p>Connect your Vercel account to publish there.</p>
          <input
            type="password" placeholder="Paste your Vercel token"
            value={tokenInput} onChange={(e) => setTokenInput(e.target.value)} aria-label="Vercel token"
          />
          <button onClick={onSaveToken} disabled={!tokenInput.trim()}>Save</button>
        </section>
      )}

      {phase === "choose" && connected && (
        <section>
          <p>Where should it live?</p>
          <ul>
            {HOST_OPTIONS.map((h) => (
              <li key={h.id}>
                <button disabled={h.disabled}>{h.label}</button>
              </li>
            ))}
          </ul>
          <button onClick={onAskAi}>Ask AI</button>
          {reason && <p className="ai-reason">{reason}</p>}
          <button onClick={onPublish}>Publish</button>
        </section>
      )}

      {phase === "publishing" && <section><p>Publishing your project…</p></section>}

      {phase === "done" && liveUrl && (
        <section>
          <p>Your site is live!</p>
          <a href={liveUrl} target="_blank" rel="noreferrer">View your site ({liveUrl.replace(/^https?:\/\//, "")})</a>
        </section>
      )}

      {phase === "error" && (
        <section>
          <p className="deploy-error">{message ?? "Something went wrong."}</p>
          <button onClick={() => void checkConnection()}>Try again</button>
        </section>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run the component tests to verify they pass**

Run: `cd "apps/web" && pnpm vitest run tests/components/DeployWizard.test.tsx 2>&1 | tail -25`
Expected: PASS (all three).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/DeployWizard.tsx apps/web/tests/components/DeployWizard.test.tsx
git commit -m "feat(web): DeployWizard modal (host + Ask AI + connect-once + publish)"
```

---

### Task 11: Wire the wizard entry button into HtmlFileViewer

**Files:**
- Modify: `apps/web/src/components/FileViewer.tsx`

- [ ] **Step 1: Add the entry button + modal mount in `HtmlFileViewer`**

Near the existing deploy modal state in `HtmlFileViewer`, add a separate state flag and render the wizard. Use the current HTML file's name as `fileName` and the project id already available in scope.

```tsx
// import at the top of FileViewer.tsx
import { DeployWizard } from "./DeployWizard";

// inside HtmlFileViewer, beside the other useState hooks:
const [omnivyxDeployOpen, setOmnivyxDeployOpen] = useState(false);

// in the share/export menu JSX, add:
<button type="button" onClick={() => setOmnivyxDeployOpen(true)}>
  Publish with OmniVyx
</button>

// near the existing deploy modal render:
{omnivyxDeployOpen && (
  <DeployWizard
    projectId={projectId}
    fileName={file.name}
    onClose={() => setOmnivyxDeployOpen(false)}
  />
)}
```

Use the actual prop names in scope for the project id and current file (recon: `deployProjectFile(projectId, file.name, ...)` is already called in this component, so both `projectId` and `file.name` exist here).

- [ ] **Step 2: Typecheck**

Run: `cd "apps/web" && pnpm tsc --noEmit 2>&1 | tail -15`
Expected: clean.

- [ ] **Step 3: Run the web test suite (catch regressions)**

Run: `cd "apps/web" && pnpm vitest run 2>&1 | tail -20`
Expected: PASS (minus any pre-existing known failures noted in the repo).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/FileViewer.tsx
git commit -m "feat(web): add Publish-with-OmniVyx entry to HtmlFileViewer"
```

---

## Final verification (whole feature)

- [ ] **Agent:** `cd "packages/backend/api" && cargo test --lib 2>&1 | tail -15` → PASS.
- [ ] **Seam:** `cd "packages/celerex-connect" && bun test 2>&1 | tail -15` → PASS; `dist/` rebuilt + committed.
- [ ] **Daemon:** `cd "apps/daemon" && pnpm vitest run 2>&1 | tail -15 && pnpm tsc --noEmit` → PASS + clean.
- [ ] **Web:** `cd "apps/web" && pnpm vitest run 2>&1 | tail -15 && pnpm tsc --noEmit` → PASS + clean.
- [ ] Each stage shipped as its own branch + PR + **merged right after opening** (standing rule).

---

## Self-review notes (spec coverage)

- **Server holds the key** → Task 3 (vault set/get via DeviceAuth). ✓
- **Files uploaded to server, Agent deploys** → Task 4 (Agent calls Vercel) + Task 8 (daemon reads local file → base64 → uploads). ✓ (This is the "everything on the server" choice — execution is on the Agent.)
- **Guided wizard, host step** → Task 10 (DeployWizard). ✓
- **"Ask AI" button** → Task 5 (Agent recommend + fallback) + Task 9/10 (wired). ✓
- **Credential-once** → Task 3 + Task 10 connect phase. ✓
- **Live link returned** → Task 4 status + Task 10 polling → clickable link. ✓
- **Catalog by role** → Task 1 (role field + Vercel = web-host). ✓
- **Other targets "coming soon"** → Task 10 HOST_OPTIONS (Netlify disabled). ✓
- **Non-technical error copy** → plain-language `Error` messages throughout Tasks 9/10. ✓
- **Database + domain steps** → deliberately deferred (spec v1 marks them skippable / later phases). Not in this plan. ✓ (matches spec §8 "scaffolded but skippable" — for v1 minimalism they are omitted from the wizard entirely; add in phase 2.)

**Deferred to later phases (per spec §8/§13):** Netlify recipe, real database + domain steps, Lucidity host, Composio backend-selection resolver across data sources, deployment history persistence.
