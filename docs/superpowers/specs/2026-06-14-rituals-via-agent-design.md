# Rituals via Agent (spec #2 · sub-project 2b)

**Date:** 2026-06-14
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (OmniVyx Agent, Rust/Axum) + celerex-design (OmniVyx Design, pnpm monorepo)
**Depends on:** 2a Connectors-via-Agent (merged); the device-token seam (`/api/design/*`, `DeviceAuth`/`design.device`); the Agent scheduler (`scheduler/`) and flow engine (`project_members.graph_json`).

---

## 1. Purpose

OmniVyx Design today schedules unattended agent work in the **local daemon** (`apps/daemon/src/routines.ts` + `orbit.ts`). That scheduler only fires while the desktop is open, which contradicts the backplane vision: scheduled work should run **even when your PC is off**.

This sub-project re-homes scheduled work onto the always-on **Agent**, and unifies the vocabulary under one concept: **Ritual**.

## 2. Taxonomy (locked with the user)

- **Ritual = scheduled.** Anything that fires on a clock (cron). Today's *routines*, *Orbit*, and *scheduled live-artifact refreshers* all become **Rituals**.
- **Automation = triggered.** Anything that fires on an event, *not* on a schedule. A genuinely separate concept — **out of scope here**; it belongs to the Agent "action system" (sub-project 2d).

The dividing line is the initiator: **clock → Ritual, event → Automation.** "Orbit" demotes from a top-level feature to a built-in Ritual *template* ("daily briefing").

### Why Rituals are Agent-hosted with no local fallback

A Ritual's value is firing while you are away (desktop off). Every candidate for "run it locally" fails that test:

- *Needs the local-CLI brain (Claude Code/Codex):* that is for **interactive** creation (you, present, at the desktop), not unattended scheduled jobs — the Agent's own API-agents handle scheduled work.
- *Touches local files / localhost / a LAN-only resource:* genuinely unreachable from the Agent, **but** only runnable while the machine is on, so it cannot be a dependable schedule. That is a manual action or a triggered Automation (2d), not a Ritual.
- *Privacy (keep data on-device):* same — only runnable while the machine is on.

The connector concern dissolves: Agent-hosted Rituals use the **Agent's own server-side connectors**; the local connector execution built in 2a is for Design's *local* AI. The Agent is both the credential vault and a server-side executor.

**Decision: Rituals run on the Agent, always-on. No local fallback.** The local-compute edge is reserved for interactive work.

## 3. Architecture — a Ritual is a scheduled `project_member`

The Agent already has the machinery:

- **Scheduler** (`packages/backend/api/src/scheduler/`): `CreateBody { project_member_id, cron_expr, timezone, enabled }`, `compute_next_fire_at` (cron + tz), and a loop (`loop.rs`/`advance.rs`) that fires a `project_member` via `run_job::TRIGGERED_BY_SCHEDULED`. Engagement-scoped via `engagement_id_for(project_member_id)`.
- **Flow engine** (`project_members.graph_json`): node graphs; the design-handoff live delivery flow (`design/scaffold.rs`, `trigger → fill → render → review → deliver`) is already a scheduled `project_member`.

So: **a Ritual = a `project_member` (a simple scheduled agent run, or a flow) + a scheduler entry that fires it on cron.** Nothing in the scheduler needs to change; we add a device-seam surface that creates/manages these for Design and reads their runs.

```
Design Rituals UI
   │  (device token seam: /api/design/rituals, DeviceAuth)
   ▼
Agent: /api/design/rituals  ──creates──►  project_member (Ritual body)
   │                          ──creates──►  scheduler entry (cron, tz, enabled)
   ▼
Agent scheduler loop (always-on)  ──fires──►  run_job (server-side agents + connectors)
   │
   ▼
run output: Document/artifact + run record  ──read back──►  Design Rituals UI (history + outputs)
```

## 4. Components

### 4.1 Agent — device-seam Ritual API (Rust)

New module `packages/backend/api/src/design/rituals.rs`, mounted in `lib.rs` alongside the other `/api/design/*` routes, all under `DeviceAuth`/`design.device` (engagement-scoped from the device token, mirroring `list_design_connectors`):

- `GET  /api/design/rituals` — list Rituals for the engagement (each = its scheduler entry + project_member summary + next_fire_at + last run).
- `POST /api/design/rituals` — create a Ritual: builds the `project_member` body from a **RitualSpec** payload, then creates the scheduler entry. One call, two writes, transactional best-effort (roll back the member if the schedule create fails).
- `PUT  /api/design/rituals/{id}` — update schedule (cron/tz/enabled) and/or the Ritual body.
- `POST /api/design/rituals/{id}/run` — run-now (fire the project_member once, off-schedule).
- `DELETE /api/design/rituals/{id}` — delete the scheduler entry + project_member.
- `GET  /api/design/rituals/{id}/runs` — recent run records (status, started/finished, output document id).

**RitualSpec** (the device-seam payload — Design's vocabulary, not raw graph JSON):
- `name`, `enabled`
- `schedule`: `{ cron_expr, timezone }`
- `body`: one of
  - `agent_run`: `{ prompt, skill_template_id?, target? }` — the routine/Orbit shape (a scheduled agent conversation). Built into a minimal `project_member` (single agent node).
  - `flow_ref`: `{ project_member_id }` — an existing flow (e.g. a live-delivery flow from `scaffold.rs`) becomes the Ritual body directly.

This keeps the seam **Design-shaped and backend-agnostic** (per the architecture constraint): Design never sends graph JSON; the Agent translates RitualSpec → project_member. When the Agent becomes a Galvo distribution, the daemon never notices.

The Orbit "daily briefing" ships as a **built-in RitualSpec template** the Agent can materialize (default cron, the briefing prompt/skill).

### 4.2 Seam — `@celerex/connect` (TypeScript)

New `packages/celerex-connect/src/rituals.ts`, mirroring `connectors.ts`:
- Types: `RitualSpec`, `RitualSummary`, `RitualRun`.
- Functions: `listRituals`, `createRitual`, `updateRitual`, `runRitualNow`, `deleteRitual`, `getRitualRuns` — each taking `(baseUrl, deviceToken, …)`.

> **Gotcha (from 2a):** the daemon imports the **built** `@celerex/connect` dist (gitignored). After seam changes run `pnpm --filter @celerex/connect build` or the daemon sees stale exports.

### 4.3 Design — rename + rewire (TypeScript/React)

- **Vocabulary rename** routines/Orbit → **Rituals** across the web app: the `/automations` page (`apps/web/src/components/TasksView.tsx`) becomes the **Rituals** surface; `OrbitConfig` (`apps/web/src/types.ts`) + `DEFAULT_ORBIT` (`apps/web/src/state/config.ts`, `templateSkillId: 'orbit-general'`) become the daily-briefing Ritual template; i18n strings; `SettingsDialog.tsx` Orbit settings. Internal `Routine`/`Orbit` *type names* may stay; **product-facing** vocabulary unifies under Ritual.
- **Rewire the Rituals surface** to create/manage Rituals via the seam → Agent (not the local scheduler), and to show **Agent-fired** run history + produced artifacts (`getRitualRuns`).
- The page is all scheduled today, so it maps cleanly to Rituals; triggered "Automations" get their own surface when 2d builds them (a stub/empty state is acceptable, no logic).

### 4.4 The local daemon scheduler — retirement path (additive-first)

Mirror the 2a additive-first discipline:
1. **Land the Agent path** (4.1–4.3) with the local scheduler still running.
2. **Point the Rituals UI at the Agent**; verify Agent Rituals fire end-to-end with the desktop closed.
3. **Retire** `routines.ts`/`orbit.ts` local scheduling once proven. Scheduled live-artifact refreshers migrate to Rituals (a `flow_ref`/`agent_run` body) in this step.

Retirement is the **last** stage — do not rip out `routines.ts` before Agent Rituals are proven in real use.

## 5. Data flow

1. User defines a Ritual in Design → `createRitual(spec)` over the seam.
2. Agent builds the `project_member` body from RitualSpec + creates the scheduler entry (`scheduler/store::Schedule`).
3. The scheduler loop fires it on cron via `run_job` (server-side agents + Agent connectors), always-on.
4. The run produces a `Document`/artifact + a run record, persisted on the Agent.
5. Design reads `listRituals` / `getRitualRuns` to show schedules, next-fire, history, and outputs.

## 6. Error handling

- **Partial create:** if the scheduler-entry write fails after the project_member is created, delete the orphan member (best-effort) and return an error. Symmetric on delete.
- **Invalid cron/tz:** reuse the scheduler's existing `compute_next_fire_at` validation; surface a 400 with the message.
- **Run failure:** recorded on the run record; the Ritual stays scheduled (a bad run does not disable the Ritual). Surfaced in the UI run history.
- **Seam/auth:** device-token scope `design.device`; engagement derived from the token (never client-supplied), matching `list_design_connectors`.

## 7. Testing

- **Agent (Rust):** RitualSpec → project_member translation (both body shapes); create rolls back the member on schedule-create failure; engagement scoping (cannot touch another engagement's Ritual); cron/tz validation 400; run-now path. Use the existing Postgres-backed test harness used by the scheduler/design tests.
- **Seam (bun):** `rituals.ts` request/response shapes against an HTTP stub (mirror `celerex-connect-connectors.test.ts`).
- **Design (vitest):** the renamed surface calls the seam (not the local scheduler); Orbit template materializes as a daily-briefing Ritual; Automations surface renders its empty state.

## 8. Out of scope

- Triggered **Automations** (2d action system) — only the empty-state placeholder here.
- Changes to the Agent scheduler internals (it already does cron → project_member).
- The exact node payload of complex Ritual flows (reuse existing flow node types + `scaffold.rs`).
- Document-workspace authoring (2c), Agent backplane / portable profile (2e).

## 9. Build order

1. Agent Ritual API (`rituals.rs`) + tests — **PR**.
2. Seam `rituals.ts` + tests + rebuild dist — **PR**.
3. Design rename + rewire to Agent + tests — **PR**.
4. Retire local scheduler + migrate refreshers — **PR** (only after 1–3 proven).

Each stage is its own PR (per the standing "PR everything" rule; GitHub Actions stays disabled).
