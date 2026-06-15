# Retire the Local Daemon Scheduler — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire the local daemon scheduler by migrating existing local routines + the Orbit daily-briefing into always-on Agent Rituals, then removing the local scheduler.

**Architecture:** PR1 adds an idempotent, best-effort migration that runs after pairing — reads local routines (SQLite) + Orbit config, maps each to a `RitualSpec`, and creates it on the Agent via the `@celerex/connect` seam; local scheduler keeps running (additive). After a manual verify gate (user confirms migrated Rituals fire on their live Agent), PR2 deletes the local scheduler (`routines.ts`/`orbit.ts`/`routes/routine.ts` + `server.ts`/`media-routes.ts` wiring + web local-routine UI) and repoints the Orbit settings UI to manage the daily-briefing Ritual.

**Tech Stack:** TypeScript/Node (daemon, Express, better-sqlite3), `@celerex/connect` seam, React/Vite (web), vitest.

**Spec:** `docs/superpowers/specs/2026-06-14-retire-local-scheduler-design.md`
**Repo:** celerex-design. **Branches:** PR1 = `feat/rituals-migrate` off main; PR2 = `feat/rituals-retire-local` off main (after PR1 merges + verify gate).

---

## File Structure

**PR1 — Migration (additive):**
- Create: `apps/daemon/src/rituals-migration.ts` — `scheduleToCron` (pure) + `migrateLocalSchedulesToRituals` (injectable deps).
- Modify: `apps/daemon/src/app-config.ts` — add `ritualsMigration` marker field + ALLOWED_KEYS entry + validator.
- Modify: `apps/daemon/src/server.ts` — invoke the migration after `seedFromToken` in the connect flow (best-effort).
- Test: `apps/daemon/tests/rituals-migration.test.ts`.

**PR2 — Removal (gated):**
- Delete: `apps/daemon/src/routines.ts`, `apps/daemon/src/routes/routine.ts`, `apps/daemon/src/orbit.ts` (+ `orbit-agent-summary.ts` if orphaned).
- Modify: `apps/daemon/src/server.ts`, `apps/daemon/src/media-routes.ts` — strip wiring + `/api/routines` + `/api/orbit/*`; delete migrated local routines.
- Modify (web): `apps/web/src/components/TasksView.tsx` (drop local list, relabel), `apps/web/src/components/SettingsDialog.tsx` (repoint Orbit settings to the daily-briefing Ritual), remove `RoutinesSection` + `/api/orbit` calls; update/remove tests.

---

# PR1 — MIGRATION (additive; buildable now)

> Branch: `feat/rituals-migrate` off main. Test command: `pnpm --filter @celerex/daemon test rituals-migration`; typecheck: `pnpm --filter @celerex/daemon typecheck`. `exactOptionalPropertyTypes: true` is ON — never pass explicit `undefined` to an optional prop.

## Task 1: `scheduleToCron` pure helper

**Files:**
- Create: `apps/daemon/src/rituals-migration.ts`
- Test: `apps/daemon/tests/rituals-migration.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/daemon/tests/rituals-migration.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { scheduleToCron } from "../src/rituals-migration.js";

describe("scheduleToCron", () => {
  it("maps hourly to a minute-of-hour cron", () => {
    expect(scheduleToCron({ kind: "hourly", minute: 30 })).toEqual({
      cron_expr: "0 30 * * * *",
      timezone: "UTC",
    });
  });
  it("maps daily to seconds-first cron with timezone", () => {
    expect(scheduleToCron({ kind: "daily", time: "09:05", timezone: "America/New_York" })).toEqual({
      cron_expr: "0 5 9 * * *",
      timezone: "America/New_York",
    });
  });
  it("maps weekdays to a 1-5 day-of-week cron", () => {
    expect(scheduleToCron({ kind: "weekdays", time: "07:00", timezone: "UTC" })).toEqual({
      cron_expr: "0 0 7 * * 1-5",
      timezone: "UTC",
    });
  });
  it("maps weekly to a single day-of-week cron", () => {
    expect(scheduleToCron({ kind: "weekly", weekday: 1, time: "18:30", timezone: "UTC" })).toEqual({
      cron_expr: "0 30 18 * * 1",
      timezone: "UTC",
    });
  });
});
```

- [ ] **Step 2: Run it — expect FAIL (module missing)**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: FAIL — cannot find `../src/rituals-migration.js`.

- [ ] **Step 3: Implement `scheduleToCron`**

Create `apps/daemon/src/rituals-migration.ts`:

```ts
// Migrate local daemon schedules (routines + Orbit) into Agent Rituals. The
// local scheduler is retired in a follow-up; this module is additive and
// idempotent. See docs/superpowers/specs/2026-06-14-retire-local-scheduler-design.md

// Mirrors RoutineSchedule from ./routines.ts (kept local to avoid importing the
// module being retired).
export type RoutineSchedule =
  | { kind: "hourly"; minute: number }
  | { kind: "daily"; time: string; timezone: string }
  | { kind: "weekdays"; time: string; timezone: string }
  | { kind: "weekly"; weekday: number; time: string; timezone: string };

export interface CronSchedule {
  cron_expr: string;
  timezone: string;
}

function hhmm(time: string): { hh: number; mm: number } {
  const [h, m] = time.split(":");
  return { hh: Number(h), mm: Number(m) };
}

/** Map a local RoutineSchedule to the Agent's 6-field (seconds-first) cron. */
export function scheduleToCron(s: RoutineSchedule): CronSchedule {
  switch (s.kind) {
    case "hourly":
      return { cron_expr: `0 ${s.minute} * * * *`, timezone: "UTC" };
    case "daily": {
      const { hh, mm } = hhmm(s.time);
      return { cron_expr: `0 ${mm} ${hh} * * *`, timezone: s.timezone };
    }
    case "weekdays": {
      const { hh, mm } = hhmm(s.time);
      return { cron_expr: `0 ${mm} ${hh} * * 1-5`, timezone: s.timezone };
    }
    case "weekly": {
      const { hh, mm } = hhmm(s.time);
      return { cron_expr: `0 ${mm} ${hh} * * ${s.weekday}`, timezone: s.timezone };
    }
  }
}
```

- [ ] **Step 4: Run it — expect PASS**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: PASS (4 tests).

> If the real `RoutineSchedule` in `apps/daemon/src/routines.ts` differs (field names/extra kinds), align this local copy to it exactly and adjust the tests. Read `routines.ts` lines around the `RoutineSchedule` type to confirm before finalizing.

- [ ] **Step 5: Commit**

```bash
git add apps/daemon/src/rituals-migration.ts apps/daemon/tests/rituals-migration.test.ts
git commit -m "feat(rituals): scheduleToCron — map local schedules to Agent cron"
```

## Task 2: app-config migration marker

**Files:**
- Modify: `apps/daemon/src/app-config.ts` (interface near line 117, ALLOWED_KEYS near 150, validators near 280)
- Test: `apps/daemon/tests/rituals-migration.test.ts` (append) — or an app-config test if one exists; check first.

- [ ] **Step 1: Recon the app-config validator pattern**

Read `apps/daemon/src/app-config.ts` around `interface AppConfigPrefs` (117), `ALLOWED_KEYS` (150), `validateOrbit`/`validateCelerex` (280/299), and how `readAppConfig`/`writeAppConfig` apply validators. Mirror that pattern exactly for the new field.

- [ ] **Step 2: Write the failing test**

Append to `apps/daemon/tests/rituals-migration.test.ts`:

```ts
import { readAppConfig, writeAppConfig } from "../src/app-config.js";

describe("app-config ritualsMigration marker", () => {
  const dataDir = process.env.OD_DATA_DIR as string;
  it("round-trips migrated routine ids + orbit marker", async () => {
    await writeAppConfig(dataDir, {
      ritualsMigration: { migratedRoutineIds: ["r1", "r2"], orbitMigratedAt: "2026-06-15T00:00:00Z" },
    });
    const cfg = await readAppConfig(dataDir);
    expect(cfg.ritualsMigration?.migratedRoutineIds).toEqual(["r1", "r2"]);
    expect(cfg.ritualsMigration?.orbitMigratedAt).toBe("2026-06-15T00:00:00Z");
  });
});
```

> Confirm how other daemon tests set `OD_DATA_DIR` / the data dir (the connectors test uses `process.env.OD_DATA_DIR`). Match that setup; if a global test setup provides it, rely on the same.

- [ ] **Step 3: Run it — expect FAIL**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: FAIL — `ritualsMigration` stripped (not in ALLOWED_KEYS) so it reads back undefined.

- [ ] **Step 4: Implement the field**

In `apps/daemon/src/app-config.ts`:

Add to `interface AppConfigPrefs` (near the other optional fields):
```ts
  // One-time local-scheduler → Agent Ritual migration bookkeeping. Per-routine
  // ids already migrated + when Orbit was migrated, so re-runs don't duplicate.
  ritualsMigration?: RitualsMigrationMarker | null;
```
Add the type (near the other Prefs interfaces):
```ts
export interface RitualsMigrationMarker {
  migratedRoutineIds: string[];
  orbitMigratedAt: string | null;
}
```
Add `'ritualsMigration'` to the `ALLOWED_KEYS` set. Add a validator mirroring `validateOrbit`/`validateCelerex`:
```ts
function validateRitualsMigration(raw: unknown): RitualsMigrationMarker | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const r = raw as Record<string, unknown>;
  const ids = Array.isArray(r.migratedRoutineIds)
    ? r.migratedRoutineIds.filter((x): x is string => typeof x === "string")
    : [];
  const orbitMigratedAt = typeof r.orbitMigratedAt === "string" ? r.orbitMigratedAt : null;
  return { migratedRoutineIds: ids, orbitMigratedAt };
}
```
Wire `validateRitualsMigration` into the read/normalize path exactly where `validateOrbit`/`validateCelerex` are applied (follow the existing switch/branch in `readAppConfig` — there is a `if (key === 'connectorCredentials')` style branch near line 484; add the analogous handling so the field is validated on read and allowed on write).

- [ ] **Step 5: Run it — expect PASS**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: PASS.

- [ ] **Step 6: Typecheck + commit**

```bash
pnpm --filter @celerex/daemon typecheck
git add apps/daemon/src/app-config.ts apps/daemon/tests/rituals-migration.test.ts
git commit -m "feat(rituals): app-config ritualsMigration idempotency marker"
```

## Task 3: migration runner (injectable, idempotent)

**Files:**
- Modify: `apps/daemon/src/rituals-migration.ts`
- Test: `apps/daemon/tests/rituals-migration.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append to `apps/daemon/tests/rituals-migration.test.ts`:

```ts
import { migrateLocalSchedulesToRituals } from "../src/rituals-migration.js";

describe("migrateLocalSchedulesToRituals", () => {
  it("creates a ritual per un-migrated routine and the orbit briefing, idempotently", async () => {
    const created: { name: string; cron: string }[] = [];
    let marker = { migratedRoutineIds: [] as string[], orbitMigratedAt: null as string | null };

    const deps = {
      listRoutines: () => [
        { id: "r1", name: "Daily digest", prompt: "Summarize", skillId: "x", schedule: { kind: "daily", time: "09:00", timezone: "UTC" } },
      ],
      orbitConfig: { enabled: true, time: "07:00" },
      createRitual: async (spec: any) => {
        created.push({ name: spec.name, cron: spec.schedule.cron_expr });
        return { ritual_id: "rit-" + spec.name } as any;
      },
      readMarker: () => marker,
      writeMarker: (m: typeof marker) => { marker = m; },
      now: () => "2026-06-15T00:00:00Z",
    };

    const report1 = await migrateLocalSchedulesToRituals(deps as any);
    expect(report1.createdRoutines).toBe(1);
    expect(report1.createdOrbit).toBe(true);
    expect(created.map((c) => c.name).sort()).toContain("Daily digest");
    expect(marker.migratedRoutineIds).toContain("r1");
    expect(marker.orbitMigratedAt).toBe("2026-06-15T00:00:00Z");

    // second run: nothing new
    const report2 = await migrateLocalSchedulesToRituals(deps as any);
    expect(report2.createdRoutines).toBe(0);
    expect(report2.createdOrbit).toBe(false);
    expect(created).toHaveLength(2); // unchanged (1 routine + 1 orbit from run 1)
  });

  it("records a failure without marking the routine migrated", async () => {
    let marker = { migratedRoutineIds: [] as string[], orbitMigratedAt: "done" as string | null };
    const deps = {
      listRoutines: () => [{ id: "r1", name: "X", prompt: "p", skillId: null, schedule: { kind: "hourly", minute: 0 } }],
      orbitConfig: { enabled: false, time: "07:00" },
      createRitual: async () => { throw new Error("agent down"); },
      readMarker: () => marker,
      writeMarker: (m: typeof marker) => { marker = m; },
      now: () => "t",
    };
    const report = await migrateLocalSchedulesToRituals(deps as any);
    expect(report.failed).toBe(1);
    expect(report.createdRoutines).toBe(0);
    expect(marker.migratedRoutineIds).not.toContain("r1");
  });
});
```

- [ ] **Step 2: Run it — expect FAIL**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: FAIL — `migrateLocalSchedulesToRituals` not exported.

- [ ] **Step 3: Implement the runner**

Append to `apps/daemon/src/rituals-migration.ts`:

```ts
import type { RitualSpec, RitualView } from "@celerex/connect";

export interface MigratedRoutine {
  id: string;
  name: string;
  prompt: string;
  skillId: string | null;
  schedule: RoutineSchedule;
}

export interface MigrationMarker {
  migratedRoutineIds: string[];
  orbitMigratedAt: string | null;
}

export interface MigrationDeps {
  listRoutines: () => MigratedRoutine[];
  orbitConfig: { enabled: boolean; time: string } | null;
  createRitual: (spec: RitualSpec) => Promise<RitualView>;
  readMarker: () => MigrationMarker;
  writeMarker: (m: MigrationMarker) => void;
  now: () => string;
  /** local timezone for the Orbit daily schedule; defaults to UTC. */
  timezone?: string;
}

export interface MigrationReport {
  createdRoutines: number;
  createdOrbit: boolean;
  skipped: number;
  failed: number;
}

const ORBIT_BRIEFING_PROMPT =
  "Produce my daily briefing: summarize recent activity across my connected sources and surface anything that needs attention.";
const ORBIT_SKILL_ID = "orbit-general";

/** Best-effort, idempotent migration of local routines + Orbit into Agent Rituals. */
export async function migrateLocalSchedulesToRituals(deps: MigrationDeps): Promise<MigrationReport> {
  const marker = deps.readMarker();
  const report: MigrationReport = { createdRoutines: 0, createdOrbit: false, skipped: 0, failed: 0 };
  const migrated = new Set(marker.migratedRoutineIds);

  for (const routine of deps.listRoutines()) {
    if (migrated.has(routine.id)) {
      report.skipped += 1;
      continue;
    }
    const schedule = scheduleToCron(routine.schedule);
    const body =
      routine.skillId != null
        ? { kind: "agent_run" as const, prompt: routine.prompt, skill_template_id: routine.skillId }
        : { kind: "agent_run" as const, prompt: routine.prompt };
    try {
      await deps.createRitual({ name: routine.name, enabled: true, schedule, body });
      migrated.add(routine.id);
      report.createdRoutines += 1;
    } catch {
      report.failed += 1; // leave unmarked so a later run retries
    }
  }

  let orbitMigratedAt = marker.orbitMigratedAt;
  if (deps.orbitConfig?.enabled && marker.orbitMigratedAt == null) {
    const schedule = scheduleToCron({ kind: "daily", time: deps.orbitConfig.time, timezone: deps.timezone ?? "UTC" });
    try {
      await deps.createRitual({
        name: "Daily briefing",
        enabled: true,
        schedule,
        body: { kind: "agent_run", prompt: ORBIT_BRIEFING_PROMPT, skill_template_id: ORBIT_SKILL_ID },
      });
      orbitMigratedAt = deps.now();
      report.createdOrbit = true;
    } catch {
      report.failed += 1;
    }
  }

  deps.writeMarker({ migratedRoutineIds: Array.from(migrated), orbitMigratedAt });
  return report;
}
```

> Note: `body` is built WITHOUT an explicit `undefined` for `skill_template_id` (omitted when null) to satisfy `exactOptionalPropertyTypes`.

- [ ] **Step 4: Run it — expect PASS**

Run: `pnpm --filter @celerex/daemon test rituals-migration`
Expected: PASS (all groups).

- [ ] **Step 5: Typecheck + commit**

```bash
pnpm --filter @celerex/daemon typecheck
git add apps/daemon/src/rituals-migration.ts apps/daemon/tests/rituals-migration.test.ts
git commit -m "feat(rituals): idempotent local→Agent migration runner"
```

## Task 4: wire migration into the connect flow

**Files:**
- Modify: `apps/daemon/src/server.ts` (the `/api/celerex/connect` handler, after `seedFromToken`; routine store `db` at ~5053, `listRoutines` imported at ~442)
- Test: `apps/daemon/tests/rituals-migration-connect.test.ts` (new) — mirror `tests/celerex-connect-connectors.test.ts`

- [ ] **Step 1: Recon the connect handler + db/routine access**

Read `server.ts` around `seedFromToken` (10558) and the `/api/celerex/connect` handler (10643), plus how `db` (5053) + `listRoutines`/`routineDbRowToContract` are used (5067). Determine where, after a successful connect + `seedFromToken`, you have the pairing (`serverUrl`+`deviceToken` from app-config `celerex`) and `db` in scope to invoke the migration.

- [ ] **Step 2: Write the failing test**

Create `apps/daemon/tests/rituals-migration-connect.test.ts` mirroring `celerex-connect-connectors.test.ts`: stub Agent that (a) handles `POST /api/celerex/connect` prerequisites the connectors test uses, (b) records `POST /api/design/rituals` calls. Seed a local routine into the daemon's store before connecting (use the same `insertRoutine(db, ...)` the server uses, or the daemon's routine API `POST /api/routines` if reachable in the test). After `POST /api/celerex/connect`, assert the stub Agent received a `POST /api/design/rituals` for that routine.

> If seeding a routine into the test DB is awkward, assert the weaker-but-real invariant: after connect with Orbit enabled in app-config, the stub Agent receives a `POST /api/design/rituals` named "Daily briefing". Pick whichever the harness supports cleanly; the goal is proving the connect flow triggers migration.

- [ ] **Step 3: Run it — expect FAIL**

Run: `pnpm --filter @celerex/daemon test rituals-migration-connect`
Expected: FAIL — no migration call yet.

- [ ] **Step 4: Implement the hook**

In the `/api/celerex/connect` handler, after `seedFromToken(...)` resolves successfully, invoke the migration best-effort (never throw into the response). Read the pairing from app-config `celerex`; build `MigrationDeps` from the real sources:
- `listRoutines: () => listRoutines(db).map(routineDbRowToContract …)` mapped to `MigratedRoutine` (id, name, prompt, skillId, schedule).
- `orbitConfig`: `readAppConfig(RUNTIME_DATA_DIR).orbit ?? null`.
- `createRitual: (spec) => createRitual(pairing.serverUrl, pairing.deviceToken, spec)` (import `createRitual` from `@celerex/connect`).
- `readMarker`/`writeMarker`: read/write `app-config.ritualsMigration` (default `{ migratedRoutineIds: [], orbitMigratedAt: null }`).
- `now: () => new Date().toISOString()`.
Wrap the whole call in try/catch; log a `tracing`/console.warn on failure. Example:
```ts
try {
  const cfg = await readAppConfig(RUNTIME_DATA_DIR);
  const pairing = cfg.celerex;
  if (pairing?.serverUrl && pairing.deviceToken) {
    const report = await migrateLocalSchedulesToRituals({
      listRoutines: () => listRoutines(db).map((row) => {
        const c = routineDbRowToContract(row, null);
        return { id: c.id, name: c.name, prompt: c.prompt, skillId: c.skillId ?? null, schedule: c.schedule };
      }),
      orbitConfig: cfg.orbit ?? null,
      createRitual: (spec) => createRitual(pairing.serverUrl!, pairing.deviceToken!, spec),
      readMarker: () => cfg.ritualsMigration ?? { migratedRoutineIds: [], orbitMigratedAt: null },
      writeMarker: (m) => { void writeAppConfig(RUNTIME_DATA_DIR, { ritualsMigration: m }); },
      now: () => new Date().toISOString(),
    });
    console.info("[rituals] migration report", report);
  }
} catch (err) {
  console.warn("[rituals] migration failed (non-fatal)", err);
}
```
Adapt symbol names to the real ones found in recon (e.g. the routine contract's field names).

- [ ] **Step 5: Run it — expect PASS**

Run: `pnpm --filter @celerex/daemon test rituals-migration-connect`
Expected: PASS.

- [ ] **Step 6: Typecheck + commit**

```bash
pnpm --filter @celerex/daemon typecheck
git add apps/daemon/src/server.ts apps/daemon/tests/rituals-migration-connect.test.ts
git commit -m "feat(rituals): run local→Agent migration after pairing (best-effort)"
```

## Task 5: PR1 verification + PR

- [ ] **Step 1: Full daemon checks**

Run: `pnpm --filter @celerex/daemon typecheck && pnpm --filter @celerex/daemon test` — green (note any pre-existing failures, don't fix unrelated ones).

- [ ] **Step 2: Open the PR**

```bash
git push -u origin feat/rituals-migrate
gh pr create --fill --base main --title "feat(rituals): migrate local routines + Orbit to Agent Rituals (2f PR1)"
```

- [ ] **Step 3: MANUAL VERIFY GATE (user)**

Do NOT start PR2 until the user confirms: with the desktop paired to a live Agent, the migrated Rituals appear in the Rituals surface and fire on schedule (or run-now). This is the "Agent Rituals proven in real use" gate. STOP here in an autonomous run.

---

# PR2 — REMOVAL (gated on the verify gate)

> Branch: `feat/rituals-retire-local` off main, only after PR1 merges AND the user confirms migration works live. tsc-driven surgery on the 14k-line `server.ts`.

## Task 6: web — drop the local routines list

**Files:**
- Modify: `apps/web/src/components/TasksView.tsx` (remove the `/api/routines` fetch + "Your rituals" local list + `RoutinesSection` usage; relabel the "Agent rituals" section to "Rituals")
- Modify/Delete: the `RoutinesSection` component if it becomes unused
- Test: update `apps/web/tests/...` that asserted on `/api/routines`

- [ ] **Step 1: Recon** — find every `/api/routines` reference + `RoutinesSection` usage in `apps/web/src`. List them.
- [ ] **Step 2: Write/adjust the failing test** — update the TasksView test(s) to expect only the Agent-rituals "Rituals" section and NO `/api/routines` fetch. Run; expect FAIL against current code.
- [ ] **Step 3: Implement** — remove the local routine fetch + list rendering; relabel the Agent section to "Rituals"; delete `RoutinesSection` if now unused.
- [ ] **Step 4: Run** — `pnpm --filter @celerex/web typecheck && pnpm --filter @celerex/web test` (baseline minus removed tests; 3 known `default-background` failures acceptable; no new failures).
- [ ] **Step 5: Commit** — `git commit -m "refactor(rituals): web shows only Agent Rituals; drop local routines list"`

## Task 7: web — repoint Orbit settings to the daily-briefing Ritual

**Files:**
- Modify: `apps/web/src/components/SettingsDialog.tsx` (the Orbit settings section)
- Test: `apps/web/tests/...SettingsDialog...` (the orbit settings test)

- [ ] **Step 1: Recon** — read the current Orbit settings UI + its `/api/orbit`/`config.orbit` reads/writes; identify the daily-briefing Ritual by name ("Daily briefing") via `GET /api/rituals`.
- [ ] **Step 2: Write the failing test** — the settings section loads the "Daily briefing" Ritual from `/api/rituals` and, on toggle/schedule change, calls `PUT /api/rituals/:id` (enable/disable + schedule) and (optionally) `DELETE`/create when enabling from absent. Run; expect FAIL.
- [ ] **Step 3: Implement** — repoint the section: read the daily-briefing Ritual from `/api/rituals`; write changes via `PUT /api/rituals/:id` (add a small web client fn in `state/rituals.ts` — `updateRitual(id, patch)` calling the daemon, mirroring `fetchRituals`). Remove `/api/orbit` calls.
- [ ] **Step 4: Run** — typecheck + tests green.
- [ ] **Step 5: Commit** — `git commit -m "refactor(rituals): Orbit settings manage the daily-briefing Agent Ritual"`

## Task 8: daemon — delete the local scheduler

**Files:**
- Delete: `apps/daemon/src/routines.ts`, `apps/daemon/src/routes/routine.ts`, `apps/daemon/src/orbit.ts` (+ `orbit-agent-summary.ts` if orphaned)
- Modify: `apps/daemon/src/server.ts`, `apps/daemon/src/media-routes.ts`

- [ ] **Step 1: Recon** — list every import/reference to the three modules + `/api/routines` + `/api/orbit/*` across `server.ts` and `media-routes.ts` (grep). Confirm (already established) that `buildOrbitSystemPrompt`/`renderOrbitTemplateSystemPrompt` are used ONLY by the Orbit run path, not general chat.
- [ ] **Step 2: Delete migrated local routines after migration** — in the connect/migration path, after a routine is confirmed migrated (in the marker), call `dbDeleteRoutine(db, id)` so the local store stops firing it. (Decide: do this in PR2 so PR1 stays purely additive.) Add/adjust a test asserting a migrated routine is deleted locally.
- [ ] **Step 3: Remove the scheduler wiring** — delete the three files; remove imports (`OrbitService`, `renderOrbitTemplateSystemPrompt`, `RoutineService`, `registerRoutineRoutes`, `routineDbRowToContract`), the `orbitService`/`routineService` instances, the Orbit run handler (`setRunHandler`/`setTemplateResolver`), `orbitDeps`, the `registerRoutineRoutes(app, …)` call, and the `/api/orbit/*` routes in BOTH `server.ts` and `media-routes.ts`.
- [ ] **Step 4: tsc-drive to clean** — `pnpm --filter @celerex/daemon typecheck` repeatedly until no dangling references; remove now-unused store helpers (`listRoutines`, `insertRoutine`, etc.) only if unreferenced.
- [ ] **Step 5: Run** — `pnpm --filter @celerex/daemon test` green (remove the now-obsolete routine/orbit route tests; keep `ritual-routes` + migration tests). Verify general chat system-prompt composition still works.
- [ ] **Step 6: Commit** — `git commit -m "refactor(rituals): retire the local daemon scheduler (routines + Orbit)"`

## Task 9: PR2 verification + PR

- [ ] **Step 1: Full checks** — daemon + web `typecheck` + `test` green (baseline; no new failures).
- [ ] **Step 2: Open the PR**

```bash
git push -u origin feat/rituals-retire-local
gh pr create --fill --base main --title "refactor(rituals): retire local scheduler; repoint Orbit settings (2f PR2)"
```

---

## Self-review notes (for the executor)

- **Spec coverage:** §4.1 scheduleToCron → Task 1. §4.2 migration runner + idempotency → Tasks 2–3. connect hook → Task 4. §4.4 daemon removal → Task 8. §4.5 web removal + Orbit repoint → Tasks 6–7. §1 two-PR verify gate → Task 5 Step 3. §7 testing → each task is TDD.
- **Type consistency:** `RitualSpec`/`RitualView` come from `@celerex/connect` (Stage 2). `RoutineSchedule` is mirrored locally in `rituals-migration.ts` (Task 1) and reused in the runner (Task 3). `RitualsMigrationMarker` (app-config, Task 2) and `MigrationMarker` (runner deps, Task 3) have the same shape `{ migratedRoutineIds: string[]; orbitMigratedAt: string | null }` — the connect hook (Task 4) adapts between them (they're structurally identical).
- **In-situ confirmations (not placeholders — integration points):** the real `RoutineSchedule` shape + routine-contract field names (`routines.ts`, `routes/routine.ts`); the app-config validator wiring site; the connect-handler scope for `db` + pairing; the web `/api/routines` + `RoutinesSection` + Orbit-settings call sites. Each task begins with the targeted recon needed.
- **Known baseline:** 3 pre-existing `default-background.test.ts` web failures are acceptable; no new failures.
