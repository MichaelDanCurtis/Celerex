# A2 Multi-Tenancy Hardening — Phase 2 (store-layer engagement guards) design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repo:** Celerex (Agent — Rust/Axum `celerex-api`, `packages/backend/api`)
**Builds on:** sub-project A (handler-layer ownership checks, merged), A2 Phase 1 (galvo_hub gate + memory scope + webhook token, PR #53, merged), and the multi-tenancy audit item #10 (memory `celerex-multitenancy-audit`).

## 1. Purpose

Add **defense-in-depth tenant isolation at the store/data layer**: every store method that fetches, mutates, or lists by a **client-influenced id** scopes its query to the caller's `engagement_id`, so a row belonging to another tenant returns **not-found / 0-rows-affected** even if a handler-layer check is ever missed or a new handler forgets one. Sub-project A fixed the *handler* layer (resolve owner → compare to claims); this hardens the layer beneath it.

## 2. Decision (with the user 2026-06-17)

**Store-layer engagement guards, NOT Postgres RLS.** Recon showed the crate runs ~60+ queries directly against the pool with only 2 transactions and per-store `PgPool` clones; true `SET LOCAL` RLS would require a non-superuser role + connection split + threading a request/task-scoped connection through all ~60 sites + every store signature + middleware + background-task GUC plumbing, plus a `FORCE RLS` "silent zero rows" footgun. Store-guards reach the same call sites as **localized, independently-testable `WHERE engagement_id` additions** with none of that machinery and no DB-role/connection change. (If DB-engine enforcement is ever wanted, it becomes a separate future slice on top of this — see Non-goals.)

## 3. Principle & mechanism

**Guard every store method that is id/parent-id addressed AND reachable from an HTTP request.** A guarded method gains an `engagement_id: Uuid` parameter; the handler passes `claims.engagement_id` (already in hand). Reads scope the `WHERE`; `update`/`delete` additionally check rows-affected and return a not-found error on 0 (a cross-tenant id silently no-ops today → becomes an explicit miss).

Per-table mechanism (recon-confirmed reachability):
- **Direct `engagement_id` column** (projects, documents, runs, plugin_installs, engagement_settings): `... AND engagement_id = $n`.
- **FK-hop tables** — add an `EXISTS` subquery over the chain rather than a denormalized column (avoids a data migration + write-path change; these are low-frequency id lookups so the subquery cost is negligible):
  - `project_members` → `projects`: `AND EXISTS (SELECT 1 FROM projects p WHERE p.id = project_members.project_id AND p.engagement_id = $n)`.
  - `schedules` → `project_members` → `projects`: `AND EXISTS (SELECT 1 FROM project_members m JOIN projects p ON p.id = m.project_id WHERE m.id = schedules.project_member_id AND p.engagement_id = $n)`.
  - `automation_triggers` → `project_members` → `projects`: same shape via `automation_triggers.project_member_id`.
  - `doc_blocks` → `documents`: `AND EXISTS (SELECT 1 FROM documents d WHERE d.id = doc_blocks.document_id AND d.engagement_id = $n)`.

## 4. Scope — methods to guard

In scope (id-addressed, request-reachable):
- **project_members store** (`src/project_members/store.rs`): `get`, `update`, `delete` (if present), `list_for_project`.
- **scheduler store** (`src/scheduler/store.rs`): `get`, `update`, `delete`, `list_for_project_member`.
- **automations store** (`src/automations/store.rs`): `get`, `update`, `delete`.
- **docs / doc_blocks** (`src/db/mod.rs`): id-addressed reads/mutations that are not already engagement-scoped (doc get/list-by-id, block append/replace by block/doc id).
- **runs / plugin_installs / reviews**: any `…by id` store read currently filtering by id only (e.g. run-by-id fetch, plugin-install-by-id, the `reviews.rs` id lookups).

Each guarded method gets a **cross-engagement test**: seed engagements A and B; assert A's `engagement_id` cannot `get`/`update`/`delete`/`list` B's rows (→ not-found / 0-rows / empty), while A can still operate on its own.

## 5. Explicitly OUT of scope (deliberately unscoped — system sweeps, not client-addressed)

These process **all** engagements by design and resolve engagement per-row downstream; scoping them would break the schedulers/executors:
- `scheduler::due` / `mark_fired`
- `automations::due` / `mark_polled`
- the run executor / `run_steps` background writer and the memory log writer
- internal helpers `engagement_id_for` / `snapshot_id_for_current` (these RESOLVE engagement; they are the resolver primitives, not client-addressed reads)

The spec names them so reviewers don't flag them as gaps; a comment at each marks it intentional.

## 6. Components / build order

Subagent-driven, one Celerex branch `feat/a2-hardening-p2` → PR → squash-merge, each task spec-and-quality reviewed (rust-reviewer — auth-critical). Tasks grouped by store to keep each reviewable:
- **T1 · project_members** store guards + handler call-site updates + cross-engagement tests.
- **T2 · scheduler** store guards + call-site updates + tests.
- **T3 · automations** store guards + call-site updates + tests.
- **T4 · docs/doc_blocks + runs/plugin_installs/reviews** sweep + tests.

Each task: add the `engagement_id` param + `WHERE`/`EXISTS` scope, update the handler/caller to pass `claims.engagement_id`, add the rows-affected not-found check on mutations, write the cross-engagement test(s), `cargo test` + `cargo build --lib` green, `rustfmt --edition 2024` changed files only.

## 7. Testing

Per guarded method, a cross-engagement assertion (seed A + B, A can't reach B's row → not-found/empty/0-rows; A reaches its own). Reuse `tests/common::start_test_db()` + `axum_test::TestServer`. The background sweeps (§5) keep their existing tests unchanged (they must still see all engagements). No new migration, no new env var, no DB-role/connection change.

## 8. Non-goals

- **No Postgres RLS / DB role / connection split / GUC / `FORCE` / policies** — the dropped true-RLS path (recon §2). A future slice could add DB-engine RLS on the 5 direct-`engagement_id` tables on top of these guards if ever wanted; out of scope here.
- No denormalized `engagement_id` columns on the FK-hop tables (EXISTS subqueries instead).
- No change to the background sweeps' cross-engagement behavior.

See memory `celerex-multitenancy-audit` (audit #10 + A/A2-Phase-1 status) and the A2 Phase 1 spec/PR #53.
