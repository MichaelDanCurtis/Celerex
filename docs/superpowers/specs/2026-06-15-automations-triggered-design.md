# Automations (triggered) — sub-project 2d

**Date:** 2026-06-15
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`) + celerex-design (seam + daemon + web)
**Builds on:** the Rituals pattern (2b/2f) — Agent-hosted scheduled `project_member`s via a seam + daemon proxy + web surface. Automations are the **triggered** counterpart.

## 1. Purpose

Complete the taxonomy locked in 2b: **Ritual = scheduled (clock); Automation = triggered (event).** This sub-project ships the first Automations slice: **connector-event-driven** Automations that fire a flow when a connector surfaces a new item, detected by **polling**, hosted always-on on the Agent. It replaces the "Automations — coming soon" stub already in the web surface.

## 2. Locked decisions (from brainstorming)

- **An Automation = a triggered `project_member`** (the flow) + an **automation trigger binding** (event source + poll config + cursor + filter). Symmetric with a Ritual = `project_member` + a cron `schedules` row.
- **Trigger type (first slice): connector-event**, **sourced by polling** (an Agent loop, sibling to the scheduler). The poll is just the detection mechanism; the Automation is event-triggered, not clock-triggered.
- **First event-source adapter: `http-api`** (generic JSON poll) — fully testable against a stub; broadly useful. The adapter layer is connector-agnostic so m365/galvo slot in later.
- **Agent-hosted / always-on** (fires with the desktop off), surfaced via the same `@celerex/connect` + daemon `/api/*` + web pattern as Rituals.
- **At-least-once** delivery: the cursor only advances past items that successfully enqueued.
- **Minimal filter** (single optional substring match on a field) — usable without a query language.
- 2d ships a **minimal create UI** (unlike Rituals, which were created by migration; nothing migrates into Automations).

## 3. Key facts from recon

- Flow engine node types: `inbound | action | filter | outbound` (`project_members/graph.rs`); the `inbound` node is the trigger/seed that echoes a payload downstream — the event item feeds it.
- `run_job::enqueue_run(pool, members, executor, notifiers, member_id, engagement_id, triggered_by)` is the firing primitive (used by Rituals + manual runs). Constants `TRIGGERED_BY_MANUAL`/`TRIGGERED_BY_SCHEDULED`; **`runs.triggered_by` CHECK allows `scheduled|manual|webhook`** — a migration adds `'connector'`.
- An existing webhook path (`POST /api/webhooks/{plugin_id}/{event}`, `webhook_dedupe`) is **plugin-manifest-driven** (`on_event_pipeline`) and writes a bare `runs` row — NOT reused here; user Automations are a new first-class layer on `project_members`.
- The Agent has server-side connectors (m365, galvo-memory, http-api) with vaulted credentials (`engagement_provider_keys` / the design connector catalog) — the poll uses these.
- The Rituals scheduler (`scheduler/{loop,advance,store,routes}.rs`) is the structural template for the poll loop + store.

## 4. Components

### 4.1 Data model (Agent, Postgres) — `automation_triggers`
A new migration:
```sql
CREATE TABLE automation_triggers (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
  connector_id TEXT NOT NULL,            -- e.g. 'http-api'
  source_config JSONB NOT NULL,          -- adapter input (http-api: {url, items_path, id_field})
  filter JSONB,                          -- optional: {field, contains}
  poll_interval_secs INT NOT NULL DEFAULT 300,
  cursor TEXT,                           -- last-seen item id/timestamp
  last_polled_at TIMESTAMPTZ,
  enabled BOOLEAN NOT NULL DEFAULT TRUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_automation_triggers_due ON automation_triggers(enabled, last_polled_at);
```
Plus: `ALTER TABLE runs DROP CONSTRAINT runs_triggered_by_chk, ADD CONSTRAINT runs_triggered_by_chk CHECK (triggered_by IN ('scheduled','manual','webhook','connector'));` and a `TRIGGERED_BY_CONNECTOR = "connector"` constant.

### 4.2 Event-source adapter (Agent, Rust)
A trait in a new `automations/` module:
```rust
pub struct PolledItem { pub id: String, pub payload: serde_json::Value }
#[async_trait]
pub trait EventSource {
    /// Fetch items newer than `cursor`; return them (oldest→newest) + the new cursor.
    async fn poll(&self, source_config: &Value, cursor: Option<&str>) -> Result<(Vec<PolledItem>, Option<String>), String>;
}
```
**One impl: `HttpApiEventSource`** — GETs `source_config.url` (auth via the vaulted `http-api` api_key, reusing the connector credential path), extracts the array at `items_path`, reads each item's `id_field` as its id, returns items whose id is past `cursor` (lexicographic/numeric/timestamp ordering documented), new cursor = max id. Connector id → adapter is a small registry so m365/galvo adapters register later.

### 4.3 Poll loop (Agent)
A tokio task sibling to `scheduler/loop.rs`: each tick, select due triggers (`enabled AND (last_polled_at IS NULL OR last_polled_at + interval ≤ now)`); for each, resolve the adapter by `connector_id`, `poll(source_config, cursor)`, apply `filter` to each new item, and for each match `enqueue_run(member, engagement, TRIGGERED_BY_CONNECTOR)` seeding the item payload into the run. **Cursor advances only past successfully-enqueued items** (at-least-once); a poll/enqueue error is logged and retried next tick (the Automation is not disabled). `last_polled_at` updated each tick. Engagement resolved from the member's project (as Rituals do).

### 4.4 Device-seam Automation API (Agent)
`packages/backend/api/src/design/automations.rs`, mounted under `DeviceAuth` alongside `/api/design/rituals` — mirrors the Rituals handlers:
- `POST /api/design/automations` — create: builds a `project_member` from `AutomationSpec.body` (reuse the Rituals `agent_run` graph builder / `rituals_project`-style container project named "Automations"), then inserts the `automation_triggers` row. Rolls back the member if the trigger insert fails.
- `GET` (list, engagement-scoped), `PUT /{id}` (update source/filter/interval/enabled), `DELETE /{id}`, `POST /{id}/run` (run-now, `triggered_by=manual`), `GET /{id}/runs`.

`AutomationSpec` = `{ name, enabled?, connector_id, source_config, filter?, poll_interval_secs?, body: { kind: "agent_run", prompt, skill_template_id? } | { kind: "flow_ref", project_member_id } }`.

### 4.5 Seam — `@celerex/connect`
`packages/celerex-connect/src/automations.ts` mirroring `rituals.ts`: types (`AutomationSpec`, `AutomationView`, `AutomationRun`) + `listAutomations`/`createAutomation`/`updateAutomation`/`deleteAutomation`/`runAutomationNow`/`getAutomationRuns`. Rebuild the dist after (the daemon imports the built output — the 2b gotcha).

### 4.6 Daemon
`apps/daemon/src/routes/automation.ts` + registration in `server.ts`, mirroring the ritual routes: `/api/automations` CRUD proxying to the Agent via the seam, reading the pairing from app-config `celerex`. 409 unpaired / 502 seam failure, as the ritual routes do.

### 4.7 Web
Replace the "Automations — coming soon" stub (in the Rituals surface / `TasksView`) with a real **Automations** section: list (name, connector source, enabled, last run), enable-disable, run-now, runs view — reading from `/api/automations` via the daemon. Plus a **minimal create form**: name, connector (http-api), `url` + `items_path` + `id_field`, optional filter (`field` + `contains`), poll interval, and an `agent_run` prompt. A `state/automations.ts` data module mirrors `state/rituals.ts`.

## 5. Data flow

```
Agent poll loop (tick) ─ due automation_trigger ─▶ EventSource.poll(source_config, cursor)
   new items past cursor ─ filter ─▶ enqueue_run(member, triggered_by='connector', payload=item)
   advance cursor past enqueued items; update last_polled_at
run produces a Document/output + run record on the Agent
Design reads /api/automations + /{id}/runs via the daemon to show status, runs, outputs
```

## 6. Error handling

- **Poll/adapter failure** (endpoint down, bad JSON): logged, tick skipped, retried next interval; Automation stays enabled.
- **Enqueue failure**: cursor does not advance past that item → retried next tick (at-least-once).
- **Bad source_config / unknown connector_id**: 400 at create time (validate the connector id against the catalog; validate required `source_config` fields per adapter).
- **Partial create**: roll back the `project_member` if the trigger insert fails (symmetric with delete).
- **Engagement scoping**: derived from the device token / the member's engagement — never client-supplied (mirrors Rituals).

## 7. Testing

- **Adapter (Rust):** `HttpApiEventSource.poll` against a stub server — returns only items past cursor, computes the new cursor, handles empty/missing `items_path`.
- **Poll loop (Rust):** a due trigger with a fake `EventSource` + stub executor fires `enqueue_run` once per new item, advances the cursor past enqueued items, and does NOT advance past a failed enqueue (at-least-once).
- **Device API (Rust):** Postgres integration tests mirroring `design_rituals` — create→member+trigger, list/delete engagement-scoped, run-now, runs, partial-create rollback, bad-connector 400.
- **Seam (bun):** `automations.ts` request/response shapes vs an HTTP stub.
- **Daemon (vitest):** `/api/automations` proxies to the Agent (stub) with the bearer token.
- **Web (vitest):** the Automations section lists + the create form posts a valid `AutomationSpec`; the stub is replaced.

## 8. Out of scope (later slices)

- Additional event-source adapters (m365 new-mail/file, galvo-memory) — the adapter trait is the seam they plug into.
- Provider push / subscriptions (real-time) — poll first.
- Rich filter query language — single substring match for v1.
- Multi-node automation flows beyond the `agent_run`/`flow_ref` body (reuse existing node types).
- Backfill/replay of historical events (cursor starts at "now" on create).

## 9. Build order (for the plan)

1. **Agent:** migration (`automation_triggers` + `triggered_by='connector'`) + `EventSource` trait + `HttpApiEventSource` + poll loop, with unit tests.
2. **Agent:** `/api/design/automations` CRUD + integration tests; wire the poll loop into startup.
3. **Seam:** `automations.ts` + dist rebuild.
4. **Daemon:** `/api/automations` proxy routes + tests.
5. **Web:** Automations section + create form + `state/automations.ts`, replacing the stub.
6. Live verification against the in-cluster Agent (mirror the 2f/2g approach: create an http-api Automation pointed at a stub endpoint, add an item, watch a run fire).

Each stage is its own PR (per the standing "PR everything" rule; GitHub Actions stays disabled).
