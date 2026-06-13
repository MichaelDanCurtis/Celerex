# Celerex Design Provider Gateway + Auto-Seed — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Celerex an LLM gateway so the Celerex Design desktop app gets provider credentials/models from the paired Celerex workspace (proxy by default, opt-in raw-key copy), seeding both of open-design's credential homes.

**Architecture:** Celerex exposes a provider-config endpoint, an Anthropic/OpenAI-compatible reverse proxy (injects the engagement's encrypted key server-side), and a confirm-gated key-copy endpoint — all authorized by a graduated `design.device` token. The device-side `@celerex/connect` package computes a pure "seed plan" that an open-design daemon route writes into the daemon `app-config.json` (CLI mode) and returns to the web client (API mode).

**Tech Stack:** Rust/Axum 0.8 + sqlx + reqwest (Celerex); TypeScript/ESM + bun test (`@celerex/connect`); Node/Express daemon (open-design).

**Two repos:**
- **CX** = `/Volumes/Main External/Development/Celerex` (branch `feat/design-provider-gateway`, based on piece C `f70c95a`).
- **CD** = `/Volumes/Main External/Development/celerex-design` (local git; package `packages/celerex-connect`).

**Prerequisite:** PR #15 (piece C) provides `Claims.scope`, `mint_scoped`, `DesignAuth`, the device token, and pairing. This plan extends them.

**Spec:** `docs/superpowers/specs/2026-06-12-celerex-design-provider-gateway-design.md`.

---

## File Structure

**CX (Celerex):**
- `packages/backend/api/src/auth/jwt.rs` — add `SCOPE_DESIGN_DEVICE` const (modify).
- `packages/backend/api/src/auth/extractors.rs` — broaden `DesignAuth`, add `DeviceAuth` (modify).
- `packages/backend/api/src/auth/mod.rs` — export `DeviceAuth` (modify).
- `packages/backend/api/src/design/mod.rs` — mint `design.device`; add `list_design_providers`, `copy_provider_key` handlers + catalog (modify).
- `packages/backend/api/src/design/proxy.rs` — the LLM reverse proxy (create).
- `packages/backend/api/src/lib.rs` — register `pub mod`… nothing new (proxy is under design); register 3 routes (modify).
- `packages/backend/api/tests/design_gateway.rs` — e2e for providers/proxy/key-copy/scope (create).
- `packages/backend/api/tests/design_pairing.rs` — update scope assertion to `design.device` (modify).

**CD (`@celerex/connect`):**
- `packages/celerex-connect/src/contract.ts` — add provider-config types (modify).
- `packages/celerex-connect/src/providers.ts` — `fetchProviderConfig`, `fetchProviderKey` (create).
- `packages/celerex-connect/src/seed.ts` — `buildSeedPlan` (create).
- `packages/celerex-connect/src/index.ts` — export new surface (modify).
- `packages/celerex-connect/test/seed.test.ts`, `test/providers.test.ts` (create).

**CD (open-design daemon):**
- `apps/daemon/src/server.ts` — add `POST /api/celerex/connect` (modify; adapt to local router idiom).

---

## Task 1 — Graduate the device token to `design.device` [CX]

**Files:**
- Modify: `packages/backend/api/src/auth/jwt.rs`
- Modify: `packages/backend/api/src/auth/extractors.rs`
- Modify: `packages/backend/api/src/auth/mod.rs`
- Modify: `packages/backend/api/src/design/mod.rs`
- Test: `packages/backend/api/tests/design_pairing.rs`

- [ ] **Step 1: Add the new scope constant.** In `auth/jwt.rs`, below `SCOPE_DESIGN_PUBLISH`:

```rust
/// The full Celerex Design device capability: publish + read provider config +
/// proxy LLM traffic. Granted to a paired device (supersedes publish-only).
pub const SCOPE_DESIGN_DEVICE: &str = "design.device";
```

- [ ] **Step 2: Mint `design.device` from pairing + the token endpoint.** In `design/mod.rs`, change the two `mint_scoped(..., SCOPE_DESIGN_PUBLISH, ...)` calls (in `mint_token` and `pair_approve`) to `SCOPE_DESIGN_DEVICE`, and update the import to bring in `SCOPE_DESIGN_DEVICE`. In `mint_token`'s response, set `scope: SCOPE_DESIGN_DEVICE.to_string()`.

- [ ] **Step 3: Broaden `DesignAuth` and add `DeviceAuth`.** In `auth/extractors.rs`, import `SCOPE_DESIGN_DEVICE` and replace the `DesignAuth` match + add `DeviceAuth`:

```rust
use crate::auth::jwt::{verify, Claims, SCOPE_DESIGN_PUBLISH, SCOPE_DESIGN_DEVICE};

// DesignAuth (ingest + providers-config): session, publish (legacy), or device.
impl<S: Send + Sync> FromRequestParts<S> for DesignAuth {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let claims = claims_from_parts(parts)?;
        match claims.scope.as_deref() {
            None | Some(SCOPE_DESIGN_PUBLISH) | Some(SCOPE_DESIGN_DEVICE) => Ok(DesignAuth(claims)),
            Some(_) => Err((StatusCode::FORBIDDEN, "token scope not permitted here")),
        }
    }
}

/// Device-only routes (LLM proxy, key copy): full session OR `design.device`.
/// A publish-only legacy token is rejected here — it cannot proxy LLM traffic.
pub struct DeviceAuth(pub Claims);

impl<S: Send + Sync> FromRequestParts<S> for DeviceAuth {
    type Rejection = (StatusCode, &'static str);
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let claims = claims_from_parts(parts)?;
        match claims.scope.as_deref() {
            None | Some(SCOPE_DESIGN_DEVICE) => Ok(DeviceAuth(claims)),
            Some(_) => Err((StatusCode::FORBIDDEN, "device scope required")),
        }
    }
}
```

- [ ] **Step 4: Export `DeviceAuth`.** In `auth/mod.rs`: `pub use extractors::{AuthClaims, DesignAuth, DeviceAuth};`

- [ ] **Step 5: Update the pairing test's scope assertion (failing test first).** In `tests/design_pairing.rs`, in `scoped_token_can_publish_but_nothing_else`, change:

```rust
    assert_eq!(body["scope"], "design.device");
```

- [ ] **Step 6: Build + run pairing tests.**

Run: `cargo test -p celerex-api --test design_pairing`
Expected: PASS (3 tests). If a publish-token assertion exists elsewhere, the scoped token is now `design.device`; ingest still works because `DesignAuth` accepts it.

- [ ] **Step 7: Commit.**

```bash
git add packages/backend/api/src/auth packages/backend/api/src/design/mod.rs packages/backend/api/tests/design_pairing.rs
git commit -m "feat(design): graduate device token to design.device + DeviceAuth extractor"
```

---

## Task 2 — `GET /api/design/providers` (config, no secrets) [CX]

**Files:**
- Modify: `packages/backend/api/src/design/mod.rs`
- Modify: `packages/backend/api/src/lib.rs`
- Test: `packages/backend/api/tests/design_gateway.rs` (create)

- [ ] **Step 1: Write the failing e2e test.** Create `tests/design_gateway.rs`:

```rust
//! Celerex Design provider gateway — providers config, LLM proxy, key copy.
mod common;
use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

async fn setup() -> (common::TestDb, axum_test::TestServer, Uuid, String) {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let token = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

#[tokio::test]
async fn providers_lists_configured_keys_without_secrets() {
    let (db, server, eng, token) = setup().await;
    // configure an anthropic key for the engagement
    sqlx::query("INSERT INTO engagement_provider_keys (engagement_id, provider_id, nonce, ciphertext) VALUES ($1,'anthropic','\\x00','\\x00')")
        .bind(eng).execute(&db.pool).await.ok(); // shape may differ — prefer the store API below
    let res = server.get("/api/design/providers")
        .add_header("authorization", format!("Bearer {token}")).await;
    res.assert_status_ok();
    let body: Value = res.json();
    let provs = body["providers"].as_array().unwrap();
    let anthropic = provs.iter().find(|p| p["id"] == "anthropic").unwrap();
    assert_eq!(anthropic["proxy_base_url"], "/api/design/llm/anthropic");
    assert!(anthropic["models"].as_array().unwrap().len() > 0);
    // never leak secrets
    assert!(!body.to_string().contains("ciphertext"));
}
```

> NOTE: set the key via the real store, not raw SQL. Replace the raw INSERT with a call through `celerex_api::providers::store::PgProviderKeysStore` if its constructor is reachable in tests, OR seed via `PUT /api/providers/keys/anthropic` using a full session token (`mint_for_tests(eng)`). Prefer the route:
```rust
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    server.put("/api/providers/keys/anthropic")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "sk-real" })).await.assert_status_ok();
```
(Confirm the request body field name by reading `src/providers/routes.rs::set_key`.)

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p celerex-api --test design_gateway providers_lists`
Expected: FAIL (route 404 / handler missing).

- [ ] **Step 3: Add the provider catalog + handler.** In `design/mod.rs`, add:

```rust
use crate::auth::extractors::DeviceAuth; // (add alongside existing imports)

/// v0.1 proxy-supported providers + their curated model lists. Only providers
/// with an Anthropic/OpenAI-compatible upstream the proxy understands appear
/// here (kimi/gemini deferred).
struct CatalogEntry { id: &'static str, label: &'static str, protocol: &'static str,
                      models: &'static [&'static str], default_model: &'static str }
const DESIGN_PROVIDER_CATALOG: &[CatalogEntry] = &[
    CatalogEntry { id: "anthropic", label: "Anthropic (Claude)", protocol: "anthropic",
        models: &["claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
        default_model: "claude-sonnet-4-6" },
    CatalogEntry { id: "openai", label: "OpenAI", protocol: "openai",
        models: &["gpt-4o", "o3"], default_model: "gpt-4o" },
];

#[derive(Serialize)]
pub struct DesignProvider {
    id: String, label: String, protocol: String, has_key: bool,
    proxy_base_url: String, models: Vec<String>, default_model: String,
}

/// `GET /api/design/providers` — provider/model config for the engagement. No secrets.
pub async fn list_design_providers(
    DesignAuth(claims): DesignAuth,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let metas = state.provider_keys.list_for_engagement(claims.engagement_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let configured: std::collections::HashSet<String> =
        metas.into_iter().map(|m| m.provider_id).collect();
    let providers: Vec<DesignProvider> = DESIGN_PROVIDER_CATALOG.iter().map(|c| DesignProvider {
        id: c.id.into(), label: c.label.into(), protocol: c.protocol.into(),
        has_key: configured.contains(c.id),
        proxy_base_url: format!("/api/design/llm/{}", c.id),
        models: c.models.iter().map(|s| s.to_string()).collect(),
        default_model: c.default_model.into(),
    }).collect();
    Ok(Json(serde_json::json!({ "providers": providers })))
}
```

- [ ] **Step 4: Register the route.** In `lib.rs` `route_chain()`, after the `/api/design/assets/{id}` route:

```rust
        .route("/api/design/providers", get(design::list_design_providers))
```

- [ ] **Step 5: Run the test to verify it passes.**

Run: `cargo test -p celerex-api --test design_gateway providers_lists`
Expected: PASS.

- [ ] **Step 6: Commit.**

```bash
git add packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_gateway.rs
git commit -m "feat(design): GET /api/design/providers config endpoint"
```

---

## Task 3 — LLM reverse proxy `ANY /api/design/llm/{provider}/{*rest}` [CX]

**Files:**
- Create: `packages/backend/api/src/design/proxy.rs`
- Modify: `packages/backend/api/src/design/mod.rs` (add `pub mod proxy;`)
- Modify: `packages/backend/api/src/lib.rs` (route)
- Test: `packages/backend/api/tests/design_gateway.rs`

- [ ] **Step 1: Write the failing proxy tests** (append to `design_gateway.rs`):

```rust
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn proxy_injects_key_and_relays_upstream() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages")).and(header("x-api-key", "sk-real"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&upstream).await;
    // point the proxy's anthropic upstream at the mock (read at request time)
    unsafe { std::env::set_var("DESIGN_LLM_ANTHROPIC_BASE", upstream.uri()); }

    let (_db, server, eng, token) = setup().await;
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    server.put("/api/providers/keys/anthropic")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "sk-real" })).await.assert_status_ok();

    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "model": "claude-sonnet-4-6", "messages": [] })).await;
    res.assert_status_ok();
    assert!(res.text().contains("\"ok\":true"));
    unsafe { std::env::remove_var("DESIGN_LLM_ANTHROPIC_BASE"); }
}

#[tokio::test]
async fn proxy_409_when_no_key_configured() {
    let (_db, server, _eng, token) = setup().await;
    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({})).await;
    res.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn proxy_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);
    let res = server.post("/api/design/llm/anthropic/v1/messages")
        .add_header("authorization", format!("Bearer {publish}"))
        .json(&serde_json::json!({})).await;
    res.assert_status(StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run to verify failure.**

Run: `cargo test -p celerex-api --test design_gateway proxy_`
Expected: FAIL (route missing).

- [ ] **Step 3: Implement `design/proxy.rs`.**

```rust
//! Celerex Design LLM gateway: an Anthropic/OpenAI-compatible reverse proxy.
//! Authenticated by a `design.device` token; injects the engagement's decrypted
//! provider key server-side so keys never leave Celerex. Upstream targets are a
//! fixed allowlist (no caller-supplied target → no SSRF).

use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::IntoResponse,
};
use bytes::Bytes;

use crate::{AppState, auth::extractors::DeviceAuth};

/// Map a provider id to (upstream base URL, protocol). Base is overridable via
/// `DESIGN_LLM_<PROVIDER>_BASE` for tests. Returns None for unknown providers.
fn upstream_for(provider: &str) -> Option<(String, &'static str)> {
    let (default_base, protocol) = match provider {
        "anthropic" => ("https://api.anthropic.com", "anthropic"),
        "openai" => ("https://api.openai.com", "openai"),
        _ => return None,
    };
    let env_key = format!("DESIGN_LLM_{}_BASE", provider.to_uppercase());
    let base = std::env::var(&env_key).unwrap_or_else(|_| default_base.to_string());
    Some((base.trim_end_matches('/').to_string(), protocol))
}

/// `ANY /api/design/llm/{provider}/{*rest}` — proxy to the provider, injecting the key.
pub async fn proxy(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path((provider, rest)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (base, protocol) = upstream_for(&provider)
        .ok_or((StatusCode::NOT_FOUND, format!("unknown provider '{provider}'")))?;
    let key = state.provider_keys.get(claims.engagement_id, &provider).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, format!("no key configured for '{provider}'")))?;

    let mut url = format!("{base}/{rest}");
    if let Some(q) = query { url.push('?'); url.push_str(&q); }

    let client = reqwest::Client::new();
    let mut req = client.request(method, &url).body(body.to_vec());
    // Relay content-type; inject provider auth.
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        req = req.header(header::CONTENT_TYPE, ct);
    }
    req = match protocol {
        "anthropic" => req.header("x-api-key", &key).header("anthropic-version", "2023-06-01"),
        _ => req.header(header::AUTHORIZATION, format!("Bearer {key}")),
    };

    let upstream = req.send().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")))?;
    let status = upstream.status();
    let ct = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let stream = upstream.bytes_stream();
    let mut resp = (status, Body::from_stream(stream)).into_response();
    if let Some(ct) = ct { resp.headers_mut().insert(header::CONTENT_TYPE, ct); }
    Ok(resp)
}
```

> If `bytes` is not already a dependency, use `axum::body::Bytes` (re-exported) instead of the `bytes` crate. Verify `reqwest` has the `stream` feature (the existing llm providers stream, so it does).

- [ ] **Step 4: Wire the module + route.** In `design/mod.rs` add `pub mod proxy;`. In `lib.rs`, import `any` in the routing use (`routing::{any, get, patch, post, put}`) and add:

```rust
        .route("/api/design/llm/{provider}/{*rest}", any(design::proxy::proxy))
```

- [ ] **Step 5: Run the proxy tests.**

Run: `cargo test -p celerex-api --test design_gateway proxy_`
Expected: PASS (3 tests). If the env-var test is flaky under parallelism, run `--test-threads=1` and see the `rust-integration-test-env-var-cross-binary-race` note below.

- [ ] **Step 6: Commit.**

```bash
git add packages/backend/api/src/design/proxy.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_gateway.rs
git commit -m "feat(design): Anthropic/OpenAI-compatible LLM reverse proxy (key injected server-side)"
```

---

## Task 4 — `GET /api/design/providers/{id}/key` (opt-in copy, gated) [CX]

**Files:**
- Modify: `packages/backend/api/src/design/mod.rs`
- Modify: `packages/backend/api/src/lib.rs`
- Test: `packages/backend/api/tests/design_gateway.rs`

- [ ] **Step 1: Failing tests** (append):

```rust
#[tokio::test]
async fn key_copy_requires_confirm_and_returns_decrypted() {
    let (_db, server, eng, token) = setup().await;
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    server.put("/api/providers/keys/anthropic")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "sk-copyme" })).await.assert_status_ok();

    // without confirm header → 400
    let res = server.get("/api/design/providers/anthropic/key")
        .add_header("authorization", format!("Bearer {token}")).await;
    res.assert_status(StatusCode::BAD_REQUEST);

    // with confirm header → decrypted key
    let res = server.get("/api/design/providers/anthropic/key")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true").await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["api_key"], "sk-copyme");
    assert_eq!(body["base_url"], "https://api.anthropic.com");
}
```

- [ ] **Step 2: Run → fail.** `cargo test -p celerex-api --test design_gateway key_copy` → FAIL.

- [ ] **Step 3: Implement the handler** in `design/mod.rs`:

```rust
/// `GET /api/design/providers/{id}/key` — opt-in raw-key copy. Confirm-gated +
/// audit-logged. Default base_url comes from the catalog.
pub async fn copy_provider_key(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, (StatusCode, String)> {
    if headers.get("x-celerex-confirm-copy").and_then(|v| v.to_str().ok()) != Some("true") {
        return Err((StatusCode::BAD_REQUEST, "raw-key copy requires x-celerex-confirm-copy: true".into()));
    }
    let key = state.provider_keys.get(claims.engagement_id, &provider).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::NOT_FOUND, "no key configured".into()))?;
    let base_url = match provider.as_str() {
        "anthropic" => "https://api.anthropic.com",
        "openai" => "https://api.openai.com",
        _ => return Err((StatusCode::NOT_FOUND, "unknown provider".into())),
    };
    tracing::warn!(engagement=%claims.engagement_id, provider=%provider, "design device copied a raw provider key");
    Ok(Json(serde_json::json!({ "provider_id": provider, "api_key": key, "base_url": base_url })))
}
```

- [ ] **Step 4: Route** in `lib.rs`:

```rust
        .route("/api/design/providers/{id}/key", get(design::copy_provider_key))
```

- [ ] **Step 5: Run → pass.** `cargo test -p celerex-api --test design_gateway key_copy` → PASS.

- [ ] **Step 6: Full backend gate.**

Run: `cargo test -p celerex-api`
Expected: all green (lib + design_ingest + design_pairing + design_gateway).

- [ ] **Step 7: Commit.**

```bash
git add packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_gateway.rs
git commit -m "feat(design): confirm-gated raw provider-key copy endpoint"
```

---

## Task 5 — `@celerex/connect`: provider config + seed plan [CD]

Work in `/Volumes/Main External/Development/celerex-design/packages/celerex-connect`.

**Files:**
- Modify: `src/contract.ts`
- Create: `src/providers.ts`, `src/seed.ts`
- Modify: `src/index.ts`
- Test: `test/providers.test.ts`, `test/seed.test.ts`

- [ ] **Step 1: Add provider-config types** to `src/contract.ts`:

```ts
export interface DesignProviderInfo {
  id: string;
  label: string;
  protocol: "anthropic" | "openai";
  has_key: boolean;
  proxy_base_url: string;   // e.g. "/api/design/llm/anthropic"
  models: string[];
  default_model: string;
}
export interface DesignProvidersResponse { providers: DesignProviderInfo[]; }
export interface CopiedProviderKey { provider_id: string; api_key: string; base_url: string; }
```

- [ ] **Step 2: Write the failing seed test** `test/seed.test.ts`:

```ts
import { test, expect } from "bun:test";
import { buildSeedPlan } from "../src/seed.js";

const anthropic = {
  id: "anthropic", label: "Anthropic (Claude)", protocol: "anthropic" as const,
  has_key: true, proxy_base_url: "/api/design/llm/anthropic",
  models: ["claude-sonnet-4-6"], default_model: "claude-sonnet-4-6",
};

test("proxy mode points both homes at the Celerex gateway + device token", () => {
  const plan = buildSeedPlan({
    mode: "proxy", celerexBaseUrl: "https://cx.example.com/", deviceToken: "dev-tok",
    provider: anthropic, model: "claude-opus-4-8",
  });
  expect(plan.web).toEqual({
    mode: "api", apiProtocol: "anthropic",
    baseUrl: "https://cx.example.com/api/design/llm/anthropic",
    apiKey: "dev-tok", model: "claude-opus-4-8",
  });
  expect(plan.daemon.agentId).toBe("claude");
  expect(plan.daemon.agentModels).toEqual({ claude: { model: "claude-opus-4-8" } });
  expect(plan.daemon.agentCliEnv.claude).toEqual({
    ANTHROPIC_BASE_URL: "https://cx.example.com/api/design/llm/anthropic",
    ANTHROPIC_AUTH_TOKEN: "dev-tok",
  });
});

test("copy mode uses the real upstream base + raw key", () => {
  const plan = buildSeedPlan({
    mode: "copy", celerexBaseUrl: "https://cx.example.com", deviceToken: "dev-tok",
    provider: anthropic, copiedKey: { provider_id: "anthropic", api_key: "sk-real", base_url: "https://api.anthropic.com" },
  });
  expect(plan.web.baseUrl).toBe("https://api.anthropic.com");
  expect(plan.web.apiKey).toBe("sk-real");
  expect(plan.daemon.agentCliEnv.claude).toEqual({
    ANTHROPIC_BASE_URL: "https://api.anthropic.com", ANTHROPIC_AUTH_TOKEN: "sk-real",
  });
});

test("openai maps to the codex agent + OPENAI_* env", () => {
  const openai = { ...anthropic, id: "openai", protocol: "openai" as const,
    proxy_base_url: "/api/design/llm/openai", default_model: "gpt-4o", models: ["gpt-4o"] };
  const plan = buildSeedPlan({ mode: "proxy", celerexBaseUrl: "https://cx", deviceToken: "t", provider: openai });
  expect(plan.daemon.agentId).toBe("codex");
  expect(plan.daemon.agentCliEnv.codex).toEqual({
    OPENAI_BASE_URL: "https://cx/api/design/llm/openai", OPENAI_API_KEY: "t",
  });
  expect(plan.web.model).toBe("gpt-4o"); // falls back to default_model
});
```

- [ ] **Step 3: Run → fail.** `cd packages/celerex-connect && bun test seed` → FAIL (module missing).

- [ ] **Step 4: Implement `src/seed.ts`:**

```ts
import type { DesignProviderInfo, CopiedProviderKey } from "./contract.js";

export interface SeedPlan {
  daemon: {
    agentId: string;
    agentModels: Record<string, { model: string }>;
    agentCliEnv: Record<string, Record<string, string>>;
  };
  web: { mode: "api"; apiProtocol: "anthropic" | "openai"; baseUrl: string; apiKey: string; model: string };
}

export interface BuildSeedPlanInput {
  mode: "proxy" | "copy";
  celerexBaseUrl: string;
  deviceToken: string;
  provider: DesignProviderInfo;
  model?: string;
  copiedKey?: CopiedProviderKey;
}

// Which coding-CLI agent speaks each protocol (v0.1).
const AGENT_FOR_PROTOCOL: Record<"anthropic" | "openai", string> = {
  anthropic: "claude",
  openai: "codex",
};
// Env var names each protocol's CLI reads (open-design env.ts convention).
const ENV_FOR_PROTOCOL = {
  anthropic: { base: "ANTHROPIC_BASE_URL", token: "ANTHROPIC_AUTH_TOKEN" },
  openai: { base: "OPENAI_BASE_URL", token: "OPENAI_API_KEY" },
} as const;

export function buildSeedPlan(input: BuildSeedPlanInput): SeedPlan {
  const { mode, provider } = input;
  const model = input.model ?? provider.default_model;
  const agentId = AGENT_FOR_PROTOCOL[provider.protocol];
  const envNames = ENV_FOR_PROTOCOL[provider.protocol];

  let baseUrl: string;
  let secret: string;
  if (mode === "copy") {
    if (!input.copiedKey) throw new Error("copy mode requires copiedKey");
    baseUrl = input.copiedKey.base_url;
    secret = input.copiedKey.api_key;
  } else {
    baseUrl = `${input.celerexBaseUrl.replace(/\/+$/, "")}${provider.proxy_base_url}`;
    secret = input.deviceToken;
  }

  return {
    daemon: {
      agentId,
      agentModels: { [agentId]: { model } },
      agentCliEnv: { [agentId]: { [envNames.base]: baseUrl, [envNames.token]: secret } },
    },
    web: { mode: "api", apiProtocol: provider.protocol, baseUrl, apiKey: secret, model },
  };
}
```

- [ ] **Step 5: Run → pass.** `bun test seed` → PASS (3 tests).

- [ ] **Step 6: Write the failing providers test** `test/providers.test.ts`:

```ts
import { test, expect } from "bun:test";
import { fetchProviderConfig, fetchProviderKey } from "../src/providers.js";
import { jsonResponse, seqFetch } from "./helpers.js";

test("fetchProviderConfig sends the device token and returns the provider list", async () => {
  const { fn, calls } = seqFetch([jsonResponse({ providers: [{ id: "anthropic" }] })]);
  const cfg = await fetchProviderConfig("https://cx/", "tok", fn);
  expect(cfg.providers[0]!.id).toBe("anthropic");
  expect(calls[0]!.url).toBe("https://cx/api/design/providers");
  expect((calls[0]!.init?.headers as Record<string, string>).authorization).toBe("Bearer tok");
});

test("fetchProviderKey requires the confirm header", async () => {
  const { fn, calls } = seqFetch([jsonResponse({ provider_id: "anthropic", api_key: "sk", base_url: "u" })]);
  const k = await fetchProviderKey("https://cx", "tok", "anthropic", fn);
  expect(k.api_key).toBe("sk");
  expect((calls[0]!.init?.headers as Record<string, string>)["x-celerex-confirm-copy"]).toBe("true");
});
```

- [ ] **Step 7: Run → fail**, then implement `src/providers.ts`:

```ts
import type { DesignProvidersResponse, CopiedProviderKey } from "./contract.js";
import type { FetchFn } from "./pairing.js";

function api(baseUrl: string, path: string): string {
  return `${baseUrl.replace(/\/+$/, "")}${path}`;
}
async function asError(res: Response, action: string): Promise<Error> {
  return new Error(`celerex ${action} failed (${res.status}): ${await res.text().catch(() => "")}`.trim());
}

export async function fetchProviderConfig(baseUrl: string, token: string, fetchFn: FetchFn = fetch): Promise<DesignProvidersResponse> {
  const res = await fetchFn(api(baseUrl, "/api/design/providers"), {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) throw await asError(res, "providers");
  return (await res.json()) as DesignProvidersResponse;
}

export async function fetchProviderKey(baseUrl: string, token: string, providerId: string, fetchFn: FetchFn = fetch): Promise<CopiedProviderKey> {
  const res = await fetchFn(api(baseUrl, `/api/design/providers/${providerId}/key`), {
    headers: { authorization: `Bearer ${token}`, "x-celerex-confirm-copy": "true" },
  });
  if (!res.ok) throw await asError(res, "provider key copy");
  return (await res.json()) as CopiedProviderKey;
}
```

- [ ] **Step 8: Export the new surface** in `src/index.ts`:

```ts
export { buildSeedPlan, type SeedPlan, type BuildSeedPlanInput } from "./seed.js";
export { fetchProviderConfig, fetchProviderKey } from "./providers.js";
export type { DesignProviderInfo, DesignProvidersResponse, CopiedProviderKey } from "./contract.js";
```

- [ ] **Step 9: Full package gate.**

Run: `bun test` then `bunx tsc@5.6.3 -p tsconfig.json --noEmit`
Expected: all tests pass (16 prior + 5 new), tsc exit 0.

- [ ] **Step 10: Commit (in the celerex-design repo).**

```bash
cd "/Volumes/Main External/Development/celerex-design"
git add packages/celerex-connect/src packages/celerex-connect/test
git commit -m "feat(connect): provider config fetch + buildSeedPlan (proxy/copy)"
```

---

## Task 6 — open-design daemon `POST /api/celerex/connect` [CD]

> **Adapt-on-read:** `apps/daemon/src/server.ts` is large with its own router idiom. Before coding, READ: (a) how an existing `app.post('/api/...', ...)` route is registered near the bottom of `server.ts`; (b) `apps/daemon/src/app-config.ts` `writeAppConfig(dataDir, prefs)` + the `RUNTIME_DATA_DIR`/`OD_DATA_DIR` resolution; (c) how the daemon imports workspace packages (to import `@celerex/connect`). Mirror those exactly.

**Files:**
- Modify: `apps/daemon/src/server.ts` (add route)
- Test: a daemon test mirroring the nearest existing route test (read `apps/daemon` tests for the harness; if none, add a focused integration test that calls the handler with a stubbed Celerex base URL using a local mock server).

- [ ] **Step 1: Write the failing test.** Mirror the closest existing daemon route test. The test should: stub a Celerex server (a tiny local server, or `wiremock`-equivalent for Node — check what the daemon tests already use) returning `GET /api/design/providers` → one anthropic provider; POST `/api/celerex/connect` with `{ celerexBaseUrl, deviceToken: "tok", mode: "proxy", providerId: "anthropic" }`; assert (a) the response contains the `web` seed half with `mode:'api'` + gateway baseUrl + token, and (b) `readAppConfig(dataDir)` now has `agentId:"claude"` and `agentCliEnv.claude.ANTHROPIC_BASE_URL` pointing at the gateway.

- [ ] **Step 2: Run → fail** (route missing).

- [ ] **Step 3: Implement the route.** Near the other `/api/*` registrations in `server.ts`:

```ts
import { fetchProviderConfig, fetchProviderKey, buildSeedPlan } from "@celerex/connect";

app.post("/api/celerex/connect", async (req, res) => {
  const { celerexBaseUrl, deviceToken, mode, providerId, model } = req.body ?? {};
  if (!celerexBaseUrl || !deviceToken || !providerId) {
    return res.status(400).json({ error: "celerexBaseUrl, deviceToken, providerId required" });
  }
  try {
    const cfg = await fetchProviderConfig(celerexBaseUrl, deviceToken);
    const provider = cfg.providers.find((p) => p.id === providerId);
    if (!provider) return res.status(404).json({ error: `provider '${providerId}' not configured in Celerex` });
    const copiedKey = mode === "copy"
      ? await fetchProviderKey(celerexBaseUrl, deviceToken, providerId)
      : undefined;
    const plan = buildSeedPlan({ mode: mode ?? "proxy", celerexBaseUrl, deviceToken, provider, model, copiedKey });
    // Write the daemon (CLI) half; reuse existing config persistence.
    const current = await readAppConfig(RUNTIME_DATA_DIR);
    await writeAppConfig(RUNTIME_DATA_DIR, {
      ...current,
      agentId: plan.daemon.agentId,
      agentModels: { ...(current.agentModels ?? {}), ...plan.daemon.agentModels },
      agentCliEnv: { ...(current.agentCliEnv ?? {}), ...plan.daemon.agentCliEnv },
    });
    // Return the web (API) half for the client to persist to localStorage.
    return res.json({ web: plan.web, agentId: plan.daemon.agentId });
  } catch (e) {
    return res.status(502).json({ error: `connect failed: ${(e as Error).message}` });
  }
});
```

> Use the actual symbols `readAppConfig`/`writeAppConfig`/`RUNTIME_DATA_DIR` as imported elsewhere in `server.ts` (confirm names on read). Ensure `@celerex/connect` is built/resolvable from the daemon — if the workspace consumes built `dist`, run its `build` script, or add it as a workspace dependency in `apps/daemon/package.json` and `pnpm install`.

- [ ] **Step 4: Run → pass.**

- [ ] **Step 5: Commit (celerex-design repo).**

```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/daemon packages/celerex-connect
git commit -m "feat(daemon): POST /api/celerex/connect — auto-seed provider config from Celerex"
```

---

## Notes / Gotchas

- **Env-var test race** (`rust-integration-test-env-var-cross-binary-race`): only `proxy_injects_key_and_relays_upstream` sets `DESIGN_LLM_ANTHROPIC_BASE`, and it removes it after. The 409/403 proxy tests never reach the env read (they fail at key-lookup/auth first), so there's no intra-binary contention. If flakiness appears, run `cargo test -p celerex-api --test design_gateway -- --test-threads=1`.
- **`mint_scoped` is `#[doc(hidden)]`-adjacent but public** — confirm it's reachable from tests (piece C exposed it as `pub`). If not, add a `#[doc(hidden)] pub fn mint_scoped_for_tests`.
- **`reqwest` Method/Bytes** — axum's `Method` is `http::Method`, which `reqwest::Client::request` accepts. Use `axum::body::Bytes` to avoid adding the `bytes` crate.
- **Celerex provider catalog models** are illustrative; confirm current model ids against `crate::llm` / Celerex's known set when implementing Task 2 (keep `claude-sonnet-4-6` / `claude-opus-4-8` consistent with the rest of the repo).

## Self-Review

- **Spec coverage:** §5.1 scope→T1; §5.2 providers→T2; §5.3 proxy→T3; §5.4 key-copy→T4; §6 connect package→T5; §7 daemon route→T6; §8 security (allowlist, confirm-gate, scope separation, header strip)→T3/T4; §9 tests→each task. UI (§10 out) intentionally excluded. No gaps.
- **Placeholder scan:** Task 6 is "adapt-on-read" by necessity (foreign 14k-line file) but ships concrete code + exact symbols to confirm; not a placeholder.
- **Type consistency:** `buildSeedPlan`/`SeedPlan`/`DesignProviderInfo` names match across T5; `DeviceAuth`/`DesignAuth` consistent T1↔T3/T4; `proxy_base_url`/`default_model`/`has_key` consistent server (T2) ↔ client (T5).
