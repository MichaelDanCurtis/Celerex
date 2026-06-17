# Multi-User Security Hardening (sub-project A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Celerex Agent safe for multiple users — close the cross-tenant data-access holes, run agents under the flow's true owner, stop silent shared-key spend, and refuse insecure boot — each fix guarded by a cross-tenant denial test.

**Architecture:** Pure server-side hardening of the Rust/Axum Agent (`packages/backend/api`). No new auth source (WorkOS is sub-project B); these fixes key off `claims.engagement_id` regardless of token origin. The work follows two existing precedents already in the codebase: the ownership-check idiom (`docs::get`, `project_members::delete` + `members.engagement_id_for`) and the cross-tenant 403 test (`tests/docs.rs::cross_engagement_get_doc_returns_403`).

**Tech Stack:** Rust, Axum, `tower_http::cors`, sqlx + Postgres, `axum_test::TestServer`, `testcontainers` (real Postgres in tests), `jsonwebtoken`.

---

## Conventions every task reuses

**Test harness** (no shared `TestServer`; each integration test file builds its own). Pattern from `tests/docs.rs`:
```rust
mod common; // tests/common/mod.rs → start_test_db()
use uuid::Uuid;
use serde::Serialize;
use jsonwebtoken::{encode, EncodingKey, Header};

#[derive(Serialize)]
struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }

fn token(eng: Uuid, proj: Option<Uuid>) -> String {
    // SAFETY: tests run single-process; env set before middleware reads it.
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); }
    let c = Claims { sub: "u".into(), engagement_id: eng.to_string(),
        project_id: proj.map(|p| p.to_string()),
        exp: (chrono::Utc::now().timestamp() + 3600) as usize };
    encode(&Header::default(), &c, &EncodingKey::from_secret(b"test-secret")).unwrap()
}

async fn seed_engagement(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO engagements (name) VALUES ($1) RETURNING id")
        .bind(name).fetch_one(pool).await.unwrap()
}
```
`start_test_db()` (`tests/common/mod.rs`) sets `CELEREX_CRED_KEY` + `CELEREX_JWT_SECRET` and runs migrations. Build the app with `celerex_api::build_app(db.pool.clone())`.

**Ownership-check idiom** (copy from `project_members::delete`): resolve the resource's engagement via a scalar resolver, compare to `a.0.engagement_id`, return 403 on mismatch, 404 if the resolver finds nothing.
```rust
let owner = match s.<store>.engagement_id_for(id).await {
    Ok(v) => v,
    Err(_) => return StatusCode::NOT_FOUND.into_response(),
};
if owner != a.0.engagement_id {
    return (StatusCode::FORBIDDEN, Json(json!({"error":"forbidden"}))).into_response();
}
```

**Run tests:** `cd "packages/backend/api" && cargo test --test <file> 2>&1 | tail -25` (integration) or `cargo test --lib <path> 2>&1 | tail -20` (unit). Full gate: `cargo test 2>&1 | tail -20`.

---

# Unit 5 — Boot safety (no insecure JWT default)

### Task 1: Require `CELEREX_JWT_SECRET` outside tests

**Files:** Modify `packages/backend/api/src/auth/jwt.rs`

- [ ] **Step 1: Add a secret helper + unit test**

Add to `jwt.rs` (and a `#[cfg(test)] mod` test):
```rust
/// The HS256 signing/verifying secret. Required in production; tests fall back
/// to a fixed value so the lib's own unit tests need no env setup.
pub(crate) fn jwt_secret() -> String {
    if let Ok(s) = std::env::var("CELEREX_JWT_SECRET") {
        if !s.is_empty() {
            return s;
        }
    }
    #[cfg(test)]
    { "test-secret".to_string() }
    #[cfg(not(test))]
    { panic!("CELEREX_JWT_SECRET must be set (refusing to start with an insecure default)"); }
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    #[test]
    fn mint_then_verify_roundtrips_with_env_secret() {
        // SAFETY: single-process test.
        unsafe { std::env::set_var("CELEREX_JWT_SECRET", "unit-secret"); }
        let eng = uuid::Uuid::new_v4();
        let t = mint(eng, "u@x.test", 3600);
        let claims = verify(&t, &jwt_secret()).unwrap();
        assert_eq!(claims.engagement_id, eng);
    }
}
```

- [ ] **Step 2: Run it (fails — `jwt_secret` undefined / mint uses inline fallback)**

Run: `cd "packages/backend/api" && cargo test --lib auth::jwt::secret_tests 2>&1 | tail -20`
Expected: FAIL to compile (no `jwt_secret`) or assertion issue.

- [ ] **Step 3: Route mint/mint_scoped + the extractor through `jwt_secret()`**

In `jwt.rs` `mint` (line ~46) and `mint_scoped` (line ~62), replace:
```rust
let key = std::env::var("CELEREX_JWT_SECRET").unwrap_or_else(|_| "test-secret".into());
```
with:
```rust
let key = jwt_secret();
```
In `src/auth/extractors.rs`, find where it reads the secret for `verify(...)` (the `std::env::var("CELEREX_JWT_SECRET")...` read) and replace that read with `crate::auth::jwt::jwt_secret()` so the verify path enforces the same rule.

- [ ] **Step 4: Run unit test + full lib build**

Run: `cd "packages/backend/api" && cargo test --lib auth:: 2>&1 | tail -20 && cargo build --lib 2>&1 | tail -5`
Expected: PASS, clean build.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/auth/jwt.rs packages/backend/api/src/auth/extractors.rs
git commit -m "fix(agent): require CELEREX_JWT_SECRET in production (no insecure default)"
```

---

# Unit 6 — CORS allowlist

### Task 2: Replace `allow_origin(Any)` with an env allowlist

**Files:** Modify `packages/backend/api/src/lib.rs` (CORS at ~346-352, import at 26)

- [ ] **Step 1: Add a parse helper + unit test**

Add near the top of `lib.rs` (and a unit test):
```rust
/// Parse CELEREX_ALLOWED_ORIGINS (comma-separated) into explicit origins.
/// Empty/unset → dev defaults. Never returns `Any` (public sites must not be open).
fn allowed_origins() -> Vec<http::HeaderValue> {
    let raw = std::env::var("CELEREX_ALLOWED_ORIGINS").unwrap_or_default();
    let list: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let defaults = ["http://localhost:5173", "http://localhost:3001"];
    let chosen: Vec<String> = if list.is_empty() { defaults.iter().map(|s| s.to_string()).collect() } else { list };
    chosen.iter().filter_map(|o| o.parse::<http::HeaderValue>().ok()).collect()
}

#[cfg(test)]
mod cors_tests {
    use super::*;
    #[test]
    fn parses_comma_list_and_defaults() {
        unsafe { std::env::set_var("CELEREX_ALLOWED_ORIGINS", "https://designer.ailuxis.com, https://x.test"); }
        let v = allowed_origins();
        assert_eq!(v.len(), 2);
        assert!(v.iter().any(|h| h.to_str().unwrap() == "https://designer.ailuxis.com"));
        unsafe { std::env::set_var("CELEREX_ALLOWED_ORIGINS", ""); }
        assert_eq!(allowed_origins().len(), 2); // dev defaults
    }
}
```
(`http` is already a transitive dep via axum; if the crate can't name it directly, use `axum::http::HeaderValue` instead — adjust the import to match what compiles.)

- [ ] **Step 2: Run it (fails — helper undefined)**

Run: `cd "packages/backend/api" && cargo test --lib cors_tests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Use the allowlist in the CorsLayer**

Replace (`lib.rs:346-352`):
```rust
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods(Any)
    .allow_headers(Any);
```
with:
```rust
let cors = CorsLayer::new()
    .allow_origin(tower_http::cors::AllowOrigin::list(allowed_origins()))
    .allow_methods(Any)
    .allow_headers(Any);
```
Keep the `use tower_http::cors::{Any, CorsLayer};` import (add `AllowOrigin` if you prefer importing it).

- [ ] **Step 4: Run test + build**

Run: `cd "packages/backend/api" && cargo test --lib cors_tests 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5`
Expected: PASS, clean.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/lib.rs
git commit -m "fix(agent): CORS allowlist via CELEREX_ALLOWED_ORIGINS (no Any origin)"
```

---

# Unit 4 — Ownership resolvers (enables Unit 1)

### Task 3: Add `engagement_id_for` resolvers for the resources Unit 1 needs

**Files:** Modify `packages/backend/api/src/db/mod.rs`, `packages/backend/api/src/scheduler/store.rs`, `packages/backend/api/src/automations/store.rs`

These scalar resolvers are the structural defense-in-depth: every handler ownership check calls one. `members.engagement_id_for(id)` already exists (`project_members/store.rs:234`). Add the missing ones.

- [ ] **Step 1: Add db resolvers (no separate unit test — exercised by Unit 1 denial tests)**

Add to `db/mod.rs`:
```rust
/// Engagement that owns a document (for block-level ownership checks).
pub async fn engagement_for_block(pool: &PgPool, block_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT d.engagement_id FROM doc_blocks b JOIN documents d ON d.id = b.document_id WHERE b.id = $1"
    ).bind(block_id).fetch_optional(pool).await
}

/// Engagement that owns a project.
pub async fn engagement_for_project(pool: &PgPool, project_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM projects WHERE id = $1")
        .bind(project_id).fetch_optional(pool).await
}

/// Engagement that owns a plugin install.
pub async fn engagement_for_install(pool: &PgPool, install_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM plugin_installs WHERE id = $1")
        .bind(install_id).fetch_optional(pool).await
}

/// Engagement that owns a run.
pub async fn engagement_for_run(pool: &PgPool, run_id: Uuid) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM runs WHERE id = $1")
        .bind(run_id).fetch_optional(pool).await
}
```

- [ ] **Step 2: Build to confirm the queries compile against the schema**

Run: `cd "packages/backend/api" && cargo build --lib 2>&1 | tail -10`
Expected: clean (these are runtime-checked sqlx queries, so build confirms types only).

- [ ] **Step 3: Commit**
```bash
git add packages/backend/api/src/db/mod.rs
git commit -m "feat(agent): engagement-ownership resolvers (block/project/install/run)"
```

(Schedules/automations HTTP handlers already resolve via `members.engagement_id_for` per the audit; their store `get` methods are used by trusted poll loops + already-guarded handlers, so leave their signatures unchanged here. Deeper store-signature scoping + RLS remain the deferred A2/belt-and-suspenders item.)

---

# Unit 1 — Ownership & auth checks on exposed routes

### Task 4: Authenticate + scope the run-event SSE stream

**Files:** Modify `packages/backend/api/src/runs/sse.rs`; Test `packages/backend/api/tests/runs_sse_auth.rs`

- [ ] **Step 1: Write the failing cross-tenant test**

Create `tests/runs_sse_auth.rs`:
```rust
mod common;
use uuid::Uuid;
// (reuse the Conventions token()/seed_engagement() helpers — paste them here)

#[tokio::test]
async fn sse_requires_auth_and_blocks_other_engagement() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let eng_a = seed_engagement(&db.pool, "A").await;
    let eng_b = seed_engagement(&db.pool, "B").await;
    let proj_a: Uuid = sqlx::query_scalar("INSERT INTO projects (engagement_id,name) VALUES ($1,'PA') RETURNING id")
        .bind(eng_a).fetch_one(&db.pool).await.unwrap();
    // a run owned by A
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO runs (id,engagement_id,project_id,project_member_id,trigger,status,snapshot_id,triggered_by) \
                 VALUES ($1,$2,$3,NULL,'{}'::jsonb,'running',NULL,'manual')")
        .bind(run_id).bind(eng_a).bind(proj_a).execute(&db.pool).await.unwrap();

    // no token → not OK
    let anon = server.get(&format!("/api/runs/{run_id}/events")).await;
    assert_ne!(anon.status_code(), axum::http::StatusCode::OK);

    // B's token → 403
    let resp = server.get(&format!("/api/runs/{run_id}/events"))
        .add_header("authorization", format!("Bearer {}", token(eng_b, None))).await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}
```

- [ ] **Step 2: Run it (fails — endpoint is unauthenticated, returns 200/stream)**

Run: `cd "packages/backend/api" && cargo test --test runs_sse_auth 2>&1 | tail -25`
Expected: FAIL (anon gets OK; B gets OK).

- [ ] **Step 3: Add auth + ownership to `stream`**

In `runs/sse.rs`, change the handler signature to take `AuthClaims` and check ownership before subscribing:
```rust
pub async fn stream(
    a: crate::auth::extractors::AuthClaims,
    State(s): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> axum::response::Response {
    match crate::db::engagement_for_run(&s.pool, run_id).await {
        Ok(Some(eng)) if eng == a.0.engagement_id => {}
        Ok(Some(_)) => return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response(),
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "run not found").into_response(),
        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
    let rx = s.run_bus.subscribe(run_id);
    let events = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => Some(Ok(Event::default().data(serde_json::to_string(&ev).ok()?))),
        Err(_) => None,
    });
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}
```
(Add `use axum::response::IntoResponse;` if needed. The return type changes from `Sse<...>` to `Response`.)

- [ ] **Step 4: Run test + build**

Run: `cd "packages/backend/api" && cargo test --test runs_sse_auth 2>&1 | tail -15 && cargo build --lib 2>&1 | tail -5`
Expected: PASS, clean.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/runs/sse.rs packages/backend/api/tests/runs_sse_auth.rs
git commit -m "fix(agent): authenticate + engagement-scope the run-event SSE stream"
```

---

### Task 5: Ownership checks on doc block writes

**Files:** Modify `packages/backend/api/src/docs/mod.rs:24-47`; Test `packages/backend/api/tests/docs_block_auth.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/docs_block_auth.rs` (paste the Conventions helpers). Create a doc as A (via `POST /api/docs` with A's token), then:
```rust
// B appends a block to A's doc → 403
let resp = server.post(&format!("/api/docs/{doc_id}/blocks"))
    .add_header("authorization", format!("Bearer {}", token(eng_b, None)))
    .json(&serde_json::json!({"type":"text","text":"x"})).await;
resp.assert_status(axum::http::StatusCode::FORBIDDEN);
// A appends → 201 (no regression)
let ok = server.post(&format!("/api/docs/{doc_id}/blocks"))
    .add_header("authorization", format!("Bearer {}", token(eng_a, Some(proj_a))))
    .json(&serde_json::json!({"type":"text","text":"x"})).await;
ok.assert_status(axum::http::StatusCode::CREATED);
```
(Check the real append-block route path + payload shape in `lib.rs`; adjust the URL/body to match. For `replace_block`, add a similar case: A creates a block, B PUTs it → 403.)

- [ ] **Step 2: Run it (fails — currently `_auth` ignored, returns 201/200 for B)**

Run: `cd "packages/backend/api" && cargo test --test docs_block_auth 2>&1 | tail -25`
Expected: FAIL.

- [ ] **Step 3: Add ownership checks**

`docs::append_block` — resolve the doc's engagement (reuse `db::get_document`):
```rust
pub async fn append_block(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match db::get_document(&s.pool, doc_id).await {
        Ok(Some(d)) if d.engagement_id == auth.0.engagement_id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, "engagement mismatch").into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, "document not found").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
    match db::append_block(&s.pool, doc_id, &payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```
`docs::replace_block` — resolve via the block's document (use `db::engagement_for_block` from Task 3):
```rust
pub async fn replace_block(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path((_doc, block)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match db::engagement_for_block(&s.pool, block).await {
        Ok(Some(eng)) if eng == auth.0.engagement_id => {}
        Ok(Some(_)) => return (StatusCode::FORBIDDEN, "engagement mismatch").into_response(),
        Ok(None) => return (StatusCode::NOT_FOUND, "block not found").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
    match db::replace_block(&s.pool, block, &payload).await {
        Ok(0) => (StatusCode::NOT_FOUND, "block not found").into_response(),
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```

- [ ] **Step 4: Run test + build** — `cargo test --test docs_block_auth 2>&1 | tail -15` → PASS.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/docs/mod.rs packages/backend/api/tests/docs_block_auth.rs
git commit -m "fix(agent): engagement ownership checks on doc block append/replace"
```

---

### Task 6: Ownership checks on project_members handlers

**Files:** Modify `packages/backend/api/src/project_members/routes.rs` (`create` :45, `get` :110, `update` :77, `list` :126); Test `packages/backend/api/tests/project_members_auth.rs`

- [ ] **Step 1: Write the failing test** (paste Conventions helpers)

Seed eng A + project A + a member in A (insert via SQL or `POST` as A). Then as B:
```rust
// B reads A's member → 403
server.get(&format!("/api/project_members/{member_id}"))
    .add_header("authorization", format!("Bearer {}", token(eng_b, None))).await
    .assert_status(axum::http::StatusCode::FORBIDDEN);
// B updates A's member → 403
server.put(&format!("/api/project_members/{member_id}"))
    .add_header("authorization", format!("Bearer {}", token(eng_b, None)))
    .json(&update_body()).await.assert_status(axum::http::StatusCode::FORBIDDEN);
// B lists A's project members → 403
server.get(&format!("/api/projects/{proj_a}/members"))
    .add_header("authorization", format!("Bearer {}", token(eng_b, None))).await
    .assert_status(axum::http::StatusCode::FORBIDDEN);
// B creates a member in A's project → 403
server.post("/api/project_members")
    .add_header("authorization", format!("Bearer {}", token(eng_b, None)))
    .json(&create_body(proj_a)).await.assert_status(axum::http::StatusCode::FORBIDDEN);
// A's own access still works (no regression)
server.get(&format!("/api/project_members/{member_id}"))
    .add_header("authorization", format!("Bearer {}", token(eng_a, Some(proj_a)))).await
    .assert_status_ok();
```
(Confirm the exact routes/bodies in `lib.rs`/`routes.rs` and adjust.)

- [ ] **Step 2: Run it (fails — `_a` ignored)**

Run: `cd "packages/backend/api" && cargo test --test project_members_auth 2>&1 | tail -25`
Expected: FAIL.

- [ ] **Step 3: Add the checks** (use `a: AuthClaims`, not `_a`)

`get` / `update` — resolve via `s.members.engagement_id_for(id)`:
```rust
let owner = match s.members.engagement_id_for(id).await {
    Ok(v) => v, Err(_) => return StatusCode::NOT_FOUND.into_response(),
};
if owner != a.0.engagement_id {
    return (StatusCode::FORBIDDEN, Json(json!({"error":"forbidden"}))).into_response();
}
// …then the existing get/update body…
```
`list` (path `project_id`) and `create` (body `project_id`) — resolve via `db::engagement_for_project`:
```rust
let owner = match crate::db::engagement_for_project(&s.pool, project_id).await {
    Ok(Some(v)) => v, Ok(None) => return StatusCode::NOT_FOUND.into_response(),
    Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
};
if owner != a.0.engagement_id {
    return (StatusCode::FORBIDDEN, Json(json!({"error":"forbidden"}))).into_response();
}
// …then the existing list/create body (use b.project_id for create)…
```

- [ ] **Step 4: Run test + build** — `cargo test --test project_members_auth 2>&1 | tail -15` → PASS.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/project_members/routes.rs packages/backend/api/tests/project_members_auth.rs
git commit -m "fix(agent): engagement ownership checks on project_members get/update/list/create"
```

---

### Task 7: Ownership check on plugin credential writes

**Files:** Modify `packages/backend/api/src/plugins/routes.rs:41`; Test `packages/backend/api/tests/plugins_creds_auth.rs`

- [ ] **Step 1: Write the failing test** — seed an install owned by A (insert via SQL into `plugin_installs` with engagement A), then B PUTs credentials for it → expect 403; A → 200.

- [ ] **Step 2: Run it (fails — `_a` ignored)** — `cargo test --test plugins_creds_auth 2>&1 | tail -25` → FAIL.

- [ ] **Step 3: Add the check** (use `a: AuthClaims`):
```rust
let owner = match crate::db::engagement_for_install(&s.pool, install).await {
    Ok(Some(v)) => v, Ok(None) => return StatusCode::NOT_FOUND.into_response(),
    Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
};
if owner != a.0.engagement_id {
    return (StatusCode::FORBIDDEN, "forbidden").into_response();
}
// …then the existing seal + upsert_credentials body…
```

- [ ] **Step 4: Run + build** → PASS.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/plugins/routes.rs packages/backend/api/tests/plugins_creds_auth.rs
git commit -m "fix(agent): engagement ownership check on plugin credential upsert"
```

---

### Task 8: Authenticate + scope the engagements endpoints

**Files:** Modify `packages/backend/api/src/engagements/mod.rs:8-20` + `db/mod.rs`; Test `packages/backend/api/tests/engagements_auth.rs`

- [ ] **Step 1: Write the failing test** — seed eng A + B; `GET /api/engagements` with no token → not OK; with A's token → returns ONLY A (length 1, id == eng_a).

- [ ] **Step 2: Run it (fails — unauthenticated, returns all)** — `cargo test --test engagements_auth 2>&1 | tail -25` → FAIL.

- [ ] **Step 3: Add auth + scope**

Add a scoped db fn to `db/mod.rs`:
```rust
pub async fn get_engagement(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Engagement>> {
    sqlx::query_as::<_, Engagement>("SELECT id, name, created_at FROM engagements WHERE id = $1")
        .bind(id).fetch_optional(pool).await
}
```
`engagements::list` — return only the caller's engagement:
```rust
pub async fn list(a: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    match db::get_engagement(&s.pool, a.0.engagement_id).await {
        Ok(Some(e)) => (StatusCode::OK, Json(vec![e])).into_response(),
        Ok(None) => (StatusCode::OK, Json(Vec::<db::Engagement>::new())).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
```
`engagements::create` — require `AuthClaims` (it stays available to the signup path, which mints its own token; this handler is now authenticated-only). Add `_a: AuthClaims` to the signature so an anonymous caller can't create engagements. (Import `AuthClaims`.)

- [ ] **Step 4: Run + build** → PASS.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/engagements/mod.rs packages/backend/api/src/db/mod.rs packages/backend/api/tests/engagements_auth.rs
git commit -m "fix(agent): authenticate engagements endpoints; list scoped to caller"
```

---

# Unit 2 — Run identity

### Task 9: Run under the flow owner; overwrite node engagement

**Files:** Modify `packages/backend/api/src/project_members/routes.rs` (`run_manual` :171), `packages/backend/api/src/project_members/run_job.rs` (`apply_doc_engagement` :85); Test `packages/backend/api/tests/run_manual_auth.rs`

- [ ] **Step 1: Write the failing test** — seed member in eng A. As B, `POST /api/project_members/{member_id}/run` → expect 403 (currently it 202s and runs under B). As A → 202.
```rust
let resp = server.post(&format!("/api/project_members/{member_id}/run"))
    .add_header("authorization", format!("Bearer {}", token(eng_b, None))).await;
resp.assert_status(axum::http::StatusCode::FORBIDDEN);
```

- [ ] **Step 2: Run it (fails — run_manual trusts caller's engagement)** — `cargo test --test run_manual_auth 2>&1 | tail -25` → FAIL.

- [ ] **Step 3: Fix `run_manual` to resolve + verify the member's engagement**

In `run_manual`, after fetching/compiling the member, resolve the true owner and use it:
```rust
let owner = match s.members.engagement_id_for(id).await {
    Ok(v) => v, Err(_) => return StatusCode::NOT_FOUND.into_response(),
};
if owner != a.0.engagement_id {
    return (StatusCode::FORBIDDEN, Json(json!({"error":"forbidden"}))).into_response();
}
// …existing compile check…
match run_job::enqueue_run(
    s.pool.clone(), s.members.clone(), s.executor.clone(), s.notifiers.clone(),
    id, owner, TRIGGERED_BY_MANUAL,
).await { /* …existing 202… */ }
```

- [ ] **Step 4: Make `apply_doc_engagement` OVERWRITE (not defer)**

In `run_job.rs:85-111`, replace the `needs`-gated insert so the run's true engagement always wins:
```rust
pub fn apply_doc_engagement(graph: &mut serde_json::Value, engagement_id: Uuid) {
    let Some(nodes) = graph.get_mut("nodes").and_then(|n| n.as_array_mut()) else { return; };
    for node in nodes {
        let exec = node.get("config").and_then(|c| c.get("execution")).and_then(|e| e.as_str());
        if exec != Some("doc") && exec != Some("llm") { continue; }
        let Some(cfg) = node.get_mut("config").and_then(|c| c.as_object_mut()) else { continue; };
        cfg.insert("engagement_id".to_string(), json!(engagement_id.to_string())); // always overwrite
    }
}
```

- [ ] **Step 5: Run test + full suite** — `cargo test --test run_manual_auth 2>&1 | tail -15 && cargo test --lib 2>&1 | tail -15` → PASS (watch for any existing run_job unit test asserting the old defer behavior; update it to expect overwrite).

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/project_members/routes.rs packages/backend/api/src/project_members/run_job.rs packages/backend/api/tests/run_manual_auth.rs
git commit -m "fix(agent): run agents under the flow owner; overwrite node engagement_id"
```

---

# Unit 3 — House-key policy

### Task 10: House-mode flag + hard-fail + authenticate the assistant

**Files:** Modify `packages/backend/api/src/llm/handler.rs:79-133`, `packages/backend/api/src/assistant/mod.rs:33-105`, `packages/backend/api/src/lib.rs` (assistant routes ~158-159); Test `packages/backend/api/tests/house_key_policy.rs`

- [ ] **Step 1: Write the failing tests**

Create `tests/house_key_policy.rs`:
```rust
// (a) assistant chat requires auth: POST /api/assistant/chat with no token → not OK.
// (b) with CELEREX_HOUSE_MODE unset/false and no vaulted key for the engagement,
//     an authenticated assistant chat → 409 (no silent house spend).
```
Set env in-test: `unsafe { std::env::set_var("CELEREX_HOUSE_MODE", "false"); }` before building the app. Assert the anon call is rejected and the authenticated-no-key call returns `409 CONFLICT`.

- [ ] **Step 2: Run it (fails — assistant is unauthenticated + house fallback silent)** — `cargo test --test house_key_policy 2>&1 | tail -25` → FAIL.

- [ ] **Step 3: Gate the house fallback in `EngagementLlmHandler::invoke`**

Add a house-mode check before the registry fallback (`handler.rs:~110`):
```rust
let house_mode = std::env::var("CELEREX_HOUSE_MODE").map(|v| v == "true" || v == "1").unwrap_or(false);
// after the per-engagement key path yields no key, for a known provider:
if !house_mode {
    return Err(DispatchError::Handler(format!("no key configured for provider '{provider_id}'")));
}
// …existing registry fallback chain only runs in house mode…
```
Map this `DispatchError::Handler("no key configured…")` to a 409 at the run/handler boundary if it surfaces to HTTP (the assistant path below returns 409 directly).

- [ ] **Step 4: Authenticate + engagement-scope the assistant**

`assistant::chat` and `chat_stream` — add `a: AuthClaims`, and resolve the provider key from the vault first; in non-house-mode with no key → 409:
```rust
pub async fn chat(
    a: crate::auth::extractors::AuthClaims,
    State(state): State<crate::AppState>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    let provider_id = body.provider_id.unwrap_or_else(|| "anthropic".to_string());
    let model = body.model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    let tools = body.tools.unwrap_or(Value::Array(vec![]));
    let house_mode = std::env::var("CELEREX_HOUSE_MODE").map(|v| v=="true"||v=="1").unwrap_or(false);
    let key = state.provider_keys.get(a.0.engagement_id, &provider_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let completion = match key {
        Some(k) => { /* build a fresh provider client with k and call complete() */ unimplemented_call(&provider_id, &model, k, body.messages, tools).await? }
        None if house_mode => {
            let p = state.providers_registry.get(&provider_id).ok_or((StatusCode::BAD_REQUEST, format!("no provider '{provider_id}'")))?;
            p.complete(&model, body.messages, tools).await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?
        }
        None => return Err((StatusCode::CONFLICT, "no key configured for this provider".into())),
    };
    Ok(Json(ChatResponse { provider_id, model, raw: completion.raw }))
}
```
For the per-engagement-key call, reuse the same construction the flow path uses (look at how `EngagementLlmHandler` builds a fresh client from a vaulted key — mirror it; replace `unimplemented_call` with that real construction). Apply the same pattern to `chat_stream`. Confirm the assistant routes in `lib.rs` still compile.

- [ ] **Step 5: Run tests + full suite**

Run: `cd "packages/backend/api" && cargo test --test house_key_policy 2>&1 | tail -15 && cargo test 2>&1 | tail -20`
Expected: PASS. (If existing assistant tests assumed no auth, update them to pass a token + set house mode.)

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/llm/handler.rs packages/backend/api/src/assistant/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/house_key_policy.rs
git commit -m "fix(agent): house-mode flag (default off) + authenticate/engagement-scope the assistant"
```

---

## Final verification

- [ ] `cd "packages/backend/api" && cargo test 2>&1 | tail -25` — all green (note the pre-existing total; no new failures).
- [ ] `cargo build --release 2>&1 | tail -5` is not required; debug build clean is enough for local.
- [ ] Each unit shipped as its own branch + PR + **merged right after opening** (standing rule); every task spec-and-quality reviewed (subagent-driven, the 2h pattern).

## Self-review notes (spec coverage)
- Unit 5 boot-safety → Task 1. ✓
- Unit 6 CORS allowlist → Task 2. ✓
- Unit 4 ownership resolvers → Task 3 (enables the handler checks; deep store-signature scoping + RLS remain deferred A2 per spec §6). ✓
- Unit 1 ownership/auth → Tasks 4 (SSE), 5 (doc blocks), 6 (project_members ×4), 7 (plugin creds), 8 (engagements). ✓ (engagements were fully unauthenticated — handled.)
- Unit 2 run identity → Task 9 (run_manual + apply_doc_engagement overwrite). ✓
- Unit 3 house-key → Task 10 (house-mode flag + 409 + assistant auth/scoping — assistant was also unauthenticated, handled). ✓
- Every fix has a cross-tenant denial (or auth-required) test. ✓
- **Deferred (A2, per spec §6):** connector per-engagement secrets, per-engagement galvo-memory scope, webhook tenant attribution, Postgres RLS, deep store-signature scoping. **B (WorkOS):** login, signup-collision/workspace-provisioning, email-verify/reset/logout, per-user settings store.
