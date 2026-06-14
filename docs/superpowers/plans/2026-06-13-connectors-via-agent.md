# Connectors via the Agent — Implementation Plan (2a)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace Composio with connectors served by the OmniVyx Agent as a credential vault, executed locally in the Design daemon (vault-adopt-local).

**Architecture:** Agent exposes a connector **catalog** (`GET /api/design/connectors`) + a confirm-gated **credential** endpoint (`GET /api/design/connectors/{id}/credential`), both mirroring the shipped provider-gateway endpoints and reusing the `engagement_provider_keys` store (connector ids namespace alongside provider ids — no new table). The seam `@celerex/connect` gains `fetchConnectors` + `fetchConnectorCredential`. The Design daemon adopts the catalog + creds on pairing and runs connectors locally (additive `agent-vault` provider alongside Composio, then Composio removed).

**Tech Stack:** Rust/Axum (`celerex-api`), TypeScript (`@celerex/connect`, bun test), Node/daemon (vitest).

**Spec:** `docs/superpowers/specs/2026-06-13-connectors-via-agent-design.md`

**Execution status (2026-06-13):** Part A + Part B executed this session (PRs opened). Part C
(daemon) is the deferred, review-gated piece — execute in a session where the celerex-design
monorepo daemon tests can run.

---

## PART A — Agent connector API (Celerex, Rust) ✅ build this session

Mirrors `list_design_providers` / `copy_provider_key` exactly (see spec + recon).

### Task A1: Connector catalog + list endpoint

**Files:**
- Modify: `packages/backend/api/src/design/mod.rs` (add catalog + structs + handler near the provider catalog ~line 379-412)
- Modify: `packages/backend/api/src/lib.rs` (add route in `route_chain()` ~line 117)
- Test: `packages/backend/api/tests/design_connectors.rs` (new)

- [ ] **Step 1: Add catalog + response structs + handler** in `design/mod.rs`:

```rust
struct ConnectorCatalogEntry {
    id: &'static str,
    label: &'static str,
    auth_kind: &'static str, // "api_key" | "app_creds" | "internal"
    tools: &'static [(&'static str, &'static str)], // (name, description) — read-only in v1
}
const DESIGN_CONNECTOR_CATALOG: &[ConnectorCatalogEntry] = &[
    ConnectorCatalogEntry { id: "m365", label: "Microsoft 365", auth_kind: "app_creds",
        tools: &[("list_recent_messages", "List the most recent Outlook messages"),
                 ("list_files", "List recent OneDrive/SharePoint files")] },
    ConnectorCatalogEntry { id: "galvo-memory", label: "Galvo Memory", auth_kind: "internal",
        tools: &[("search", "Search the engagement's agent memory")] },
    ConnectorCatalogEntry { id: "http-api", label: "Generic API (key)", auth_kind: "api_key",
        tools: &[("get", "GET a configured JSON HTTP endpoint")] },
];

#[derive(serde::Serialize, Clone)]
pub struct ConnectorToolInfo { name: String, description: String }

#[derive(serde::Serialize)]
pub struct DesignConnector {
    id: String, label: String, auth_kind: String, configured: bool,
    tools: Vec<ConnectorToolInfo>,
}

/// `GET /api/design/connectors` — connector catalog for the engagement. No secrets.
pub async fn list_design_connectors(
    DesignAuth(claims): DesignAuth,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let metas = state.provider_keys.list_for_engagement(claims.engagement_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let configured: std::collections::HashSet<String> =
        metas.into_iter().map(|m| m.provider_id).collect();
    let connectors: Vec<DesignConnector> = DESIGN_CONNECTOR_CATALOG.iter().map(|c| DesignConnector {
        id: c.id.into(), label: c.label.into(), auth_kind: c.auth_kind.into(),
        // internal connectors need no stored secret; others require a vaulted cred.
        configured: c.auth_kind == "internal" || configured.contains(c.id),
        tools: c.tools.iter().map(|(n, d)| ConnectorToolInfo { name: (*n).into(), description: (*d).into() }).collect(),
    }).collect();
    Ok(Json(serde_json::json!({ "connectors": connectors })))
}
```

- [ ] **Step 2: Register the route** in `lib.rs` `route_chain()`, right after the providers route:

```rust
.route("/api/design/connectors", get(design::list_design_connectors))
```

- [ ] **Step 3: Write the test** `tests/design_connectors.rs` (copy `setup()` from `design_gateway.rs`):

```rust
use axum::http::StatusCode;
use serde_json::Value;
mod common;

async fn setup() -> (common::TestDb, axum_test::TestServer, uuid::Uuid, String) {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng: uuid::Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('WS') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let token = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.device", 3600);
    (db, server, eng, token)
}

#[tokio::test]
async fn lists_connectors_with_configured_flags() {
    let (_db, server, eng, token) = setup().await;
    let res = server.get("/api/design/connectors")
        .add_header("authorization", format!("Bearer {token}")).await;
    res.assert_status_ok();
    let body: Value = res.json();
    let connectors = body["connectors"].as_array().unwrap();
    // galvo-memory is internal → always configured; http-api needs a key → not yet.
    let mem = connectors.iter().find(|c| c["id"] == "galvo-memory").unwrap();
    assert_eq!(mem["configured"], true);
    let http = connectors.iter().find(|c| c["id"] == "http-api").unwrap();
    assert_eq!(http["configured"], false);
    let _ = eng;
}
```

- [ ] **Step 4: Verify compile + test**

Run: `cd packages/backend/api && cargo check -p celerex-api`
Expected: compiles. Then (if Docker is available for testcontainers):
`cargo test -p celerex-api --test design_connectors`
Expected: PASS. (If Docker unavailable, `cargo check` is the gate; the test runs in CI/locally.)

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_connectors.rs
git commit -m "feat(design): connector catalog endpoint (GET /api/design/connectors)"
```

### Task A2: Confirm-gated connector credential endpoint

**Files:**
- Modify: `packages/backend/api/src/design/mod.rs` (add handler)
- Modify: `packages/backend/api/src/lib.rs` (add route)
- Test: `packages/backend/api/tests/design_connectors.rs` (add tests)

- [ ] **Step 1: Add handler** in `design/mod.rs` (mirror `copy_provider_key`):

```rust
/// `GET /api/design/connectors/{id}/credential` — confirm-gated credential adopt.
pub async fn get_connector_credential(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, (StatusCode, String)> {
    if headers.get("x-celerex-confirm-copy").and_then(|v| v.to_str().ok()) != Some("true") {
        return Err((StatusCode::BAD_REQUEST, "credential adopt requires x-celerex-confirm-copy: true".into()));
    }
    let entry = DESIGN_CONNECTOR_CATALOG.iter().find(|c| c.id == id)
        .ok_or((StatusCode::NOT_FOUND, "unknown connector".to_string()))?;
    if entry.auth_kind == "internal" {
        // internal connectors carry no user secret; the daemon resolves them itself.
        return Ok(Json(serde_json::json!({ "connector_id": id, "kind": "internal", "value": Value::Null })));
    }
    let value = state.provider_keys.get(claims.engagement_id, &id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::NOT_FOUND, "connector not configured".to_string()))?;
    tracing::warn!(engagement=%claims.engagement_id, connector=%id, "design device adopted a connector credential");
    Ok(Json(serde_json::json!({ "connector_id": id, "kind": entry.auth_kind, "value": value })))
}
```

- [ ] **Step 2: Register the route** in `lib.rs`:

```rust
.route("/api/design/connectors/{id}/credential", get(design::get_connector_credential))
```

- [ ] **Step 3: Add tests** to `design_connectors.rs`:

```rust
#[tokio::test]
async fn credential_requires_confirm_and_returns_adopted_value() {
    let (_db, server, eng, token) = setup().await;
    let session = celerex_api::auth::jwt::mint_for_tests(eng);
    // store an http-api key via the existing provider-key setter (connectors share the store)
    server.put("/api/providers/keys/http-api")
        .add_header("authorization", format!("Bearer {session}"))
        .json(&serde_json::json!({ "key": "secret-123" })).await.assert_status_ok();

    // without confirm → 400
    server.get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {token}")).await
        .assert_status(StatusCode::BAD_REQUEST);

    // with confirm → adopted value
    let res = server.get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true").await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["kind"], "api_key");
    assert_eq!(body["value"], "secret-123");
}

#[tokio::test]
async fn internal_connector_credential_is_null() {
    let (_db, server, _eng, token) = setup().await;
    let res = server.get("/api/design/connectors/galvo-memory/credential")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-celerex-confirm-copy", "true").await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["kind"], "internal");
    assert!(body["value"].is_null());
}

#[tokio::test]
async fn credential_rejects_publish_only_token() {
    let (_db, server, eng, _token) = setup().await;
    let publish = celerex_api::auth::jwt::mint_scoped(eng, "dev", "design.publish", 3600);
    server.get("/api/design/connectors/http-api/credential")
        .add_header("authorization", format!("Bearer {publish}"))
        .add_header("x-celerex-confirm-copy", "true").await
        .assert_status(StatusCode::FORBIDDEN);
}
```

- [ ] **Step 4: Verify** `cargo check -p celerex-api` (+ `cargo test --test design_connectors` if Docker). **Step 5: Commit**

```bash
git add packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_connectors.rs
git commit -m "feat(design): confirm-gated connector credential endpoint"
```

---

## PART B — Seam connector client (`@celerex/connect`) ✅ build this session

Mirrors `fetchProviderConfig` / `fetchProviderKey` (recon item 1a). Repo: `celerex-design`.

### Task B1: Connector fetchers + types

**Files:**
- Create: `packages/celerex-connect/src/connectors.ts`
- Modify: `packages/celerex-connect/src/index.ts` (export)
- Test: `packages/celerex-connect/test/connectors.test.ts` (new)

- [ ] **Step 1: Create `connectors.ts`** (mirror `providers.ts`; reuse the local `api()`/`asError()` helpers — import from the same module they live in, e.g. `./providers.js` or `./http.js`; match the existing import path used by `providers.ts`):

```typescript
import { api, asError, type FetchFn } from "./providers.js";

export type ConnectorAuthKind = "api_key" | "app_creds" | "internal";

export interface DesignConnector {
  id: string;
  label: string;
  auth_kind: ConnectorAuthKind;
  configured: boolean;
  tools: { name: string; description: string }[];
}
export interface DesignConnectorsResponse { connectors: DesignConnector[]; }
export interface AdoptedConnectorCredential {
  connector_id: string;
  kind: ConnectorAuthKind | "internal";
  value: string | null;
}

export async function fetchConnectors(
  baseUrl: string, token: string, fetchFn: FetchFn = fetch,
): Promise<DesignConnectorsResponse> {
  const res = await fetchFn(api(baseUrl, "/api/design/connectors"), {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) throw await asError(res, "connectors");
  return (await res.json()) as DesignConnectorsResponse;
}

export async function fetchConnectorCredential(
  baseUrl: string, token: string, connectorId: string, fetchFn: FetchFn = fetch,
): Promise<AdoptedConnectorCredential> {
  const res = await fetchFn(api(baseUrl, `/api/design/connectors/${connectorId}/credential`), {
    headers: { authorization: `Bearer ${token}`, "x-celerex-confirm-copy": "true" },
  });
  if (!res.ok) throw await asError(res, "connector credential");
  return (await res.json()) as AdoptedConnectorCredential;
}
```

(If `api`/`asError`/`FetchFn` are not exported from `providers.ts`, import them from wherever `providers.ts` imports them — check `providers.ts`'s own imports and copy.)

- [ ] **Step 2: Export from `index.ts`** — add:

```typescript
export * from "./connectors.js";
```

- [ ] **Step 3: Write the test** `test/connectors.test.ts` (mirror provider tests, use `./helpers.js`):

```typescript
import { test, expect } from "bun:test";
import { fetchConnectors, fetchConnectorCredential } from "../src/connectors.js";
import { jsonResponse, seqFetch } from "./helpers.js";

test("fetchConnectors GETs the catalog with the bearer token", async () => {
  const { fn, calls } = seqFetch([
    jsonResponse({ connectors: [{ id: "m365", label: "Microsoft 365", auth_kind: "app_creds", configured: false, tools: [] }] }),
  ]);
  const out = await fetchConnectors("http://localhost:3001", "tok-1", fn);
  expect(out.connectors[0]!.id).toBe("m365");
  expect(calls[0]!.url).toBe("http://localhost:3001/api/design/connectors");
  expect((calls[0]!.init?.headers as Record<string, string>).authorization).toBe("Bearer tok-1");
});

test("fetchConnectorCredential sends the confirm header", async () => {
  const { fn, calls } = seqFetch([
    jsonResponse({ connector_id: "http-api", kind: "api_key", value: "secret-123" }),
  ]);
  const out = await fetchConnectorCredential("http://localhost:3001", "tok-1", "http-api", fn);
  expect(out.value).toBe("secret-123");
  expect(calls[0]!.url).toBe("http://localhost:3001/api/design/connectors/http-api/credential");
  expect((calls[0]!.init?.headers as Record<string, string>)["x-celerex-confirm-copy"]).toBe("true");
});
```

- [ ] **Step 4: Verify** `cd packages/celerex-connect && bun test` → PASS; `bunx tsc -p tsconfig.json --noEmit` (or the package's typecheck) clean.

- [ ] **Step 5: Commit**

```bash
git add packages/celerex-connect/src/connectors.ts packages/celerex-connect/src/index.ts packages/celerex-connect/test/connectors.test.ts
git commit -m "feat(connect): connector catalog + credential fetchers"
```

---

## PART C — Design daemon: adopt + execute locally, remove Composio ⏸ DEFERRED (review-gated)

> Execute in a session where the celerex-design monorepo builds + daemon vitest runs. This
> is the large, deeply-integrated change. Approach is **additive-first** (add an `agent-vault`
> provider alongside Composio, prove it, then delete Composio) to keep the app working at
> each step.

### Task C1: app-config — adopted connector store
- Modify `apps/daemon/src/app-config.ts`: add `connectorCredentials?: Record<string, { kind: string; value: string | null }> | null` to `AppConfigPrefs` + its allowed-keys set. Test: vitest round-trips the new field.

### Task C2: seedFromToken adopts connectors
- Modify `apps/daemon/src/server.ts` `seedFromToken`: after the provider seed, call `fetchConnectors` + (for each `configured` non-internal) `fetchConnectorCredential`, and persist into `connectorCredentials`. Test: vitest with a mocked seam asserts creds land in app-config.

### Task C3: local executors
- Create `apps/daemon/src/connectors/executors/{graph.ts,httpApiKey.ts,memory.ts}` — each takes `(toolName, input, adoptedCredential)` → `BoundedJsonObject`. `graph` calls Microsoft Graph with the adopted app-creds (client-credentials token); `httpApiKey` does a GET with `Authorization` from the adopted key; `memory` calls galvo-memory. Tests: vitest against a mocked `fetch`.

### Task C4: agent-vault provider at the execution seam
- Add an `agent-vault` authentication branch in `apps/daemon/src/connectors/service.ts` `executeConnectorProviderTool` (the recon's seam at ~service.ts:846): when `definition.authentication === 'agent-vault'`, dispatch to the matching executor (C3) using the adopted credential (C1). Add static `agent-vault` connector definitions (m365 / galvo-memory / http-api) to the fast-definitions list, marked connected when `configured`. Tests: vitest executes a read tool end-to-end through the seam with a mocked executor.

### Task C5: point the UI + remove Composio
- `apps/web/src/components/SettingsDialog.tsx` `ConnectorSection`: replace the Composio API-key form with a read-only "Configure connectors in OmniVyx Agent" status (driven by `/api/connectors` + Agent connection state). Keep `ConnectorsBrowser` + the `/api/connectors/*` route contract.
- Delete `apps/daemon/src/connectors/{composio-config.ts, composio.ts, composio-curation.ts, composio-descriptions.ts}` + `config.composio` once C2-C4 are green. Test: an integration test asserting no `composio` import path remains and `/api/connectors` returns the agent-vault catalog.

---

## Self-Review

**Spec coverage:** §3 architecture → A1/A2 (catalog+credential) + B1 (seam) + C1-C5 (daemon).
§4.1 Agent endpoints → A1/A2 (under `DeviceAuth`, confirm-gated, no secrets in catalog). §4.2
seam → B1. §4.3 daemon reimplementation + Composio removal → C1-C5. §5 data flow → C2 (adopt)
+ C3/C4 (execute). §6 credentials (vault, per-engagement, confirm-gated) → A2 + reuse of
`engagement_provider_keys`. §7 errors → A2 (`not configured` 404; internal null) + C4
(`not_configured`/`needs_resync`); read-only enforced by catalog (read tools only) + C4. §8
testing → A (`design_connectors.rs`), B (`connectors.test.ts`), C (vitest). §9 server-side
execution correctly NOT built (deferred 2e). **Gap:** the Agent-side "Connectors" settings UI
(spec §4.1) is intentionally omitted from v1 build — connector creds are set via the existing
`PUT /api/providers/keys/{id}` setter (connectors share the store); a dedicated UI is a small
follow-up, noted here.

**Placeholder scan:** none — every code step is complete. C tasks reference exact files +
the recon seam points; their per-step test code is to be filled at execution against the live
monorepo (flagged DEFERRED, not a placeholder in an executing task).

**Type consistency:** `DesignConnector`/`ConnectorToolInfo`/`auth_kind` match across Rust (A)
and TS (B). `x-celerex-confirm-copy: true` consistent A2↔B1. `connector_id`/`kind`/`value`
response shape identical in A2 and B1's `AdoptedConnectorCredential`.
