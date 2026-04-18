# Celerex Sprint 1 — Jean's Newsletter Slice (Design)

> **Status:** Design, approved 2026-04-18. Next step: implementation plan via `superpowers:writing-plans`.

## Goal

Ship the "Jean's Weekly Newsletter" user story end-to-end on top of the Sprint 0 substrate (tag `sprint-0-foundation`). A business user can create a Project Member in a visual canvas, wire data-source, synthesis, and output nodes, schedule weekly execution, observe runs in a live monitor, see the agent fleet at a glance, be notified on failure, and land a new dated BlockSuite doc each week.

**Why this slice first:** Jean's path exercises the full substrate (scheduler, canvas, plugins, file I/O, LLM, notifier, run observability) without depending on the riskier UX bets of Document Servicers or the Agent Deployer (which land in Bob's slice, Sprint 2). Ideation-to-Docs (Story 1) is explicitly Sprint 3+.

## Non-goals (explicit deferrals)

- Real Microsoft / workIQ OAuth — fixture plugin only. Real HTTP is a follow-on slice once workIQ infra is stood up.
- Real Stargate HTTP — fixture plugin only.
- Real Teams / email notifier integrations — trait + stub backends only (Console + "would-send" log).
- Outlook / Google Workspace calendar sync — schema has hooks for it; feature is V2+.
- Tool-use mode test harness beyond a `MockLlm` — real-LLM tool-use tests are manual-only in CI.
- Document Servicer blocks, Agent Deployer — Sprint 2.
- Ideation-to-Docs wizard (answer-a-few-AI-questions) — Sprint 3+.

## Architecture

### Data model (new migrations on top of Sprint 0's `0001`–`0004`)

| Migration | Purpose |
|---|---|
| `0005_schedules.sql` | `schedules (id, project_member_id, cron_expr, timezone, next_fire_at, last_fired_at, enabled, external_calendar_source NULL, external_event_id NULL, last_synced_at NULL)`. The last three fields are V2 hooks for Outlook/Google import-export. |
| `0006_project_members.sql` | `project_members (id, project_id, name, description, graph_json, provider_config_json, created_at, updated_at, version)`. Owns a graph. |
| `0007_member_snapshots.sql` | `project_member_snapshots (id, project_member_id, version, graph_json, provider_config_json, created_at)`. Immutable; edit creates a new version. |
| `0008_notifications.sql` | `notifications (id, project_member_id, run_id, event, payload_json, sent_at, delivery_status)`. For fleet-dashboard feed + notifier dispatch. |
| Amend `runs` | Add `snapshot_id` (FK, nullable for legacy) and `triggered_by` enum (`scheduled`, `manual`, `webhook`). |

### Backend components

- **`celerex-api::scheduler`** — tokio task polling `schedules WHERE next_fire_at <= now()` every 30s. Fires via orchestrator, advances `next_fire_at` using the `cron` crate. Missed-fires policy: catch up at most one run per schedule on startup (avoid thundering herd).
- **`celerex-api::project_members`** — CRUD routes. Edit creates an immutable snapshot; `runs.snapshot_id` always points at the version that actually executed. Exposes `POST /api/project_members/:id/run` for manual trigger.
- **`celerex-api::llm::provider`** — trait extracted from Sprint 0's concrete Anthropic client. `Provider::complete(model, messages, tools) -> Completion`. Implementations: `AnthropicProvider` (repackage existing), `StubProvider` (for tests). Multi-provider architecture pattern: any agent node picks its provider + model per node config; a separate "workspace assistant" LLM powers the workspace chat interface (scope: config only in Sprint 1, UI hook-up V2).
- **`celerex-api::plugins::file_connector`** — trait with `extract_markdown(file_id) -> String`, `tool_definitions(file_id) -> Vec<ToolDef>`, `invoke_tool(file_id, tool_name, args) -> Value`. Fixture impls for XLSX (via `calamine`) and DOCX (via `docx-rs`). Each file-input node on the canvas has an `extraction_mode: "markdown" | "tools"` config field (default `markdown`).
- **`celerex-api::plugins::{workiq, stargate}`** — fixture plugins returning canned realistic payloads. Manifest-based (Sprint 0 schema).
- **`celerex-api::notifiers`** — trait + `ConsoleNotifier` + `StubTeamsNotifier` (logs what it would send). Dispatched on `run_failed`.
- **`celerex-api::sse`** — `GET /api/runs/:run_id/events` streams `run_step_*` events. Backed by `tokio::sync::broadcast` channel fed from `PgRunStore` writes.

### Frontend components

- **`<AgentBuilderCanvas>`** — shared React Flow component. Props: `{ graph, mode: 'edit' | 'monitor', onEdit?, runState? }`. Edit mode: palette sidebar, drag, wiring. Monitor mode: node-status colors (grey / blue-pulse / green / red), animated edges on data flow, click-for-drawer showing step inputs/outputs. Node category styling (4 types: inbound/action/filter/outbound per Node-RED paradigm) ported from pre-pivot-archive's `WorkflowDesigner.tsx` (lift list — palette + categorization only, not the hand-rolled canvas).
- **Builder page** — hosts canvas in edit mode with palette sidebar + right-side node config panel. Includes "Run now" button that flips the view to Monitor mode on the resulting run.
- **Monitor page** — hosts canvas in monitor mode with a run-selector dropdown (past runs + "Live"). SSE client subscribes to `/api/runs/:run_id/events` for live runs. Same canvas, different host.
- **Fleet dashboard** — new top-level view. Table of all Project Members in the engagement: name, last status, next scheduled fire, currently-running indicator. Click row → Monitor page for that agent's latest run.
- **Project Member creation flow** — entry point in sidebar's Project tree: "+ Add agent to project" → Builder page with empty graph.

## Data flow: one Monday run

1. **09:00 Monday local time.** Scheduler tokio loop finds `schedules WHERE next_fire_at <= now()`. Selects Jean's row.
2. Scheduler loads `project_member_snapshots` at current version → constructs `JobRequest` → hands to orchestrator. Inserts `runs` row with `snapshot_id` and `triggered_by='scheduled'`.
3. Orchestrator walks the graph DAG (Sprint 0 executor). Nodes fire in topo order:
   - `schedule.tick` (inbound, no-op kick)
   - `workiq.pull_metrics` (fixture → canned JSON)
   - `stargate.query` (fixture → canned rows)
   - `excel.extract` × 3 (markdown mode → markdown tables)
   - `docx.extract` × 2 (markdown)
   - `llm.synthesize` (Anthropic Sonnet, concatenated context → newsletter markdown)
   - `document.create` (outbound → new BlockSuite doc `"Weekly Newsletter 2026-04-20"` under Project)
4. Each step: `run_steps` row written → `tokio::sync::broadcast` publishes event → SSE subscribers receive.
5. Monitor page (if open) paints nodes green in real time. Fleet dashboard row updates.
6. **On any node failure:** `run_steps.status='failed'`, run halts, notifier fires. ConsoleNotifier logs; StubTeamsNotifier logs "would send to …".
7. Scheduler advances `schedules.next_fire_at` to the next cron occurrence.

**Manual trigger variant:** user clicks Run now → `POST /api/project_members/:id/run` → identical orchestrator flow with `triggered_by='manual'`. UI auto-navigates Builder → Monitor on the new `run_id`.

## Error handling

- **Per-step retries** — Sprint 0 already has retry-from-failed-step with upstream success preservation. Carry forward unchanged.
- **Scheduler downtime** — catch-up-at-most-one on startup. Jean loses at most one missed Monday if she was offline; won't get six re-runs after a week of downtime.
- **Extraction errors** — `file_connector` returns `Result<String, ConnectorError>`; becomes a failed `run_step` with human-readable error.
- **Provider errors** — bubble up via `Provider::complete` → failed step → notifier.
- **Version drift on edit during run** — runs pin to `snapshot_id`. Jean editing the graph mid-run does not affect the executing run; next scheduled fire picks up the new version.

## Testing strategy

- **Backend integration tests** (testcontainers-postgres + fixture plugins):
  - Scheduler tick simulation: freeze time, advance, assert orchestrator dispatch + doc written.
  - Snapshot-on-edit: edit a member, assert new snapshot row, assert run FKs old snapshot.
  - File connectors (both modes): XLSX + DOCX fixtures → assert markdown / tool-use outputs.
  - Notifier dispatch: failed run → assert notifier called + `notifications` row.
  - SSE: subscribe, simulate run, assert events streamed in order.
- **`MockLlm` provider** — scripted tool_use responses for tools-mode file-connector tests.
- **Frontend component tests:**
  - `<AgentBuilderCanvas>` mode switching (edit ↔ monitor) with mock runState.
  - Fleet dashboard sort/filter/pulse indicator.
  - SSE client subscription lifecycle (connect, reconnect, close).
- **E2E demo test** (as the sprint's acceptance criterion) — full graph executes via manual trigger, asserts new BlockSuite doc `"Weekly Newsletter <date>"` appears, asserts fleet dashboard reflects success.

## Implementation sequence (to feed writing-plans)

Ordering designed so each letter has a runnable substrate + tests before the next.

A. Scheduler crate + `0005_schedules.sql` + tokio loop + `cron` advance logic.
B. Project Member CRUD + `0006` + `0007` + snapshot-on-edit + `POST /run` endpoint.
C. Provider trait extraction from Sprint 0 Anthropic; repackage `AnthropicProvider`; add `StubProvider`.
D. File connector trait + XLSX/DOCX markdown extractors (`calamine`, `docx-rs`).
E. Tool-use mode for file connectors + `MockLlm` harness wired to Sprint 0's existing `tool_use` round-trip loop.
F. `workIQ` + `Stargate` fixture plugins (manifests + canned payloads).
G. SSE endpoint + `tokio::sync::broadcast` channel wired through `PgRunStore` + amend `runs` schema (`snapshot_id`, `triggered_by`).
H. Notifier trait + `ConsoleNotifier` + `StubTeamsNotifier` + `0008_notifications.sql`.
I. Frontend: shared `<AgentBuilderCanvas>` on React Flow.
J. Frontend: Builder page (palette + categorization ported from pre-pivot `WorkflowDesigner.tsx`).
K. Frontend: Monitor page (SSE client, run selector, step drawer).
L. Frontend: Fleet dashboard (query all members, live indicator via SSE aggregate).
M. Frontend: Project Member creation flow from sidebar tree.
N. E2E acceptance test: Jean's full graph executes manually, asserts dated newsletter doc created.

## Open items for plan writing (not design-level)

- Cron library choice: `cron` vs `tokio-cron-scheduler`. Both viable; planner picks based on timezone support.
- React Flow version pin.
- Exact shape of `NotificationEvent` enum.
- `graph_json` schema — JSON Schema definition to enforce at the API boundary.
