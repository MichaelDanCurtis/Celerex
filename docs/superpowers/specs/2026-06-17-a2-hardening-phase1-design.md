# A2 Multi-Tenancy Hardening — Phase 1 design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repo:** Celerex (Agent — Rust/Axum `celerex-api`, `packages/backend/api`)
**Builds on:** sub-project A (security hardening, merged — fixed the handler-layer IDOR/SSE/run-identity/house-key holes) and the multi-tenancy audit (memory `celerex-multitenancy-audit`). This is the deferred **A2 fast-follow**, Phase 1.

## 1. Purpose

Close the three remaining **contained** cross-tenant holes the audit flagged, mirroring sub-project A's rhythm. Full Postgres RLS (the fourth item) is **deferred to A2 Phase 2** (its own spec) because it requires a DB role/connection change + per-transaction GUC discipline + `FORCE RLS` across ~8 tables — a different blast radius than these three.

## 2. Scope (locked with the user 2026-06-17)

Three Agent-only fixes, one branch + PR:
1. **galvo_hub admin gate** — env allowlist of admin engagements.
2. **Per-engagement galvo-memory scope** — stop pooling all tenants' run history.
3. **Per-install webhook token** — fix cross-tenant misattribution AND add the missing inbound secret.

**Out of scope (Phase 2):** Postgres RLS / store-layer engagement guards. **Reframed out entirely** (per the audit's user notes): per-engagement connector secrets (under tenant=Docker-stack, deployment-global M365 app creds are correct; per-user delegated M365 auth is a separate later concern).

## 3. Current state (recon 2026-06-17, against `main`)

- **galvo_hub** (`src/galvo_hub_routes.rs:50,119`; mounted `lib.rs:146-148`): `register`/`invoke`/`status` take `AuthClaims` but never reference `claims.engagement_id`; they read/write a **node-global keystore file** (`galvo_hub::keystore_path()` → `GALVO_HUB_KEYSTORE` or `./galvo-hub-keys.json`). **No admin/role concept exists anywhere** in the codebase (grep-confirmed).
- **memory scope** (`src/connectors/memory.rs:24`): `default_scope()` → `CELEREX_MEMORY_SCOPE` or `"project:celerex"`. `log_run_session(flow_name, engagement_id, run_id, outcome)` (`:150`) **already has** `engagement_id` but writes the global scope. Called from `src/project_members/run_job.rs:173` with `engagement_id` in scope. The `Memory` connector node (`MemoryHandler::invoke`) also uses `default_scope()`. No other consumers of the scope string.
- **webhooks** (`src/webhooks/mod.rs:34`; route `lib.rs:96` `/api/webhooks/{plugin_id}/{event}`): `ingest` looks up `plugin_installs WHERE plugin_id=$1 LIMIT 1` → takes the **first install across all engagements**, then `INSERT INTO runs (…, engagement_id, …)` with that mis-picked engagement. `plugin_installs` (`0003_plugins.sql`): `id UUID PK`, `engagement_id UUID NOT NULL`, `plugin_id TEXT` (unique only per `(engagement_id, plugin_id)`), **no token/secret column**. The inbound route has **no authentication at all**.
- Next migration number: **0021**. Tests: integration tests under `tests/` (e.g. `webhooks.rs`, `engagements.rs`), `tests/common/mod.rs::start_test_db()` + `axum_test::TestServer`.

## 4. Design

### 4.1 galvo_hub admin gate
- New env **`CELEREX_HUB_ADMIN_ENGAGEMENTS`** = comma-separated engagement UUIDs. Parsed **once at startup** into a `HashSet<Uuid>` stored on `AppState` (mirror how `CELEREX_ALLOWED_ORIGINS` is parsed at boot). Malformed UUIDs are skipped with a `tracing::warn`.
- `register`, `invoke`, AND `status` change from `_a: AuthClaims` to using the claims: if `!admin_engagements.contains(&claims.engagement_id)` → **403 FORBIDDEN** (`"hub admin access required"`).
- **Empty/unset allowlist = deny all** (federation is operator-only until an engagement is explicitly granted). This is the safe default — today any authenticated tenant can act on the node-global keystore.
- No migration, no role table. The "admin" notion lives entirely in env, consistent with `CELEREX_HOUSE_MODE`/`CELEREX_ALLOWED_ORIGINS` from sub-project A.

### 4.2 Per-engagement memory scope
- Add a helper `engagement_scope(engagement_id: Uuid) -> String` → `format!("engagement:{engagement_id}")`.
- `log_run_session` writes `engagement_scope(engagement_id)` instead of `default_scope()`.
- `MemoryHandler::invoke` derives its scope from the run's engagement (the engagement is available in the job/run context — wire it through if not already on the handler input; recon: `run_job.rs` has `engagement_id` at the call site). 
- `CELEREX_MEMORY_SCOPE` is retained as an **optional override** for deliberately-shared single-tenant deployments: if set, it wins (back-compat); if unset, the default is now per-engagement (was `project:celerex`). Document the behavior change.
- Localized: no other consumers of the scope string.

### 4.3 Per-install webhook token
- **Migration `0021_plugin_install_webhook_token.sql`:** add `webhook_token TEXT NOT NULL UNIQUE` to `plugin_installs`, with a default generating a random token for existing rows (e.g. `DEFAULT encode(gen_random_bytes(24), 'hex')` — requires `pgcrypto`, already used via `gen_random_uuid()`; if `gen_random_bytes` isn't available, backfill in a follow-up `UPDATE` with app-generated tokens). Add an index on `webhook_token` (UNIQUE already provides it).
- **Inbound route** changes from `/api/webhooks/{plugin_id}/{event}` to **`/api/webhooks/{token}/{event}`**. `ingest` looks up `plugin_installs WHERE webhook_token=$1` → unambiguous `(id, engagement_id, manifest)` for exactly one install. Unknown token → 404/202 (don't leak existence; mirror current "received" semantics but for a real miss return 404). Dedupe + run-creation logic otherwise unchanged, now keyed to the correct engagement.
- **Surface the token:** the install API (`plugins` create/list) returns `webhook_token`; whoever configures the external webhook uses the tokenized URL. The token doubles as the inbound **secret** (closes the "no auth on inbound webhooks" hole) — it must be treated as a credential (not logged; returned only to the owning engagement's authenticated calls).
- Old `plugin_id`-based lookup removed.

## 5. Testing
- **galvo_hub:** integration test — an engagement NOT in `CELEREX_HUB_ADMIN_ENGAGEMENTS` → 403 on register/invoke/status; an engagement IN the allowlist → passes the gate (mock/stub the hub client so the test asserts the gate, not live federation). Empty allowlist → 403 for everyone.
- **memory scope:** unit test on `engagement_scope`; assert `log_run_session` posts `scope == "engagement:{uuid}"` (inject the sidecar base + capture the request via wiremock, or assert the body the helper builds). Two engagements → two distinct scopes.
- **webhooks:** integration test — two engagements each install the same `plugin_id`; a webhook to engagement A's **token** creates a run under A (not B); a webhook to B's token → run under B; unknown token → 404; migration backfills a token for a pre-existing install. Assert the run's `engagement_id` matches the token's install.
- All stubbed; no live Galvo/sidecar required. `cargo test` + `cargo build --lib` green.

## 6. Build order
Subagent-driven, one Celerex branch `feat/a2-hardening-p1` → PR → squash-merge, each task spec-and-quality reviewed (rust-reviewer — auth-critical). Tasks: T1 galvo_hub admin gate (+ AppState wiring + test); T2 per-engagement memory scope (+ test); T3 webhook token (migration + tokenized route + install-API surfacing + test). Mirrors sub-project A's cadence.

## 7. Deferred → A2 Phase 2 (separate spec)
**Full Postgres RLS.** Requires: a dedicated non-superuser `celerex_app` DB role (the app connects as it; migrations run as owner), `ALTER TABLE … FORCE ROW LEVEL SECURITY` on tenant-data tables (`projects`, `project_members`, `schedules`, `automation_triggers`, `docs`, `doc_blocks`, `runs`, `plugin_installs`, …; NOT identity tables `engagements`/`users` which are queried to establish identity before any engagement context), `CREATE POLICY … USING (engagement_id = current_setting('app.current_engagement')::uuid)`, and a per-transaction `SET LOCAL app.current_engagement` discipline across ALL data access — including background tasks (scheduler, automations poll, webhook executor, the fire-and-forget memory log) which must set the context from the resolved engagement. Staged rollout (log-only/permissive → FORCE) + a per-table cross-engagement test matrix. New k8s secret + Postgres provisioning. Its own spec→plan→build.

See memory `celerex-multitenancy-audit` (the audit + A-complete status) and `omnivyx-architecture` (tenant=Docker-stack identity hierarchy, which is why these Galvo/webhook items remain in scope while connector-secrets was reframed out).
