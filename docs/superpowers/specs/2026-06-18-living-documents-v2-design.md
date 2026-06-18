# Living Documents v2 — recurring fill via Ritual — design

**Date:** 2026-06-18
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`, `packages/backend/api`) + celerex-design (web)
**Builds on:** Living Documents v1 (the agent-fill primitive — `template_slot_fills`, `src/design/living.rs`, the per-slot one-shot completion `run_fill`); Rituals 2b (a Ritual = a scheduled `project_member` fired by the Agent cron scheduler → `enqueue_run`); the live-bundle scaffold (`scaffold_live_flow` → a `trigger→fill→render→review→deliver` member, `design_templates.flow_member_id`); the executor model (`Handler::invoke(config, inputs)`, `apply_doc_engagement`).

## 1. Purpose

Make a Living Document **fill and ship on a schedule, unattended** — the user designs a templated doc once (with agent-fill slots from v1), then a weekly (or any-cadence) **Ritual** fires the existing publish scaffold so the slots are re-filled with fresh content and the rendered doc is produced automatically. "Design once → it runs weekly." (Real external delivery — email/website/YouTube — is **v3**; v2's `deliver` step writes the rendered Agent doc, which runs fine unattended.)

## 2. Decision (with the user 2026-06-18)

**Run the WHOLE scaffold on schedule** (`trigger→fill→render→review→deliver`), not just the fill. Consequence: the scaffold's human-approval **`review` gate must not stall an unattended run** — so **scheduled runs auto-approve** the review (manual runs stay gated). Confirmed acceptable because v2's `deliver` only writes an Agent doc (no external send until v3); when v3 adds real delivery, a notify/approve step can be reintroduced there.

## 3. Current state (recon 2026-06-18)

- **Scheduler dispatch** (`src/scheduler/loop.rs`, `src/lib.rs:306-341`): the tick calls `store.due(now)`, resolves the engagement via `members.engagement_id_for(s.project_member_id)`, and calls `enqueue_run(member_id, engagement_id, TRIGGERED_BY_SCHEDULED)` on the member's graph. **No new scheduler plumbing needed.**
- **Executor model**: `Handler` trait `async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError>`; registered in `src/lib.rs:228-251` (e.g. `DocHandler::new(pool.clone())`). `apply_doc_engagement` (`run_job.rs:79-109`) injects `engagement_id` into nodes whose `config.execution` is `doc`/`llm`/`memory`.
- **scaffold_live_flow** (`src/design/scaffold.rs:26-81`): 5 nodes. `fill` is a **placeholder** (`execution:"input"`, `inputs:"{{steps.trigger.outputs}}"`). `render` (`execution:"render"`) carries `template_html`+`slots` inline and substitutes `{{slot:name}}` from `{{steps.fill.outputs}}` at run time. `review` (`execution:"review"`, a `filter`) is the human gate. `deliver` (`execution:"doc"`) writes the rendered markdown as an outbound Agent doc. Output is the `project_member.graph_json`; `ensure_delivery_flow` (`src/design/mod.rs:298-343`) creates/updates this member and sets `design_templates.flow_member_id`.
- **v1 fill** (`src/design/living.rs`): `run_fill(state, engagement_id, instruction, slot_type, provider_id, model)` (one slot, one-shot completion) + `fill_all` (lists `template_slot_fills`, fills each, persists `last_value`). Currently AppState/StatusCode-coupled.
- Next migration: **0023** (none needed unless review-gate state requires it — it does not).

## 4. Design

### 4.1 `slot_fill` executor (Agent)
- Add `ExecutionType::SlotFill` (orchestrator crate) mapped from `config.execution == "slot_fill"`.
- `SlotFillHandler` (`src/design/slot_fill_handler.rs`) registered at boot with `pool` + `provider_keys` + `providers_registry` (the pieces `run_fill` needs for `resolve_engagement_provider` + house fallback). `invoke`: read `template_id` + injected `engagement_id` from `config`; run the **shared fill** for all the template's slots; **persist** each `last_value` (keeps v1 on-demand render fresh) **and** return `{ "data": { "<slot>": "<value>", … } }` shaped for the downstream `render` node's `{{steps.fill.outputs}}` substitution.
- **Refactor v1 to share the core**: extract a `pub(crate)` fill fn (e.g. `fill_template_slots(pool, provider_keys, registry, engagement_id, template_id) -> Result<Vec<(slot,value)>, FillError>`) free of `AppState`/`StatusCode`; both `living::fill_all` (the HTTP endpoint) and `SlotFillHandler::invoke` call it. (DRY — no duplicated completion logic.)
- Add `slot_fill` to `apply_doc_engagement`'s injection match so the fill node is tenant-scoped.
- Update `scaffold_live_flow` to bake the fill node as `execution:"slot_fill"` with `template_id` in its config (the fn gains a `template_id` param; `ensure_delivery_flow` already has it in scope).

### 4.2 Review auto-approve on scheduled runs (Agent)
- The `review` executor (`execution:"review"`) currently produces a human-approval gate. Make it **auto-approve when the run was triggered by the scheduler** — i.e., when there's no human. Mechanism (pick the cleanest at impl): the run carries its `triggered_by` (`scheduled` vs `manual`); the review executor passes through (emits the content as approved) on `scheduled`, and retains the gate on `manual`. If `triggered_by` isn't reachable inside the executor, set an `auto_approve` flag on the review node config when the run is scheduled (parallel to how `engagement_id` is injected) — recon during impl picks the lower-friction path. Manual/interactive runs keep the existing gate behavior.

### 4.3 Attach a schedule (Agent + thin web)
- **`POST /api/design/templates/{template_id}/schedule`** (DeviceAuth, engagement-scoped via the v1 `own_template` check) body `{ cron, timezone }` → creates a Ritual (schedule row) pointing at the template's `flow_member_id` (reusing the Rituals create path); returns `{ ritual_id, next_fire_at }`. `DELETE` / re-`POST` to change cadence (upsert one schedule per template's flow member). If `flow_member_id` is null (template not published live), 409 with a clear message.
- **Web**: a **"Publish automatically: [Off / Daily / Weekly ▼]"** control in the Living slots panel (celerex-design) that calls the schedule endpoint via the session Bearer (the v1 direct-Agent pattern). Non-technical users never see cron/Ritual internals.

## 5. Testing
- **slot_fill executor:** unit/integration — a graph with a `slot_fill` node fills the template's slots, persists `last_value`, and outputs the slot map; stub the provider (wiremock, as v1's `living_docs_fill` test does). Cross-engagement: the executor only fills the run's injected engagement's template.
- **End-to-end scheduled run:** mirror `tests/scheduler_dispatch.rs` — seed a live template + its scaffold member (with the `slot_fill` fill node) + a due schedule; let the dispatch fire `enqueue_run(..., scheduled)`; assert the run completes through `fill→render→review(auto-approved)→deliver` and a delivered doc exists with the filled values rendered.
- **Review auto-approve:** a scheduled run passes the review node unattended; a manual run still gates.
- **schedule endpoint:** create schedule → a schedule row exists for the template's flow member; cross-engagement → 404; unpublished template → 409; unauth → 401.
- **Web:** vitest for the cadence control (calls schedule endpoint; off/daily/weekly).
- Reuse the testcontainers harness; **run the full `cargo test`** after any executor/signature change (the cascade lesson).

## 6. Build order
Subagent-driven, two-stage review (rust-reviewer for Agent — the fill executor is auth-critical: it must fill only the run's engagement's template; the review auto-approve must NOT weaken manual runs), branch+PR+merge per repo. **Stage 1 (Agent, Celerex):** T1 `slot_fill` executor + shared fill fn + scaffold wiring + engagement injection; T2 review auto-approve-on-scheduled; T3 `schedule` endpoint. **Stage 2 (web, celerex-design):** the cadence control. Honors `celerex_file_connectors_principle`.

## 7. Non-goals / deferred (v3 and later)
- **Real external delivery** (email/website/YouTube/social) — v2's `deliver` writes the Agent doc only; v3 wires real destinations via 2a/2h connectors + likely reintroduces a notify/approve step.
- Connector-backed fill ("fill this table from <X>") — rides on the shared fill fn once the fill-agent reaches 2a connector tools.
- Per-run history UI, multiple schedules per template, typed/structured slot values, cross-origin render HTML-escaping.

See memory `omnivyx-architecture` (2c v1 + the v1→v2→v3 decomposition; Rituals 2b), the v1 spec/plan, and the recon (scheduler dispatch + executor model + scaffold fill placeholder).
