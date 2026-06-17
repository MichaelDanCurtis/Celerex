# Multi-User Security Hardening — sub-project A

**Date:** 2026-06-16
**Status:** Design — awaiting user review before writing-plans
**Repo:** Celerex (Agent — Rust/Axum `celerex-api`, `packages/backend/api/src`)
**Builds on / driven by:** the multi-tenancy audit (see memory `celerex-multitenancy-audit`). The server was built effectively single-tenant ("one house"); these holes are latent today but become live exploits the moment real users log into a public site. This sub-project makes the Agent **safe for multiple users** — the prerequisite before web login (sub-project B) and Copilot OAuth.

## 1. Audience / why now

OmniVyx is becoming a **public, multi-user site**: many independent accounts, one per `engagement_id`. An agent (Galvo etc.) runs **in service of the user who owns the work**. Every API key, connector credential, document, flow, and run must be isolated to its owning account. The audit found that **identity is enforced inconsistently**: the per-user key vault is solid, but a cluster of routes trust client-supplied ids, one stream is unauthenticated, and AI calls silently fall back to a shared operator key. This pass closes those gaps.

## 2. Locked decisions (from brainstorming)

- **Ownership-check everywhere.** Any handler acting on a resource id must verify that resource belongs to `claims.engagement_id` and return **403** on mismatch (404 where existence itself is sensitive). The pattern already exists in `docs::get` and `project_members::delete` — apply it uniformly.
- **An agent runs under the FLOW's owner**, derived from the resource, never from who called or what id they passed.
- **House-key policy:** the env/"house" provider keys go behind an explicit **`CELEREX_HOUSE_MODE` flag, default OFF**. With it off (public site), a named provider with **no vaulted key → hard-fail 409** ("add your key"), never silent spend on the operator's account.
- **Defense-in-depth:** push `engagement_id` into the unscoped store queries so a forgotten handler check can't leak. **Postgres RLS is deferred** (a later belt-and-suspenders).
- **Connector secrets (M365/Galvo) + the shared galvo-memory scope + webhook tenant-attribution are a FAST-FOLLOW** (sub-project A2), not in this pass — they only matter once those connectors/inbound paths are used, and keeping A focused on the login-exposed surface ships safety sooner.
- **Boot safety:** the server must refuse to start without a real `CELEREX_JWT_SECRET` (no `"test-secret"` fallback).

## 3. Key facts from the audit (ground truth — file:line)

- **Unauth stream:** `runs/sse.rs:12` — `GET /api/runs/{run_id}/events` calls `run_bus.subscribe(run_id)` with **no auth extractor**.
- **IDOR handlers** (extract claims, then ignore and act on a client id):
  - `docs/mod.rs:24` `append_block` (`Path(doc_id)`), `docs/mod.rs:36` `replace_block` (`Path(block)`)
  - `project_members/routes.rs` `create` (body `project_id`), `get` (`Path(id)`), `update` (`Path(id)`), `list` (`Path(project_id)`)
  - `plugins/routes.rs:41` `upsert_credentials` (`Path(install)`)
- **Run identity:** `run_manual` (`project_members/routes.rs:178`) calls unscoped `members.get(id)` and passes the **caller's** `engagement_id` into `enqueue_run`. `run_job.rs::apply_doc_engagement` (`run_job.rs:100-109`) **defers** to an author-supplied `engagement_id` in node config instead of overwriting; `llm/handler.rs:88-93` then trusts it. The scheduler path does it correctly via `engagement_id_for(member)` (the helper to reuse).
- **House keys:** built from env at `lib.rs:167-185`; fallback in `llm/handler.rs:110-129`. The design proxy already does the correct hard-fail: `design/proxy.rs:49,51` (`provider_keys.get(claims.engagement_id, …)` → 409 if absent).
- **Workspace assistant:** `assistant/mod.rs:37,65` uses `state.providers_registry` directly with **no engagement key lookup**; confirm it requires `AuthClaims`.
- **Open account endpoints:** `engagements::list` (`db/mod.rs:25`, `GET /api/engagements`) returns all tenants, no auth; `engagements::create` no auth.
- **Unscoped store methods:** `project_members/store.rs:138` `get`, `:161` `list_for_project`, `:197` `update`; `scheduler/store.rs` `get`; `automations/store.rs` `get` — all `WHERE id = $1` only.
- **JWT fallback:** `auth/jwt.rs:46,62` falls back to literal `"test-secret"` if `CELEREX_JWT_SECRET` unset.
- **Ownership-check precedent to copy:** `docs::get`, `project_members::delete`, and `scheduler`/`automations` HTTP handlers all resolve the target's engagement then compare to claims → 403.

## 4. Components / work units

### Unit 1 — Ownership & auth checks on exposed routes
A small reusable guard plus per-handler application.

- **Add a guard helper.** A function the handlers call after resolving a resource's engagement, e.g.:
  ```rust
  // returns Err((403, "...")) when the resource isn't the caller's
  fn ensure_owned(resource_engagement: Uuid, claims_engagement: Uuid) -> Result<(), (StatusCode, String)>;
  ```
  For resources reachable only by an id, add/inu­se a store method that returns the resource's `engagement_id` (or the row) so the handler can compare. Where a store method is unscoped today, prefer making the **store query itself** take `engagement_id` (see Unit 4) so the check is structural.
- **`GET /api/runs/{run_id}/events`** — add the auth extractor (`AuthClaims`), look up the run's `engagement_id`, `ensure_owned(...)`, then `run_bus.subscribe`. Deny (404) unknown/again-not-owned run ids.
- **The 7 IDOR handlers** — `docs::append_block`/`replace_block`, `project_members::create`/`get`/`update`/`list`, `plugins::upsert_credentials`: resolve the target's engagement (the document's, the project's, the member's, the install's) and `ensure_owned` before the read/write. For `create`, verify the body `project_id` belongs to the caller.
- **`engagements::list`** — scope to the caller (return only the caller's engagement) or move behind an admin guard; **`engagements::create`** — require `AuthClaims` (or restrict to the signup path that already creates engagements).

### Unit 2 — Run/execution identity
- **`run_manual`** — derive the engagement from the **member** (`engagement_id_for(member)`), not the caller; `ensure_owned` (the member must belong to the caller) → 403 on mismatch; pass the member's engagement into `enqueue_run`.
- **`apply_doc_engagement`** (`run_job.rs`) — **overwrite** the `engagement_id` on every `doc`/`llm` node from the trusted run engagement; never defer to an author-supplied value.
- Result: a run always executes under its true owner's engagement, resolving that owner's vaulted keys and writing to that owner's data.

### Unit 3 — House-key policy
- **Gate the env house registry behind `CELEREX_HOUSE_MODE`** (env flag, default **false**). When false, the `providers_registry` fallback is not used for real provider calls.
- **`EngagementLlmHandler::invoke`** — when house mode is off and the engagement has no vaulted key for the named provider, return a clear **409 "no key configured"** (mirror `design/proxy.rs:51`) instead of falling back.
- **Workspace assistant** (`assistant/mod.rs`) — route through the engagement-scoped key resolution (look up `provider_keys.get(claims.engagement_id, …)`); ensure the chat endpoints require `AuthClaims`. In house mode off + no key → the same 409.
- House mode on (single-tenant/dev) preserves today's behavior, so local dev and your own "house" deployment still work.

### Unit 4 — Store-layer tenant scoping (defense-in-depth)
- Add `engagement_id` to the unscoped store queries so a forgotten handler check cannot leak:
  - `project_members::get`/`update`/`list_for_project` → add `engagement_id` (via the `project_members→projects→engagements` chain) to the WHERE/join, and a not-found/forbidden result when it doesn't match.
  - `scheduler::get`, `automations::get` → same.
- Handlers pass `claims.engagement_id` into these methods. This makes Unit 1's checks structural rather than relying on every caller remembering.

### Unit 5 — Boot safety
- **`auth/jwt.rs`** — require `CELEREX_JWT_SECRET`; if unset, **fail to start** (or return a hard error) rather than signing/verifying with `"test-secret"`. Keep a test-only override gated behind `#[cfg(test)]` so the test suite still runs.

## 5. Testing strategy

Each fix becomes a **cross-tenant denial test** (turning a hole into a regression guard). Using the existing testcontainers + auth test harness (`tests/auth.rs` pattern — signup two engagements, get two tokens):

- **Per IDOR handler:** user A creates a resource; user B (different engagement) attempts read/write by id → asserts **403/404**, and A still succeeds (no regression).
- **SSE:** unauthenticated `GET /api/runs/{id}/events` → 401/403; B for A's run → 403; A for A's run → streams.
- **run_manual:** B attempts to run A's member → 403; A's run executes under A's engagement (assert the `runs` row + key resolution use A).
- **House mode:** with `CELEREX_HOUSE_MODE` off and no vaulted key, an LLM/assistant call → 409 (not a silent house-key success); with a vaulted key → succeeds; with house mode on → falls back (preserved).
- **engagements::list:** returns only the caller's engagement (or 403 without admin).
- **Boot:** unset `CELEREX_JWT_SECRET` → startup error (test the secret-loader function).

All tests use stubs/test Postgres; no external provider calls.

## 6. Non-goals / deferred (sub-project A2 fast-follow)

- **Per-engagement connector secrets** — M365 (`AZURE_*`) / Galvo (`GALVO_*`) resolved from the vault at runtime instead of global env (`run_job.rs:94` only injects `doc`/`llm` nodes today).
- **Per-engagement galvo-memory scope** — replace the single shared `CELEREX_MEMORY_SCOPE` (`connectors/memory.rs:25`).
- **Webhook tenant attribution** — `plugin_installs WHERE plugin_id=$1 LIMIT 1` (`webhooks/mod.rs:34`) misattributes across engagements; route by install id / per-engagement token.
- **Postgres RLS** on the `project_members→projects→engagements` chain.
- **Web login + per-user settings store** — that's **sub-project B** (next), not here.

## 7. Build order

Subagent-driven, branch + PR + merge per stage, every task spec-and-quality reviewed (the pattern used for 2h). Suggested ordering: **Unit 5 (boot safety, trivial) → Unit 4 (store scoping, enables 1) → Unit 1 (ownership checks) → Unit 2 (run identity) → Unit 3 (house-key policy)**, each landing with its cross-tenant denial tests. Then sub-project B (web login + settings), then Copilot.

See memory `celerex-multitenancy-audit`, `omnivyx-architecture` (web-vs-desktop + state-location), `celerex-design-handoff`.
