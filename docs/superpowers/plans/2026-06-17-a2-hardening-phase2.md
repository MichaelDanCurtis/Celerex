# A2 Multi-Tenancy Hardening — Phase 2 (store-layer engagement guards) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add defense-in-depth tenant isolation at the store layer — every id/parent-id-addressed, request-reachable store method scopes its query to the caller's `engagement_id`, so a cross-tenant id returns not-found / 0-rows.

**Architecture:** Add an `engagement_id: Uuid` parameter to the vulnerable store methods (and their trait signatures), scope the SQL (`AND engagement_id = $n` for direct-column tables; `AND EXISTS (...)` over the FK chain otherwise), make `update`/`delete` return a not-found error when 0 rows match, and pass `claims.engagement_id` from the handler call sites. No migration, no RLS, no new env. Background sweeps stay global by design.

**Tech Stack:** Rust/Axum (`celerex-api`), sqlx/Postgres, `axum_test::TestServer` + `tests/common::start_test_db()`.

## Global Constraints

- Run from `cd "packages/backend/api"`. Format ONLY changed files: `rustfmt --edition 2024 <file>` (plain rustfmt defaults to edition 2015 and chokes on `async fn`; revert any pre-existing-drift reformatting in files you touch — the repo is not edition-2024-formatted, so keep only your logical edits).
- Tests run under the **default parallel harness** (do NOT add `--test-threads=1`). Use `tests/common::start_test_db()` + `axum_test::TestServer::new(celerex_api::build_app(db.pool.clone()))`. Mint per-engagement tokens with the existing helper `celerex_api::auth::jwt::mint_for_tests(engagement_id)` (used in `tests/engagements.rs`); `start_test_db()` already sets `CELEREX_JWT_SECRET` + `CELEREX_CRED_KEY`.
- Changing a trait method signature cascades: update (1) the trait definition, (2) the `Pg…Store` impl, (3) ANY other impl (test fakes/mocks — grep for `impl <Trait> for`), (4) ALL call sites. Each task's Step 1 greps for these before editing.
- **OUT OF SCOPE — leave global, add a one-line `// intentional: system-wide sweep, resolves engagement per-row` comment where you might otherwise scope:** `scheduler::due`/`mark_fired`, `automations::due`/`mark_polled`, the run executor + `run_steps` writer (`run_job.rs`/`runs/mod.rs`), the memory log writer, and the resolver helpers `engagement_id_for`/`snapshot_id_for_current` (these RESOLVE engagement; they are primitives, not client-addressed reads).
- Mutation guard pattern: after an `UPDATE … WHERE id=$1 AND <scope>` / `DELETE …`, check the affected/returned rows; if none, return the store's not-found error variant (don't silently succeed).
- FK-hop EXISTS chains (use verbatim, binding the engagement as the scope param):
  - project_members→projects: `AND EXISTS (SELECT 1 FROM projects p WHERE p.id = project_members.project_id AND p.engagement_id = $E)`
  - schedules→…: `AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = schedules.project_member_id AND p.engagement_id = $E)`
  - automation_triggers→…: `AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = automation_triggers.project_member_id AND p.engagement_id = $E)`
  - doc_blocks→documents: `AND EXISTS (SELECT 1 FROM documents d WHERE d.id = doc_blocks.document_id AND d.engagement_id = $E)`
  - Direct-column tables (projects, documents, runs, plugin_installs, engagement_settings): `AND engagement_id = $E`.

---

# Stage 1 — Agent (branch `feat/a2-hardening-p2`)

### Task 1: project_members store guards

**Files:** Modify `src/project_members/store.rs` (trait `MembersStore` + `PgMembersStore`), `src/project_members/routes.rs` (call sites), any other `impl MembersStore`; Test `tests/members_scope.rs`

**Interfaces:**
- Produces: `MembersStore::get(&self, id: Uuid, engagement_id: Uuid) -> Result<Option<Member>, String>`; `update(&self, id: Uuid, engagement_id: Uuid, …existing args…) -> Result<Member, String>` (Err on cross-tenant/no-match); `delete(&self, id: Uuid, engagement_id: Uuid) -> Result<(), String>` (if a delete exists; Err/NotFound on 0 rows); `list_for_project(&self, project_id: Uuid, engagement_id: Uuid) -> Result<Vec<Member>, String>`. `engagement_id_for`/`snapshot_id_for_current`/`create` UNCHANGED.

- [ ] **Step 1: Recon.** `grep -rn "impl MembersStore\|\.get(\|\.update(\|\.list_for_project(\|members\." src/` to find the trait, all impls (incl. test fakes), and every call site. Read `src/project_members/store.rs` `get`/`update`/`list_for_project` (id-only today) + `routes.rs` handlers (they have `AuthClaims(claims)` → `claims.engagement_id`). Note the not-found error convention the store uses.

- [ ] **Step 2: Write failing test** `tests/members_scope.rs`:
```rust
mod common;
use celerex_api::auth::jwt::mint_for_tests; // adapt to the real helper name found in tests/engagements.rs

#[tokio::test]
async fn members_get_update_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    // Seed engagement A + a project + a member under A (mirror tests/engagements.rs / project_members tests for the seed path —
    // prefer creating via the authenticated API with token(A) so ids are real).
    // Seed engagement B + token(B).
    // 1. GET the member by id with token(B) -> 404 (B can't see A's member).
    // 2. GET the same member with token(A) -> 200.
    // 3. PUT/update the member with token(B) -> 404 (no cross-tenant mutation).
    // 4. list members for A's project with token(B) -> 404/empty (can't enumerate A's project).
}
```
Use the project_members HTTP routes for the assertions (the handlers call the store; this proves the store guard end-to-end). Read the existing `tests/` project_members/run tests for the exact seed + route shapes.

- [ ] **Step 3: Run red** `cargo test --test members_scope 2>&1 | tail -20`.

- [ ] **Step 4: Add `engagement_id` to the trait + impl.** In `src/project_members/store.rs`:
  - Trait `MembersStore`: add `engagement_id: Uuid` to `get`, `update`, `list_for_project` (and `delete` if present).
  - `PgMembersStore::get`: `... WHERE id = $1 AND EXISTS (SELECT 1 FROM projects p WHERE p.id = project_members.project_id AND p.engagement_id = $2)`.
  - `list_for_project`: `... WHERE project_id = $1 AND EXISTS (SELECT 1 FROM projects p WHERE p.id = $1 AND p.engagement_id = $2)` (scopes the parent project to the engagement; returns empty if the project isn't the caller's).
  - `update`: add the same `AND EXISTS(...)` to the `UPDATE … WHERE id=$1` and return the not-found error if the update matched 0 rows (the `RETURNING` yields no row → map to NotFound rather than a generic error).
  - Update any test-fake impl found in Step 1 to match the new signatures.

- [ ] **Step 5: Update call sites** in `src/project_members/routes.rs` (and anywhere else from Step 1) to pass `claims.engagement_id`. The scheduler/automation/run paths that call `members.get(...)` for resolution should pass the engagement they already resolve — BUT if a background sweep calls `members.get` without a request engagement, use `engagement_id_for` (the unscoped resolver) there instead, or pass the resolved engagement; do NOT break the sweeps (see Global Constraints).

- [ ] **Step 6: Run** `cargo test --test members_scope 2>&1 | tail -20 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt --edition 2024` changed files.

- [ ] **Step 7: Commit**
```bash
git add packages/backend/api/src/project_members/store.rs packages/backend/api/src/project_members/routes.rs packages/backend/api/tests/members_scope.rs
git commit -m "feat(agent): engagement-scope project_members store reads/mutations (defense-in-depth)"
```

### Task 2: scheduler store guards

**Files:** Modify `src/scheduler/store.rs` (trait `SchedulesStore` + `PgSchedulesStore`), call sites (scheduler routes / design rituals), any other impl; Test `tests/schedules_scope.rs`

**Interfaces:**
- Produces: `SchedulesStore::get(&self, id, engagement_id) -> Result<Option<Schedule>,String>`; `update(&self, id, engagement_id, …) -> Result<…>` (NotFound on cross-tenant); `delete(&self, id, engagement_id) -> Result<…>` (NotFound on 0 rows); `list_for_project_member(&self, project_member_id, engagement_id) -> Result<Vec<Schedule>,String>`. `due`/`mark_fired`/`create` UNCHANGED (global sweep).

- [ ] **Step 1: Recon.** `grep -rn "impl SchedulesStore\|schedules\.\|\.due(\|mark_fired" src/` — find the trait, impls, and call sites (request-path vs the SchedulerLoop background tick). Read `src/scheduler/store.rs`. Confirm `due`/`mark_fired` are ONLY called from the background loop (leave them global).

- [ ] **Step 2: Write failing test** `tests/schedules_scope.rs` — seed engagement A with a project_member + a schedule (via the rituals/schedule API with token(A)); seed B + token(B). Assert: GET schedule by id with token(B) → 404; with token(A) → 200; update/delete with token(B) → 404; list_for_project_member of A's member with token(B) → 404/empty. Use the schedule/rituals HTTP routes.

- [ ] **Step 3: Run red** `cargo test --test schedules_scope 2>&1 | tail -20`.

- [ ] **Step 4: Scope the methods.** Add `engagement_id` to `get`/`update`/`delete`/`list_for_project_member` (trait + impl). Use the schedules→project_members→projects EXISTS chain (Global Constraints). `update`/`delete` return NotFound on 0 affected rows. Leave `due`/`mark_fired` untouched with the intentional comment.

- [ ] **Step 5: Update call sites** (scheduler routes / `design/rituals.rs` — whatever invokes `get`/`update`/`delete`/`list_for_project_member` from a request) to pass `claims.engagement_id`. Background tick keeps calling `due`/`mark_fired` unchanged.

- [ ] **Step 6: Run** `cargo test --test schedules_scope 2>&1 | tail -20 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt` changed files.

- [ ] **Step 7: Commit**
```bash
git add packages/backend/api/src/scheduler/store.rs packages/backend/api/src/scheduler/ packages/backend/api/src/design/rituals.rs packages/backend/api/tests/schedules_scope.rs
git commit -m "feat(agent): engagement-scope scheduler store reads/mutations (defense-in-depth)"
```

### Task 3: automations store guards

**Files:** Modify `src/automations/store.rs` (trait `AutomationTriggersStore` + `PgAutomationTriggersStore`), `src/design/automations.rs` call sites, any other impl; Test `tests/automations_scope.rs`

**Interfaces:**
- Produces: `AutomationTriggersStore::get(&self, id, engagement_id) -> Result<Option<AutomationTrigger>,String>`; `update(&self, id, engagement_id, …) -> Result<…>` (NotFound cross-tenant); `delete(&self, id, engagement_id) -> Result<…>` (NotFound on 0). `due`/`mark_polled`/`create`/`list_for_engagement` (already engagement-scoped) UNCHANGED.

- [ ] **Step 1: Recon.** `grep -rn "impl AutomationTriggersStore\|triggers\.\|automations" src/` — trait, impls, call sites; confirm `due`/`mark_polled` are background-only and `list_for_engagement` is already scoped. Read `src/automations/store.rs` + `src/design/automations.rs`.

- [ ] **Step 2: Write failing test** `tests/automations_scope.rs` — seed A with a project_member + an automation trigger (via the automations API with token(A)); seed B + token(B). Assert GET/update/delete by id with token(B) → 404; with token(A) → 200/ok. Use the automations HTTP routes.

- [ ] **Step 3: Run red** `cargo test --test automations_scope 2>&1 | tail -20`.

- [ ] **Step 4: Scope the methods.** Add `engagement_id` to `get`/`update`/`delete` (trait + impl), using the automation_triggers→project_members→projects EXISTS chain. `update`/`delete` return NotFound on 0 rows. Leave `due`/`mark_polled` global (intentional comment); `list_for_engagement` already scoped — leave.

- [ ] **Step 5: Update call sites** in `src/design/automations.rs` (request handlers) to pass `claims.engagement_id`.

- [ ] **Step 6: Run** `cargo test --test automations_scope 2>&1 | tail -20 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt` changed files.

- [ ] **Step 7: Commit**
```bash
git add packages/backend/api/src/automations/store.rs packages/backend/api/src/design/automations.rs packages/backend/api/tests/automations_scope.rs
git commit -m "feat(agent): engagement-scope automations store reads/mutations (defense-in-depth)"
```

### Task 4: docs/doc_blocks + runs/plugin_installs/reviews sweep

**Files:** Modify `src/db/mod.rs` (doc + doc_block + runs + plugin_installs id-addressed fns), `src/reviews.rs`, the call sites in `src/docs/`, `src/runs/`, `src/plugins/`; Test `tests/store_scope_sweep.rs`

**Interfaces:**
- Produces: each id-addressed db fn gains `engagement_id: Uuid` and scopes (direct column for documents/runs/plugin_installs; doc_blocks via the documents EXISTS chain). Mutations return NotFound on 0 rows.

- [ ] **Step 1: Recon.** This is a SWEEP — enumerate precisely first. `grep -rn "WHERE id = \$1\|WHERE .*_id = \$1" src/db/mod.rs src/reviews.rs` and list every id-addressed fn that is NOT already engagement-scoped and IS reachable from a request handler (cross-check callers in `src/docs/`, `src/runs/routes.rs`, `src/plugins/`, `src/reviews.rs`). Build the exact list before editing. Sub-project A already fixed several doc/doc-block handlers at the HANDLER layer — here we scope the STORE/db fns beneath them. EXCLUDE the background writers (`run_steps` insert, etc. — Global Constraints). If a fn is already scoped (e.g. some docs fns from A), skip it and note so in your report.

- [ ] **Step 2: Write failing test** `tests/store_scope_sweep.rs` — for each guarded surface (a doc by id, a doc_block append/replace, a run by id, a plugin_install by id, a review by id), seed under A, attempt access with token(B) → 404, with token(A) → ok. Group as several `#[tokio::test]` fns (one per surface) using the real HTTP routes.

- [ ] **Step 3: Run red** `cargo test --test store_scope_sweep 2>&1 | tail -20`.

- [ ] **Step 4: Scope the fns.** For each enumerated fn: add `engagement_id`, scope the SQL (direct `AND engagement_id = $E` for documents/runs/plugin_installs; the doc_blocks→documents EXISTS chain for block ops), mutations return NotFound on 0 rows. Update call sites to pass `claims.engagement_id`. Be surgical — only the fns from Step 1's list.

- [ ] **Step 5: Run** `cargo test --test store_scope_sweep 2>&1 | tail -25 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt` changed files.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/db/mod.rs packages/backend/api/src/reviews.rs packages/backend/api/src/docs/ packages/backend/api/src/runs/ packages/backend/api/src/plugins/ packages/backend/api/tests/store_scope_sweep.rs
git commit -m "feat(agent): engagement-scope docs/doc_blocks/runs/plugin_installs/reviews store reads (defense-in-depth)"
```

---

## Final verification
- [ ] `cd "packages/backend/api" && cargo test --test members_scope --test schedules_scope --test automations_scope --test store_scope_sweep 2>&1 | tail -20 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS (default parallel harness). Existing scheduler/automation background-sweep tests still green (they must still see all engagements).
- [ ] One Celerex branch `feat/a2-hardening-p2` → PR → squash-merge.

## Self-review (spec coverage)
- project_members get/update/delete/list_for_project scoped → Task 1. ✓
- scheduler get/update/delete/list_for_project_member scoped → Task 2. ✓
- automations get/update/delete scoped → Task 3. ✓
- docs/doc_blocks + runs/plugin_installs/reviews id-addressed reads scoped → Task 4. ✓
- FK-hop EXISTS chains (project_members/schedules/automation_triggers/doc_blocks) + direct-column scoping → Global Constraints, applied per task. ✓
- update/delete return not-found on cross-tenant 0-rows → every task Step 4. ✓
- Background sweeps + resolver helpers deliberately untouched (intentional comments) → Global Constraints. ✓
- Cross-engagement test per guarded method → every task. ✓
- No migration / no env / no RLS / no DB-role-connection change → none added. ✓
