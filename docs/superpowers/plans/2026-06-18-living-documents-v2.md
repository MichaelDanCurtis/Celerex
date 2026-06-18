# Living Documents v2 (recurring fill via Ritual) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Living Document's publish scaffold (trigger→fill→render→review→deliver) runs unattended on a schedule — a Ritual fires it on a cadence, the slots re-fill with fresh content, and the rendered doc is produced automatically.

**Architecture:** A new `slot_fill` executor node makes the scaffold's placeholder fill node real (reusing v1's per-slot completion via a shared fn); the `review` node auto-approves on scheduled runs (manual stays gated); a `schedule` convenience endpoint attaches a Ritual to the template's existing flow member. The scheduler already fires `enqueue_run(member)` on any graph — no new scheduler plumbing.

**Tech Stack:** Rust/Axum (`celerex-api`) + the `celerex-orchestrator` crate (ExecutionType/Handler/graph exec), sqlx/Postgres, `axum_test::TestServer` + testcontainers; Next.js web (`apps/web`), vitest.

## Global Constraints

- Agent: `cd "packages/backend/api"`; web: `cd "apps/web"` in celerex-design. Format ONLY changed files: `rustfmt --edition 2024 <file>` (revert pre-existing-drift reformatting — the repo is not edition-2024-formatted).
- **After ANY Rust signature/enum change run the FULL `cargo test` (all binaries)** — adding an `ExecutionType` variant or changing `scaffold_live_flow`'s signature can break sibling test binaries (the cascade lesson). Grep `tests/` + the orchestrator crate for callers.
- Agent routes use `DeviceAuth` → `claims.engagement_id: Uuid`; the schedule endpoint reuses v1's `own_template` (404 cross-tenant) before creating anything.
- Tenant safety: the `slot_fill` node MUST fill only the run's injected `engagement_id`'s template. `apply_doc_engagement` injects `engagement_id`; the executor reads it from config (never trusts a request-supplied engagement). Review auto-approve must apply ONLY to scheduled runs — manual/interactive runs keep the human gate.
- **DRY:** v1's per-slot completion logic is extracted into ONE shared fn used by both the HTTP endpoint (`living::fill_all`) and the executor — do not duplicate the completion loop.
- Fill = one-shot completion (v1's `run_fill`); not a sub-flow. v3 (real external delivery) is OUT OF SCOPE — v2's `deliver` writes the Agent doc only.
- Next migration: **0023** (likely none needed).

---

# Stage 1 — Agent (branch `feat/living-docs-v2-agent`)

### Task 1: `slot_fill` executor + shared fill fn + scaffold wiring

**Files:** Modify `packages/backend/agent-orchestrator/src/...` (the `ExecutionType` enum + its exec-string mapping), `packages/backend/api/src/design/living.rs` (extract shared fn) , `src/design/scaffold.rs` (fill node), `src/design/mod.rs` (`ensure_delivery_flow` passes template_id; `pub mod slot_fill_handler;`), `src/project_members/run_job.rs` (`apply_doc_engagement` match), `src/lib.rs` (register handler); Create `src/design/slot_fill_handler.rs`; Test `tests/living_docs_v2_executor.rs`, `tests/living_docs_v2_scheduled.rs`

**Interfaces:**
- Produces: `pub(crate) async fn living::fill_template_slots(pool: &PgPool, provider_keys: &Arc<dyn ProviderKeysStore>, registry: &Arc<ProviderRegistry>, engagement_id: Uuid, template_id: Uuid) -> Result<Vec<(String, String)>, String>` (slot_name, value pairs; persists each `last_value`); `ExecutionType::SlotFill`; `SlotFillHandler`.

- [ ] **Step 1: Recon (decisive).** Find (a) the `ExecutionType` enum in `agent-orchestrator` + how a node `config.execution` string maps to a variant (the match/`from_str`) — you'll add `SlotFill`/`"slot_fill"`. (b) The `Handler` trait (`invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError>` — exact name/path/error type) + `DocHandler`/`MemoryHandler` invoke + how they're constructed at `src/lib.rs:228-251`. (c) **The render node's expected input shape**: read the `render` executor (ExecutionType::Render handler) — it substitutes `{{slot:name}}` from `steps.fill.outputs`; determine EXACTLY what it reads (`steps.fill.outputs.data` as a `{slot:value}` object? or `steps.fill.outputs` directly?). The `slot_fill` output MUST match that shape. (d) `run_fill` + `fill_all` internals in `living.rs` and what `resolve_engagement_provider` + the house fallback need (pool / `state.provider_keys` / `state.providers_registry`). (e) `scaffold_live_flow` current fill+render nodes + `ensure_delivery_flow` (has `template_id` in scope).

- [ ] **Step 2: Add the `ExecutionType::SlotFill` variant** + its `"slot_fill"` string mapping in the orchestrator crate (mirror an existing variant like `Render`/`Doc`). Build the orchestrator crate.

- [ ] **Step 3: Extract the shared fill fn.** In `src/design/living.rs`, refactor `run_fill` + the `fill_all` loop into `pub(crate) async fn fill_template_slots(pool, provider_keys, registry, engagement_id, template_id) -> Result<Vec<(String,String)>, String>` that: lists `slot_fills::list_slot_fills(pool, template_id)`, for each runs the one-shot completion (the existing `run_fill` body, but taking `pool`+`provider_keys`+`registry` instead of `&AppState` — call `resolve_engagement_provider`'s underlying logic; if that needs AppState, extract ITS core too OR pass the 3 fields), persists `slot_fills::set_value`, and returns the `(slot,value)` pairs. Update `living::fill_all` (the HTTP handler) to call `fill_template_slots(&state.pool, &state.provider_keys, &state.providers_registry, claims.engagement_id, template_id)` then shape its JSON response from the returned pairs. **No behavior change to the v1 endpoint** — its existing tests (`tests/living_docs_fill.rs`) must still pass.

- [ ] **Step 4: `SlotFillHandler`** `src/design/slot_fill_handler.rs`:
```rust
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;
use sqlx::PgPool;
// import the orchestrator Handler trait + DispatchError (path per recon), ProviderKeysStore, ProviderRegistry

pub struct SlotFillHandler {
    pool: PgPool,
    provider_keys: Arc<dyn crate::providers::store::ProviderKeysStore>, // adapt path
    registry: Arc<celerex_orchestrator::llm::registry::ProviderRegistry>, // adapt path
}
impl SlotFillHandler {
    pub fn new(pool: PgPool, provider_keys: Arc<dyn crate::providers::store::ProviderKeysStore>, registry: Arc<celerex_orchestrator::llm::registry::ProviderRegistry>) -> Self {
        Self { pool, provider_keys, registry }
    }
}
#[async_trait]
impl Handler for SlotFillHandler { // adapt trait path
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let template_id = config.get("template_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| /* DispatchError ctor */ dispatch_err("slot_fill: missing template_id"))?;
        let engagement_id = config.get("engagement_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| dispatch_err("slot_fill: missing engagement_id"))?;
        let pairs = crate::design::living::fill_template_slots(&self.pool, &self.provider_keys, &self.registry, engagement_id, template_id)
            .await.map_err(|e| dispatch_err(&format!("slot_fill: {e}")))?;
        let mut data = serde_json::Map::new();
        for (slot, value) in pairs { data.insert(slot, json!(value)); }
        Ok(json!({ "data": data })) // MATCH the render node's expected shape from recon (data:{slot:value})
    }
}
```
Replace `dispatch_err`/trait/paths with the real ones from recon. Register in `src/lib.rs` beside the others:
```rust
dispatcher.register(ExecutionType::SlotFill, Arc::new(crate::design::slot_fill_handler::SlotFillHandler::new(pool.clone(), provider_keys.clone(), providers_registry.clone())));
```
Add `pub mod slot_fill_handler;` to `src/design/mod.rs`.

- [ ] **Step 5: Inject engagement into slot_fill nodes.** In `src/project_members/run_job.rs` `apply_doc_engagement`, add `"slot_fill"` to the exec match (`exec != Some("doc") && ... && exec != Some("slot_fill")`) so the fill node receives `engagement_id`.

- [ ] **Step 6: Wire the scaffold fill node.** In `src/design/scaffold.rs`, change `scaffold_live_flow` to take `template_id: Uuid` and bake the fill node as:
```rust
{ "id": "fill", "type": "action", "config": { "execution": "slot_fill", "template_id": template_id.to_string() } }
```
Update `ensure_delivery_flow` (`src/design/mod.rs`) to pass `template_id`. Confirm the `render` node still reads `{{steps.fill.outputs}}` and the shape matches Step 4's output (recon Step 1c).

- [ ] **Step 7: Tests.**
  - `tests/living_docs_v2_executor.rs`: build a minimal graph with a `slot_fill` node (template_id of a seeded template with an instruction'd slot) + a wiremock provider (mirror `tests/living_docs_fill.rs` stubbing); run it via `enqueue_run` (or invoke the handler directly with config carrying template_id+engagement_id); assert the slot's `last_value` persisted AND the node output is `{data:{slot:value}}`.
  - `tests/living_docs_v2_scheduled.rs`: mirror `tests/scheduler_dispatch.rs` — seed a live template + its scaffold member (fill node = slot_fill, with a slot instruction) + a due `schedule`; run the dispatch closure (or `enqueue_run(..., TRIGGERED_BY_SCHEDULED)`); poll until the run completes; assert fill→render→deliver produced a delivered doc whose content contains the filled slot value substituted into the template HTML. (This depends on T2's review auto-approve to not stall — if T2 isn't done yet, this test may need the review node temporarily absent or the auto-approve stubbed; note the dependency and prefer ordering T2 before this assertion, OR assert up to the review gate here and complete the full end-to-end assertion after T2.)
- [ ] **Step 8: Run** `cargo test --test living_docs_v2_executor 2>&1 | tail -20 && cargo test --test living_docs_fill 2>&1 | tail -6 && cargo build --lib 2>&1 | tail -5` then the FULL `cargo test 2>&1 | tail -20` (enum/signature cascade). `rustfmt` changed files.

- [ ] **Step 9: Commit**
```bash
git add packages/backend/agent-orchestrator/src/ packages/backend/api/src/design/living.rs packages/backend/api/src/design/slot_fill_handler.rs packages/backend/api/src/design/scaffold.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/project_members/run_job.rs packages/backend/api/src/lib.rs packages/backend/api/tests/living_docs_v2_executor.rs packages/backend/api/tests/living_docs_v2_scheduled.rs
git commit -m "feat(agent): slot_fill executor — scaffold fill node fills template slots (shared fill fn)"
```

### Task 2: review auto-approve on scheduled runs

**Files:** Modify the review executor (recon for its location — likely `src/...` or the orchestrator), and possibly `src/project_members/run_job.rs` (inject an `auto_approve` flag); Test `tests/living_docs_v2_review.rs`

**Interfaces:**
- Consumes: the run's `triggered_by` (`scheduled` vs `manual`).
- Produces: a review executor that passes content through (approved) on scheduled runs, gates on manual.

- [ ] **Step 1: Recon.** Find the `review` executor (`ExecutionType::Review` / `execution:"review"` handler) — how it currently gates (does it pause the run / require a separate approve call / emit a pending status?). Find how `triggered_by` is known at run time: is it in scope where the graph is compiled/executed (`enqueue_run` has `triggered_by`), and does it reach executors? If NOT reachable inside the executor, the cleanest path is to **inject an `auto_approve: true` flag into review-node configs when `triggered_by == scheduled`** — parallel to `apply_doc_engagement` (add an `apply_scheduled_auto_approve(graph, triggered_by)` that sets `auto_approve` on `execution:"review"` nodes when scheduled). Pick the lower-friction path; report which.

- [ ] **Step 2: Write failing test** `tests/living_docs_v2_review.rs`: a graph with a `review` node — when run with `TRIGGERED_BY_SCHEDULED` it passes through (emits the reviewed content as approved, run continues to completion); when run manually it retains the existing gate behavior (paused/pending — assert the existing manual semantics, whatever recon shows). 

- [ ] **Step 3: Implement.** Either: (a) the review executor reads `config.auto_approve` and, when true, emits `{content: <input content>, approved: true}` and lets the run continue; + inject `auto_approve` for scheduled runs in `run_job.rs`. OR (b) thread `triggered_by` into the executor context and branch there. Keep manual behavior identical.

- [ ] **Step 4: Run** `cargo test --test living_docs_v2_review 2>&1 | tail -15 && cargo build --lib 2>&1 | tail -5` → PASS. Re-run `tests/living_docs_v2_scheduled.rs` from T1 — the full end-to-end should now complete through deliver. FULL `cargo test 2>&1 | tail -15`. `rustfmt` changed files.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/ packages/backend/agent-orchestrator/src/ packages/backend/api/tests/living_docs_v2_review.rs
git commit -m "feat(agent): review node auto-approves on scheduled runs (manual stays gated)"
```

### Task 3: `schedule` convenience endpoint

**Files:** Modify `src/design/living.rs` (handler), `src/lib.rs` (route); Test `tests/living_docs_v2_schedule.rs`

**Interfaces:**
- Consumes: v1 `own_template`; the Rituals create path; `design_templates.flow_member_id`.
- Produces: `POST /api/design/templates/{template_id}/schedule` body `{ cron, timezone }` → `{ ritual_id, next_fire_at }`.

- [ ] **Step 1: Recon.** Read the Rituals create path (`src/design/rituals.rs` `create_ritual` / `build_ritual_graph`, the `FlowRef` vs `AgentRun` body — you want FlowRef: schedule an EXISTING member) + the `schedules` store create + `compute_next_fire_at`. Read how to read `design_templates.flow_member_id` (nullable). Confirm the cron/timezone validation the rituals path already does (reuse it).

- [ ] **Step 2: Write failing test** `tests/living_docs_v2_schedule.rs`: seed engagement A + a live template with a non-null `flow_member_id`; `POST .../{tid}/schedule` (Bearer A) `{cron:"0 0 9 * * 1", timezone:"UTC"}` → 200 `{ritual_id, next_fire_at}` + assert a `schedules` row exists for the template's `flow_member_id`. Cross-engagement (Bearer B) → 404. A template with `flow_member_id` null → 409. Unauth → 401.

- [ ] **Step 3: Implement** `schedule` handler in `living.rs`:
```rust
#[derive(serde::Deserialize)]
pub struct ScheduleBody { pub cron: String, pub timezone: String }

pub async fn schedule(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path(template_id): Path<Uuid>, Json(b): Json<ScheduleBody>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let flow_member_id = /* recon: SELECT flow_member_id FROM design_templates WHERE id=$1 */;
    let member_id = flow_member_id.ok_or((StatusCode::CONFLICT, "template has no live publish flow; publish it live first".into()))?;
    // create a schedule on that member, mirroring the rituals FlowRef create path
    let (ritual_id, next_fire_at) = /* recon: reuse rituals create-for-member: validate cron, compute_next_fire_at, insert schedule row scoped to the member */;
    Ok(Json(json!({ "ritual_id": ritual_id, "next_fire_at": next_fire_at })))
}
```
Replace the `/* recon */` with the real rituals-create reuse. Mount in `src/lib.rs`: `.route("/api/design/templates/{template_id}/schedule", post(design::living::schedule))`. (Decide upsert-one-schedule-per-member vs allow multiple; v2 = one — if a schedule already exists for the member, update its cron/timezone.)

- [ ] **Step 4: Run** `cargo test --test living_docs_v2_schedule 2>&1 | tail -20 && cargo build --lib 2>&1 | tail -5 && cargo test 2>&1 | tail -10` → PASS. `rustfmt` changed files.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/design/living.rs packages/backend/api/src/lib.rs packages/backend/api/tests/living_docs_v2_schedule.rs
git commit -m "feat(agent): schedule endpoint — attach a recurring fill+publish Ritual to a template"
```

**Stage 1 ships as one Celerex branch `feat/living-docs-v2-agent` → PR → squash-merge.**

---

# Stage 2 — Web (celerex-design `apps/web`, branch `feat/living-docs-v2-web`)

### Task 4: "Publish automatically" cadence control

**Files:** Modify `apps/web/src/state/living-doc.ts` (add `scheduleFill`), `apps/web/src/components/living/LivingSlotsPanel.tsx` (cadence control); Test `apps/web/tests/state/living-doc.test.ts` (add a case), `apps/web/tests/components/LivingSlotsPanel.test.tsx` (add a case)

- [ ] **Step 1: Recon.** Read `apps/web/src/state/living-doc.ts` (the existing helpers' conventions) + `apps/web/src/components/living/LivingSlotsPanel.tsx` (where to add the control) + the existing tests.

- [ ] **Step 2: Add the state helper** to `living-doc.ts`:
```ts
export async function scheduleFill(base: string, sessionToken: string, templateId: string, cron: string, timezone: string, fetchFn: typeof fetch = fetch): Promise<{ ritual_id: string; next_fire_at: string }> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/design/templates/${templateId}/schedule`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}`, "content-type": "application/json" },
    body: JSON.stringify({ cron, timezone }),
  });
  if (!res.ok) throw new Error("Could not schedule automatic publishing.");
  return res.json() as Promise<{ ritual_id: string; next_fire_at: string }>;
}
```
Add a test in `tests/state/living-doc.test.ts` asserting URL + POST + Bearer + body `{cron,timezone}`.

- [ ] **Step 3: Add the cadence control** to `LivingSlotsPanel`: an "Publish automatically: [Off / Daily / Weekly]" select that maps the choice to a cron (Daily → `0 0 9 * * *`, Weekly → `0 0 9 * * 1`) + `timezone` from `Intl.DateTimeFormat().resolvedOptions().timeZone`, calls `scheduleFill(agentBaseUrl(), session.token, templateId, cron, tz)` on change, shows the returned next-fire time. Mounted-ref guarded like the rest. (Off = no call / a future unschedule — for v2, Off can be a no-op placeholder or call a delete; keep it simple: Daily/Weekly call schedule, Off is inert with a note.)
- [ ] **Step 4: Test** add to `tests/components/LivingSlotsPanel.test.tsx`: selecting "Weekly" calls `scheduleFill` with the weekly cron. Mock `scheduleFill`.

- [ ] **Step 5: Run** `cd "apps/web" && pnpm vitest run tests/state/living-doc.test.ts tests/components/LivingSlotsPanel.test.tsx 2>&1 | tail -15 && pnpm tsc -b --noEmit 2>&1 | tail -8 && pnpm vitest run 2>&1 | tail -20` → green (except the 3 known pre-existing `default-background` failures).

- [ ] **Step 6: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/living-doc.ts apps/web/src/components/living/LivingSlotsPanel.tsx apps/web/tests/state/living-doc.test.ts apps/web/tests/components/LivingSlotsPanel.test.tsx
git commit -m "feat(web): Publish-automatically cadence control (schedule recurring fill)"
```

**Stage 2 ships as one celerex-design branch `feat/living-docs-v2-web` → PR → squash-merge.**

---

## Final verification
- [ ] Agent: `cd "packages/backend/api" && cargo test --test living_docs_v2_executor --test living_docs_v2_review --test living_docs_v2_schedule --test living_docs_v2_scheduled 2>&1 | tail -15 && cargo test 2>&1 | tail -10` → PASS (incl. v1 living_docs_fill/slots unbroken).
- [ ] Web: `cd "apps/web" && pnpm vitest run tests/state/living-doc.test.ts tests/components/LivingSlotsPanel.test.tsx 2>&1 | tail -8 && pnpm tsc -b --noEmit` → green/clean.
- [ ] Two branches: Celerex `feat/living-docs-v2-agent` PR; celerex-design `feat/living-docs-v2-web` PR.

## Self-review (spec coverage)
- slot_fill executor (real fill node, shared fn, engagement injection, scaffold wiring) → Task 1. ✓
- review auto-approve on scheduled (manual gated) → Task 2. ✓
- schedule endpoint (Ritual on flow_member_id, 409 unpublished, cross-engagement 404) → Task 3. ✓
- web cadence control → Task 4. ✓
- Whole scaffold runs unattended on schedule end-to-end (fill→render→review→deliver-as-doc) → Task 1 scheduled test, completed after Task 2. ✓
- Auth: fill executor fills only the run's injected engagement's template; schedule engagement-scoped; review auto-approve scheduled-only → Tasks 1-3 + reviews. ✓
- DRY shared fill fn (v1 endpoint + executor); v1 endpoint behavior unchanged → Task 1 (living_docs_fill still passes). ✓
- v3 (real external publish), connector fill, typed values deferred → spec §7. ✓
