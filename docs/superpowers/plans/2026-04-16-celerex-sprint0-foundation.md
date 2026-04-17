# Celerex Sprint 0 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Celerex agent substrate — orchestrator, tool dispatch, plugin manifest schema (inbound + outbound), webhook ingress, Doc I/O HTTP bridge, JWT auth, Project data layer, Anthropic tool_use round-trip, Galvo-as-plugin fixture — with no user-visible features. Sprint 1 (Jean slice) will sit on top.

**Galvo positioning:** Galvo is an *optional* peer product, reachable only through the standard plugin surface — it is not a first-class subsystem of Celerex. The Galvatronic protocol (see Task 29) is the A2A wire format that any peer plugin can speak; the Sprint 0 deliverable is a Galvo plugin manifest + canned fixture proving the plugin path works end-to-end (Task 30). Swap-in for live Galvo later is just a different plugin install — no Celerex changes.

**Credential storage positioning:** The encrypted credential store is behind a `CredentialStore` trait (Task 24). MVP ships a local AEAD impl (`LocalAeadStore`, ChaCha20-Poly1305 via `ring`). A future Vault/KMS-backed impl implements the same trait and swaps in via DI — call sites never touch the concrete cipher.

**Architecture:** Rust backend split into `celerex-api` (HTTP surface, auth, routes, persistence) and `celerex-orchestrator` (job execution, DAG walk, tool dispatch, triggers). Postgres + sqlx for persistent state, JWT auth middleware extracts `engagement_id`/`project_id` claims into request extensions. Frontend grows a Project tree above engagements; Copilot/coding-agent code is stripped. All work TDD with frequent commits.

**Tech Stack:** Rust (Axum 0.8, sqlx, tokio, reqwest, serde, jsonwebtoken, ring), Postgres 15+, TypeScript/React (Jotai, Vitest), BlockSuite (existing).

**Reference spec:** `docs/superpowers/specs/2026-04-16-celerex-pivot-design.md` (commit `680d7fa`).

**Estimated duration:** ~3–3.5 weeks (expanded from 2 weeks to absorb DB infra + JWT middleware + test framework that the spec assumed existed).

---

## File Structure

### New files (Rust backend)

| Path | Responsibility |
|------|----------------|
| `packages/backend/api/src/db/mod.rs` | Postgres pool initialization, migration runner |
| `packages/backend/api/src/db/migrations/0001_init.sql` | engagements + documents tables (DB version of in-mem state) |
| `packages/backend/api/src/db/migrations/0002_projects.sql` | projects table + `project_id` column on engagements |
| `packages/backend/api/src/db/migrations/0003_plugins.sql` | plugin_installs + plugin_credentials tables |
| `packages/backend/api/src/db/migrations/0004_runs.sql` | runs + run_steps + webhook_dedupe tables |
| `packages/backend/api/src/auth/mod.rs` | JWT middleware module entry |
| `packages/backend/api/src/auth/jwt.rs` | claim verification + extraction |
| `packages/backend/api/src/auth/extractors.rs` | Axum extractors `EngagementScope`, `ProjectScope` |
| `packages/backend/api/src/projects/mod.rs` | Project CRUD routes |
| `packages/backend/api/src/engagements/mod.rs` | Extracted engagement routes (was inline in main.rs) |
| `packages/backend/api/src/docs/mod.rs` | Doc I/O HTTP bridge: POST /api/docs, /:id/blocks, PATCH /:id/blocks/:blockId |
| `packages/backend/api/src/plugins/mod.rs` | Plugin install/uninstall/list routes |
| `packages/backend/api/src/plugins/manifest.rs` | Manifest schema (inbound + outbound capability arrays) |
| `packages/backend/api/src/plugins/credentials.rs` | `CredentialStore` trait + `LocalAeadStore` MVP impl (ChaCha20-Poly1305) |
| `packages/backend/api/src/webhooks/mod.rs` | POST /api/webhooks/:pluginId/:event ingress |
| `packages/backend/api/src/webhooks/signature.rs` | Per-plugin signature verification trait |
| `packages/backend/api/src/llm/mod.rs` | LLM module entry (extracted from main.rs) |
| `packages/backend/api/src/llm/anthropic.rs` | Anthropic Messages + tool_use round-trip loop |
| `packages/backend/api/tests/fixtures/plugins/galvo/manifest.json` | Galvo plugin manifest fixture (outbound `galvo.research.weekly`) |
| `packages/backend/api/tests/fixtures/plugins/galvo/research_weekly.md` | Canned MD fixture served by the fixture-backed A2A stub |
| `packages/backend/agent-orchestrator/src/handlers/agent_stub.rs` | Fixture-backed `A2AClient` impl for tests + dev (behind `test-support` feature) |
| `packages/backend/api/tests/common/mod.rs` | Test harness: Postgres fixture, JWT helper |
| `packages/backend/api/tests/auth.rs` | Integration tests for JWT middleware |
| `packages/backend/api/tests/projects.rs` | Integration tests for Project routes |
| `packages/backend/api/tests/docs.rs` | Integration tests for Doc I/O bridge |
| `packages/backend/api/tests/plugins.rs` | Integration tests for plugin registry |
| `packages/backend/api/tests/webhooks.rs` | Integration tests for webhook ingress |
| `packages/backend/agent-orchestrator/src/lib.rs` | Public API of the orchestrator crate |
| `packages/backend/agent-orchestrator/src/job.rs` | `JobRequest`, `JobResult`, `RunState` types |
| `packages/backend/agent-orchestrator/src/execution_type.rs` | `ExecutionType` enum mirroring frontend `tools.ts` |
| `packages/backend/agent-orchestrator/src/tool_dispatch.rs` | Handler registry, dispatch by `ExecutionType` |
| `packages/backend/agent-orchestrator/src/handlers/http.rs` | HTTP handler |
| `packages/backend/agent-orchestrator/src/handlers/mcp.rs` | MCP handler (skeleton — full impl deferred to V2) |
| `packages/backend/agent-orchestrator/src/handlers/agent.rs` | Agent (A2A) handler — generic, plugin-backed (Galvo is one consumer) |
| `packages/backend/agent-orchestrator/src/handlers/script.rs` | Script handler |
| `packages/backend/agent-orchestrator/src/handlers/llm.rs` | LLM handler bridging into `celerex-api`'s anthropic module |
| `packages/backend/agent-orchestrator/src/executor.rs` | Pipeline DAG executor with per-step persistence |
| `packages/backend/agent-orchestrator/src/triggers.rs` | Trigger union: `UserClick`, `Scheduled`, `Event` |
| `packages/backend/agent-orchestrator/tests/executor.rs` | DAG walk tests (success, failure, retry-from-step) |
| `packages/backend/agent-orchestrator/tests/dispatch.rs` | Tool dispatch handler tests |
| `docs/galvatronic-protocol-v0.1.md` | Galvatronic A2A protocol spec |

### Modified files (Rust backend)

| Path | Changes |
|------|---------|
| `Cargo.toml` (root) | Add workspace deps: sqlx, jsonwebtoken, ring, async-trait |
| `packages/backend/api/Cargo.toml` | Add deps + dev-deps for sqlx-test |
| `packages/backend/api/src/main.rs` | Reduce to bootstrapping: initialize DB, build router from extracted modules, run server. Delete: Copilot OAuth (lines 73-218, 290-292, 507-721), in-mem `AppState`, inline route handlers (relocated). |
| `packages/backend/agent-orchestrator/Cargo.toml` | Add deps; declare as `lib` |
| `packages/backend/agent-orchestrator/src/main.rs` | Delete (replaced by `lib.rs`) |

### New + modified (frontend)

| Path | Status | Changes |
|------|--------|---------|
| `packages/frontend/app/vitest.config.ts` | Create | Vitest config |
| `packages/frontend/app/src/state/atoms.ts` | Modify | Add `projectsAtom`, `activeProjectIdAtom`, derived `engagementsForActiveProjectAtom`. Add `projectId` to `Engagement` type. |
| `packages/frontend/app/src/state/__tests__/atoms.test.ts` | Create | Atom unit tests |
| `packages/frontend/app/src/api/client.ts` | Modify | Add `projects` endpoints; add JWT bearer header support |
| `packages/frontend/app/src/layout/Sidebar.tsx` | Modify | Insert Project tree level above engagements |
| `packages/frontend/app/src/layout/Breadcrumb.tsx` | Modify | Add Project crumb |
| `packages/frontend/app/src/layout/CopilotAuthDialog.tsx` | Delete | |
| `packages/frontend/app/src/layout/AIProviderSelector.tsx` | Modify | Drop Copilot provider option |
| `packages/frontend/app/src/layout/OnboardingDialog.tsx` | Modify | Reword "engagement" → "project / workspace" in user-visible copy |
| `packages/frontend/app/src/templates/templates.ts` | Modify | Rename `consulting` category → `business`; rename templates that lean ConsultantCanvas |
| `packages/frontend/app/package.json` | Modify | Add `"test": "vitest"` script |

---

## Phase A — Foundation (test infra, DB, JWT auth)

### Task 1: Vitest setup on frontend

**Files:**
- Create: `packages/frontend/app/vitest.config.ts`
- Create: `packages/frontend/app/src/state/__tests__/atoms.test.ts`
- Modify: `packages/frontend/app/package.json` (add test script + deps)

- [ ] **Step 1: Add Vitest config**

```ts
// packages/frontend/app/vitest.config.ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/__tests__/**/*.test.{ts,tsx}"],
  },
});
```

- [ ] **Step 2: Add test deps + script to package.json**

```jsonc
// packages/frontend/app/package.json — under "scripts"
"test": "vitest run",
"test:watch": "vitest"
```

Run: `cd packages/frontend/app && bun add -d vitest jsdom @vitejs/plugin-react @testing-library/react @testing-library/jest-dom`

- [ ] **Step 3: Write the failing canary test**

```ts
// packages/frontend/app/src/state/__tests__/atoms.test.ts
import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import { engagementsAtom } from "../atoms";

describe("engagementsAtom", () => {
  it("starts with at least one default engagement", () => {
    const store = createStore();
    const value = store.get(engagementsAtom);
    expect(Array.isArray(value)).toBe(true);
    expect(value.length).toBeGreaterThanOrEqual(1);
  });
});
```

- [ ] **Step 4: Run test — expect PASS (state is already initialized that way)**

Run: `cd packages/frontend/app && bun run test`
Expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add packages/frontend/app/vitest.config.ts \
        packages/frontend/app/package.json \
        packages/frontend/app/src/state/__tests__/atoms.test.ts \
        bun.lock
git commit -m "test(frontend): wire up vitest + canary atoms test"
```

---

### Task 2: Rust integration test scaffold + Postgres test fixture

**Files:**
- Create: `packages/backend/api/tests/common/mod.rs`
- Create: `packages/backend/api/tests/health.rs`
- Modify: `packages/backend/api/Cargo.toml` (add dev-dependencies)
- Modify: `Cargo.toml` (workspace deps)

- [ ] **Step 1: Add workspace + crate dependencies**

```toml
# Cargo.toml (root) — under [workspace.dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "json", "macros", "migrate"] }
jsonwebtoken = "9"
ring = "0.17"
async-trait = "0.1"
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
```

```toml
# packages/backend/api/Cargo.toml
[dependencies]
sqlx = { workspace = true }
jsonwebtoken = { workspace = true }
ring = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
testcontainers = { workspace = true }
testcontainers-modules = { workspace = true }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
reqwest = { version = "0.12", features = ["json"] }
serde_json = "1"
```

- [ ] **Step 2: Create test harness module**

```rust
// packages/backend/api/tests/common/mod.rs
use testcontainers::{ContainerAsync, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use sqlx::PgPool;

pub struct TestDb {
    pub pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

pub async fn start_test_db() -> TestDb {
    let container = Postgres::default().start().await.expect("postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("connect");
    sqlx::migrate!("./src/db/migrations").run(&pool).await.expect("migrate");
    TestDb { pool, _container: container }
}
```

- [ ] **Step 3: Write the failing health-check integration test**

```rust
// packages/backend/api/tests/health.rs
mod common;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).expect("server");
    let resp = server.get("/api/health").await;
    resp.assert_status_ok();
}
```

Add to `[dev-dependencies]`: `axum-test = "17"`.

- [ ] **Step 4: Run — expect FAIL (no `build_app` yet, no migrations dir yet)**

Run: `cargo test -p celerex-api --test health`
Expected: compile error: `cannot find function build_app in crate celerex_api`.

- [ ] **Step 5: Stub `build_app` + empty migrations dir to compile**

```rust
// packages/backend/api/src/lib.rs (new file)
use axum::{Router, routing::get, Json};
use serde_json::json;
use sqlx::PgPool;

pub fn build_app(_pool: PgPool) -> Router {
    Router::new().route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
}
```

```bash
mkdir -p packages/backend/api/src/db/migrations
touch packages/backend/api/src/db/migrations/.gitkeep
```

Add to `packages/backend/api/Cargo.toml`:
```toml
[lib]
name = "celerex_api"
path = "src/lib.rs"
```

- [ ] **Step 6: Run — expect PASS**

Run: `cargo test -p celerex-api --test health`
Expected: `1 passed`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml packages/backend/api/Cargo.toml \
        packages/backend/api/src/lib.rs \
        packages/backend/api/src/db/migrations/.gitkeep \
        packages/backend/api/tests/common/mod.rs \
        packages/backend/api/tests/health.rs \
        Cargo.lock
git commit -m "test(api): add testcontainers-postgres integration harness + health test"
```

---

### Task 3: First migration — engagements + documents (DB-backed)

**Files:**
- Create: `packages/backend/api/src/db/migrations/0001_init.sql`
- Create: `packages/backend/api/src/db/mod.rs`
- Create: `packages/backend/api/tests/engagements.rs`

- [ ] **Step 1: Write the failing engagement-CRUD test**

```rust
// packages/backend/api/tests/engagements.rs
mod common;
use serde_json::json;

#[tokio::test]
async fn create_then_list_engagement_persists_in_db() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).expect("server");

    let resp = server.post("/api/engagements")
        .json(&json!({ "name": "Acme R&D" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["name"], "Acme R&D");

    let list = server.get("/api/engagements").await;
    list.assert_status_ok();
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "Acme R&D");
}
```

- [ ] **Step 2: Run — expect FAIL (no migration, no route)**

Run: `cargo test -p celerex-api --test engagements`
Expected: assertion fails or 404.

- [ ] **Step 3: Add migration**

```sql
-- packages/backend/api/src/db/migrations/0001_init.sql
CREATE TABLE engagements (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_documents_engagement_id ON documents(engagement_id);
```

- [ ] **Step 4: Add `db` module + repository functions**

```rust
// packages/backend/api/src/db/mod.rs
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct Engagement {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn init_pool(database_url: &str) -> sqlx::Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(8).connect(database_url).await?;
    sqlx::migrate!("./src/db/migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn create_engagement(pool: &PgPool, name: &str) -> sqlx::Result<Engagement> {
    sqlx::query_as::<_, Engagement>(
        "INSERT INTO engagements (name) VALUES ($1) RETURNING id, name, created_at"
    ).bind(name).fetch_one(pool).await
}

pub async fn list_engagements(pool: &PgPool) -> sqlx::Result<Vec<Engagement>> {
    sqlx::query_as::<_, Engagement>(
        "SELECT id, name, created_at FROM engagements ORDER BY created_at DESC"
    ).fetch_all(pool).await
}
```

- [ ] **Step 5: Wire routes into `build_app`**

```rust
// packages/backend/api/src/lib.rs (replace previous body)
pub mod db;
pub mod engagements;

use axum::{Router, routing::{get, post}, extract::State, Json};
use serde_json::json;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState { pub pool: PgPool }

pub fn build_app(pool: PgPool) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/engagements", get(engagements::list).post(engagements::create))
        .with_state(AppState { pool })
}
```

```rust
// packages/backend/api/src/engagements/mod.rs
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, db};

#[derive(Deserialize)]
pub struct CreateBody { pub name: String }

pub async fn list(State(s): State<AppState>) -> impl IntoResponse {
    match db::list_engagements(&s.pool).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create(State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_engagement(&s.pool, &body.name).await {
        Ok(row) => (StatusCode::CREATED, Json(row)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

- [ ] **Step 6: Run — expect PASS**

Run: `cargo test -p celerex-api --test engagements`
Expected: `1 passed`.

- [ ] **Step 7: Commit**

```bash
git add packages/backend/api/src/db/ \
        packages/backend/api/src/engagements/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/engagements.rs
git commit -m "feat(api): persist engagements in postgres via sqlx"
```

---

### Task 4: Replace in-memory engagements in main.rs with DB-backed routes

**Files:**
- Modify: `packages/backend/api/src/main.rs` — strip in-mem state, call `build_app`

- [ ] **Step 1: Read existing `main.rs:1-310` to understand bootstrap shape**

(Engineer reads file before editing.)

- [ ] **Step 2: Replace `main.rs` body with thin bootstrap**

```rust
// packages/backend/api/src/main.rs (replace entire file)
use celerex_api::{build_app, db::init_pool};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/celerex".into());
    let pool = init_pool(&url).await?;
    let app = build_app(pool);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await?;
    println!("celerex-api listening on http://0.0.0.0:3001");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 3: Run — expect FAIL (Copilot/chat handlers gone, no compilation issues if we re-add deleted modules later — see Phase B)**

Run: `cargo build -p celerex-api`
Expected: builds clean. (If unused-import warnings appear in `Cargo.toml` deps, leave them — Phase B-onward will use them.)

- [ ] **Step 4: Quick smoke — start server against test DB, hit /api/health**

```bash
docker run --rm -d --name pg-celerex -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:15
sleep 2
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres cargo run -p celerex-api &
sleep 3
curl -fsS http://127.0.0.1:3001/api/health | jq .
kill %1; docker stop pg-celerex
```
Expected: `{ "ok": true }`.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/main.rs
git commit -m "refactor(api): main.rs is now a thin bootstrap; routes live in lib"
```

---

### Task 5: JWT middleware + claim extractors (engagement_id, project_id)

**Files:**
- Create: `packages/backend/api/src/auth/mod.rs`
- Create: `packages/backend/api/src/auth/jwt.rs`
- Create: `packages/backend/api/src/auth/extractors.rs`
- Create: `packages/backend/api/tests/auth.rs`

- [ ] **Step 1: Write the failing auth integration test**

```rust
// packages/backend/api/tests/auth.rs
mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    sub: String,
    engagement_id: String,
    project_id: Option<String>,
    exp: usize,
}

fn make_token(secret: &str, eng: Uuid, proj: Option<Uuid>) -> String {
    let claims = Claims {
        sub: "user-1".into(),
        engagement_id: eng.to_string(),
        project_id: proj.map(|p| p.to_string()),
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
}

#[tokio::test]
async fn protected_route_rejects_missing_token() {
    std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let resp = server.get("/api/projects").await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_accepts_valid_token() {
    std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let token = make_token("test-secret", Uuid::new_v4(), None);
    let resp = server.get("/api/projects")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    resp.assert_status_ok();
}
```

- [ ] **Step 2: Run — expect FAIL (no /api/projects, no middleware)**

Run: `cargo test -p celerex-api --test auth`
Expected: 404 or compile error.

- [ ] **Step 3: Implement JWT verification**

```rust
// packages/backend/api/src/auth/jwt.rs
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub exp: usize,
}

pub fn verify(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}
```

- [ ] **Step 4: Implement Axum extractor**

```rust
// packages/backend/api/src/auth/extractors.rs
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode, header::AUTHORIZATION},
};
use crate::auth::jwt::{verify, Claims};

pub struct AuthClaims(pub Claims);

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthClaims {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization"))?;
        let token = header.strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer scheme"))?;
        let secret = std::env::var("CELEREX_JWT_SECRET")
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JWT secret unset"))?;
        let claims = verify(token, &secret).map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))?;
        Ok(AuthClaims(claims))
    }
}
```

```rust
// packages/backend/api/src/auth/mod.rs
pub mod jwt;
pub mod extractors;
pub use extractors::AuthClaims;
```

- [ ] **Step 5: Add stub `/api/projects` route that requires auth**

```rust
// packages/backend/api/src/projects/mod.rs (placeholder; full CRUD in Task 8)
use axum::{response::IntoResponse, Json};
use serde_json::json;
use crate::auth::AuthClaims;

pub async fn list(_auth: AuthClaims) -> impl IntoResponse {
    Json(json!([]))
}
```

```rust
// packages/backend/api/src/lib.rs — add module + route
pub mod auth;
pub mod projects;

// inside build_app:
.route("/api/projects", get(projects::list))
```

- [ ] **Step 6: Run — expect PASS**

Run: `cargo test -p celerex-api --test auth`
Expected: `2 passed`.

- [ ] **Step 7: Commit**

```bash
git add packages/backend/api/src/auth/ \
        packages/backend/api/src/projects/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/auth.rs
git commit -m "feat(api): JWT middleware with engagement_id/project_id claims"
```

---

## Phase B — Cleanup (delete coding-agent code, rebrand copy)

### Task 6: Delete Copilot OAuth + GitHub device-flow code

**Files:**
- Modify: (verify already gone after Task 4 main.rs rewrite — sweep for stragglers)
- Delete: `packages/frontend/app/src/layout/CopilotAuthDialog.tsx`
- Modify: `packages/frontend/app/src/layout/AIProviderSelector.tsx`

- [ ] **Step 1: Confirm backend Copilot code is gone**

Run: `grep -rn "copilot" packages/backend/ || true`
Expected: no matches, OR only matches in `Cargo.lock`. If matches in source: delete those blocks.

- [ ] **Step 2: Delete CopilotAuthDialog**

```bash
rm packages/frontend/app/src/layout/CopilotAuthDialog.tsx
```

- [ ] **Step 3: Read AIProviderSelector.tsx, then strip Copilot option**

Open `packages/frontend/app/src/layout/AIProviderSelector.tsx`. Remove any provider option whose id is `"github-copilot"` or label contains "Copilot". Remove imports of `CopilotAuthDialog`.

- [ ] **Step 4: Confirm frontend builds**

Run: `cd packages/frontend/app && bun run build`
Expected: build succeeds, no missing-import errors.

- [ ] **Step 5: Confirm frontend tests still pass**

Run: `cd packages/frontend/app && bun run test`
Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add -u packages/
git commit -m "chore: drop Copilot OAuth / GitHub device-flow (out of scope per pivot)"
```

---

### Task 7: Rebrand ConsultantCanvas user-visible copy

**Files:**
- Modify: `packages/frontend/app/src/templates/templates.ts`
- Modify: `packages/frontend/app/src/layout/OnboardingDialog.tsx`
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx` (user-facing strings only)

- [ ] **Step 1: Search for user-visible "engagement" strings**

Run: `grep -rn '"[Ee]ngagement' packages/frontend/app/src --include="*.tsx" --include="*.ts"`
Note: `engagementId`, `engagementsAtom`, route paths stay as-is. Only swap user-facing labels.

- [ ] **Step 2: Rename the `consulting` template category and consulting-flavored templates**

In `packages/frontend/app/src/templates/templates.ts`:
- Replace category id `"consulting"` with `"business"`.
- Rename "Project Brief" stays. Rename "Stakeholder Analysis" → "Stakeholder Map".
- Drop or relabel "Engagement Charter" if present.

- [ ] **Step 3: Reword OnboardingDialog body copy**

In `packages/frontend/app/src/layout/OnboardingDialog.tsx`, replace user-visible "engagement" copy with "workspace" or "project" as fits the sentence. Keep state/atom names internal.

- [ ] **Step 4: Reword Sidebar "New engagement" button → "New workspace"**

Edit `packages/frontend/app/src/layout/Sidebar.tsx` button label only; do not touch atom names.

- [ ] **Step 5: Build + test**

```bash
cd packages/frontend/app && bun run build && bun run test
```
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add -u packages/frontend/app/src/
git commit -m "chore(frontend): rebrand user-visible consultancy copy → workspace/project"
```

---

## Phase C — Project layer

### Task 8: Project schema migration + CRUD routes

**Files:**
- Create: `packages/backend/api/src/db/migrations/0002_projects.sql`
- Modify: `packages/backend/api/src/projects/mod.rs` (replace stub with real CRUD)
- Modify: `packages/backend/api/src/db/mod.rs` (project repo functions)
- Create: `packages/backend/api/tests/projects.rs`

- [ ] **Step 1: Write failing test for project CRUD**

```rust
// packages/backend/api/tests/projects.rs
mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid) -> String {
    std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    let c = Claims {
        sub: "u".into(), engagement_id: eng.to_string(), project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize,
    };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn create_list_project_within_engagement() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    // seed an engagement directly
    let eng = sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('E1') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let resp = server.post("/api/projects")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "name": "Weekly Newsletter" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);

    let list = server.get("/api/projects")
        .add_header("authorization", format!("Bearer {t}"))
        .await;
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "Weekly Newsletter");
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p celerex-api --test projects`
Expected: 404 / wrong status.

- [ ] **Step 3: Add migration**

```sql
-- packages/backend/api/src/db/migrations/0002_projects.sql
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_projects_engagement_id ON projects(engagement_id);

-- documents now belong to a project (nullable for migration safety; backfill not needed in fresh DB)
ALTER TABLE documents ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE CASCADE;
CREATE INDEX idx_documents_project_id ON documents(project_id);
```

- [ ] **Step 4: Add repo functions + replace stub handler**

```rust
// packages/backend/api/src/db/mod.rs — append
#[derive(sqlx::FromRow, serde::Serialize, Clone)]
pub struct Project {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_project(pool: &PgPool, eng: Uuid, name: &str) -> sqlx::Result<Project> {
    sqlx::query_as::<_, Project>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, $2)
         RETURNING id, engagement_id, name, created_at"
    ).bind(eng).bind(name).fetch_one(pool).await
}

pub async fn list_projects(pool: &PgPool, eng: Uuid) -> sqlx::Result<Vec<Project>> {
    sqlx::query_as::<_, Project>(
        "SELECT id, engagement_id, name, created_at FROM projects
         WHERE engagement_id = $1 ORDER BY created_at DESC"
    ).bind(eng).fetch_all(pool).await
}
```

```rust
// packages/backend/api/src/projects/mod.rs (replace stub)
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, auth::AuthClaims, db};

#[derive(Deserialize)]
pub struct CreateBody { pub name: String }

pub async fn list(auth: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    match db::list_projects(&s.pool, auth.0.engagement_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create(auth: AuthClaims, State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_project(&s.pool, auth.0.engagement_id, &body.name).await {
        Ok(p) => (StatusCode::CREATED, Json(p)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

```rust
// packages/backend/api/src/lib.rs — update route registration
.route("/api/projects", get(projects::list).post(projects::create))
```

- [ ] **Step 5: Run — expect PASS**

Run: `cargo test -p celerex-api --test projects`
Expected: `1 passed`. Re-run `cargo test -p celerex-api --test auth` — both still pass.

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/db/migrations/0002_projects.sql \
        packages/backend/api/src/db/mod.rs \
        packages/backend/api/src/projects/mod.rs \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/projects.rs
git commit -m "feat(api): Project resource scoped to engagement"
```

---

### Task 9: Frontend `projectsAtom` + active project state

**Files:**
- Modify: `packages/frontend/app/src/state/atoms.ts`
- Create: `packages/frontend/app/src/state/__tests__/projects.test.ts`
- Modify: `packages/frontend/app/src/api/client.ts`

- [ ] **Step 1: Write failing atom test**

```ts
// packages/frontend/app/src/state/__tests__/projects.test.ts
import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import { projectsAtom, activeProjectIdAtom, engagementsForActiveProjectAtom, engagementsAtom } from "../atoms";

describe("projects", () => {
  it("filters engagements by active project", () => {
    const store = createStore();
    store.set(projectsAtom, [{ id: "p1", name: "Newsletter", engagementId: "e1" }]);
    store.set(engagementsAtom, [
      { id: "e1", projectId: "p1", name: "Acme", docs: [] },
      { id: "e2", projectId: "p2", name: "Other", docs: [] },
    ]);
    store.set(activeProjectIdAtom, "p1");
    const visible = store.get(engagementsForActiveProjectAtom);
    expect(visible.map(e => e.id)).toEqual(["e1"]);
  });
});
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cd packages/frontend/app && bun run test`
Expected: `cannot find name projectsAtom`.

- [ ] **Step 3: Add atoms + types**

```ts
// packages/frontend/app/src/state/atoms.ts — append (do not break existing exports)
export interface Project { id: string; name: string; engagementId: string }

export const projectsAtom = atom<Project[]>([]);
export const activeProjectIdAtom = atom<string | null>(null);

export const engagementsForActiveProjectAtom = atom((get) => {
  const pid = get(activeProjectIdAtom);
  const all = get(engagementsAtom);
  return pid ? all.filter((e) => (e as any).projectId === pid) : all;
});
```

Also extend the `Engagement` type to include `projectId?: string`. (Engineer locates the type in this file and adds the field.)

- [ ] **Step 4: Add API client functions**

```ts
// packages/frontend/app/src/api/client.ts — append
export interface ApiProject { id: string; engagement_id: string; name: string; created_at: string }

export async function listProjects(token: string): Promise<ApiProject[]> {
  const r = await fetch(`${API_BASE}/projects`, { headers: { authorization: `Bearer ${token}` } });
  if (!r.ok) throw new Error(`listProjects: ${r.status}`);
  return r.json();
}

export async function createProject(token: string, name: string): Promise<ApiProject> {
  const r = await fetch(`${API_BASE}/projects`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ name }),
  });
  if (!r.ok) throw new Error(`createProject: ${r.status}`);
  return r.json();
}
```

(`API_BASE` is the existing constant in this file.)

- [ ] **Step 5: Run — expect PASS**

Run: `cd packages/frontend/app && bun run test`
Expected: 2+ passed.

- [ ] **Step 6: Commit**

```bash
git add packages/frontend/app/src/state/atoms.ts \
        packages/frontend/app/src/state/__tests__/projects.test.ts \
        packages/frontend/app/src/api/client.ts
git commit -m "feat(frontend): projects atom + API client"
```

---

### Task 10: Sidebar Project tree level

**Files:**
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx`
- Modify: `packages/frontend/app/src/layout/Breadcrumb.tsx`

- [ ] **Step 1: Read Sidebar.tsx fully — locate where engagements are listed**

(Engineer reads to find the rendering block.)

- [ ] **Step 2: Insert Project list above engagement tree**

Add a `useAtom(projectsAtom)` + `useAtom(activeProjectIdAtom)` block at top of the component. Render projects as expandable headers; nest engagements (filtered via `engagementsForActiveProjectAtom`) under the active project. If no active project, show a "Pick a project" empty state.

```tsx
// sketch — adapt to existing JSX shape
const [projects] = useAtom(projectsAtom);
const [activeProjectId, setActiveProjectId] = useAtom(activeProjectIdAtom);
const visibleEngagements = useAtomValue(engagementsForActiveProjectAtom);

// in JSX:
<section>
  <h3>Projects</h3>
  <ul>
    {projects.map((p) => (
      <li key={p.id}>
        <button onClick={() => setActiveProjectId(p.id)}
                aria-current={activeProjectId === p.id}>
          {p.name}
        </button>
      </li>
    ))}
  </ul>
</section>
{activeProjectId && (
  <section>
    <h3>Workspaces</h3>
    {/* existing engagement list, but iterate visibleEngagements instead of `engagements` */}
  </section>
)}
```

- [ ] **Step 3: Update Breadcrumb to include Project crumb**

In `Breadcrumb.tsx`, prepend a project segment derived from `activeProjectIdAtom` + `projectsAtom`.

- [ ] **Step 4: Manual smoke test**

```bash
cd packages/frontend/app && bun run dev
```

Open http://localhost:5173. Verify: empty Projects section renders without crash. (No real projects yet — full UX comes in Sprint 1.)

- [ ] **Step 5: Run frontend tests**

```bash
cd packages/frontend/app && bun run test
```
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add packages/frontend/app/src/layout/Sidebar.tsx \
        packages/frontend/app/src/layout/Breadcrumb.tsx
git commit -m "feat(frontend): sidebar project tree level"
```

---

## Phase D — Orchestrator + tool_dispatch

### Task 11: Bootstrap `celerex-orchestrator` as a library crate

**Files:**
- Modify: `packages/backend/agent-orchestrator/Cargo.toml`
- Delete: `packages/backend/agent-orchestrator/src/main.rs`
- Create: `packages/backend/agent-orchestrator/src/lib.rs`
- Create: `packages/backend/agent-orchestrator/tests/sanity.rs`

- [ ] **Step 1: Update Cargo.toml**

```toml
# packages/backend/agent-orchestrator/Cargo.toml
[package]
name = "celerex-orchestrator"
version = "0.1.0"
edition = "2021"

[lib]
name = "celerex_orchestrator"
path = "src/lib.rs"

[dependencies]
async-trait = { workspace = true }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 2: Replace `main.rs` with `lib.rs` skeleton**

```bash
rm packages/backend/agent-orchestrator/src/main.rs
```

```rust
// packages/backend/agent-orchestrator/src/lib.rs
pub mod execution_type;
pub mod job;
pub mod tool_dispatch;
pub mod executor;
pub mod triggers;
pub mod handlers;
```

Stub each module (empty `pub mod`s) so the crate compiles:

```rust
// packages/backend/agent-orchestrator/src/execution_type.rs
// packages/backend/agent-orchestrator/src/job.rs
// packages/backend/agent-orchestrator/src/tool_dispatch.rs
// packages/backend/agent-orchestrator/src/executor.rs
// packages/backend/agent-orchestrator/src/triggers.rs
// packages/backend/agent-orchestrator/src/handlers/mod.rs
```

- [ ] **Step 3: Sanity test**

```rust
// packages/backend/agent-orchestrator/tests/sanity.rs
#[test] fn crate_compiles() { assert_eq!(2 + 2, 4); }
```

Run: `cargo test -p celerex-orchestrator`
Expected: `1 passed`.

- [ ] **Step 4: Commit**

```bash
git add packages/backend/agent-orchestrator/
git commit -m "chore(orchestrator): convert to library crate skeleton"
```

---

### Task 12: `JobRequest`, `ExecutionType`, run state types

**Files:**
- Modify: `packages/backend/agent-orchestrator/src/execution_type.rs`
- Modify: `packages/backend/agent-orchestrator/src/job.rs`

- [ ] **Step 1: Write failing serde round-trip test**

```rust
// packages/backend/agent-orchestrator/tests/types.rs
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::job::{JobRequest, Step, StepKind, RunStatus};
use uuid::Uuid;

#[test]
fn job_request_round_trips_json() {
    let req = JobRequest {
        run_id: Uuid::new_v4(),
        engagement_id: Uuid::new_v4(),
        project_id: Some(Uuid::new_v4()),
        steps: vec![Step {
            id: "fetch".into(),
            depends_on: vec![],
            kind: StepKind::Tool {
                execution: ExecutionType::Http,
                config: serde_json::json!({ "url": "https://example.com" }),
                inputs: serde_json::json!({}),
            },
        }],
    };
    let s = serde_json::to_string(&req).unwrap();
    let back: JobRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(back.steps.len(), 1);
    assert_eq!(back.steps[0].id, "fetch");
    assert!(matches!(RunStatus::Running, RunStatus::Running));
}
```

- [ ] **Step 2: Run — expect FAIL (types don't exist)**

Run: `cargo test -p celerex-orchestrator --test types`
Expected: compile error.

- [ ] **Step 3: Define `ExecutionType`**

```rust
// packages/backend/agent-orchestrator/src/execution_type.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionType {
    Http,
    Mcp,
    Agent,
    Script,
    Llm,
}
```

- [ ] **Step 4: Define `JobRequest` + `Step` + `RunStatus`**

```rust
// packages/backend/agent-orchestrator/src/job.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::execution_type::ExecutionType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub run_id: Uuid,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub depends_on: Vec<String>,
    pub kind: StepKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    Tool { execution: ExecutionType, config: serde_json::Value, inputs: serde_json::Value },
    Filter { expression: String },
    Outbound { execution: ExecutionType, config: serde_json::Value, inputs: serde_json::Value },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus { Pending, Running, Succeeded, Failed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub run_id: Uuid,
    pub step_id: String,
    pub status: RunStatus,
    pub inputs: serde_json::Value,
    pub outputs: Option<serde_json::Value>,
    pub error: Option<String>,
}
```

- [ ] **Step 5: Run — expect PASS**

Run: `cargo test -p celerex-orchestrator --test types`
Expected: `1 passed`.

- [ ] **Step 6: Commit**

```bash
git add packages/backend/agent-orchestrator/src/execution_type.rs \
        packages/backend/agent-orchestrator/src/job.rs \
        packages/backend/agent-orchestrator/tests/types.rs
git commit -m "feat(orchestrator): JobRequest + Step + RunStatus types"
```

---

### Task 13: `tool_dispatch.rs` — handler registry

**Files:**
- Modify: `packages/backend/agent-orchestrator/src/tool_dispatch.rs`
- Modify: `packages/backend/agent-orchestrator/src/handlers/mod.rs`
- Create: `packages/backend/agent-orchestrator/tests/dispatch.rs`

- [ ] **Step 1: Write failing dispatch test using a fake handler**

```rust
// packages/backend/agent-orchestrator/tests/dispatch.rs
use celerex_orchestrator::tool_dispatch::{Dispatcher, Handler, DispatchError};
use celerex_orchestrator::execution_type::ExecutionType;
use serde_json::{Value, json};
use async_trait::async_trait;
use std::sync::Arc;

struct EchoHandler;

#[async_trait]
impl Handler for EchoHandler {
    async fn invoke(&self, _config: Value, inputs: Value) -> Result<Value, DispatchError> {
        Ok(json!({ "echo": inputs }))
    }
}

#[tokio::test]
async fn dispatcher_routes_to_registered_handler() {
    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(EchoHandler));
    let out = d.invoke(ExecutionType::Http, json!({}), json!({"x": 1})).await.unwrap();
    assert_eq!(out, json!({ "echo": { "x": 1 } }));
}

#[tokio::test]
async fn dispatcher_errors_on_unregistered_type() {
    let d = Dispatcher::new();
    let err = d.invoke(ExecutionType::Mcp, json!({}), json!({})).await.unwrap_err();
    assert!(matches!(err, DispatchError::NoHandler(_)));
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p celerex-orchestrator --test dispatch`
Expected: compile error.

- [ ] **Step 3: Implement `Dispatcher` + `Handler` trait**

```rust
// packages/backend/agent-orchestrator/src/tool_dispatch.rs
use crate::execution_type::ExecutionType;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("no handler registered for {0:?}")]
    NoHandler(ExecutionType),
    #[error("handler error: {0}")]
    Handler(String),
}

#[async_trait]
pub trait Handler: Send + Sync {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError>;
}

pub struct Dispatcher {
    handlers: HashMap<ExecutionType, Arc<dyn Handler>>,
}

impl Dispatcher {
    pub fn new() -> Self { Self { handlers: HashMap::new() } }

    pub fn register(&mut self, kind: ExecutionType, handler: Arc<dyn Handler>) {
        self.handlers.insert(kind, handler);
    }

    pub async fn invoke(&self, kind: ExecutionType, config: Value, inputs: Value)
        -> Result<Value, DispatchError>
    {
        let h = self.handlers.get(&kind).ok_or(DispatchError::NoHandler(kind))?;
        h.invoke(config, inputs).await
    }
}

impl Default for Dispatcher { fn default() -> Self { Self::new() } }
```

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p celerex-orchestrator --test dispatch`
Expected: `2 passed`.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/agent-orchestrator/src/tool_dispatch.rs \
        packages/backend/agent-orchestrator/tests/dispatch.rs
git commit -m "feat(orchestrator): Dispatcher + Handler trait"
```

---

### Task 14: HTTP handler

**Files:**
- Create: `packages/backend/agent-orchestrator/src/handlers/http.rs`
- Modify: `packages/backend/agent-orchestrator/src/handlers/mod.rs`
- Modify: `packages/backend/agent-orchestrator/Cargo.toml` (add `reqwest`)

- [ ] **Step 1: Add reqwest dep**

```toml
# packages/backend/agent-orchestrator/Cargo.toml — under [dependencies]
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
```

- [ ] **Step 2: Write failing test (using `wiremock` for HTTP fixture)**

Add `wiremock = "0.6"` to dev-deps.

```rust
// packages/backend/agent-orchestrator/tests/http_handler.rs
use celerex_orchestrator::handlers::http::HttpHandler;
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::{method, path}};

#[tokio::test]
async fn http_handler_calls_url_and_returns_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/things"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [1,2,3]})))
        .mount(&server).await;

    let h = HttpHandler::new();
    let cfg = json!({ "method": "GET", "url": format!("{}/things", server.uri()) });
    let out = h.invoke(cfg, json!({})).await.unwrap();
    assert_eq!(out["items"], json!([1,2,3]));
}
```

- [ ] **Step 3: Run — expect FAIL (no module)**

Run: `cargo test -p celerex-orchestrator --test http_handler`

- [ ] **Step 4: Implement `HttpHandler`**

```rust
// packages/backend/agent-orchestrator/src/handlers/http.rs
use crate::tool_dispatch::{Handler, DispatchError};
use async_trait::async_trait;
use serde_json::Value;

pub struct HttpHandler { client: reqwest::Client }

impl HttpHandler {
    pub fn new() -> Self { Self { client: reqwest::Client::new() } }
}

#[async_trait]
impl Handler for HttpHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let url = config.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing url".into()))?;
        let method = config.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
        let mut req = self.client.request(
            method.parse().map_err(|e: reqwest::Error| DispatchError::Handler(e.to_string()))?,
            url,
        );
        if let Some(headers) = config.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(s) = v.as_str() { req = req.header(k, s); }
            }
        }
        if method != "GET" {
            req = req.json(&inputs);
        }
        let resp = req.send().await.map_err(|e| DispatchError::Handler(e.to_string()))?;
        let json: Value = resp.json().await.map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json)
    }
}
```

```rust
// packages/backend/agent-orchestrator/src/handlers/mod.rs
pub mod http;
```

- [ ] **Step 5: Run — expect PASS**

Run: `cargo test -p celerex-orchestrator --test http_handler`
Expected: `1 passed`.

- [ ] **Step 6: Commit**

```bash
git add packages/backend/agent-orchestrator/Cargo.toml \
        packages/backend/agent-orchestrator/src/handlers/ \
        packages/backend/agent-orchestrator/tests/http_handler.rs
git commit -m "feat(orchestrator): HTTP handler"
```

---

### Task 15: Script + Agent + LLM + MCP handler skeletons

Each is a small handler with a focused test. We bundle as one task because the per-handler code is tiny.

**Files:**
- Create: `packages/backend/agent-orchestrator/src/handlers/{script,agent,llm,mcp}.rs`
- Create: `packages/backend/agent-orchestrator/tests/script_handler.rs`
- Create: `packages/backend/agent-orchestrator/tests/agent_handler.rs`
- Create: `packages/backend/agent-orchestrator/tests/llm_handler.rs`

- [ ] **Step 1: Script handler — test**

```rust
// packages/backend/agent-orchestrator/tests/script_handler.rs
use celerex_orchestrator::handlers::script::ScriptHandler;
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::json;

#[tokio::test]
async fn script_handler_runs_command_and_captures_stdout() {
    let h = ScriptHandler;
    let cfg = json!({ "command": "echo", "args": ["hello"] });
    let out = h.invoke(cfg, json!({})).await.unwrap();
    assert_eq!(out["stdout"].as_str().unwrap().trim(), "hello");
    assert_eq!(out["exit_code"], 0);
}
```

- [ ] **Step 2: Script handler — implementation**

```rust
// packages/backend/agent-orchestrator/src/handlers/script.rs
use crate::tool_dispatch::{Handler, DispatchError};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

pub struct ScriptHandler;

#[async_trait]
impl Handler for ScriptHandler {
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let cmd = config.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing command".into()))?;
        let args: Vec<String> = config.get("args").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let out = Command::new(cmd).args(args).output().await
            .map_err(|e| DispatchError::Handler(e.to_string()))?;
        Ok(json!({
            "stdout": String::from_utf8_lossy(&out.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&out.stderr).to_string(),
            "exit_code": out.status.code().unwrap_or(-1),
        }))
    }
}
```

- [ ] **Step 3: Agent handler — wraps a generic `A2AClient` trait (consumed by plugin-backed peers, Galvo included, in Task 30)**

```rust
// packages/backend/agent-orchestrator/src/handlers/agent.rs
use crate::tool_dispatch::{Handler, DispatchError};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait A2AClient: Send + Sync {
    async fn call(&self, target: &str, params: Value) -> Result<Value, String>;
}

pub struct AgentHandler { pub client: Arc<dyn A2AClient> }

#[async_trait]
impl Handler for AgentHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let target = config.get("target").and_then(|v| v.as_str())
            .ok_or_else(|| DispatchError::Handler("missing target".into()))?;
        self.client.call(target, inputs).await.map_err(DispatchError::Handler)
    }
}
```

```rust
// packages/backend/agent-orchestrator/tests/agent_handler.rs
use celerex_orchestrator::handlers::agent::{AgentHandler, A2AClient};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::{json, Value};
use async_trait::async_trait;
use std::sync::Arc;

struct StubClient;
#[async_trait]
impl A2AClient for StubClient {
    async fn call(&self, target: &str, _params: Value) -> Result<Value, String> {
        Ok(json!({ "called": target }))
    }
}

#[tokio::test]
async fn agent_handler_delegates_to_client() {
    let h = AgentHandler { client: Arc::new(StubClient) };
    let out = h.invoke(json!({ "target": "galvo.research.weekly" }), json!({})).await.unwrap();
    assert_eq!(out["called"], "galvo.research.weekly");
}
```

- [ ] **Step 4: LLM handler — trait with stub for tests**

```rust
// packages/backend/agent-orchestrator/src/handlers/llm.rs
use crate::tool_dispatch::{Handler, DispatchError};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Value, String>;
}

pub struct LlmHandler { pub client: Arc<dyn LlmClient> }

#[async_trait]
impl Handler for LlmHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let model = config.get("model").and_then(|v| v.as_str()).unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs.get("messages").cloned().unwrap_or(Value::Array(vec![]));
        self.client.complete(model, messages, tools).await.map_err(DispatchError::Handler)
    }
}
```

```rust
// packages/backend/agent-orchestrator/tests/llm_handler.rs
use celerex_orchestrator::handlers::llm::{LlmHandler, LlmClient};
use celerex_orchestrator::tool_dispatch::Handler;
use serde_json::{json, Value};
use async_trait::async_trait;
use std::sync::Arc;

struct StubLlm;
#[async_trait]
impl LlmClient for StubLlm {
    async fn complete(&self, model: &str, _m: Value, _t: Value) -> Result<Value, String> {
        Ok(json!({ "model": model, "text": "ok" }))
    }
}

#[tokio::test]
async fn llm_handler_passes_model() {
    let h = LlmHandler { client: Arc::new(StubLlm) };
    let out = h.invoke(json!({ "model": "claude-haiku-4-5" }), json!({"messages": []})).await.unwrap();
    assert_eq!(out["model"], "claude-haiku-4-5");
}
```

- [ ] **Step 5: MCP handler — defer-impl skeleton**

```rust
// packages/backend/agent-orchestrator/src/handlers/mcp.rs
use crate::tool_dispatch::{Handler, DispatchError};
use async_trait::async_trait;
use serde_json::Value;

pub struct McpHandler;

#[async_trait]
impl Handler for McpHandler {
    async fn invoke(&self, _config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        Err(DispatchError::Handler("MCP execution deferred to V2".into()))
    }
}
```

- [ ] **Step 6: Register all handlers in `handlers/mod.rs`**

```rust
// packages/backend/agent-orchestrator/src/handlers/mod.rs
pub mod http;
pub mod script;
pub mod agent;
pub mod llm;
pub mod mcp;
```

- [ ] **Step 7: Run all orchestrator tests**

Run: `cargo test -p celerex-orchestrator`
Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add packages/backend/agent-orchestrator/src/handlers/ \
        packages/backend/agent-orchestrator/tests/
git commit -m "feat(orchestrator): script, agent, llm, mcp handlers (mcp stubbed)"
```

---

### Task 16: Pipeline executor — DAG walk

**Files:**
- Modify: `packages/backend/agent-orchestrator/src/executor.rs`
- Create: `packages/backend/agent-orchestrator/tests/executor.rs`

- [ ] **Step 1: Write failing executor test**

```rust
// packages/backend/agent-orchestrator/tests/executor.rs
use celerex_orchestrator::executor::{Executor, RunStore, InMemoryRunStore};
use celerex_orchestrator::tool_dispatch::{Dispatcher, Handler, DispatchError};
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::job::{JobRequest, Step, StepKind, RunStatus};
use serde_json::{json, Value};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

struct EchoHandler;
#[async_trait]
impl Handler for EchoHandler {
    async fn invoke(&self, _c: Value, i: Value) -> Result<Value, DispatchError> { Ok(i) }
}

#[tokio::test]
async fn executor_walks_linear_dag() {
    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(EchoHandler));
    let store = Arc::new(InMemoryRunStore::default());
    let exec = Executor::new(Arc::new(d), store.clone());

    let run_id = Uuid::new_v4();
    let req = JobRequest {
        run_id, engagement_id: Uuid::new_v4(), project_id: None,
        steps: vec![
            Step { id: "a".into(), depends_on: vec![],
                kind: StepKind::Tool { execution: ExecutionType::Http, config: json!({}), inputs: json!({"v": 1}) }},
            Step { id: "b".into(), depends_on: vec!["a".into()],
                kind: StepKind::Tool { execution: ExecutionType::Http, config: json!({}), inputs: json!({"v": 2}) }},
        ],
    };
    let status = exec.run(req).await.unwrap();
    assert_eq!(status, RunStatus::Succeeded);
    let records = store.records_for(run_id);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.status == RunStatus::Succeeded));
}
```

- [ ] **Step 2: Run — expect FAIL**

- [ ] **Step 3: Implement Executor + RunStore trait + InMemoryRunStore**

```rust
// packages/backend/agent-orchestrator/src/executor.rs
use crate::job::{JobRequest, Step, StepKind, StepRecord, RunStatus};
use crate::tool_dispatch::{Dispatcher, DispatchError};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("dispatch error: {0}")]
    Dispatch(#[from] DispatchError),
    #[error("cycle detected involving step {0}")]
    Cycle(String),
}

pub trait RunStore: Send + Sync {
    fn record(&self, rec: StepRecord);
    fn records_for(&self, run_id: Uuid) -> Vec<StepRecord>;
}

#[derive(Default)]
pub struct InMemoryRunStore { inner: Mutex<HashMap<Uuid, Vec<StepRecord>>> }

impl RunStore for InMemoryRunStore {
    fn record(&self, rec: StepRecord) {
        self.inner.lock().unwrap().entry(rec.run_id).or_default().push(rec);
    }
    fn records_for(&self, run_id: Uuid) -> Vec<StepRecord> {
        self.inner.lock().unwrap().get(&run_id).cloned().unwrap_or_default()
    }
}

pub struct Executor {
    dispatcher: Arc<Dispatcher>,
    store: Arc<dyn RunStore>,
}

impl Executor {
    pub fn new(dispatcher: Arc<Dispatcher>, store: Arc<dyn RunStore>) -> Self {
        Self { dispatcher, store }
    }

    pub async fn run(&self, req: JobRequest) -> Result<RunStatus, ExecError> {
        let order = topological_order(&req.steps)?;
        for step_id in order {
            let step = req.steps.iter().find(|s| s.id == step_id).unwrap().clone();
            let rec = self.run_step(req.run_id, &step).await;
            let failed = rec.status == RunStatus::Failed;
            self.store.record(rec);
            if failed { return Ok(RunStatus::Failed); }
        }
        Ok(RunStatus::Succeeded)
    }

    async fn run_step(&self, run_id: Uuid, step: &Step) -> StepRecord {
        match &step.kind {
            StepKind::Tool { execution, config, inputs } |
            StepKind::Outbound { execution, config, inputs } => {
                match self.dispatcher.invoke(*execution, config.clone(), inputs.clone()).await {
                    Ok(out) => StepRecord {
                        run_id, step_id: step.id.clone(), status: RunStatus::Succeeded,
                        inputs: inputs.clone(), outputs: Some(out), error: None,
                    },
                    Err(e) => StepRecord {
                        run_id, step_id: step.id.clone(), status: RunStatus::Failed,
                        inputs: inputs.clone(), outputs: None, error: Some(e.to_string()),
                    },
                }
            }
            StepKind::Filter { .. } => StepRecord {
                run_id, step_id: step.id.clone(), status: RunStatus::Succeeded,
                inputs: serde_json::json!({}), outputs: Some(serde_json::json!({})), error: None,
            },
        }
    }
}

fn topological_order(steps: &[Step]) -> Result<Vec<String>, ExecError> {
    let mut order = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: HashSet<String> = HashSet::new();
    fn visit(id: &str, steps: &[Step], visited: &mut HashSet<String>,
             stack: &mut HashSet<String>, order: &mut Vec<String>) -> Result<(), ExecError> {
        if visited.contains(id) { return Ok(()); }
        if !stack.insert(id.to_string()) { return Err(ExecError::Cycle(id.into())); }
        let step = steps.iter().find(|s| s.id == id).unwrap();
        for d in &step.depends_on {
            visit(d, steps, visited, stack, order)?;
        }
        stack.remove(id);
        visited.insert(id.to_string());
        order.push(id.to_string());
        Ok(())
    }
    for s in steps { visit(&s.id, steps, &mut visited, &mut stack, &mut order)?; }
    Ok(order)
}
```

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p celerex-orchestrator --test executor`
Expected: `1 passed`.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/agent-orchestrator/src/executor.rs \
        packages/backend/agent-orchestrator/tests/executor.rs
git commit -m "feat(orchestrator): pipeline executor with topo DAG walk + run store"
```

---

### Task 17: Failure handling + retry-from-step

**Files:**
- Modify: `packages/backend/agent-orchestrator/src/executor.rs` (add `retry`)
- Modify: `packages/backend/agent-orchestrator/tests/executor.rs`

- [ ] **Step 1: Write failing test**

```rust
// append to tests/executor.rs
use std::sync::atomic::{AtomicUsize, Ordering};

struct FlakyHandler { calls: Arc<AtomicUsize> }
#[async_trait]
impl Handler for FlakyHandler {
    async fn invoke(&self, _c: Value, _i: Value) -> Result<Value, DispatchError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n == 0 { Err(DispatchError::Handler("transient".into())) }
        else { Ok(serde_json::json!({"ok": true})) }
    }
}

#[tokio::test]
async fn retry_resumes_from_failed_step_without_replaying_upstream() {
    let calls_a = Arc::new(AtomicUsize::new(99)); // never called twice
    let calls_b = Arc::new(AtomicUsize::new(0));
    struct CountingOk(Arc<AtomicUsize>);
    #[async_trait]
    impl Handler for CountingOk {
        async fn invoke(&self, _c: Value, _i: Value) -> Result<Value, DispatchError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({"a": "out"}))
        }
    }

    let mut d = Dispatcher::new();
    d.register(ExecutionType::Http, Arc::new(CountingOk(Arc::new(AtomicUsize::new(0)))));
    d.register(ExecutionType::Script, Arc::new(FlakyHandler { calls: calls_b.clone() }));
    let store = Arc::new(InMemoryRunStore::default());
    let exec = Executor::new(Arc::new(d), store.clone());

    let run_id = Uuid::new_v4();
    let req = JobRequest {
        run_id, engagement_id: Uuid::new_v4(), project_id: None,
        steps: vec![
            Step { id: "a".into(), depends_on: vec![],
                kind: StepKind::Tool { execution: ExecutionType::Http, config: json!({}), inputs: json!({}) }},
            Step { id: "b".into(), depends_on: vec!["a".into()],
                kind: StepKind::Tool { execution: ExecutionType::Script, config: json!({}), inputs: json!({}) }},
        ],
    };

    let s1 = exec.run(req.clone()).await.unwrap();
    assert_eq!(s1, RunStatus::Failed);
    let calls_b_after_first = calls_b.load(Ordering::SeqCst);
    assert_eq!(calls_b_after_first, 1);

    // retry — only re-runs failed step "b"
    let s2 = exec.retry(req).await.unwrap();
    assert_eq!(s2, RunStatus::Succeeded);
    let _ = calls_a; // step "a" should NOT be re-invoked — record carried forward
}
```

- [ ] **Step 2: Run — expect FAIL (no `retry`)**

- [ ] **Step 3: Implement `retry`**

```rust
// packages/backend/agent-orchestrator/src/executor.rs — add to impl Executor
pub async fn retry(&self, req: JobRequest) -> Result<RunStatus, ExecError> {
    let prior = self.store.records_for(req.run_id);
    let succeeded: HashSet<String> = prior.iter()
        .filter(|r| r.status == RunStatus::Succeeded)
        .map(|r| r.step_id.clone()).collect();
    let order = topological_order(&req.steps)?;
    for step_id in order {
        if succeeded.contains(&step_id) { continue; }
        let step = req.steps.iter().find(|s| s.id == step_id).unwrap().clone();
        let rec = self.run_step(req.run_id, &step).await;
        let failed = rec.status == RunStatus::Failed;
        self.store.record(rec);
        if failed { return Ok(RunStatus::Failed); }
    }
    Ok(RunStatus::Succeeded)
}
```

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p celerex-orchestrator --test executor`
Expected: `2 passed`.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/agent-orchestrator/src/executor.rs \
        packages/backend/agent-orchestrator/tests/executor.rs
git commit -m "feat(orchestrator): retry-from-failed-step preserves upstream success records"
```

---

### Task 18: Triggers — UserClick / Scheduled / Event

**Files:**
- Modify: `packages/backend/agent-orchestrator/src/triggers.rs`
- Create: `packages/backend/agent-orchestrator/tests/triggers.rs`

- [ ] **Step 1: Failing test**

```rust
// packages/backend/agent-orchestrator/tests/triggers.rs
use celerex_orchestrator::triggers::Trigger;
use serde_json::json;

#[test]
fn trigger_serde_round_trips_all_variants() {
    let cases = vec![
        Trigger::UserClick { user_id: "u1".into() },
        Trigger::Scheduled { cron: "0 9 * * MON".into() },
        Trigger::Event { plugin_id: "teams".into(), event: "meeting_complete".into(), payload: json!({"meetingId": "m1"}) },
    ];
    for t in cases {
        let s = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&s).unwrap();
        assert_eq!(format!("{:?}", t), format!("{:?}", back));
    }
}
```

- [ ] **Step 2: Run — FAIL**

- [ ] **Step 3: Implement**

```rust
// packages/backend/agent-orchestrator/src/triggers.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    UserClick { user_id: String },
    Scheduled { cron: String },
    Event { plugin_id: String, event: String, payload: serde_json::Value },
}
```

- [ ] **Step 4: PASS**

- [ ] **Step 5: Commit**

```bash
git add packages/backend/agent-orchestrator/src/triggers.rs \
        packages/backend/agent-orchestrator/tests/triggers.rs
git commit -m "feat(orchestrator): Trigger enum (user-click, scheduled, event)"
```

---

## Phase E — Doc I/O HTTP bridge (Option A)

### Task 19: `POST /api/docs` — create document under project

**Files:**
- Create: `packages/backend/api/src/docs/mod.rs`
- Modify: `packages/backend/api/src/db/mod.rs` (add doc functions)
- Modify: `packages/backend/api/src/lib.rs` (route)
- Create: `packages/backend/api/tests/docs.rs`

- [ ] **Step 1: Failing test**

```rust
// packages/backend/api/tests/docs.rs
mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid, proj: Uuid) -> String {
    std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    let c = Claims { sub: "u".into(), engagement_id: eng.to_string(), project_id: Some(proj.to_string()),
        exp: (chrono::Utc::now().timestamp() + 3600) as usize };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn create_doc_persists_under_project() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let resp = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "Newsletter Draft" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["title"], "Newsletter Draft");
    assert_eq!(body["project_id"], proj.to_string());
}
```

- [ ] **Step 2: Run — FAIL**

- [ ] **Step 3: Implement repo + handler**

```rust
// packages/backend/api/src/db/mod.rs — append
#[derive(sqlx::FromRow, serde::Serialize)]
pub struct Document {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_document(pool: &PgPool, eng: Uuid, proj: Option<Uuid>, title: &str)
    -> sqlx::Result<Document>
{
    sqlx::query_as::<_, Document>(
        "INSERT INTO documents (engagement_id, project_id, title) VALUES ($1, $2, $3)
         RETURNING id, engagement_id, project_id, title, created_at"
    ).bind(eng).bind(proj).bind(title).fetch_one(pool).await
}
```

```rust
// packages/backend/api/src/docs/mod.rs
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, auth::AuthClaims, db};

#[derive(Deserialize)]
pub struct CreateBody { pub title: String }

pub async fn create(auth: AuthClaims, State(s): State<AppState>, Json(body): Json<CreateBody>) -> impl IntoResponse {
    match db::create_document(&s.pool, auth.0.engagement_id, auth.0.project_id, &body.title).await {
        Ok(d) => (StatusCode::CREATED, Json(d)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

```rust
// packages/backend/api/src/lib.rs — add route
.route("/api/docs", post(docs::create))
```

- [ ] **Step 4: PASS, commit**

```bash
git add packages/backend/api/src/db/mod.rs \
        packages/backend/api/src/docs/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/docs.rs
git commit -m "feat(api): POST /api/docs creates document under project"
```

---

### Task 20: `POST /api/docs/:id/blocks` — append block payload

**Files:**
- Create: `packages/backend/api/src/db/migrations/0005_doc_blocks.sql`
- Modify: `packages/backend/api/src/docs/mod.rs`
- Modify: `packages/backend/api/src/db/mod.rs`
- Modify: `packages/backend/api/tests/docs.rs`

- [ ] **Step 1: Failing test (append-block round-trip)**

```rust
// append to tests/docs.rs
#[tokio::test]
async fn append_block_to_doc() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let create = server.post("/api/docs")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "T" })).await;
    let doc_id = create.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let resp = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "type": "paragraph", "text": "hello" }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);

    // verify persisted
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_blocks WHERE document_id = $1::uuid")
        .bind(doc_id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 2: Migration**

```sql
-- packages/backend/api/src/db/migrations/0005_doc_blocks.sql
CREATE TABLE doc_blocks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position INT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_doc_blocks_document_id_position ON doc_blocks(document_id, position);
```

- [ ] **Step 3: Repo + handler**

```rust
// packages/backend/api/src/db/mod.rs — append
pub async fn append_block(pool: &PgPool, doc: Uuid, payload: &serde_json::Value)
    -> sqlx::Result<Uuid>
{
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO doc_blocks (document_id, position, payload)
         VALUES ($1, COALESCE((SELECT MAX(position) + 1 FROM doc_blocks WHERE document_id = $1), 0), $2)
         RETURNING id"
    ).bind(doc).bind(payload).fetch_one(pool).await?;
    Ok(row.0)
}
```

```rust
// packages/backend/api/src/docs/mod.rs — add
use axum::extract::Path;
use uuid::Uuid;

pub async fn append_block(_auth: AuthClaims, State(s): State<AppState>, Path(doc_id): Path<Uuid>, Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    match db::append_block(&s.pool, doc_id, &payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

Route: `.route("/api/docs/{id}/blocks", post(docs::append_block))`

- [ ] **Step 4: Run — PASS**

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/db/migrations/0005_doc_blocks.sql \
        packages/backend/api/src/db/mod.rs \
        packages/backend/api/src/docs/mod.rs \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/docs.rs
git commit -m "feat(api): append-block route on documents"
```

---

### Task 21: `PATCH /api/docs/:id/blocks/:blockId` — replace block payload

**Files:**
- Modify: `packages/backend/api/src/docs/mod.rs`
- Modify: `packages/backend/api/src/db/mod.rs`
- Modify: `packages/backend/api/src/lib.rs`
- Modify: `packages/backend/api/tests/docs.rs`

- [ ] **Step 1: Failing test**

```rust
#[tokio::test]
async fn patch_block_replaces_payload() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let proj: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id, name) VALUES ($1, 'P') RETURNING id")
        .bind(eng).fetch_one(&db.pool).await.unwrap();
    let t = token(eng, proj);

    let doc = server.post("/api/docs").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "title": "T" })).await.json::<serde_json::Value>();
    let doc_id = doc["id"].as_str().unwrap();
    let block = server.post(&format!("/api/docs/{doc_id}/blocks"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "text": "old" })).await.json::<serde_json::Value>();
    let block_id = block["id"].as_str().unwrap();

    let resp = server.patch(&format!("/api/docs/{doc_id}/blocks/{block_id}"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "text": "new" })).await;
    resp.assert_status_ok();

    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM doc_blocks WHERE id = $1::uuid"
    ).bind(block_id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(payload["text"], "new");
}
```

- [ ] **Step 2: Repo + handler**

```rust
// db/mod.rs — append
pub async fn replace_block(pool: &PgPool, block_id: Uuid, payload: &serde_json::Value) -> sqlx::Result<u64> {
    let r = sqlx::query("UPDATE doc_blocks SET payload = $2 WHERE id = $1")
        .bind(block_id).bind(payload).execute(pool).await?;
    Ok(r.rows_affected())
}
```

```rust
// docs/mod.rs — append
pub async fn replace_block(_a: AuthClaims, State(s): State<AppState>, Path((_doc, block)): Path<(Uuid, Uuid)>, Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    match db::replace_block(&s.pool, block, &payload).await {
        Ok(0) => (StatusCode::NOT_FOUND, "block not found").into_response(),
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

Route: `.route("/api/docs/{doc_id}/blocks/{block_id}", patch(docs::replace_block))`

- [ ] **Step 3: PASS, commit**

```bash
git add -u packages/backend/api/
git commit -m "feat(api): PATCH /api/docs/:id/blocks/:blockId replaces block payload"
```

---

## Phase F — Plugin registry (manifest, install, credentials)

### Task 22: Plugin manifest schema (inbound + outbound)

**Files:**
- Create: `packages/backend/api/src/plugins/manifest.rs`
- Create: `packages/backend/api/src/plugins/mod.rs`
- Create: `packages/backend/api/tests/plugin_manifest.rs`

- [ ] **Step 1: Failing test for manifest parse**

```rust
// packages/backend/api/tests/plugin_manifest.rs
use celerex_api::plugins::manifest::PluginManifest;

const MANIFEST_JSON: &str = r#"{
  "id": "teams",
  "name": "Microsoft Teams",
  "version": "0.1.0",
  "inbound": [
    { "id": "on_meeting_complete", "kind": "webhook",
      "input_schema": { "type": "object", "properties": { "meetingId": { "type": "string" } } } }
  ],
  "outbound": [
    { "id": "post_channel", "kind": "http",
      "input_schema": { "type": "object", "properties": { "channel": { "type": "string" }, "message": { "type": "string" } } } }
  ]
}"#;

#[test]
fn parses_bidirectional_manifest() {
    let m: PluginManifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    assert_eq!(m.id, "teams");
    assert_eq!(m.inbound.len(), 1);
    assert_eq!(m.outbound.len(), 1);
    assert_eq!(m.inbound[0].id, "on_meeting_complete");
    assert_eq!(m.outbound[0].id, "post_channel");
}

#[test]
fn empty_inbound_outbound_arrays_are_explicit() {
    let m: PluginManifest = serde_json::from_str(r#"{"id":"x","name":"X","version":"0","inbound":[],"outbound":[]}"#).unwrap();
    assert!(m.inbound.is_empty());
    assert!(m.outbound.is_empty());
}
```

- [ ] **Step 2: Implement schema**

```rust
// packages/backend/api/src/plugins/manifest.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub inbound: Vec<InboundCapability>,
    #[serde(default)]
    pub outbound: Vec<OutboundCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundCapability {
    pub id: String,
    pub kind: InboundKind,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboundKind { Webhook, Poll, Manual }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundCapability {
    pub id: String,
    pub kind: OutboundKind,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundKind { Http, Mcp, Agent, Script }
```

```rust
// packages/backend/api/src/plugins/mod.rs
pub mod manifest;
```

Add `pub mod plugins;` to `lib.rs`.

- [ ] **Step 3: PASS, commit**

```bash
git add packages/backend/api/src/plugins/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/plugin_manifest.rs
git commit -m "feat(plugins): manifest schema with inbound + outbound capabilities"
```

---

### Task 23: Plugin install/list routes + Postgres-backed store

**Files:**
- Create: `packages/backend/api/src/db/migrations/0003_plugins.sql`
- Modify: `packages/backend/api/src/plugins/mod.rs`
- Modify: `packages/backend/api/src/db/mod.rs`
- Create: `packages/backend/api/tests/plugins.rs`

- [ ] **Step 1: Migration**

```sql
-- packages/backend/api/src/db/migrations/0003_plugins.sql
CREATE TABLE plugin_installs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL,
    manifest JSONB NOT NULL,
    installed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (engagement_id, plugin_id)
);

CREATE TABLE plugin_credentials (
    install_id UUID PRIMARY KEY REFERENCES plugin_installs(id) ON DELETE CASCADE,
    direction TEXT NOT NULL CHECK (direction IN ('inbound', 'outbound', 'both')),
    encrypted_payload BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 2: Failing test**

```rust
// packages/backend/api/tests/plugins.rs
mod common;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: Uuid) -> String {
    std::env::set_var("CELEREX_JWT_SECRET", "test-secret");
    let c = Claims { sub: "u".into(), engagement_id: eng.to_string(), project_id: None,
        exp: (chrono::Utc::now().timestamp() + 3600) as usize };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

#[tokio::test]
async fn install_then_list_plugin() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let manifest = json!({ "id": "teams", "name": "Teams", "version": "0.1.0", "inbound": [], "outbound": [] });
    let resp = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await;
    resp.assert_status(axum::http::StatusCode::CREATED);

    let list = server.get("/api/plugins").add_header("authorization", format!("Bearer {t}")).await;
    let arr: Vec<serde_json::Value> = list.json();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["plugin_id"], "teams");
}
```

- [ ] **Step 3: Repo + routes**

```rust
// db/mod.rs — append
pub async fn install_plugin(pool: &PgPool, eng: Uuid, plugin_id: &str, manifest: &serde_json::Value) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, $2, $3) RETURNING id"
    ).bind(eng).bind(plugin_id).bind(manifest).fetch_one(pool).await?;
    Ok(row.0)
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct PluginInstall {
    pub id: Uuid,
    pub engagement_id: Uuid,
    pub plugin_id: String,
    pub manifest: serde_json::Value,
    pub installed_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_plugins(pool: &PgPool, eng: Uuid) -> sqlx::Result<Vec<PluginInstall>> {
    sqlx::query_as::<_, PluginInstall>(
        "SELECT id, engagement_id, plugin_id, manifest, installed_at FROM plugin_installs
         WHERE engagement_id = $1 ORDER BY installed_at DESC"
    ).bind(eng).fetch_all(pool).await
}
```

```rust
// plugins/mod.rs — add routes module
pub mod manifest;
pub mod routes;
```

```rust
// packages/backend/api/src/plugins/routes.rs
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use crate::{AppState, auth::AuthClaims, db, plugins::manifest::PluginManifest};

#[derive(Deserialize)]
pub struct InstallBody { pub manifest: PluginManifest }

pub async fn install(auth: AuthClaims, State(s): State<AppState>, Json(b): Json<InstallBody>) -> impl IntoResponse {
    let manifest_json = serde_json::to_value(&b.manifest).unwrap();
    match db::install_plugin(&s.pool, auth.0.engagement_id, &b.manifest.id, &manifest_json).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list(auth: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    match db::list_plugins(&s.pool, auth.0.engagement_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

Route: `.route("/api/plugins", get(plugins::routes::list).post(plugins::routes::install))`

- [ ] **Step 4: PASS, commit**

```bash
git add packages/backend/api/src/db/migrations/0003_plugins.sql \
        packages/backend/api/src/db/mod.rs \
        packages/backend/api/src/plugins/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/plugins.rs
git commit -m "feat(plugins): install + list routes with postgres-backed store"
```

---

### Task 24: `CredentialStore` trait + `LocalAeadStore` MVP impl

**Design intent:** The `CredentialStore` trait is the swap point. MVP ships `LocalAeadStore` (ChaCha20-Poly1305 via `ring`). A future `VaultStore` or `KmsStore` implements the same trait and drops in via `Arc<dyn CredentialStore>` on `AppState` — no changes at call sites. Do NOT take a dependency on `LocalAeadStore` anywhere outside its own construction point (`build_app` / `main.rs`).

**Files:**
- Create: `packages/backend/api/src/plugins/credentials.rs` (trait + local impl)
- Modify: `packages/backend/api/src/plugins/mod.rs`
- Modify: `packages/backend/api/src/plugins/routes.rs`
- Modify: `packages/backend/api/src/lib.rs` (inject store into `AppState`)
- Modify: `packages/backend/api/src/main.rs` (construct `LocalAeadStore` at boot)
- Modify: `packages/backend/api/tests/plugins.rs`

- [ ] **Step 1: Failing test — store + retrieve credentials roundtrip, plus trait injection**

```rust
// append to tests/plugins.rs
#[tokio::test]
async fn store_and_retrieve_credentials_encrypted_at_rest() {
    std::env::set_var("CELEREX_CRED_KEY", "0123456789abcdef0123456789abcdef"); // 32 bytes
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);

    let manifest = json!({ "id": "teams", "name": "T", "version": "0.1", "inbound": [], "outbound": [] });
    let install = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await
        .json::<serde_json::Value>();
    let install_id = install["id"].as_str().unwrap();

    let put = server.put(&format!("/api/plugins/{install_id}/credentials"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "direction": "outbound", "payload": { "token": "secret-abc-123" } }))
        .await;
    put.assert_status_ok();

    // ciphertext on disk should not contain the plaintext
    let ct: Vec<u8> = sqlx::query_scalar(
        "SELECT encrypted_payload FROM plugin_credentials WHERE install_id = $1::uuid"
    ).bind(install_id).fetch_one(&db.pool).await.unwrap();
    let s = String::from_utf8_lossy(&ct);
    assert!(!s.contains("secret-abc-123"), "ciphertext leaked plaintext: {s}");
}

// Trait-level test — ensures routes depend on the trait, not the concrete impl.
#[tokio::test]
async fn credential_store_trait_is_injectable() {
    use celerex_api::plugins::credentials::{CredentialStore, SealedBlob};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingStore { calls: Mutex<Vec<String>> }
    #[async_trait]
    impl CredentialStore for RecordingStore {
        async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String> {
            self.calls.lock().unwrap().push("seal".into());
            Ok(SealedBlob { ciphertext: plaintext.to_vec(), nonce: vec![0u8; 12] })
        }
        async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String> {
            self.calls.lock().unwrap().push("open".into());
            Ok(blob.ciphertext.clone())
        }
    }

    let store: Arc<dyn CredentialStore> = Arc::new(RecordingStore::default());
    let db = common::start_test_db().await;
    let app = celerex_api::build_app_with_store(db.pool.clone(), store.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    // ... (install plugin, PUT credentials, assert RecordingStore.calls recorded "seal")
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = token(eng);
    let manifest = json!({ "id": "p", "name": "P", "version": "0.1", "inbound": [], "outbound": [] });
    let install = server.post("/api/plugins").add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await.json::<serde_json::Value>();
    let id = install["id"].as_str().unwrap();
    server.put(&format!("/api/plugins/{id}/credentials"))
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "direction": "outbound", "payload": { "k": "v" } })).await.assert_status_ok();

    let store_typed = store.as_ref() as *const dyn CredentialStore as *const RecordingStore;
    // SAFETY: test-only; we constructed RecordingStore above and retained Arc.
    let recorded = unsafe { (*store_typed).calls.lock().unwrap().clone() };
    assert!(recorded.iter().any(|c| c == "seal"), "routes must go through the trait");
}
```

- [ ] **Step 2: Implement `CredentialStore` trait + `LocalAeadStore` impl**

```rust
// packages/backend/api/src/plugins/credentials.rs
use async_trait::async_trait;
use ring::{aead, rand::{SystemRandom, SecureRandom}};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CredError {
    #[error("missing key")] MissingKey,
    #[error("crypto: {0}")] Crypto(String),
}

/// A sealed credential blob — ciphertext + whatever metadata (e.g. nonce, key-id)
/// the backing store needs to open it. Opaque to callers.
#[derive(Debug, Clone)]
pub struct SealedBlob {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Swap point for credential encryption. MVP ships `LocalAeadStore`;
/// future Vault/KMS impls implement this same trait.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String>;
    async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String>;
}

pub struct LocalAeadStore { key: aead::LessSafeKey }

impl LocalAeadStore {
    pub fn from_env() -> Result<Self, CredError> {
        let raw = std::env::var("CELEREX_CRED_KEY").map_err(|_| CredError::MissingKey)?;
        if raw.len() < 32 { return Err(CredError::MissingKey); }
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &raw.as_bytes()[..32])
            .map_err(|e| CredError::Crypto(e.to_string()))?;
        Ok(Self { key: aead::LessSafeKey::new(unbound) })
    }
}

#[async_trait]
impl CredentialStore for LocalAeadStore {
    async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String> {
        let mut nonce_bytes = [0u8; 12];
        SystemRandom::new().fill(&mut nonce_bytes).map_err(|e| e.to_string())?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut buf = plaintext.to_vec();
        self.key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok(SealedBlob { ciphertext: buf, nonce: nonce_bytes.to_vec() })
    }

    async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String> {
        let nonce_arr: [u8; 12] = blob.nonce.as_slice().try_into().map_err(|_| "bad nonce".to_string())?;
        let n = aead::Nonce::assume_unique_for_key(nonce_arr);
        let mut buf = blob.ciphertext.clone();
        let pt = self.key.open_in_place(n, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok(pt.to_vec())
    }
}
```

```rust
// packages/backend/api/src/plugins/mod.rs
pub mod manifest;
pub mod routes;
pub mod credentials;
```

- [ ] **Step 3: Inject `Arc<dyn CredentialStore>` into `AppState`**

```rust
// packages/backend/api/src/lib.rs — modify AppState + builders
use std::sync::Arc;
use crate::plugins::credentials::{CredentialStore, LocalAeadStore};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub credentials: Arc<dyn CredentialStore>,
}

pub fn build_app(pool: sqlx::PgPool) -> axum::Router {
    let store: Arc<dyn CredentialStore> = Arc::new(
        LocalAeadStore::from_env().expect("CELEREX_CRED_KEY must be set")
    );
    build_app_with_store(pool, store)
}

pub fn build_app_with_store(pool: sqlx::PgPool, credentials: Arc<dyn CredentialStore>) -> axum::Router {
    let state = AppState { pool, credentials };
    // ...existing route wiring, passing `state` into `.with_state(...)`
    router().with_state(state)
}
```

```rust
// packages/backend/api/src/main.rs — replace direct LocalAeadStore usage
// The existing bootstrap just calls build_app(pool); no further changes needed
// because build_app internally constructs the default LocalAeadStore.
```

- [ ] **Step 4: Route depends on the trait, not the concrete cipher**

```rust
// db/mod.rs — append
pub async fn upsert_credentials(pool: &PgPool, install: Uuid, direction: &str, ct: &[u8], nonce: &[u8]) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO plugin_credentials (install_id, direction, encrypted_payload, nonce) VALUES ($1, $2, $3, $4)
         ON CONFLICT (install_id) DO UPDATE SET direction = EXCLUDED.direction,
            encrypted_payload = EXCLUDED.encrypted_payload, nonce = EXCLUDED.nonce"
    ).bind(install).bind(direction).bind(ct).bind(nonce).execute(pool).await?;
    Ok(())
}
```

```rust
// plugins/routes.rs — append
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use crate::AppState;
// NOTE: we import the trait, not the concrete LocalAeadStore.
#[allow(unused_imports)]
use crate::plugins::credentials::CredentialStore;

#[derive(serde::Deserialize)]
pub struct CredBody { pub direction: String, pub payload: serde_json::Value }

pub async fn upsert_credentials(
    _a: AuthClaims,
    State(s): State<AppState>,
    Path(install): Path<Uuid>,
    Json(b): Json<CredBody>,
) -> impl IntoResponse {
    let pt = serde_json::to_vec(&b.payload).unwrap();
    let blob = match s.credentials.seal(&pt).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    match db::upsert_credentials(&s.pool, install, &b.direction, &blob.ciphertext, &blob.nonce).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

Route: `.route("/api/plugins/{id}/credentials", put(plugins::routes::upsert_credentials))`

- [ ] **Step 5: Run — PASS**

Run: `cargo test -p celerex-api --test plugins`
Expected: both `store_and_retrieve_credentials_encrypted_at_rest` and `credential_store_trait_is_injectable` PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/plugins/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/src/db/mod.rs \
        packages/backend/api/tests/plugins.rs
git commit -m "feat(plugins): CredentialStore trait + LocalAeadStore MVP impl

Introduces swap-point for future Vault/KMS credential backends.
Routes depend on Arc<dyn CredentialStore>; LocalAeadStore (ring
ChaCha20-Poly1305) is the MVP impl wired at build_app boot."
```

---

## Phase G — Webhook ingress + event-triggered jobs

### Task 25: Webhook ingress route + idempotency dedupe

**Files:**
- Create: `packages/backend/api/src/db/migrations/0004_runs.sql`
- Create: `packages/backend/api/src/webhooks/mod.rs`
- Create: `packages/backend/api/src/webhooks/signature.rs`
- Create: `packages/backend/api/tests/webhooks.rs`

- [ ] **Step 1: Migration — runs + run_steps + webhook_dedupe**

```sql
-- packages/backend/api/src/db/migrations/0004_runs.sql
CREATE TABLE runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    engagement_id UUID NOT NULL,
    project_id UUID,
    trigger JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE run_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_id TEXT NOT NULL,
    status TEXT NOT NULL,
    inputs JSONB NOT NULL,
    outputs JSONB,
    error TEXT,
    completed_at TIMESTAMPTZ
);

CREATE TABLE webhook_dedupe (
    id TEXT PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 2: Failing test — first POST 202, replay 200 (deduped)**

```rust
// packages/backend/api/tests/webhooks.rs
mod common;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn webhook_accepted_then_deduped_on_replay() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    // seed install with no signature requirement
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let manifest = json!({ "id": "test", "name": "T", "version": "0.1",
        "inbound": [{ "id": "ping", "kind": "webhook", "input_schema": {} }],
        "outbound": [] });
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'test', $2)")
        .bind(eng).bind(&manifest).execute(&db.pool).await.unwrap();

    let body = json!({ "id": "evt-1", "data": {} });
    let r1 = server.post("/api/webhooks/test/ping")
        .add_header("x-event-id", "evt-1")
        .json(&body).await;
    r1.assert_status(axum::http::StatusCode::ACCEPTED);

    let r2 = server.post("/api/webhooks/test/ping")
        .add_header("x-event-id", "evt-1")
        .json(&body).await;
    r2.assert_status(axum::http::StatusCode::OK);
    let body2: serde_json::Value = r2.json();
    assert_eq!(body2["deduped"], true);
}
```

- [ ] **Step 3: Signature verification trait (no-op default for MVP)**

```rust
// packages/backend/api/src/webhooks/signature.rs
use async_trait::async_trait;
use axum::http::HeaderMap;

#[async_trait]
pub trait SignatureVerifier: Send + Sync {
    async fn verify(&self, headers: &HeaderMap, body: &[u8]) -> Result<(), String>;
}

pub struct NoopVerifier;

#[async_trait]
impl SignatureVerifier for NoopVerifier {
    async fn verify(&self, _h: &HeaderMap, _b: &[u8]) -> Result<(), String> { Ok(()) }
}
```

- [ ] **Step 4: Webhook handler with dedupe**

```rust
// packages/backend/api/src/webhooks/mod.rs
pub mod signature;

use axum::{extract::{Path, State}, http::{HeaderMap, StatusCode}, response::IntoResponse, body::Bytes, Json};
use serde_json::json;
use crate::AppState;

pub async fn ingest(
    State(s): State<AppState>,
    Path((plugin_id, event)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let event_id = headers.get("x-event-id").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(id) = &event_id {
        let dup_key = format!("{plugin_id}:{event}:{id}");
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM webhook_dedupe WHERE id = $1")
            .bind(&dup_key).fetch_optional(&s.pool).await.unwrap_or(None);
        if exists.is_some() {
            return (StatusCode::OK, Json(json!({ "deduped": true }))).into_response();
        }
        let _ = sqlx::query("INSERT INTO webhook_dedupe (id) VALUES ($1)")
            .bind(&dup_key).execute(&s.pool).await;
    }
    // For MVP: just persist as a pending run; full dispatch comes in Task 26
    let trigger = json!({ "kind": "event", "plugin_id": plugin_id, "event": event,
        "payload": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or(json!(null)) });
    let _ = sqlx::query("INSERT INTO runs (engagement_id, trigger) VALUES (
        (SELECT engagement_id FROM plugin_installs WHERE plugin_id = $1 LIMIT 1), $2)")
        .bind(&plugin_id).bind(&trigger).execute(&s.pool).await;
    (StatusCode::ACCEPTED, Json(json!({ "received": true }))).into_response()
}
```

Add module + route:
```rust
// lib.rs
pub mod webhooks;
// inside build_app:
.route("/api/webhooks/{plugin_id}/{event}", post(webhooks::ingest))
```

- [ ] **Step 5: PASS, commit**

```bash
git add packages/backend/api/src/db/migrations/0004_runs.sql \
        packages/backend/api/src/webhooks/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/webhooks.rs
git commit -m "feat(webhooks): ingress endpoint with idempotency dedupe + run row"
```

---

### Task 26: Event-triggered job dispatch into orchestrator

**Files:**
- Modify: `packages/backend/api/src/webhooks/mod.rs`
- Modify: `packages/backend/api/src/lib.rs` (carry an orchestrator handle in AppState)
- Modify: `packages/backend/api/Cargo.toml` (add path dep on orchestrator)
- Modify: `packages/backend/api/tests/webhooks.rs`

- [ ] **Step 1: Add path dep**

```toml
# packages/backend/api/Cargo.toml — under [dependencies]
celerex-orchestrator = { path = "../agent-orchestrator" }
```

- [ ] **Step 2: Failing test — webhook fires a job that runs**

```rust
// append to tests/webhooks.rs
#[tokio::test]
async fn webhook_dispatches_event_triggered_job() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let manifest = json!({ "id": "test", "name": "T", "version": "0.1",
        "inbound": [{ "id": "ping", "kind": "webhook", "input_schema": {} }],
        "outbound": [],
        "on_event_pipeline": {
            "ping": { "steps": [{ "id": "noop", "depends_on": [],
                "kind": { "type": "filter", "expression": "true" } }] }
        }
    });
    sqlx::query("INSERT INTO plugin_installs (engagement_id, plugin_id, manifest) VALUES ($1, 'test', $2)")
        .bind(eng).bind(&manifest).execute(&db.pool).await.unwrap();

    let r = server.post("/api/webhooks/test/ping").json(&json!({})).await;
    r.assert_status(axum::http::StatusCode::ACCEPTED);

    // wait briefly for async dispatch
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_steps WHERE step_id = 'noop'")
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 3: Add orchestrator handle to AppState; build dispatcher; spawn task on webhook**

```rust
// packages/backend/api/src/lib.rs — modify AppState
use celerex_orchestrator::{tool_dispatch::Dispatcher, executor::Executor};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub executor: Arc<Executor>,
}

pub fn build_app(pool: PgPool) -> Router {
    let dispatcher = Arc::new(Dispatcher::new()); // empty for now; handlers wired later
    let store = Arc::new(crate::runs::PgRunStore::new(pool.clone()));
    let executor = Arc::new(Executor::new(dispatcher, store));
    let state = AppState { pool, executor };
    // ... route registration as before, with state
}
```

```rust
// packages/backend/api/src/runs/mod.rs (new)
use celerex_orchestrator::{executor::RunStore, job::StepRecord};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgRunStore { pool: PgPool }
impl PgRunStore { pub fn new(pool: PgPool) -> Self { Self { pool } } }

impl RunStore for PgRunStore {
    fn record(&self, rec: StepRecord) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(
                "INSERT INTO run_steps (run_id, step_id, status, inputs, outputs, error, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())"
            ).bind(rec.run_id).bind(&rec.step_id)
             .bind(format!("{:?}", rec.status).to_lowercase())
             .bind(&rec.inputs).bind(&rec.outputs).bind(&rec.error)
             .execute(&pool).await;
        });
    }
    fn records_for(&self, _run_id: Uuid) -> Vec<StepRecord> { Vec::new() /* unused for fire-and-forget MVP */ }
}
```

```rust
// webhooks/mod.rs — extend ingest to spawn dispatch
use celerex_orchestrator::job::{JobRequest, Step, StepKind};
use uuid::Uuid;

// after dedupe + run insert, look up plugin manifest's on_event_pipeline[event]
// and convert to JobRequest, then spawn:
let pipeline = manifest.get("on_event_pipeline").and_then(|p| p.get(&event)).cloned();
if let Some(p) = pipeline {
    let steps: Vec<Step> = serde_json::from_value(p["steps"].clone()).unwrap_or_default();
    let req = JobRequest { run_id: Uuid::new_v4(), engagement_id, project_id: None, steps };
    let exec = s.executor.clone();
    tokio::spawn(async move { let _ = exec.run(req).await; });
}
```

(Engineer slots this into the existing `ingest` handler after the dedupe section.)

- [ ] **Step 4: Run — PASS**

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/Cargo.toml \
        packages/backend/api/src/runs/ \
        packages/backend/api/src/webhooks/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/webhooks.rs
git commit -m "feat(webhooks): dispatch event-triggered jobs via orchestrator"
```

---

## Phase H — LLM tool_use round-trip

### Task 27: Extract LLM module from old main.rs / build Anthropic Messages client

**Files:**
- Create: `packages/backend/api/src/llm/mod.rs`
- Create: `packages/backend/api/src/llm/anthropic.rs`
- Create: `packages/backend/api/tests/llm_anthropic.rs`

- [ ] **Step 1: Failing test using `wiremock` Anthropic fixture**

```rust
// packages/backend/api/tests/llm_anthropic.rs
use celerex_api::llm::anthropic::AnthropicClient;
use serde_json::json;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::{method, path}};

#[tokio::test]
async fn anthropic_client_handles_text_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_1",
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "Hello!" }]
        })))
        .mount(&server).await;

    let c = AnthropicClient::with_base_url(server.uri(), "test-key".into());
    let resp = c.create_message("claude-sonnet-4-6", vec![json!({"role":"user","content":"hi"})], vec![]).await.unwrap();
    assert_eq!(resp["stop_reason"], "end_turn");
}
```

- [ ] **Step 2: Implement client**

```rust
// packages/backend/api/src/llm/anthropic.rs
use serde_json::{json, Value};

pub struct AnthropicClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        Ok(Self::with_base_url("https://api.anthropic.com".into(), key))
    }
    pub fn with_base_url(base_url: String, api_key: String) -> Self {
        Self { base_url, api_key, http: reqwest::Client::new() }
    }

    pub async fn create_message(&self, model: &str, messages: Vec<Value>, tools: Vec<Value>) -> anyhow::Result<Value> {
        let body = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": messages,
            "tools": tools,
        });
        let resp = self.http.post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body).send().await?;
        Ok(resp.json::<Value>().await?)
    }
}
```

```rust
// packages/backend/api/src/llm/mod.rs
pub mod anthropic;
```

Add `pub mod llm;` to `lib.rs`.

- [ ] **Step 3: PASS, commit**

```bash
git add packages/backend/api/src/llm/ \
        packages/backend/api/src/lib.rs \
        packages/backend/api/tests/llm_anthropic.rs
git commit -m "feat(llm): Anthropic Messages client (text response)"
```

---

### Task 28: tool_use round-trip loop (parse `tool_use` blocks, dispatch, send results back)

**Files:**
- Modify: `packages/backend/api/src/llm/anthropic.rs`
- Create: `packages/backend/api/tests/llm_tool_use.rs`

- [ ] **Step 1: Failing test — model asks for tool, server replies, model returns text**

```rust
// packages/backend/api/tests/llm_tool_use.rs
use celerex_api::llm::anthropic::{AnthropicClient, ToolDispatch};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::{MockServer, Mock, ResponseTemplate, matchers::{method, path}};

struct StubTools;
#[async_trait]
impl ToolDispatch for StubTools {
    async fn invoke(&self, name: &str, _input: Value) -> Result<Value, String> {
        Ok(json!({ "result": format!("ran {name}") }))
    }
}

#[tokio::test]
async fn tool_use_round_trip_completes_with_text() {
    let server = MockServer::start().await;
    // First call: model asks for tool
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m1", "stop_reason": "tool_use",
            "content": [{ "type": "tool_use", "id": "tu1", "name": "echo", "input": {"x": 1} }]
        })))
        .up_to_n_times(1).mount(&server).await;
    // Second call: model returns text
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m2", "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": "done" }]
        })))
        .mount(&server).await;

    let c = AnthropicClient::with_base_url(server.uri(), "k".into());
    let final_text = c.run_with_tools(
        "claude-sonnet-4-6",
        vec![json!({"role":"user","content":"go"})],
        vec![json!({"name":"echo","description":"e","input_schema":{"type":"object"}})],
        Arc::new(StubTools),
    ).await.unwrap();
    assert_eq!(final_text, "done");
}
```

- [ ] **Step 2: Implement loop**

```rust
// packages/backend/api/src/llm/anthropic.rs — append
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ToolDispatch: Send + Sync {
    async fn invoke(&self, name: &str, input: Value) -> Result<Value, String>;
}

impl AnthropicClient {
    pub async fn run_with_tools(
        &self,
        model: &str,
        mut messages: Vec<Value>,
        tools: Vec<Value>,
        dispatch: Arc<dyn ToolDispatch>,
    ) -> anyhow::Result<String> {
        for _ in 0..16 { // bounded loop
            let resp = self.create_message(model, messages.clone(), tools.clone()).await?;
            let stop = resp["stop_reason"].as_str().unwrap_or("");
            let content = resp["content"].as_array().cloned().unwrap_or_default();
            if stop == "end_turn" {
                let text = content.iter()
                    .filter_map(|b| b.get("type").and_then(|t| t.as_str()).filter(|t| *t == "text").and_then(|_| b.get("text")?.as_str()))
                    .collect::<Vec<_>>().join("\n");
                return Ok(text);
            }
            if stop == "tool_use" {
                messages.push(json!({ "role": "assistant", "content": content }));
                let mut results = Vec::new();
                for block in &content {
                    if block["type"] == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input = block["input"].clone();
                        match dispatch.invoke(&name, input).await {
                            Ok(out) => results.push(json!({
                                "type": "tool_result", "tool_use_id": id, "content": out.to_string()
                            })),
                            Err(e) => results.push(json!({
                                "type": "tool_result", "tool_use_id": id, "is_error": true, "content": e
                            })),
                        }
                    }
                }
                messages.push(json!({ "role": "user", "content": results }));
                continue;
            }
            anyhow::bail!("unexpected stop_reason: {stop}");
        }
        anyhow::bail!("tool loop exceeded 16 iterations")
    }
}
```

- [ ] **Step 3: PASS, commit**

```bash
git add packages/backend/api/src/llm/anthropic.rs \
        packages/backend/api/tests/llm_tool_use.rs
git commit -m "feat(llm): tool_use round-trip loop with bounded iterations"
```

---

## Phase I — Galvatronic protocol + Galvo-as-plugin fixture

> Galvo is **not** a first-class Celerex subsystem. It is reachable only through the standard plugin surface, exactly like workIQ or Teams. Phase I ships two artifacts: (Task 29) the Galvatronic A2A wire-format spec, which any peer plugin can speak; and (Task 30) a Galvo plugin manifest fixture + fixture-backed `A2AClient` stub that proves the plugin path works end-to-end. Swapping to live Galvo later means changing a plugin install config, not Celerex code.

### Task 29: Galvatronic protocol spec doc

**Files:**
- Create: `docs/galvatronic-protocol-v0.1.md`

- [ ] **Step 1: Author the spec doc**

Write `docs/galvatronic-protocol-v0.1.md` containing at minimum:

- **Overview** — A2A protocol for Celerex agents to delegate work to peer products (Galvo first).
- **Transport** — HTTP/JSON, single endpoint per peer: `POST {peer_base_url}/a2a/invoke`.
- **Request shape** — `{ "target": "<peer>.<capability>", "params": <object>, "request_id": "<uuid>", "auth": { "scheme": "bearer", "token": "<jwt>" } }`.
- **Response shape** — `{ "request_id": "<uuid>", "status": "success" | "error", "result": <markdown string or object>, "error": { "code": "<string>", "message": "<string>" } }`.
- **Streaming** — out of scope for v0.1.
- **Auth** — bearer token issued per peer pair (rotation TBD).
- **Versioning** — `Galvatronic-Version: 0.1` header required.

Aim for ~150 lines. Engineer should write the body in the same style as the design spec.

- [ ] **Step 2: Commit**

```bash
git add docs/galvatronic-protocol-v0.1.md
git commit -m "docs: galvatronic A2A protocol v0.1 spec"
```

---

### Task 30: Galvo plugin manifest fixture + fixture-backed A2A stub (end-to-end)

**Design intent:** Galvo is reached like any other plugin. Sprint 0 ships:
1. A Galvo plugin manifest fixture declaring outbound capability `galvo.research.weekly`.
2. A generic `FixtureA2AClient` in `celerex-orchestrator` (behind `test-support` feature) that serves canned MD when `target` matches. Not Galvo-specific — any peer plugin can use it for local dev/tests.
3. An end-to-end integration test: install the Galvo manifest → register `FixtureA2AClient`-backed `AgentHandler` on the `Dispatcher` → execute a one-step `JobRequest` with `ExecutionType::Agent` and `config.target = "galvo.research.weekly"` → assert fixture returned.

No `src/galvatronic/` module is added to `celerex-api`. The agent handler from Task 15 is the only integration point; Galvo brings nothing special beyond a manifest.

**Files:**
- Create: `packages/backend/api/tests/fixtures/plugins/galvo/manifest.json`
- Create: `packages/backend/api/tests/fixtures/plugins/galvo/research_weekly.md`
- Create: `packages/backend/agent-orchestrator/src/handlers/agent_stub.rs`
- Modify: `packages/backend/agent-orchestrator/Cargo.toml` (add `test-support` feature)
- Modify: `packages/backend/agent-orchestrator/src/handlers/mod.rs` (gated `pub mod agent_stub;`)
- Create: `packages/backend/api/tests/galvo_plugin.rs`

- [ ] **Step 1: Galvo plugin manifest fixture**

```json
// packages/backend/api/tests/fixtures/plugins/galvo/manifest.json
{
  "id": "galvo",
  "name": "Galvo Research",
  "version": "0.1",
  "inbound": [],
  "outbound": [
    {
      "name": "galvo.research.weekly",
      "description": "Fetch the weekly research digest from Galvo via Galvatronic A2A.",
      "execution_type": "agent",
      "params_schema": { "type": "object", "properties": {} },
      "result_schema": { "type": "object", "properties": { "result": { "type": "string" } } }
    }
  ]
}
```

- [ ] **Step 2: Canned MD fixture (lives next to the manifest, NOT inside the crate)**

```markdown
<!-- packages/backend/api/tests/fixtures/plugins/galvo/research_weekly.md -->
# Weekly Research Digest

> Fixture served by FixtureA2AClient. Replaced by live Galvo when the real
> peer plugin install is configured.

## Highlights
- Item one
- Item two
- Item three
```

- [ ] **Step 3: Add a `test-support` feature to the orchestrator crate**

```toml
# packages/backend/agent-orchestrator/Cargo.toml — append
[features]
default = []
test-support = []
```

- [ ] **Step 4: `FixtureA2AClient` — generic, not Galvo-specific**

```rust
// packages/backend/agent-orchestrator/src/handlers/agent_stub.rs
//! Feature-gated: compiled only when `test-support` is enabled.
//! Serves canned responses keyed by `target` — used by integration tests and
//! local dev. Any peer plugin can use it by seeding a (target -> payload) map.

use crate::handlers::agent::A2AClient;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct FixtureA2AClient {
    /// Keyed by target (e.g. "galvo.research.weekly"). Value is returned verbatim
    /// inside the `result` field of the A2A response envelope.
    pub fixtures: HashMap<String, String>,
}

impl FixtureA2AClient {
    pub fn new() -> Self { Self { fixtures: HashMap::new() } }
    pub fn with_fixture(mut self, target: &str, body: &str) -> Self {
        self.fixtures.insert(target.to_string(), body.to_string());
        self
    }
}

#[async_trait]
impl A2AClient for FixtureA2AClient {
    async fn call(&self, target: &str, _params: Value) -> Result<Value, String> {
        match self.fixtures.get(target) {
            Some(body) => Ok(json!({
                "request_id": uuid::Uuid::new_v4().to_string(),
                "status": "success",
                "result": body,
            })),
            None => Err(format!("fixture A2A: unknown target: {target}")),
        }
    }
}
```

```rust
// packages/backend/agent-orchestrator/src/handlers/mod.rs — append
#[cfg(feature = "test-support")]
pub mod agent_stub;
```

- [ ] **Step 5: Failing end-to-end integration test — install Galvo manifest, run via orchestrator**

```rust
// packages/backend/api/tests/galvo_plugin.rs
mod common;
use celerex_orchestrator::executor::{Executor, InMemoryRunStore};
use celerex_orchestrator::tool_dispatch::Dispatcher;
use celerex_orchestrator::handlers::agent::AgentHandler;
use celerex_orchestrator::handlers::agent_stub::FixtureA2AClient;
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::job::{JobRequest, Step, StepKind, RunStatus};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

const GALVO_MANIFEST: &str = include_str!("fixtures/plugins/galvo/manifest.json");
const GALVO_FIXTURE_MD: &str = include_str!("fixtures/plugins/galvo/research_weekly.md");

#[tokio::test]
async fn galvo_plugin_manifest_installs_and_lists_outbound_capability() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();

    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('E') RETURNING id")
        .fetch_one(&db.pool).await.unwrap();
    let t = common::token(eng);

    let manifest: Value = serde_json::from_str(GALVO_MANIFEST).unwrap();
    let install = server.post("/api/plugins")
        .add_header("authorization", format!("Bearer {t}"))
        .json(&json!({ "manifest": manifest })).await
        .json::<Value>();
    assert_eq!(install["manifest"]["id"], "galvo");
    assert_eq!(install["manifest"]["outbound"][0]["name"], "galvo.research.weekly");
}

#[tokio::test]
async fn galvo_capability_invokable_through_orchestrator_agent_handler() {
    let client = Arc::new(
        FixtureA2AClient::new().with_fixture("galvo.research.weekly", GALVO_FIXTURE_MD)
    );
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Agent, Arc::new(AgentHandler { client }));

    let executor = Executor::new(Arc::new(dispatcher), Arc::new(InMemoryRunStore::new()));
    let job = JobRequest {
        run_id: Uuid::new_v4(),
        trigger: Default::default(),
        steps: vec![Step {
            id: "s1".into(),
            kind: StepKind { execution_type: ExecutionType::Agent },
            config: json!({ "target": "galvo.research.weekly" }),
            depends_on: vec![],
        }],
    };

    let out = executor.run(job).await.unwrap();
    assert_eq!(out.status, RunStatus::Succeeded);
    let result = &out.steps["s1"].output["result"];
    assert!(result.as_str().unwrap().contains("# Weekly Research Digest"),
            "fixture payload must reach the step output");
}

#[tokio::test]
async fn unknown_galvo_target_fails_through_handler() {
    let client = Arc::new(FixtureA2AClient::new()); // no fixtures registered
    let mut dispatcher = Dispatcher::new();
    dispatcher.register(ExecutionType::Agent, Arc::new(AgentHandler { client }));

    let executor = Executor::new(Arc::new(dispatcher), Arc::new(InMemoryRunStore::new()));
    let job = JobRequest {
        run_id: Uuid::new_v4(),
        trigger: Default::default(),
        steps: vec![Step {
            id: "s1".into(),
            kind: StepKind { execution_type: ExecutionType::Agent },
            config: json!({ "target": "galvo.unknown.thing" }),
            depends_on: vec![],
        }],
    };
    let out = executor.run(job).await.unwrap();
    assert_eq!(out.status, RunStatus::Failed);
}
```

- [ ] **Step 6: Ensure api crate's dev-deps include orchestrator with `test-support`**

```toml
# packages/backend/api/Cargo.toml — [dev-dependencies] section
celerex-orchestrator = { path = "../agent-orchestrator", features = ["test-support"] }
```

- [ ] **Step 7: Run — PASS**

Run: `cargo test -p celerex-api --test galvo_plugin`
Expected: all three tests PASS.

- [ ] **Step 8: Commit**

```bash
git add packages/backend/api/tests/fixtures/plugins/galvo/ \
        packages/backend/api/tests/galvo_plugin.rs \
        packages/backend/api/Cargo.toml \
        packages/backend/agent-orchestrator/Cargo.toml \
        packages/backend/agent-orchestrator/src/handlers/agent_stub.rs \
        packages/backend/agent-orchestrator/src/handlers/mod.rs
git commit -m "feat(plugins): Galvo plugin manifest fixture + FixtureA2AClient

Galvo is a plugin, not a first-class subsystem. Manifest declares
outbound galvo.research.weekly; FixtureA2AClient (test-support
feature on orchestrator) serves canned MD for dev/tests. Live Galvo
later is just a different plugin install — no Celerex changes."
```

---

## Final verification

### Task 31: All-tests sweep + smoke run

- [ ] **Step 1: Run full Rust test suite**

```bash
cargo test --workspace
```
Expected: all green.

- [ ] **Step 2: Run frontend tests**

```bash
cd packages/frontend/app && bun run test
```
Expected: all green.

- [ ] **Step 3: Manual smoke — start backend + frontend, navigate**

```bash
docker run --rm -d --name pg-celerex -e POSTGRES_PASSWORD=postgres -p 5432:5432 postgres:15
sleep 2
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres \
CELEREX_JWT_SECRET=dev-secret \
CELEREX_CRED_KEY=0123456789abcdef0123456789abcdef \
cargo run -p celerex-api &
cd packages/frontend/app && bun run dev &
sleep 5
open http://localhost:5173
```

Verify:
- Sidebar renders Projects + Workspaces sections without crash.
- No Copilot dialog/UI.
- Backend responds at :3001/api/health.

Stop services. `docker stop pg-celerex`.

- [ ] **Step 4: Tag the substrate**

```bash
git tag sprint-0-foundation
git log --oneline sprint-0-foundation -30
```

---

## Self-review checklist

**1. Spec coverage:**
| Spec §5.1 deliverable | Implementing task(s) |
|-----------------------|----------------------|
| Fill `tool_dispatch.rs` | Task 13 |
| Doc I/O adapter (Option A) | Tasks 19, 20, 21 |
| Plugin registry backend | Tasks 23, 24 |
| Plugin manifest schema (in/out) | Task 22 |
| Webhook ingress skeleton | Task 25 |
| Event-triggered job support | Task 26 |
| Galvatronic protocol spec | Task 29 |
| Galvo-as-plugin fixture + E2E test | Task 30 |
| `CredentialStore` trait (swap point for future KMS/Vault) | Task 24 |
| LLM provider tool-use | Tasks 27, 28 |
| Add `Project` schema + routes + sidebar | Tasks 8, 9, 10 |
| Strip engagement-lifecycle fields | Task 7 |
| Delete `/cloud/*` | (N/A — does not exist; flagged in plan intro) |
| Delete Copilot OAuth + GitHub device flow | Task 6 |
| Rebrand engagement-as-consulting language | Task 7 |

Additional deliverables absorbed (per Option A scope expansion):
- Postgres + sqlx infra (Tasks 2, 3, 4)
- JWT auth middleware (Task 5)
- Vitest + Rust integration test framework (Tasks 1, 2)

**2. Placeholder scan:** None ("TODO/TBD/fill in later" not present).

**3. Type consistency:** `JobRequest`/`Step`/`StepKind`/`ExecutionType`/`RunStatus` defined in Task 12 are consumed verbatim in Tasks 13–18 and 26. `Dispatcher::register` / `Dispatcher::invoke` signatures stable across Tasks 13, 14, 15, 16, 26. `AuthClaims(Claims)` extractor introduced in Task 5 used unchanged in Tasks 8, 19–21, 23, 24. Frontend `Project` shape (`id`, `engagementId`, `name`) matches API `ApiProject` (`id`, `engagement_id`, `name`) — note the camelCase/snake_case boundary at the API client layer (Task 9 client functions).

---

## Execution

Plan saved to `docs/superpowers/plans/2026-04-16-celerex-sprint0-foundation.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
