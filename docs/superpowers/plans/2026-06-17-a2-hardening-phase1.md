# A2 Multi-Tenancy Hardening — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the three remaining contained cross-tenant holes from the audit — gate galvo_hub federation to admin engagements, scope galvo-memory per engagement, and route inbound webhooks by a per-install secret token.

**Architecture:** Three independent Agent-only fixes in `packages/backend/api`. (1) An env allowlist (`CELEREX_HUB_ADMIN_ENGAGEMENTS`) parsed once at boot onto `AppState`, checked by the galvo_hub handlers (deny-by-default). (2) Memory scope derived from the run's `engagement_id` instead of a global default. (3) A `webhook_token` column on `plugin_installs` + a tokenized inbound route, replacing the cross-tenant `plugin_id LIMIT 1` lookup. Each ships behind a stubbed integration test; no live Galvo/sidecar needed.

**Tech Stack:** Rust/Axum (`celerex-api`), sqlx/Postgres, `axum_test::TestServer` + `tests/common/mod.rs::start_test_db()`, `wiremock`.

---

## Conventions

- Run from `cd "packages/backend/api"`. Tests: `cargo test --test <name>`; lib: `cargo build --lib`. Format ONLY changed files: `rustfmt --edition 2024 <file>` (revert any pre-existing-drift reformatting elsewhere — plain `rustfmt` defaults to edition 2015 and chokes on `async fn`).
- Integration tests use `tests/common/mod.rs::start_test_db()` (sets `CELEREX_CRED_KEY` + `CELEREX_JWT_SECRET`) + `axum_test::TestServer::new(celerex_api::build_app(db.pool.clone()))`. Mint a per-engagement session token with the helper used across tests:
```rust
#[derive(serde::Serialize)] struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: uuid::Uuid) -> String {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); } // SAFETY: single-process test
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None, exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}
```
- Read `tests/engagements.rs` / `tests/webhooks.rs` for the exact seed-engagement SQL + bootstrap; mirror it.
- **Some env vars must be set BEFORE `build_app`** if parsed at boot (the allowlist in T1). Tests that need a specific allowlist must `std::env::set_var(...)` before constructing the app, and should run with `--test-threads=1` semantics if env races — prefer constructing the app inside each test after setting the env (the existing CORS test pattern is the reference; if the allowlist must be parsed at boot, build the app per-test after setting the env var).

---

# Stage 1 — Agent (branch `feat/a2-hardening-p1`)

### Task 1: galvo_hub admin gate

**Files:** Modify `src/galvo_hub_routes.rs`, `src/lib.rs` (AppState construction / the struct holding boot config), and wherever `AppState` is defined; Test `tests/galvo_hub_admin.rs`

- [ ] **Step 1: Recon.** Read (a) `src/galvo_hub_routes.rs` — the `register` (~:50), `invoke` (~:119), `status` handlers + their current `AuthClaims` usage; (b) how `AppState` is defined and constructed, and where `CELEREX_ALLOWED_ORIGINS` is parsed at boot (mirror that parse + storage location exactly — likely a field on `AppState` or a boot-config struct); (c) the `AuthClaims` shape (`claims.engagement_id: Uuid`). Note the exact `AppState` field-add pattern.

- [ ] **Step 2: Add the allowlist to AppState.** Where `CELEREX_ALLOWED_ORIGINS` is parsed at boot, add a sibling parse:
```rust
// Parse comma-separated engagement UUIDs; skip malformed with a warn. Empty/unset = deny all.
let hub_admin_engagements: std::collections::HashSet<uuid::Uuid> = std::env::var("CELEREX_HUB_ADMIN_ENGAGEMENTS")
    .unwrap_or_default()
    .split(',')
    .map(|s| s.trim())
    .filter(|s| !s.is_empty())
    .filter_map(|s| match uuid::Uuid::parse_str(s) {
        Ok(u) => Some(u),
        Err(_) => { tracing::warn!(value=%s, "ignoring malformed CELEREX_HUB_ADMIN_ENGAGEMENTS entry"); None }
    })
    .collect();
```
Store it on `AppState` (add `pub hub_admin_engagements: std::collections::HashSet<uuid::Uuid>,` to the struct and set it in the constructor — match how `allowed_origins` or similar is stored). If `AppState` is built inside `build_app`, parse it there so tests that set the env before `build_app` pick it up.

- [ ] **Step 3: Gate the handlers.** In `src/galvo_hub_routes.rs`, change `register`, `invoke`, AND `status` to take the claims and check membership. Example for `register` (apply the same guard to all three):
```rust
pub async fn register(
    AuthClaims(claims): AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<RegisterBody>,
) -> impl IntoResponse {
    if !s.hub_admin_engagements.contains(&claims.engagement_id) {
        return (axum::http::StatusCode::FORBIDDEN, "hub admin access required").into_response();
    }
    // ... existing body unchanged ...
}
```
(Replace the current `_a: AuthClaims` / `State(_s)` with the named bindings. Keep each handler's existing logic after the guard.)

- [ ] **Step 4: Test** `tests/galvo_hub_admin.rs`:
```rust
mod common;
// token() helper here (see Conventions)

#[tokio::test]
async fn hub_routes_deny_non_admin_and_empty_allowlist() {
    let db = common::start_test_db().await;
    // Empty allowlist (env unset) → everyone denied.
    unsafe { std::env::remove_var("CELEREX_HUB_ADMIN_ENGAGEMENTS"); }
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let eng = uuid::Uuid::new_v4();
    // seed engagement `eng` (mirror tests/engagements.rs seed SQL)
    // ...
    let res = server.post("/api/galvo/hub/status").add_header("authorization", format!("Bearer {}", token(eng))).await;
    assert_eq!(res.status_code(), 403);
}

#[tokio::test]
async fn hub_routes_allow_admin_engagement() {
    let db = common::start_test_db().await;
    let eng = uuid::Uuid::new_v4();
    // seed engagement `eng`
    unsafe { std::env::set_var("CELEREX_HUB_ADMIN_ENGAGEMENTS", &eng.to_string()); }
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    // `status` passes the gate; downstream may fail without a live hub keystore — assert it is NOT 403.
    let res = server.get("/api/galvo/hub/status").add_header("authorization", format!("Bearer {}", token(eng))).await;
    assert_ne!(res.status_code(), 403);
}
```
Adapt: the `status` route's HTTP method (GET per `lib.rs:147`), the seed SQL, and the "passes gate" assertion (use whichever hub route is cheapest to exercise without a live keystore — assert `!= 403`). Because the allowlist is parsed at boot, **build the app AFTER setting the env in each test** (note the env-ordering caveat from Conventions; if env races across tests are a problem, gate these two tests into one `#[tokio::test]` that sets/unsets sequentially).

- [ ] **Step 5: Run** `cargo test --test galvo_hub_admin 2>&1 | tail -20 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt --edition 2024` changed files only.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/galvo_hub_routes.rs packages/backend/api/src/lib.rs packages/backend/api/tests/galvo_hub_admin.rs
git commit -m "feat(agent): gate galvo_hub register/invoke/status behind CELEREX_HUB_ADMIN_ENGAGEMENTS (deny by default)"
```

### Task 2: per-engagement memory scope

**Files:** Modify `src/connectors/memory.rs` (+ `MemoryHandler::invoke` call path if the scope is used there); Test in `src/connectors/memory.rs` (`#[cfg(test)]`) or `tests/memory_scope.rs`

- [ ] **Step 1: Recon.** Read `src/connectors/memory.rs` — `default_scope()` (~:24), `log_run_session(flow_name, engagement_id, run_id, outcome)` (~:150), and `MemoryHandler::invoke` (find where it calls `default_scope()`). Determine whether `MemoryHandler::invoke` has the run's `engagement_id` available in its input; if not, note what it would take to thread it (check the handler's input struct + how it's constructed in `run_job.rs`). Read the `log_run_session` call site `src/project_members/run_job.rs:173` (confirm `engagement_id` in scope).

- [ ] **Step 2: Add the scope helper** in `src/connectors/memory.rs`:
```rust
/// Per-engagement memory scope. Honors CELEREX_MEMORY_SCOPE as an explicit override
/// (deliberately-shared single-tenant deployments); otherwise scopes to the engagement.
pub fn engagement_scope(engagement_id: uuid::Uuid) -> String {
    match std::env::var("CELEREX_MEMORY_SCOPE") {
        Ok(s) if !s.trim().is_empty() => s,
        _ => format!("engagement:{engagement_id}"),
    }
}
```
(Keep `default_scope()` for any non-engagement caller, but `log_run_session` and the Memory node switch to `engagement_scope`.)

- [ ] **Step 3: Use it in `log_run_session`.** Change the `scope` it builds from `default_scope()` to `engagement_scope(engagement_id)` (the fn already receives `engagement_id`).

- [ ] **Step 4: Use it in `MemoryHandler::invoke`.** If the handler has the engagement available, derive `engagement_scope(engagement_id)` there instead of `default_scope()`. If the engagement is NOT currently threaded into the handler input, thread it: add an `engagement_id: Uuid` field to the handler's input/context and set it where the handler is constructed in the run path (mirror how other per-run context reaches connector handlers). If threading is non-trivial / out of proportion, note it in the report and at minimum fix `log_run_session` (the run-history pooling is the primary leak); flag the Memory-node scope as a follow-up. Prefer threading if it's a localized change.

- [ ] **Step 5: Test.** Unit test on the helper + a wiremock capture of `log_run_session`'s POST. Add to `src/connectors/memory.rs` `#[cfg(test)]` (or `tests/memory_scope.rs`):
```rust
#[test]
fn engagement_scope_formats_and_overrides() {
    unsafe { std::env::remove_var("CELEREX_MEMORY_SCOPE"); }
    let e = uuid::Uuid::nil();
    assert_eq!(engagement_scope(e), format!("engagement:{e}"));
    unsafe { std::env::set_var("CELEREX_MEMORY_SCOPE", "project:shared"); }
    assert_eq!(engagement_scope(e), "project:shared");
    unsafe { std::env::remove_var("CELEREX_MEMORY_SCOPE"); }
}
```
For the POST capture: `log_run_session` spawns a `reqwest` POST to `{sidecar_base}/api/Session`. If `sidecar_base_url()` reads an env (e.g. a sidecar base var), point it at a `wiremock::MockServer` in the test, set two different engagement ids, and assert the captured request body's `scope` is `engagement:{uuid}` for each (distinct). If `log_run_session` is fire-and-forget (`tokio::spawn`), the test must await the request landing — mount the mock with an expectation and give the spawned task a moment (e.g. poll the mock's received-requests, bounded). If capturing the spawned POST is awkward, refactor `log_run_session` to build the JSON body via a small pure helper `session_body(flow_name, engagement_id, run_id, outcome) -> serde_json::Value` and unit-test THAT directly (asserting `body["scope"] == format!("engagement:{engagement_id}")`) — the lowest-friction reliable test. Pick the pure-helper path if the spawn makes capture flaky.

- [ ] **Step 6: Run** `cargo test --lib connectors::memory 2>&1 | tail -15 && cargo build --lib 2>&1 | tail -5` (or `cargo test --test memory_scope`) → PASS. `rustfmt --edition 2024` changed files.

- [ ] **Step 7: Commit**
```bash
git add packages/backend/api/src/connectors/memory.rs packages/backend/api/src/project_members/run_job.rs
git commit -m "feat(agent): scope galvo-memory per engagement (engagement:{id}); CELEREX_MEMORY_SCOPE as override"
```

### Task 3: per-install webhook token

**Files:** Create `src/db/migrations/0021_plugin_install_webhook_token.sql`; Modify `src/webhooks/mod.rs`, `src/lib.rs` (route), the plugins create/list API (`src/plugins/…`); Test `tests/webhook_token.rs`

- [ ] **Step 1: Recon.** Read `src/webhooks/mod.rs` (the `ingest` handler ~:9-57, lookup ~:34) + the route `src/lib.rs:96`. Read the plugins API (`src/plugins/routes.rs` + store) — how installs are created + listed, the response shape (to add `webhook_token`). Confirm `pgcrypto` availability (`gen_random_uuid()` is used in `0003_plugins.sql`, implying the extension or pg17 builtin — check whether `gen_random_bytes` is available; if unsure, use the app-backfill fallback in Step 2).

- [ ] **Step 2: Migration** `src/db/migrations/0021_plugin_install_webhook_token.sql`:
```sql
-- Per-install webhook token: fixes cross-tenant misattribution + adds the missing inbound secret.
ALTER TABLE plugin_installs ADD COLUMN webhook_token TEXT;

-- Backfill existing rows with a random token. gen_random_bytes requires pgcrypto;
-- gen_random_uuid() is already used in 0003, so a UUID-derived token is a safe, dependency-free fallback:
UPDATE plugin_installs
   SET webhook_token = replace(gen_random_uuid()::text, '-', '') || replace(gen_random_uuid()::text, '-', '')
 WHERE webhook_token IS NULL;

ALTER TABLE plugin_installs ALTER COLUMN webhook_token SET NOT NULL;
ALTER TABLE plugin_installs ADD CONSTRAINT plugin_installs_webhook_token_key UNIQUE (webhook_token);
```
(Two concatenated dashless UUIDs = a 64-char random hex-ish token, no pgcrypto dependency. If recon confirms `gen_random_bytes` is available and preferred, `encode(gen_random_bytes(24),'hex')` is fine — but the UUID form is dependency-free and sufficient.)

- [ ] **Step 3: Generate a token on install.** In the plugins install/create path, set `webhook_token` for new rows. If inserts are app-driven, generate the token in Rust:
```rust
let webhook_token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
```
and include it in the INSERT. (If the column default-backfills only existing rows, NEW rows still need a value — either add a SQL `DEFAULT` to the column or set it in the app insert. Prefer setting it in the app insert so it's explicit; if a DEFAULT is easier, add `DEFAULT (replace(gen_random_uuid()::text,'-','') || replace(gen_random_uuid()::text,'-',''))` to the column in the migration.)

- [ ] **Step 4: Tokenized inbound route.** In `src/lib.rs:96` change:
```rust
.route("/api/webhooks/{token}/{event}", post(webhooks::ingest))
```
In `src/webhooks/mod.rs` `ingest`, change the path extraction to `(token, event)` and the lookup from the `plugin_id LIMIT 1` query to:
```rust
let install: Option<(Uuid, Uuid, serde_json::Value)> = sqlx::query_as(
    "SELECT id, engagement_id, manifest FROM plugin_installs WHERE webhook_token = $1"
).bind(&token).fetch_optional(&s.pool).await.unwrap_or(None);

let Some((_install_id, engagement_id, manifest)) = install else {
    return (StatusCode::NOT_FOUND, Json(json!({ "error": "unknown webhook" }))).into_response();
};
```
Everything downstream (dedupe key — switch the dedupe key from `plugin_id` to `token` or `install_id`; run creation under `engagement_id`) stays, now keyed to the correct engagement. **Do not log the token.**

- [ ] **Step 5: Surface the token** in the plugins create + list API responses so the owner can build the webhook URL. Add `webhook_token` to the install response struct/JSON (returned only on the engagement-scoped authenticated calls — never broadcast). Adapt to the actual response shape found in recon.

- [ ] **Step 6: Test** `tests/webhook_token.rs`:
```rust
mod common;
// token() helper (for the authenticated plugins API, if needed)

#[tokio::test]
async fn webhook_routes_to_correct_engagement_by_token() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    // Seed engagement A and B (mirror tests/engagements.rs).
    // Insert a plugin_installs row for A with plugin_id="p" and a known webhook_token "tokA",
    // and one for B with the SAME plugin_id="p" and webhook_token "tokB" (direct SQL insert).
    // POST /api/webhooks/tokA/ping  -> assert a row in `runs` with engagement_id == A.
    // POST /api/webhooks/tokB/ping  -> assert a row in `runs` with engagement_id == B.
    // POST /api/webhooks/unknowntok/ping -> 404.
    // (Query `runs` by the inserted run trigger or count per engagement to assert isolation.)
}
```
Flesh out with the real seed SQL + the `runs` assertion (query `SELECT engagement_id FROM runs WHERE ...` after the POST). Assert the cross-tenant property explicitly: A's token never produces a run under B. Also assert the migration left a non-null `webhook_token` on a pre-existing install (insert a row with `webhook_token` omitted before running — but since the column is NOT NULL post-migration, instead assert that an app-created install has a non-empty token via the plugins API).

- [ ] **Step 7: Run** `cargo test --test webhook_token 2>&1 | tail -25 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt --edition 2024` changed files.

- [ ] **Step 8: Commit**
```bash
git add packages/backend/api/src/db/migrations/0021_plugin_install_webhook_token.sql packages/backend/api/src/webhooks/mod.rs packages/backend/api/src/lib.rs packages/backend/api/src/plugins/ packages/backend/api/tests/webhook_token.rs
git commit -m "feat(agent): per-install webhook token — tokenized inbound route, correct-engagement attribution + inbound secret"
```

---

## Final verification
- [ ] `cd "packages/backend/api" && cargo test --test galvo_hub_admin --test webhook_token 2>&1 | tail -15 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS.
- [ ] One Celerex branch `feat/a2-hardening-p1` → PR → squash-merge.

## Self-review (spec coverage)
- 1a galvo_hub admin gate (env allowlist, deny-default, register/invoke/status) → Task 1. ✓
- 1b per-engagement memory scope (`engagement:{uuid}`, CELEREX_MEMORY_SCOPE override, log_run_session + Memory node) → Task 2. ✓
- 1c per-install webhook token (migration 0021, tokenized route, correct-engagement attribution, inbound secret, surfaced in plugins API, removed plugin_id LIMIT 1) → Task 3. ✓
- Tests: deny/allow gate, scope formatting + per-engagement, cross-tenant webhook isolation + unknown-token 404 → Tasks 1-3. ✓
- Token treated as a credential (not logged) → Task 3 Step 4 note. ✓
- RLS explicitly deferred to Phase 2 (NOT in this plan) → spec §7. ✓
