# Scheduler CRUD HTTP API

Shipped: 2026-05-19 — `ae2861f`

## Problem

The scheduler loop (`api/src/scheduler/loop.rs`) and store shipped in Sprint 1, but with no HTTP surface — schedules could only be inserted directly via SQL from integration tests. There was no way for the frontend or a user to create a recurring Project Member trigger.

## Design

Four endpoints, all engagement-scoped via the existing JWT `Claims` extractor:

| Method | Path | Body | Response |
|---|---|---|---|
| `POST` | `/api/schedules` | `{ project_member_id, cron_expr, timezone, enabled? }` | `201 + Schedule` |
| `GET` | `/api/schedules?project_member_id=<uuid>` | — | `200 + Schedule[]` |
| `PUT` | `/api/schedules/{id}` | same as POST | `200 + Schedule` |
| `DELETE` | `/api/schedules/{id}` | — | `204` |

### `SchedulesStore` trait extensions

Added to `api/src/scheduler/store.rs`:

```rust
async fn list_for_project_member(&self, project_member_id: Uuid) -> Result<Vec<Schedule>, String>;
async fn get(&self, id: Uuid) -> Result<Option<Schedule>, String>;
async fn update(&self, id: Uuid, cron_expr: &str, timezone: &str, enabled: bool, next_fire_at: DateTime<Utc>) -> Result<(), String>;
async fn delete(&self, id: Uuid) -> Result<(), String>;
```

`Schedule` gained `#[derive(Serialize)]` so route handlers can `Json(schedule)` it directly.

### Auth

Every endpoint resolves the relevant `project_member_id` to its `engagement_id` via `AppState::members.engagement_id_for(...)` and compares to `claims.engagement_id`. Mismatch → 403.

### Cron + timezone

`next_fire_at` is recomputed on every create/update via `cron::Schedule::from_str` + `chrono_tz::Tz`, then `.upcoming(tz).next()` converted to UTC. Bad cron or timezone → 400.

## Test coverage

`tests/scheduler_routes.rs` — 6 integration tests using `start_test_db()`:

- `create_schedule_returns_201_and_persists`
- `list_schedules_filters_by_project_member`
- `update_schedule_recomputes_next_fire_at`
- `delete_schedule_removes_row`
- `invalid_cron_returns_400`
- `cross_engagement_access_returns_403`

## What's not done yet

- No pause/resume convenience endpoint — must PUT the full body to flip `enabled`.
- No "fire now" override.
- No paginated list.
- No frontend UI.
- No "schedule fired" distinct notifier event.
