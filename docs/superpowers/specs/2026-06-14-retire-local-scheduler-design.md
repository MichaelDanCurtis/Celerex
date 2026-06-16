# Retire the Local Daemon Scheduler (sub-project 2f)

**Date:** 2026-06-14
**Status:** Design — awaiting user review before writing-plans
**Repo:** celerex-design (OmniVyx Design, pnpm monorepo) — daemon + web
**Depends on:** 2b Rituals stages 1–3 (merged): Agent Ritual API (`/api/design/rituals`), the `@celerex/connect` ritual client, the daemon `/api/rituals` proxy, and the additive web "Agent rituals" surface.

## 1. Purpose

2b Rituals stages 1–3 added the Agent-hosted Ritual path **additively** — the local daemon scheduler (`apps/daemon/src/{routines.ts, orbit.ts, routes/routine.ts}`) still runs untouched. This sub-project retires that local scheduler so Rituals live solely on the always-on Agent, and migrates existing local routines + the Orbit daily-briefing into Agent Rituals.

This was deferred from 2b Stage 4 once recon showed it is a real, surgical refactor — not a file deletion — and that its precondition (Agent Rituals proven in real use) was unmet.

## 2. Locked decisions

- **Orbit → Agent Ritual.** The daily briefing converges to the online-first model: a server-side, always-on `agent_run` Ritual on the Agent's brain + connectors. It **loses** local-CLI execution, the local `orbit-…` project, design-system resolution, and the `renderOrbitTemplateSystemPrompt` machinery. The briefing intent rides in the Ritual prompt. (User decision — the briefing is largely a connector digest the Agent already serves server-side.)
- **Best-effort auto-migration.** On upgrade, existing local routines + Orbit config are read once and re-created as Agent Rituals (idempotently), then local entries are removed. There is local config worth preserving.
- **Two PRs with a manual verify gate.** PR1 migrates (additive — local scheduler still runs); the user confirms the migrated Rituals appear and fire on their paired Agent; PR2 then removes the local scheduler. Migration is the "Agent Rituals proven in real use" proof the original gate required.
- **Repoint the Orbit settings UI** to manage the new daily-briefing Ritual (enable + schedule) rather than deleting it — the web Rituals surface is read-only today, so the user must retain a control surface for the briefing.

## 3. Key facts from recon (de-risks the removal)

- `orbit.ts` splits cleanly: a **scheduling** half (`OrbitService` `setTimeout` timer: `scheduleNext`/`start`/`stop`) and a **prompt/template** half (`buildOrbitPrompt`/`buildOrbitSystemPrompt`/`renderOrbitTemplateSystemPrompt`). The prompt builders are used **only** within `orbit.ts` and the Orbit run handler (`server.ts:14621`) — **not** by general chat system-prompt composition. So the whole Orbit subsystem can be removed together once Orbit is an Agent Ritual.
- `RoutineSchedule` is a union: `hourly{minute}`, `daily{time,timezone}`, `weekdays{time,timezone}`, `weekly{weekday,time,timezone}` (see `routines.ts:nextRunAtForSchedule`). The Agent scheduler uses 6-field cron (seconds-first, via the `cron` crate).
- Local routines persist in the daemon SQLite store (`listRoutines(db)`, `routineDbRowToContract`); a `Routine` has `{ id, prompt, schedule, skillId, projectTarget, context.skillIds }`.
- `/api/orbit/status` + `/api/orbit/run` are registered in **both** `server.ts` (~10933) and `media-routes.ts` (~421); `/api/routines*` via `registerRoutineRoutes` (`routes/routine.ts`). Removal must hit all sites.

## 4. Components

### 4.1 `scheduleToCron` — pure mapping helper (PR1)
`scheduleToCron(s: RoutineSchedule): { cron_expr: string; timezone: string }`:
- `hourly{minute}` → `0 {minute} * * * *` (timezone: the daemon's local tz / UTC fallback)
- `daily{time:"HH:MM", timezone}` → `0 {MM} {HH} * * *`
- `weekdays{time, timezone}` → `0 {MM} {HH} * * 1-5`
- `weekly{weekday, time, timezone}` → `0 {MM} {HH} * * {weekday}`

Standalone, fully unit-tested. Lives in a new daemon module (e.g. `apps/daemon/src/rituals-migration.ts`) alongside the runner.

### 4.2 Migration runner — daemon, one-time, idempotent (PR1)
`migrateLocalSchedulesToRituals()`:
- **Precondition:** pairing exists (Agent base URL + device token in app-config `celerex`). If unpaired, no-op and defer; invoke after a successful `POST /api/celerex/connect` (same hook `seedFromToken` uses).
- **Routines:** `listRoutines(db)` → for each not-yet-migrated routine, build `RitualSpec { name: routine.name, enabled: true, schedule: scheduleToCron(routine.schedule), body: { kind: "agent_run", prompt: routine.prompt, skill_template_id: routine.skillId ?? undefined } }` → `createRitual(serverUrl, deviceToken, spec)`. (Respect `exactOptionalPropertyTypes`: omit `skill_template_id` when null, don't pass `undefined`.)
- **Orbit:** if Orbit config is enabled, build a daily-briefing `agent_run` RitualSpec (briefing prompt + `templateSkillId`; schedule from the Orbit config) → `createRitual`.
- **Idempotency:** track migrated routine ids + an `orbitMigratedAt` marker in app-config; skip already-migrated entries. Handles reruns and partial failures without duplicating.
- **No deletion here.** Logs a structured migration report (created/skipped/failed). PR2 deletes local.

### 4.3 Orbit → Ritual mapping detail (PR1)
The Orbit config maps to one `agent_run` Ritual carrying the briefing prompt + the configured template skill id; schedule from the Orbit config. Server-side execution on the Agent; local-only machinery is intentionally dropped (see §2).

### 4.4 Removal — daemon (PR2)
- Delete `apps/daemon/src/routines.ts`, `apps/daemon/src/routes/routine.ts`, `apps/daemon/src/orbit.ts` (+ `orbit-agent-summary.ts` if it becomes orphaned — confirm).
- Strip from `server.ts`: the imports (`OrbitService`, `renderOrbitTemplateSystemPrompt`, `RoutineService`, `registerRoutineRoutes`, `routineDbRowToContract`), the `orbitService`/`routineService` instantiation + wiring, the Orbit run handler (`setRunHandler`/`setTemplateResolver`), and `orbitDeps`/`registerRoutineRoutes(app, …)`.
- Remove `/api/routines*` and `/api/orbit/*` route registrations in **both** `server.ts` and `media-routes.ts`.
- Keep the local SQLite `routines` table read path only as long as the migration needs it; after migration the table is dead — drop the helpers (`listRoutines` etc.) if unreferenced. Leave the table file in place (no destructive DB migration needed for a local store).
- Driven by `tsc` until clean — no dangling references.

### 4.5 Removal — web (PR2)
- Drop the local "Your rituals" (`/api/routines`) list and `RoutinesSection`; the Rituals surface shows **only** the Agent "Agent rituals" section (rename it back to just "Rituals" once it's the sole source).
- **Repoint the Orbit settings** (`SettingsDialog`): instead of configuring the local Orbit timer, it reads/writes the daily-briefing Agent Ritual via the daemon (`/api/rituals` — at minimum enable/disable + schedule via the existing `PUT /api/rituals/:id`). This gives the surface its first edit capability (deferred in 2b Task 10) for the one Ritual that needs it.
- Remove `/api/orbit` calls from the web; update/remove tests asserting on `/api/routines` + `/api/orbit`.

## 5. Data flow (post-retirement)

```
Define/edit a Ritual (Design)
   → daemon /api/rituals → Agent /api/design/rituals → scheduler entry + project_member
Agent scheduler (always-on) fires the project_member server-side
   → run record + output Document on the Agent
Design reads /api/rituals + /api/rituals/:id/runs to show schedules, runs, outputs
```
No local timer remains. The daily briefing is one such Ritual.

## 6. Error handling

- **Unpaired at migration:** no-op + defer to post-connect; never throws into startup.
- **Per-routine migration failure:** logged in the report, marked not-migrated (so a later run retries); does not abort the whole migration or block startup.
- **Duplicate prevention:** the migrated-id markers make re-runs safe.
- **Removal safety:** PR2 is gated on the user confirming PR1's migration worked on their machine; `tsc` + the test suite guard against dangling references; explicit check that chat system-prompt composition is unaffected (no shared Orbit builders — confirmed in §3).

## 7. Testing

- **Unit (PR1):** `scheduleToCron` for all four schedule kinds + timezone passthrough; migration mapping (routine → RitualSpec, Orbit → RitualSpec); idempotency (re-run creates nothing new); unpaired → no-op.
- **Integration (PR1):** daemon test with a stub Agent — migration creates the expected Rituals with correct cron + bearer token (mirror `ritual-routes.test.ts`).
- **Regression (PR2):** daemon `typecheck` + suite green with the three files deleted (no dangling refs); web suite green minus the removed local-routine/orbit tests; a check that general chat still composes its system prompt (nothing depended on the deleted Orbit builders).
- Baseline note: 3 pre-existing `default-background.test.ts` web failures are unrelated; no new failures allowed.

## 8. Out of scope

- Full ritual-management UI (create/edit/delete/run for arbitrary Rituals) — only the daily-briefing edit (enable+schedule) is added here; the rest is a later UI sub-project.
- Triggered Automations (2d).
- Any change to the Agent-side Ritual API (complete in 2b Stage 1).
- Destructive deletion of the local SQLite routines table file.

## 9. Build order

1. **PR1 — Migrate:** `scheduleToCron` + migration runner + tests (additive; local scheduler still runs). → **manual verify gate:** user pairs to a live Agent, confirms migrated Rituals appear + fire.
2. **PR2 — Remove:** delete the local scheduler (daemon + web), repoint Orbit settings, update tests — only after the gate passes.

Each PR is its own branch off main (per the standing "PR everything" rule; GitHub Actions stays disabled).
