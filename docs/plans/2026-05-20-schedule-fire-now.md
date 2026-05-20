# Schedule "Fire Now" Endpoint

Shipped: 2026-05-20 — `e97bd5c`

## Problem

Once the schedules CRUD shipped (`ae2861f`), the only way to actually exercise a recurring job end-to-end was to wait for the cron tick. That's fine for production but miserable for development and integration testing — a daily `0 0 9 * * MON *` schedule means the next observable side effect is up to a week away. The runtime path through the scheduler loop (`mark_fired` → `enqueue_run` with `triggered_by = "scheduled"`) is also subtly different from the manual `POST /api/project_members/{id}/run` path, so testing scheduled-trigger behaviour required either editing `next_fire_at` in the DB by hand or pointing the system clock somewhere unhelpful.

## Design

A single new route, mounted alongside the existing CRUD:

| Method | Path | Body | Response |
|---|---|---|---|
| `POST` | `/api/schedules/{id}/fire` | — | `200 + { schedule, run_id }` |

The handler `fire` lives next to the existing CRUD handlers in `packages/backend/api/src/scheduler/routes.rs`. It collapses the three things the scheduler loop does on a tick into one synchronous request:

1. `load_schedule_pg(id)` — 404 if the schedule doesn't exist.
2. Engagement check via `engagement_for_member` against the caller's JWT — 403 on mismatch, mirroring every other schedules endpoint.
3. Recompute `next_fire_at` from the schedule's existing `cron_expr` + `timezone` using the same `compute_next_fire_at` helper the create/update paths use. Corrupted rows (somehow holding an invalid cron) would otherwise surface as 500; instead they return 400 with a clear message.
4. `store.mark_fired(id, fired_at, next_fire_at)` — writes `last_fired_at = now()` and the recomputed `next_fire_at` so the periodic loop won't re-fire the same tick.
5. `run_job::enqueue_run(..., TRIGGERED_BY_SCHEDULED)` — same path the periodic loop uses. The `triggered_by` column on the resulting `runs` row reads `"scheduled"`, so audit queries can't tell a manual `/fire` apart from a real cron tick. That's intentional — the whole point is to exercise the production code path.

### Response shape

```rust
#[derive(Serialize)]
pub struct ScheduleFireResponse {
    pub schedule: Schedule,
    pub run_id: Uuid,
}
```

The caller gets the updated `Schedule` (so they can refresh their UI without a follow-up `GET`) plus the new `run_id` (so they can immediately subscribe to `/api/runs/{run_id}/events` to watch it execute).

### Why not pause-resume / dry-run as part of this?

This endpoint is intentionally a one-shot "fire as if the cron had ticked." It deliberately does NOT:

- Skip the `mark_fired` update — that would make `/fire` observably different from a real tick.
- Allow overriding `triggered_by` — the column stays `"scheduled"`. Manual-vs-scheduled audit slicing still works through the existing `/api/project_members/{id}/run` route.
- Bulk-fire all schedules — each call is single-id.

## Test coverage

Three new integration tests in `packages/backend/api/tests/scheduler_routes.rs`, all using `start_test_db()`:

- `fire_schedule_runs_member_and_marks_fired` — creates a schedule with a cron that won't naturally fire for nearly a year (`0 0 9 1 1 * *`), `POST`s `/fire`, then asserts: response carries a `run_id` + the schedule's `id`; response `last_fired_at` is within ±1s of "now"; DB row's `last_fired_at` is non-null and `next_fire_at` is in the future; polls `runs.triggered_by` for up to 5s with a 50ms delay and asserts it equals `"scheduled"`.
- `fire_unknown_schedule_returns_404` — asserts 404 for an unknown UUID.
- `fire_cross_engagement_returns_403` — seeds a schedule under engagement A, attempts to fire it with a token minted for engagement B, asserts 403.

The router setup in the test file mounts the route explicitly:

```rust
.route(
    "/api/schedules/{id}/fire",
    post(celerex_api::scheduler::routes::fire),
)
```

## What's not done yet

- **No bulk fire** — no `POST /api/schedules/fire?project_member_id=…` shortcut to drain a backlog in one call.
- **No dry-run mode** — there's no `?dry_run=true` that would recompute `next_fire_at` without writing it back or enqueueing a run. Useful for "show me what would happen" UI affordances.
- **No `triggered_by` override** — every `/fire` records `triggered_by = "scheduled"`. A future `POST /api/schedules/{id}/fire { triggered_by: "manual_override" }` would let UAT runs be sliced out of cron analytics.
- **No frontend button yet** — the SchedulesPanel doesn't surface this. Until then, hitting it from `curl` is the only way to use it.
- **No queue-depth protection** — a misbehaving caller can pound `/fire` and stack up runs. The scheduler loop is naturally rate-limited by cron arithmetic; this endpoint isn't.
