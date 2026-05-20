# Frontend Wiring Pass 1 — Builder / Monitor / Fleet / Schedules / Assistant / Engagements

Shipped: 2026-05-20 — `a582dc0` + `6e19413` + `e97bd5c` + `b19d20f`

## Problem

By the end of W5 the backend had a complete vertical slice — Project Member CRUD, manual + scheduled runs, SSE event stream, fleet view, multi-provider LLM registry, assistant chat, schedules CRUD — but the React app at `packages/frontend/app` was still wired against pre-pivot APIs. The chat panel pointed at a never-implemented `/api/chat`, the canvas had no Save/Run, schedules had no UI surface, the fleet dashboard didn't refresh, runs had nowhere to be observed, and engagements/projects were still managed via ad-hoc text inputs. This pass closes the loop so a user can build → save → run → watch in the browser, against the same backend the integration tests hit.

## Design

The cuts intentionally stay shallow — each panel does the obvious thing against the existing typed client and the existing jotai atoms. No new state machines, no router rewrite, no plugin UI.

### `client.ts` — typed surface for everything new

`packages/frontend/app/src/api/client.ts` gained typed helpers for every endpoint the backend exposed in W5/W6:

- **Schedules CRUD** — `createSchedule`, `listSchedules`, `updateSchedule`, `deleteSchedule` against `/api/schedules` (also `ApiSchedule` + `CreateScheduleRequest` types).
- **Workspace assistant** — `assistantChat(token, messages, providerId?, model?)` against `/api/assistant/chat`, returning `AssistantChatResponse { provider_id, model, raw }` with the provider's *raw* body untouched.
- **Provider introspection** — `listProviders(token)` against `/api/providers`, defensively reading `body.providers ?? []` so an older backend that returns a bare array still works.
- **Doc viewer (in flight, W7)** — `listEngagementDocs`, `getDoc`, `listDocBlocks` are stubbed in alongside `ApiDocBlock` / `ApiDocWithCount` so the doc-viewer page (landing in W7) has a typed entry point. These call the as-yet-unmerged `/api/docs/{id}` / `/api/docs/{id}/blocks` shapes the backend agent is wiring in parallel.

The legacy `sendChatMessage` (pointing at `/api/chat`, never implemented) is **left in place** so any pre-pivot screen still building against it keeps compiling. The AIPanel no longer calls it.

### AIPanel — workspace co-pilot rewired

`packages/frontend/app/src/layout/AIPanel.tsx` now goes through `assistantChat`:

- On mount (when `tokenAtom` is set), fires `listProviders(token)` and stores the result in `backendProviders`. The selected `backendProviderId` defaults to the first entry; failures are swallowed so the picker stays usable with the implicit "default" fallback.
- A new header row exposes a `<select>` for backend providers plus a free-text `modelOverride` (monospace input, "model (optional)" placeholder). The override beats the existing front-end-side `selectedModelAtom`; both are passed through to `/api/assistant/chat`.
- `extractReply(resp)` handles three raw shapes — Anthropic (`raw.content[0].text`), OpenAI (`raw.choices[0].message.content`), Gemini (`raw.candidates[0].content.parts[0].text`) — falling back to `"(no reply)"`. Kimi works incidentally because it returns the OpenAI shape.
- Errors surface as an `assistant` message bubble prefixed `Error:`; no toast spam.

History is **not** replayed to the backend — every send is single-turn. The frontend's `chatMessagesAtom` is the only conversation memory.

### BuilderPage — Save + Run wiring

`packages/frontend/app/src/pages/BuilderPage.tsx` now drives the canvas through the project-member API:

- **Hydration** — when `activeMemberIdAtom` is set, `getMember(token, id)` populates `name`, `graph`, and `providerConfig`.
- **Save** — `data-testid="save-agent"` button calls `updateMember` (existing) or `createMember` (new), stores the resulting id in `activeMemberIdAtom`, toasts `"Saved"`.
- **Run now** — `data-testid="run-now"` calls `createMember` first if there's no member yet, then `runMemberNow(token, id)`, drops the returned `run_id` into `activeRunIdAtom`, and flips `activeViewAtom` to `"monitor"`. The transition is the only navigation glue between the two pages.
- **Tab switcher** — once a member exists, a `Config | Schedules` toggle swaps the right pane between `NodeConfigPanel` and `SchedulesPanel`. Pre-save members only see Config (you can't schedule something that doesn't have an id yet).

### SchedulesPanel — schedule CRUD per member

`packages/frontend/app/src/components/SchedulesPanel.tsx` is the right-pane companion in BuilderPage:

- Lists existing schedules for `projectMemberId` in a table (cron / timezone / next fire / enabled / delete).
- Add-schedule form: cron text input, timezone `<select>` over a hardcoded list (`UTC`, `America/New_York`, `America/Los_Angeles`, `Europe/London`, `Asia/Tokyo`), enabled checkbox, "Add schedule" button. Submits to `createSchedule`, then re-fetches.
- Delete buttons fire `deleteSchedule` then re-fetch.
- Toasts on every success/failure via `addToastAtom`.

No pause/resume control yet — the user has to delete + re-add to flip `enabled`. The checkbox in the row is `readOnly`. (See "what's not done.")

### MonitorPage — SSE wiring with auto-close

`packages/frontend/app/src/pages/MonitorPage.tsx` uses the new `useRunEvents` hook from `packages/frontend/app/src/api/sse.ts`:

- `useRunEvents(runId)` opens `EventSource(/api/runs/{runId}/events)`, parses each `ServerEvent`, derives a `RunState` (per-step `idle | running | succeeded | failed`), tracks per-step `outputs` / `error` in an `io` map, and accumulates an ordered `events: RunEventLogEntry[]` log with monotonic `seq` and wall-clock `ts`.
- The hook **auto-closes** the EventSource on `run_completed` (and on `onerror`, since the browser sees the backend's deliberate close as an error). The returned `closed` flag flips to `true` so the status pill switches from `Live · N events` to `Run completed`.
- MonitorPage renders the canvas in `mode="monitor"` so node colour reflects `runState`, a `StepDrawer` for the selected step (outputs / error), and a fixed right-hand `RunEventsLog` (320px) that timestamps each event with colour-coded labels.
- When `runId` is null the log shows `"Start a run from the Builder to see live step events here."` — no error state.

### FleetDashboard — live polling

`packages/frontend/app/src/pages/FleetDashboard.tsx` now polls `fetchFleet(token)` every 5 seconds via `setInterval`:

- Tracks `lastUpdated: number | null` and renders `Last updated: HH:MM:SS` in the header (testid `fleet-last-updated`).
- Errors render as `Refresh error: <message>` in the same slot — the poll keeps retrying, so transient failures heal themselves.
- Each row carries `data-testid="fleet-row-{id}"` and clicks dispatch `setMember(r.id) + setView("monitor")` so an operator can drill from "running now" into the live event stream in one click.

### EngagementManager — two-pane modal

`packages/frontend/app/src/layout/EngagementManager.tsx` replaces the previous text-input flow with a proper modal:

- Left column: list of engagements with delete-X buttons + a create form.
- Right column: list of projects for the selected engagement, an active-project marker, and a create-project form. Project creation is JWT-scoped (`apiCreateProject(token, name)`) so it's gated behind a sign-in banner when `tokenAtom` is empty.
- On open, `refreshAll()` does `fetchEngagements()` and `listProjects(token)` in parallel; each failure is logged but doesn't block the other. The dialog stays usable in the partial-data case.
- Default selection on open: the active project's engagement if any, else the first engagement.
- Lots of `data-testid` hooks for the E2E harness (`engagement-manager`, `create-engagement-form`, `delete-engagement-{id}`, `create-project-form`, `project-row-{id}`, etc.).

### useProjectsSync — hydration on app load

`packages/frontend/app/src/hooks/useProjectsSync.ts` is a one-shot hook that fires once per app session: `listProjects(token)` → map → `setProjects(projectsAtom)`. Used by `Sidebar.tsx` so the project list is populated before the user opens the EngagementManager. Missing token = no-op (silently marks `synced: true`). API failures fall back to whatever's already in `projectsAtom` and log a `console.warn`. Exposes a `refresh()` callback for callers (currently unused but available for mutation paths).

## Test coverage

The frontend repo doesn't yet have a Vitest / Playwright harness running against these screens in CI. Each `data-testid` attribute was added in anticipation of the Playwright pass scheduled for W7. Smoke testing is currently manual against a live dev backend.

The backend half of the wiring (every endpoint these screens call) is covered by:
- `tests/scheduler_routes.rs` — schedules CRUD + `/fire`
- `tests/assistant_chat.rs` — `/api/assistant/chat` (4 tests)
- `tests/providers_endpoint.rs` — `/api/providers` (2 tests)
- `tests/fleet.rs` — `/api/project_members/fleet`
- `tests/project_members.rs` — member CRUD + manual run
- `tests/sse.rs` — `/api/runs/{id}/events`

## What's not done yet

- **No streaming chat responses** — AIPanel awaits the full `assistantChat` promise. Mirrors the backend's single-shot `Provider::complete`.
- **No doc viewer** — `getDoc` / `listEngagementDocs` / `listDocBlocks` are typed and wired but no page consumes them yet. W7 ships the viewer page.
- **No real auth / sign-in flow** — `tokenAtom` is populated out-of-band (dev-token paste or env). No login form, no refresh.
- **No run history view** — MonitorPage is "current run only", driven by `activeRunIdAtom`. You can drill from Fleet, but you can't browse past runs of a single member.
- **No plugin install UI** — the backend has plugin endpoints; nothing in the frontend surfaces them.
- **No member detail page** — Builder is the editor, Monitor is the live view; there's no "list all members under this project with their last status" between the two.
- **No settings page** — no UI for the engagement-level provider keys, the assistant model default, or notifier config.
- **No pause/resume on schedules** — the `enabled` checkbox is read-only; flipping requires delete + recreate.
- **No frontend tests yet** — the testids are scaffolding for the Playwright pass that lands in W7.
