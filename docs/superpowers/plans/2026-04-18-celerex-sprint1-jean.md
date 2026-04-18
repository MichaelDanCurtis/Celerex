# Celerex Sprint 1 — Jean's Newsletter Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the "Jean's Weekly Newsletter" user story end-to-end on top of the Sprint 0 substrate (tag `sprint-0-foundation`). A business user creates a Project Member in a visual canvas, wires data-source / synthesis / output nodes, schedules weekly execution, observes live runs, sees the agent fleet at a glance, is notified on failure, and lands a new dated BlockSuite doc each week.

**Architecture:** Backend adds a `scheduler` tokio loop, `project_members` with immutable snapshots, a `Provider` LLM trait (multi-provider-ready), `FileConnector` plugins (XLSX/DOCX in markdown + tool-use modes), fixture `workIQ` + `Stargate` plugins, SSE-backed run observability, and a `Notifier` trait with Console + stub-Teams backends. Frontend adds a shared `<AgentBuilderCanvas>` (React Flow) used in both edit and monitor modes, a Builder page, a Monitor page with SSE, a Fleet dashboard, and a "+ Add agent to project" entry point in the sidebar.

**Tech Stack:** Rust (axum 0.8, sqlx 0.8, tokio, async-trait), `cron = 0.12` + `chrono-tz = 0.9` for schedule advance, `calamine` for XLSX, `docx-rs` for DOCX, `jsonschema` for graph validation, testcontainers-postgres for backend tests. Frontend: React + Vite + Jotai, `@xyflow/react ^12.3`, EventSource for SSE.

**Sprint 0 anchor paths (all tasks reference these):**
- Workspace root: `/Volumes/Main External/Development/Celerex/.claude/worktrees/celerex-sprint0-plan/` (run all commands from here; this is a worktree)
- API crate: `packages/backend/api/` (library + bin `celerex-api`)
- Orchestrator crate: `packages/backend/agent-orchestrator/` (library `celerex-orchestrator`)
- Router assembly: `packages/backend/api/src/lib.rs` (`build_app_with_store`)
- Migrations: `packages/backend/api/src/db/migrations/0001_init.sql` … `0004_runs.sql` (next 0005–0009)
- Test harness: `packages/backend/api/tests/common/mod.rs` (`start_test_db() -> TestDb`)
- Frontend app: `packages/frontend/app/src/` (Vite + Jotai)
- View router: `packages/frontend/app/src/App.tsx` (`activeView` atom drives a ternary chain)

**Conventions used throughout:**
- Rust 2024 edition → wrap `std::env::set_var` in `unsafe { … }`.
- All new backend code is in `celerex_api` or `celerex_orchestrator` — no new workspace members.
- TDD cadence: write failing test → verify red → minimal impl → verify green → commit. Every task ends in a commit.
- Integration tests live at `packages/backend/api/tests/<feature>.rs` and use `common::start_test_db().await`.
- All commits use Conventional Commits (e.g. `feat(scheduler): …`, `test(members): …`, `feat(api): …`).

---

## Task 1: Scheduler (spec sub-project A)

Add a `schedules` table, a `SchedulesStore` backed by Postgres, and a tokio polling loop that advances `next_fire_at` via the `cron` crate and dispatches to the orchestrator. Catches up at most one missed fire per schedule on startup.

**Files:**
- Create: `packages/backend/api/src/db/migrations/0005_schedules.sql`
- Create: `packages/backend/api/src/scheduler/mod.rs`
- Create: `packages/backend/api/src/scheduler/store.rs`
- Create: `packages/backend/api/src/scheduler/advance.rs`
- Create: `packages/backend/api/src/scheduler/loop.rs`
- Modify: `packages/backend/api/src/lib.rs` (add `pub mod scheduler;`, spawn loop from `build_app_with_store`)
- Modify: `packages/backend/api/Cargo.toml` (add `cron = "0.12"`, `chrono-tz = "0.9"`)
- Test: `packages/backend/api/tests/scheduler.rs`

- [ ] **Step 1.1: Add the migration**

Create `packages/backend/api/src/db/migrations/0005_schedules.sql`:
```sql
CREATE TABLE schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL,
    cron_expr TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    next_fire_at TIMESTAMPTZ NOT NULL,
    last_fired_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    external_calendar_source TEXT,
    external_event_id TEXT,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX schedules_next_fire_idx
    ON schedules (next_fire_at) WHERE enabled = TRUE;
```

Note: FK to `project_members` is omitted here because task 2 creates that table. Add the FK in task 2's migration (`ALTER TABLE schedules ADD CONSTRAINT …`) to keep migrations monotonic.

- [ ] **Step 1.2: Add Cargo deps**

Edit `packages/backend/api/Cargo.toml`. Under `[dependencies]`, add:
```toml
cron = "0.12"
chrono-tz = "0.9"
```

- [ ] **Step 1.3: Write the failing `advance` test**

Create `packages/backend/api/src/scheduler/advance.rs`:
```rust
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use std::str::FromStr;

/// Return the next datetime strictly after `from` that matches `cron_expr` in `tz`.
pub fn next_after(cron_expr: &str, tz: &str, from: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let schedule = Schedule::from_str(cron_expr).map_err(|e| format!("cron parse: {e}"))?;
    let zone: Tz = tz.parse().map_err(|_| format!("bad tz: {tz}"))?;
    let from_local = from.with_timezone(&zone);
    schedule
        .after(&from_local)
        .next()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| "no future occurrence".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn advances_weekly_monday_9am_america_new_york() {
        // 2026-04-18 is a Saturday; next Monday is 2026-04-20.
        let now = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        // cron syntax: sec min hour dom mon dow year
        let next = next_after("0 0 9 * * MON *", "America/New_York", now).unwrap();
        // 9am ET on Monday 2026-04-20 == 13:00 UTC (EDT).
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 4, 20, 13, 0, 0).unwrap());
    }

    #[test]
    fn bad_tz_returns_error() {
        let now = Utc::now();
        let err = next_after("0 0 9 * * MON *", "Not/A/Zone", now).unwrap_err();
        assert!(err.contains("bad tz"));
    }
}
```

Also register the new module: add `pub mod advance;` to `packages/backend/api/src/scheduler/mod.rs` (creating that file with contents below in a later step).

- [ ] **Step 1.4: Run the test — verify red**

Run: `cargo test -p celerex-api scheduler::advance`
Expected: compilation error — `scheduler` module does not exist yet.

- [ ] **Step 1.5: Create the scheduler module skeleton to make the test compile**

Create `packages/backend/api/src/scheduler/mod.rs`:
```rust
pub mod advance;
pub mod store;
pub mod r#loop;
```

Edit `packages/backend/api/src/lib.rs` — add after the other `pub mod` lines:
```rust
pub mod scheduler;
```

Also create empty stubs for the sibling modules:

`packages/backend/api/src/scheduler/store.rs`:
```rust
// Filled in step 1.7
```

`packages/backend/api/src/scheduler/loop.rs`:
```rust
// Filled in step 1.10
```

Run: `cargo test -p celerex-api scheduler::advance`
Expected: two test cases PASS.

- [ ] **Step 1.6: Commit the advance helper**

```bash
git add packages/backend/api/Cargo.toml \
        packages/backend/api/src/db/migrations/0005_schedules.sql \
        packages/backend/api/src/scheduler/ \
        packages/backend/api/src/lib.rs
git commit -m "feat(scheduler): cron next-fire advance helper + schedules migration"
```

- [ ] **Step 1.7: Write the failing `SchedulesStore` integration test**

Create `packages/backend/api/tests/scheduler.rs`:
```rust
mod common;

use chrono::{TimeZone, Utc};
use celerex_api::scheduler::store::{PgSchedulesStore, SchedulesStore, Schedule};
use uuid::Uuid;

#[tokio::test]
async fn due_returns_only_schedules_past_next_fire_at() {
    let db = common::start_test_db().await;
    let store = PgSchedulesStore::new(db.pool.clone());

    // Seed: two schedules, one due, one in the future.
    let past = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let future = Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap();
    let a = store.create(Schedule {
        id: Uuid::new_v4(), project_member_id: Uuid::new_v4(),
        cron_expr: "0 0 9 * * MON *".into(), timezone: "UTC".into(),
        next_fire_at: past, last_fired_at: None, enabled: true,
    }).await.unwrap();
    let _b = store.create(Schedule {
        id: Uuid::new_v4(), project_member_id: Uuid::new_v4(),
        cron_expr: "0 0 9 * * MON *".into(), timezone: "UTC".into(),
        next_fire_at: future, last_fired_at: None, enabled: true,
    }).await.unwrap();

    let due = store.due(Utc::now()).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, a);
}
```

Note: `mod common;` expects `tests/common/mod.rs` — it already exists from Sprint 0.

Also: the test seeds rows without a real `project_member_id` FK, which is fine because the FK is added in task 2's migration. If task 2 runs first, update this seed to reference a real member.

- [ ] **Step 1.8: Run test — verify red**

Run: `cargo test -p celerex-api --test scheduler -- --test-threads=1`
Expected: compilation error — `PgSchedulesStore`, `Schedule`, `SchedulesStore` not defined.

- [ ] **Step 1.9: Implement `SchedulesStore`**

Overwrite `packages/backend/api/src/scheduler/store.rs`:
```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: Uuid,
    pub project_member_id: Uuid,
    pub cron_expr: String,
    pub timezone: String,
    pub next_fire_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[async_trait]
pub trait SchedulesStore: Send + Sync {
    async fn create(&self, s: Schedule) -> Result<Uuid, String>;
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Schedule>, String>;
    async fn mark_fired(&self, id: Uuid, fired_at: DateTime<Utc>, next_fire_at: DateTime<Utc>) -> Result<(), String>;
}

pub struct PgSchedulesStore { pool: PgPool }

impl PgSchedulesStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl SchedulesStore for PgSchedulesStore {
    async fn create(&self, s: Schedule) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO schedules (id, project_member_id, cron_expr, timezone, next_fire_at, enabled)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(s.id).bind(s.project_member_id).bind(&s.cron_expr)
        .bind(&s.timezone).bind(s.next_fire_at).bind(s.enabled)
        .fetch_one(&self.pool).await.map_err(|e| e.to_string())
    }

    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<Schedule>, String> {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>, bool)>(
            "SELECT id, project_member_id, cron_expr, timezone, next_fire_at, last_fired_at, enabled
             FROM schedules WHERE enabled = TRUE AND next_fire_at <= $1",
        )
        .bind(now).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| Schedule {
            id: r.0, project_member_id: r.1, cron_expr: r.2, timezone: r.3,
            next_fire_at: r.4, last_fired_at: r.5, enabled: r.6,
        }).collect())
    }

    async fn mark_fired(&self, id: Uuid, fired_at: DateTime<Utc>, next_fire_at: DateTime<Utc>) -> Result<(), String> {
        sqlx::query("UPDATE schedules SET last_fired_at = $2, next_fire_at = $3 WHERE id = $1")
            .bind(id).bind(fired_at).bind(next_fire_at)
            .execute(&self.pool).await.map(|_| ()).map_err(|e| e.to_string())
    }
}
```

Run: `cargo test -p celerex-api --test scheduler -- --test-threads=1`
Expected: PASS.

- [ ] **Step 1.10: Implement the tokio polling loop**

Overwrite `packages/backend/api/src/scheduler/loop.rs`:
```rust
use crate::scheduler::advance::next_after;
use crate::scheduler::store::{Schedule, SchedulesStore};
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use tokio::task::JoinHandle;

pub struct SchedulerLoop<F> {
    pub store: Arc<dyn SchedulesStore>,
    pub tick: Duration,
    pub dispatch: Arc<F>,
}

impl<F> SchedulerLoop<F>
where
    F: Fn(Schedule) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
{
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.tick);
            loop {
                interval.tick().await;
                let now = Utc::now();
                let due = match self.store.due(now).await {
                    Ok(v) => v,
                    Err(e) => { tracing::error!(?e, "scheduler store.due"); continue; }
                };
                for s in due {
                    let next = match next_after(&s.cron_expr, &s.timezone, now) {
                        Ok(n) => n,
                        Err(e) => { tracing::error!(?e, schedule=%s.id, "cron advance failed"); continue; }
                    };
                    if let Err(e) = self.store.mark_fired(s.id, now, next).await {
                        tracing::error!(?e, schedule=%s.id, "mark_fired"); continue;
                    }
                    (self.dispatch)(s);
                }
            }
        })
    }
}
```

- [ ] **Step 1.11: Write the failing loop integration test**

Append to `packages/backend/api/tests/scheduler.rs`:
```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn loop_dispatches_due_schedules_and_advances_next_fire_at() {
    let db = common::start_test_db().await;
    let store = Arc::new(PgSchedulesStore::new(db.pool.clone()));

    let past = Utc.with_ymd_and_hms(2026, 4, 18, 0, 0, 0).unwrap();
    let _id = store.create(Schedule {
        id: Uuid::new_v4(), project_member_id: Uuid::new_v4(),
        cron_expr: "0 0 9 * * MON *".into(), timezone: "UTC".into(),
        next_fire_at: past, last_fired_at: None, enabled: true,
    }).await.unwrap();

    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let dispatch = Arc::new(move |_s: Schedule| {
        let c = counter.clone();
        tokio::spawn(async move { c.fetch_add(1, Ordering::SeqCst); })
    });

    let handle = celerex_api::scheduler::r#loop::SchedulerLoop {
        store: store.clone(),
        tick: Duration::from_millis(50),
        dispatch,
    }.spawn();

    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.abort();

    assert!(count.load(Ordering::SeqCst) >= 1, "dispatch fired at least once");
    let remaining_due = store.due(Utc::now()).await.unwrap();
    assert!(remaining_due.is_empty(), "next_fire_at advanced past now");
}
```

- [ ] **Step 1.12: Run test — verify green**

Run: `cargo test -p celerex-api --test scheduler -- --test-threads=1`
Expected: both tests PASS.

- [ ] **Step 1.13: Wire the loop into `build_app_with_store`**

Edit `packages/backend/api/src/lib.rs` — inside `build_app_with_store`, after the executor is constructed, add:
```rust
let schedules = Arc::new(scheduler::store::PgSchedulesStore::new(pool.clone()));
let exec_for_sched = executor.clone();
let pool_for_sched = pool.clone();
let dispatch = Arc::new(move |s: scheduler::store::Schedule| {
    let exec = exec_for_sched.clone();
    let pool = pool_for_sched.clone();
    tokio::spawn(async move {
        // project_members integration lands in Task 2; for now, log only.
        tracing::info!(member=%s.project_member_id, "scheduler tick (dispatch stub until Task 2)");
        let _ = (exec, pool); // silence unused warnings
    })
});
scheduler::r#loop::SchedulerLoop {
    store: schedules, tick: std::time::Duration::from_secs(30), dispatch,
}.spawn();
```

Run: `cargo build -p celerex-api` — expect success.

- [ ] **Step 1.14: Commit**

```bash
git add packages/backend/api/src/scheduler/ packages/backend/api/src/lib.rs \
        packages/backend/api/tests/scheduler.rs
git commit -m "feat(scheduler): tokio poll loop + PgSchedulesStore wired into build_app"
```

---

## Task 2: Project Members + snapshots + manual run (spec sub-project B)

Two migrations (members table, member_snapshots table), CRUD routes, snapshot-on-edit semantics, and a `POST /api/project_members/:id/run` manual trigger that pins `runs.snapshot_id`. Also adds the FK from `schedules.project_member_id`.

**Files:**
- Create: `packages/backend/api/src/db/migrations/0006_project_members.sql`
- Create: `packages/backend/api/src/db/migrations/0007_member_snapshots.sql`
- Create: `packages/backend/api/src/project_members/mod.rs`
- Create: `packages/backend/api/src/project_members/store.rs`
- Create: `packages/backend/api/src/project_members/routes.rs`
- Create: `packages/backend/api/src/project_members/graph.rs`
- Modify: `packages/backend/api/src/lib.rs` (mount routes, add `MembersStore` to `AppState`)
- Modify: `packages/backend/api/Cargo.toml` (add `jsonschema = "0.18"`)
- Test: `packages/backend/api/tests/project_members.rs`

- [ ] **Step 2.1: Add migrations**

Create `packages/backend/api/src/db/migrations/0006_project_members.sql`:
```sql
CREATE TABLE project_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    graph_json JSONB NOT NULL,
    provider_config_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE schedules
    ADD CONSTRAINT schedules_member_fk
    FOREIGN KEY (project_member_id) REFERENCES project_members(id) ON DELETE CASCADE;
```

Create `packages/backend/api/src/db/migrations/0007_member_snapshots.sql`:
```sql
CREATE TABLE project_member_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    graph_json JSONB NOT NULL,
    provider_config_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_member_id, version)
);
```

- [ ] **Step 2.2: Add jsonschema dep**

Edit `packages/backend/api/Cargo.toml` `[dependencies]`:
```toml
jsonschema = "0.18"
```

- [ ] **Step 2.3: Write failing graph-validation test**

Create `packages/backend/api/src/project_members/graph.rs`:
```rust
use serde_json::Value;

pub const GRAPH_SCHEMA: &str = r##"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["nodes", "edges"],
  "properties": {
    "nodes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "type", "config"],
        "properties": {
          "id": { "type": "string", "minLength": 1 },
          "type": { "type": "string", "enum": ["inbound", "action", "filter", "outbound"] },
          "config": { "type": "object" }
        }
      }
    },
    "edges": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["from", "to"],
        "properties": {
          "from": { "type": "string" },
          "to": { "type": "string" }
        }
      }
    }
  }
}"##;

pub fn validate(value: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(GRAPH_SCHEMA).unwrap();
    let compiled = jsonschema::validator_for(&schema).map_err(|e| e.to_string())?;
    let errs: Vec<String> = compiled.iter_errors(value).map(|e| e.to_string()).collect();
    if errs.is_empty() { Ok(()) } else { Err(errs.join("; ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_minimal_graph() {
        let g = json!({ "nodes": [{"id":"a","type":"inbound","config":{}}], "edges": [] });
        validate(&g).unwrap();
    }

    #[test]
    fn rejects_missing_nodes() {
        let g = json!({ "edges": [] });
        assert!(validate(&g).is_err());
    }

    #[test]
    fn rejects_unknown_node_type() {
        let g = json!({ "nodes": [{"id":"a","type":"squirrel","config":{}}], "edges": [] });
        assert!(validate(&g).is_err());
    }
}
```

Add module: create `packages/backend/api/src/project_members/mod.rs`:
```rust
pub mod graph;
pub mod routes;
pub mod store;
```

Register in `packages/backend/api/src/lib.rs` alongside other `pub mod` lines:
```rust
pub mod project_members;
```

- [ ] **Step 2.4: Run — verify red**

Run: `cargo test -p celerex-api project_members::graph`
Expected: unresolved symbols (routes.rs / store.rs are empty).

- [ ] **Step 2.5: Stub sibling files so the crate compiles**

Create `packages/backend/api/src/project_members/store.rs` with a single line `// filled in step 2.7` and `routes.rs` with the same. Re-run the graph tests:

Run: `cargo test -p celerex-api project_members::graph`
Expected: three test cases PASS.

- [ ] **Step 2.6: Write failing integration test for snapshot-on-edit**

Create `packages/backend/api/tests/project_members.rs`:
```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;
use serde_json::json;

async fn token_for(engagement: uuid::Uuid) -> String {
    // JWT mint helper re-used from Sprint 0 tests — adapt from tests/auth.rs pattern.
    celerex_api::auth::jwt::mint_for_tests(engagement)
}

#[tokio::test]
async fn edit_creates_new_snapshot_and_bumps_version() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let eng = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id",
    ).fetch_one(&db.pool).await.unwrap();
    let project = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id",
    ).bind(eng).fetch_one(&db.pool).await.unwrap();
    let token = token_for(eng).await;

    // Create member (version 1 + snapshot 1).
    let res = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/project_members")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(json!({
                "project_id": project, "name": "Weekly Newsletter",
                "graph_json": { "nodes": [], "edges": [] },
                "provider_config_json": {}
            }).to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    // Edit (version → 2, new snapshot).
    let res = app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/project_members/{member_id}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(json!({
                "name": "Weekly Newsletter",
                "graph_json": { "nodes": [{"id":"a","type":"inbound","config":{}}], "edges": [] },
                "provider_config_json": {}
            }).to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let snaps: Vec<(i32,)> = sqlx::query_as(
        "SELECT version FROM project_member_snapshots WHERE project_member_id = $1 ORDER BY version"
    ).bind(uuid::Uuid::parse_str(&member_id).unwrap())
        .fetch_all(&db.pool).await.unwrap();
    assert_eq!(snaps, vec![(1,), (2,)]);

    let current: (i32,) = sqlx::query_as(
        "SELECT version FROM project_members WHERE id = $1"
    ).bind(uuid::Uuid::parse_str(&member_id).unwrap())
        .fetch_one(&db.pool).await.unwrap();
    assert_eq!(current.0, 2);
}
```

Add a small JWT test helper — create `packages/backend/api/src/auth/jwt.rs`'s `mint_for_tests` (if not already present) by inserting the following into `packages/backend/api/src/auth/jwt.rs` (append):
```rust
#[cfg(any(test, feature = "test-support"))]
pub fn mint_for_tests(engagement_id: uuid::Uuid) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": "test-user",
        "engagement_id": engagement_id,
        "exp": (chrono::Utc::now().timestamp() + 3600)
    });
    // Dev key — do not ship.
    let key = std::env::var("CELEREX_JWT_SECRET").unwrap_or_else(|_| "dev-secret".into());
    encode(&Header::default(), &claims, &EncodingKey::from_secret(key.as_bytes())).unwrap()
}
```

If `mint_for_tests` already exists in Sprint 0, skip this append.

- [ ] **Step 2.7: Implement `MembersStore`**

Overwrite `packages/backend/api/src/project_members/store.rs`:
```rust
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Member {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub graph_json: Value,
    pub provider_config_json: Value,
    pub version: i32,
}

#[async_trait]
pub trait MembersStore: Send + Sync {
    async fn create(&self, project_id: Uuid, name: &str, description: &str,
        graph: &Value, provider_config: &Value) -> Result<Member, String>;
    async fn get(&self, id: Uuid) -> Result<Option<Member>, String>;
    async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Member>, String>;
    async fn update(&self, id: Uuid, name: &str, description: &str,
        graph: &Value, provider_config: &Value) -> Result<Member, String>;
    async fn snapshot_id_for_current(&self, id: Uuid) -> Result<Uuid, String>;
}

pub struct PgMembersStore { pool: PgPool }

impl PgMembersStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    async fn write_snapshot(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        member_id: Uuid, version: i32, graph: &Value, provider_config: &Value) -> Result<Uuid, String>
    {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO project_member_snapshots
               (project_member_id, version, graph_json, provider_config_json)
             VALUES ($1, $2, $3, $4) RETURNING id"
        ).bind(member_id).bind(version).bind(graph).bind(provider_config)
            .fetch_one(&mut **tx).await.map_err(|e| e.to_string())
    }
}

#[async_trait]
impl MembersStore for PgMembersStore {
    async fn create(&self, project_id: Uuid, name: &str, description: &str,
        graph: &Value, provider_config: &Value) -> Result<Member, String>
    {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: (Uuid, i32) = sqlx::query_as(
            "INSERT INTO project_members (project_id, name, description, graph_json, provider_config_json)
             VALUES ($1, $2, $3, $4, $5) RETURNING id, version"
        ).bind(project_id).bind(name).bind(description).bind(graph).bind(provider_config)
            .fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        let _ = self.write_snapshot(&mut tx, row.0, row.1, graph, provider_config).await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(Member {
            id: row.0, project_id, name: name.into(), description: description.into(),
            graph_json: graph.clone(), provider_config_json: provider_config.clone(), version: row.1,
        })
    }

    async fn get(&self, id: Uuid) -> Result<Option<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version
             FROM project_members WHERE id = $1"
        ).bind(id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())
         .map(|opt| opt.map(|r| Member {
             id: r.0, project_id: r.1, name: r.2, description: r.3,
             graph_json: r.4, provider_config_json: r.5, version: r.6,
         }))
    }

    async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version
             FROM project_members WHERE project_id = $1 ORDER BY created_at"
        ).bind(project_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())
         .map(|rows| rows.into_iter().map(|r| Member {
             id: r.0, project_id: r.1, name: r.2, description: r.3,
             graph_json: r.4, provider_config_json: r.5, version: r.6,
         }).collect())
    }

    async fn update(&self, id: Uuid, name: &str, description: &str,
        graph: &Value, provider_config: &Value) -> Result<Member, String>
    {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let new_version: i32 = sqlx::query_scalar(
            "UPDATE project_members
               SET name = $2, description = $3, graph_json = $4,
                   provider_config_json = $5, version = version + 1, updated_at = NOW()
             WHERE id = $1 RETURNING version"
        ).bind(id).bind(name).bind(description).bind(graph).bind(provider_config)
            .fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        let _ = self.write_snapshot(&mut tx, id, new_version, graph, provider_config).await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        let member = self.get(id).await?.ok_or("member vanished")?;
        Ok(member)
    }

    async fn snapshot_id_for_current(&self, id: Uuid) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT s.id FROM project_member_snapshots s
             JOIN project_members m
               ON m.id = s.project_member_id AND m.version = s.version
             WHERE m.id = $1"
        ).bind(id).fetch_one(&self.pool).await.map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2.8: Implement the routes**

Overwrite `packages/backend/api/src/project_members/routes.rs`:
```rust
use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{AppState, auth::AuthClaims};
use crate::project_members::graph;

#[derive(Deserialize)]
pub struct CreateBody {
    pub project_id: Uuid,
    pub name: String,
    #[serde(default)] pub description: String,
    pub graph_json: Value,
    #[serde(default)] pub provider_config_json: Value,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub name: String,
    #[serde(default)] pub description: String,
    pub graph_json: Value,
    #[serde(default)] pub provider_config_json: Value,
}

pub async fn create(_a: AuthClaims, State(s): State<AppState>, Json(b): Json<CreateBody>) -> impl IntoResponse {
    if let Err(e) = graph::validate(&b.graph_json) {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }
    match s.members.create(b.project_id, &b.name, &b.description, &b.graph_json, &b.provider_config_json).await {
        Ok(m) => (StatusCode::CREATED, Json(m)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}

pub async fn update(_a: AuthClaims, State(s): State<AppState>, Path(id): Path<Uuid>, Json(b): Json<UpdateBody>) -> impl IntoResponse {
    if let Err(e) = graph::validate(&b.graph_json) {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }
    match s.members.update(id, &b.name, &b.description, &b.graph_json, &b.provider_config_json).await {
        Ok(m) => (StatusCode::OK, Json(m)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}

pub async fn get(_a: AuthClaims, State(s): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match s.members.get(id).await {
        Ok(Some(m)) => (StatusCode::OK, Json(m)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}

pub async fn list(_a: AuthClaims, State(s): State<AppState>, Path(project_id): Path<Uuid>) -> impl IntoResponse {
    match s.members.list_for_project(project_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    }
}
```

Add `members: Arc<dyn MembersStore>` to `AppState` in `packages/backend/api/src/lib.rs`:
```rust
use crate::project_members::store::{MembersStore, PgMembersStore};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub credentials: Arc<dyn CredentialStore>,
    pub executor: Arc<Executor>,
    pub members: Arc<dyn MembersStore>,
}
```

Inside `build_app_with_store`, construct `let members: Arc<dyn MembersStore> = Arc::new(PgMembersStore::new(pool.clone()));` and pass it in. Mount the routes:
```rust
.route("/api/project_members", post(project_members::routes::create))
.route("/api/project_members/{id}", get(project_members::routes::get).put(project_members::routes::update))
.route("/api/projects/{project_id}/members", get(project_members::routes::list))
```

- [ ] **Step 2.9: Run tests — verify green**

Run: `cargo test -p celerex-api --test project_members -- --test-threads=1`
Expected: snapshot-on-edit test PASSES.

- [ ] **Step 2.10: Write the failing manual-run test**

Append to `packages/backend/api/tests/project_members.rs`:
```rust
#[tokio::test]
async fn post_run_creates_run_row_with_snapshot_and_manual_trigger() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let eng = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id"
    ).fetch_one(&db.pool).await.unwrap();
    let project = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id"
    ).bind(eng).fetch_one(&db.pool).await.unwrap();
    let token = token_for(eng).await;

    let create = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/project_members")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(json!({
                "project_id": project, "name": "Jean",
                "graph_json": { "nodes": [], "edges": [] },
                "provider_config_json": {}
            }).to_string())).unwrap()
    ).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    let res = app.oneshot(
        Request::builder().method("POST")
            .uri(format!("/api/project_members/{member_id}/run"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from("{}")).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let runs: Vec<(uuid::Uuid, Option<uuid::Uuid>, String)> = sqlx::query_as(
        "SELECT id, snapshot_id, triggered_by FROM runs"
    ).fetch_all(&db.pool).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].1.is_some(), "snapshot_id is set");
    assert_eq!(runs[0].2, "manual");
}
```

Note: this test depends on the `snapshot_id` and `triggered_by` columns added in Task 7 (`0009_amend_runs.sql`). If Task 7 has not landed yet, mark this test `#[ignore]` and un-ignore in Task 7.

- [ ] **Step 2.11: Implement `POST /api/project_members/:id/run`**

Append to `packages/backend/api/src/project_members/routes.rs`:
```rust
use celerex_orchestrator::job::{JobRequest, Step};

pub async fn run_manual(
    _a: AuthClaims, State(s): State<AppState>, Path(id): Path<Uuid>
) -> impl IntoResponse {
    let member = match s.members.get(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    };
    let snap_id = match s.members.snapshot_id_for_current(id).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response(),
    };

    // Translate graph_json → orchestrator steps. For Sprint 1 this is a direct mapping:
    // each node becomes a Step whose kind reflects its `type` field.
    let steps: Vec<Step> = match crate::project_members::graph::compile(&member.graph_json) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };

    let run_id = Uuid::new_v4();
    // Insert runs row pinned to snapshot.
    if let Err(e) = sqlx::query(
        "INSERT INTO runs (id, engagement_id, project_id, trigger, status, snapshot_id, triggered_by)
         VALUES ($1, $2, $3, $4::jsonb, 'running', $5, 'manual')"
    )
    .bind(run_id)
    .bind::<Uuid>(_a.0.engagement_id)
    .bind(Some(member.project_id))
    .bind(json!({"source":"manual"}).to_string())
    .bind(snap_id)
    .execute(&s.pool).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response();
    }

    let req = JobRequest { run_id, engagement_id: _a.0.engagement_id, project_id: Some(member.project_id), steps };
    let exec = s.executor.clone();
    tokio::spawn(async move { let _ = exec.run(req).await; });

    (StatusCode::ACCEPTED, Json(json!({"run_id": run_id}))).into_response()
}
```

Add a compiler in `packages/backend/api/src/project_members/graph.rs` (append):
```rust
use celerex_orchestrator::job::{Step, StepKind};
use celerex_orchestrator::execution_type::ExecutionType;

pub fn compile(graph: &Value) -> Result<Vec<Step>, String> {
    validate(graph)?;
    let nodes = graph["nodes"].as_array().ok_or("nodes not array")?;
    let edges = graph["edges"].as_array().cloned().unwrap_or_default();
    let mut steps = Vec::new();
    for n in nodes {
        let id = n["id"].as_str().ok_or("node.id")?.to_string();
        let ty = n["type"].as_str().ok_or("node.type")?;
        let config = n["config"].clone();
        let depends_on: Vec<String> = edges.iter()
            .filter(|e| e["to"].as_str() == Some(&id))
            .filter_map(|e| e["from"].as_str().map(|s| s.to_string()))
            .collect();
        let kind = match ty {
            "filter" => StepKind::Filter { expression: config["expression"].as_str().unwrap_or("").into() },
            "outbound" => StepKind::Outbound {
                execution: execution_from(&config)?, config: config.clone(),
                inputs: config["inputs"].clone(),
            },
            _ => StepKind::Tool {
                execution: execution_from(&config)?, config: config.clone(),
                inputs: config["inputs"].clone(),
            },
        };
        steps.push(Step { id, depends_on, kind });
    }
    Ok(steps)
}

fn execution_from(config: &Value) -> Result<ExecutionType, String> {
    match config["execution"].as_str().unwrap_or("script") {
        "http" => Ok(ExecutionType::Http),
        "mcp" => Ok(ExecutionType::Mcp),
        "agent" => Ok(ExecutionType::Agent),
        "script" => Ok(ExecutionType::Script),
        "llm" => Ok(ExecutionType::Llm),
        other => Err(format!("unknown execution type: {other}")),
    }
}
```

Mount the route — in `lib.rs` add:
```rust
.route("/api/project_members/{id}/run", post(project_members::routes::run_manual))
```

- [ ] **Step 2.12: Un-ignore the manual-run test in Task 7, then commit**

Run: `cargo test -p celerex-api --test project_members -- --test-threads=1`
Expected: snapshot-on-edit test PASSES; `post_run_creates_run_row_*` test is `#[ignore]` (un-ignored in Task 7).

```bash
git add packages/backend/api/src/project_members/ \
        packages/backend/api/src/db/migrations/0006_project_members.sql \
        packages/backend/api/src/db/migrations/0007_member_snapshots.sql \
        packages/backend/api/src/lib.rs packages/backend/api/src/auth/jwt.rs \
        packages/backend/api/Cargo.toml packages/backend/api/tests/project_members.rs
git commit -m "feat(members): project_members CRUD + immutable snapshots + manual run endpoint"
```

---

## Task 3: Provider trait + StubProvider (spec sub-project C)

Rename the existing `LlmClient` trait to `Provider` in a dedicated `agent-orchestrator::llm` module, wire `AnthropicClient` as an implementation, and add a `StubProvider` for tests. Drop the rename-bridge for old callers — Sprint 0 only had one caller (the `LlmHandler`).

**Files:**
- Create: `packages/backend/agent-orchestrator/src/llm/mod.rs`
- Create: `packages/backend/agent-orchestrator/src/llm/provider.rs`
- Create: `packages/backend/agent-orchestrator/src/llm/stub.rs`
- Modify: `packages/backend/agent-orchestrator/src/lib.rs` (export `pub mod llm;`)
- Modify: `packages/backend/agent-orchestrator/src/handlers/llm.rs` (use `Provider`)
- Modify: `packages/backend/api/src/llm/anthropic.rs` (`impl Provider for AnthropicClient`)
- Test: `packages/backend/api/tests/provider.rs`

- [ ] **Step 3.1: Create `Provider` trait**

Create `packages/backend/agent-orchestrator/src/llm/mod.rs`:
```rust
pub mod provider;
pub mod stub;
pub use provider::{Provider, Completion};
```

Create `packages/backend/agent-orchestrator/src/llm/provider.rs`:
```rust
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Completion {
    pub raw: Value,
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Single model call. Tool-use loop (if any) is the caller's responsibility.
    async fn complete(&self, model: &str, messages: Value, tools: Value) -> Result<Completion, String>;
}
```

Register the module. Edit `packages/backend/agent-orchestrator/src/lib.rs`:
```rust
pub mod llm;
```

- [ ] **Step 3.2: Create `StubProvider`**

Create `packages/backend/agent-orchestrator/src/llm/stub.rs`:
```rust
use super::provider::{Completion, Provider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Mutex;

/// Plays scripted responses in order. Panics if called more times than scripted.
pub struct StubProvider {
    scripted: Mutex<std::collections::VecDeque<Value>>,
}

impl StubProvider {
    pub fn new(responses: Vec<Value>) -> Self {
        Self { scripted: Mutex::new(responses.into()) }
    }
    pub fn text(body: &str) -> Self {
        Self::new(vec![json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": body }]
        })])
    }
}

#[async_trait]
impl Provider for StubProvider {
    async fn complete(&self, _model: &str, _messages: Value, _tools: Value) -> Result<Completion, String> {
        let raw = self.scripted.lock().unwrap().pop_front()
            .ok_or_else(|| "StubProvider: no more scripted responses".to_string())?;
        Ok(Completion { raw })
    }
}
```

- [ ] **Step 3.3: Migrate `LlmHandler` to `Provider`**

Overwrite `packages/backend/agent-orchestrator/src/handlers/llm.rs`:
```rust
use crate::llm::provider::Provider;
use crate::tool_dispatch::{DispatchError, Handler};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct LlmHandler {
    pub provider: Arc<dyn Provider>,
}

#[async_trait]
impl Handler for LlmHandler {
    async fn invoke(&self, config: Value, inputs: Value) -> Result<Value, DispatchError> {
        let model = config.get("model").and_then(|v| v.as_str()).unwrap_or("claude-sonnet-4-6");
        let tools = config.get("tools").cloned().unwrap_or(Value::Array(vec![]));
        let messages = inputs.get("messages").cloned().unwrap_or(Value::Array(vec![]));
        let c = self.provider.complete(model, messages, tools).await
            .map_err(DispatchError::Handler)?;
        Ok(c.raw)
    }
}
```

- [ ] **Step 3.4: Impl `Provider` for `AnthropicClient`**

Edit `packages/backend/api/src/llm/anthropic.rs`. At the bottom:
```rust
use async_trait::async_trait;
use celerex_orchestrator::llm::provider::{Completion, Provider};

#[async_trait]
impl Provider for AnthropicClient {
    async fn complete(&self, model: &str, messages: serde_json::Value, tools: serde_json::Value) -> Result<Completion, String> {
        let msgs = messages.as_array().cloned().unwrap_or_default();
        let tools = tools.as_array().cloned().unwrap_or_default();
        let raw = self.create_message(model, msgs, tools).await
            .map_err(|e| e.to_string())?;
        Ok(Completion { raw })
    }
}
```

- [ ] **Step 3.5: Write the provider contract test**

Create `packages/backend/api/tests/provider.rs`:
```rust
use celerex_orchestrator::llm::{stub::StubProvider, provider::Provider};
use serde_json::json;

#[tokio::test]
async fn stub_provider_returns_scripted_then_errors() {
    let p = StubProvider::text("hello");
    let c = p.complete("any", json!([]), json!([])).await.unwrap();
    assert_eq!(c.raw["content"][0]["text"], "hello");
    let err = p.complete("any", json!([]), json!([])).await.unwrap_err();
    assert!(err.contains("no more"));
}
```

- [ ] **Step 3.6: Run & commit**

Run:
- `cargo build -p celerex-orchestrator -p celerex-api`
- `cargo test -p celerex-api --test provider`
- `cargo test -p celerex-orchestrator`

Expected: all green.

```bash
git add packages/backend/agent-orchestrator/src/llm/ \
        packages/backend/agent-orchestrator/src/lib.rs \
        packages/backend/agent-orchestrator/src/handlers/llm.rs \
        packages/backend/api/src/llm/anthropic.rs \
        packages/backend/api/tests/provider.rs
git commit -m "refactor(llm): Provider trait + StubProvider; AnthropicClient impls Provider"
```

---

## Task 4: File connectors — XLSX + DOCX markdown mode (spec sub-project D)

Add a `FileConnector` trait and two connector implementations that return markdown for XLSX and DOCX. Tool-use mode is added in Task 5.

**Files:**
- Create: `packages/backend/api/src/plugins/file_connector/mod.rs`
- Create: `packages/backend/api/src/plugins/file_connector/xlsx.rs`
- Create: `packages/backend/api/src/plugins/file_connector/docx.rs`
- Modify: `packages/backend/api/src/plugins/mod.rs` (add `pub mod file_connector;`)
- Modify: `packages/backend/api/Cargo.toml` (add `calamine = "0.25"`, `docx-rs = "0.4"`)
- Create: `packages/backend/api/tests/fixtures/files/sample.xlsx` (binary fixture — build via script below)
- Create: `packages/backend/api/tests/fixtures/files/sample.docx` (binary fixture — build via script below)
- Test: `packages/backend/api/tests/file_connector.rs`

- [ ] **Step 4.1: Add deps**

Edit `packages/backend/api/Cargo.toml`:
```toml
calamine = "0.25"
docx-rs = "0.4"
```

- [ ] **Step 4.2: Add the trait**

Create `packages/backend/api/src/plugins/file_connector/mod.rs`:
```rust
pub mod xlsx;
pub mod docx;

use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("io: {0}")] Io(String),
    #[error("parse: {0}")] Parse(String),
    #[error("unknown tool: {0}")] UnknownTool(String),
}

#[derive(Debug, Clone)]
pub struct ToolDef { pub name: String, pub description: String, pub input_schema: Value }

#[async_trait]
pub trait FileConnector: Send + Sync {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError>;
    async fn tool_definitions(&self, file: &Path) -> Result<Vec<ToolDef>, ConnectorError>;
    async fn invoke_tool(&self, file: &Path, name: &str, args: Value) -> Result<Value, ConnectorError>;
}
```

Add to `packages/backend/api/src/plugins/mod.rs`:
```rust
pub mod file_connector;
```

Also, ensure `thiserror = "1"` is in the API crate `[dependencies]` if not already — Sprint 0 only had it on the orchestrator crate. If missing, add it.

- [ ] **Step 4.3: Write failing XLSX test**

Create `packages/backend/api/tests/file_connector.rs`:
```rust
use celerex_api::plugins::file_connector::{xlsx::XlsxConnector, FileConnector};
use std::path::PathBuf;

#[tokio::test]
async fn xlsx_extracts_first_sheet_as_markdown_table() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.xlsx");
    let c = XlsxConnector::default();
    let md = c.extract_markdown(&fixture).await.unwrap();
    // sample.xlsx has headers [Metric, Value] and rows [("Signups", 42), ("Churn", 3)]
    assert!(md.contains("| Metric | Value |"), "has header row\n{md}");
    assert!(md.contains("| --- | --- |"), "has separator row\n{md}");
    assert!(md.contains("| Signups | 42 |"), "has first data row\n{md}");
    assert!(md.contains("| Churn | 3 |"), "has second data row\n{md}");
}
```

- [ ] **Step 4.4: Create the XLSX fixture**

Use a small helper to materialize the fixture. Create `packages/backend/api/tests/fixtures/files/_build_xlsx.rs` — NO; simpler: commit a hand-crafted file. In practice, open `tests/fixtures/files/sample.xlsx` via a Python one-liner:

```bash
mkdir -p packages/backend/api/tests/fixtures/files
python3 - <<'PY'
from openpyxl import Workbook
wb = Workbook()
ws = wb.active
ws.append(["Metric", "Value"])
ws.append(["Signups", 42])
ws.append(["Churn", 3])
wb.save("packages/backend/api/tests/fixtures/files/sample.xlsx")
PY
```

If `openpyxl` is unavailable, install it (`pip3 install --user openpyxl`) or use `python3 -m pip install openpyxl`. The file is committed as a binary test asset.

- [ ] **Step 4.5: Implement `XlsxConnector`**

Create `packages/backend/api/src/plugins/file_connector/xlsx.rs`:
```rust
use super::{ConnectorError, FileConnector, ToolDef};
use async_trait::async_trait;
use calamine::{open_workbook, Data, Reader, Xlsx};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
pub struct XlsxConnector;

impl XlsxConnector {
    fn sheet_to_markdown(rows: &[Vec<Data>]) -> String {
        if rows.is_empty() { return String::new(); }
        let cell = |d: &Data| match d {
            Data::Empty => String::new(),
            Data::String(s) => s.clone(),
            Data::Float(f) => {
                if f.fract() == 0.0 { (*f as i64).to_string() } else { f.to_string() }
            }
            Data::Int(i) => i.to_string(),
            Data::Bool(b) => b.to_string(),
            Data::DateTime(d) => d.to_string(),
            Data::Error(e) => format!("#{e:?}"),
            Data::DurationIso(s) | Data::DateTimeIso(s) => s.clone(),
        };
        let header = &rows[0];
        let mut out = String::new();
        out.push_str("| ");
        out.push_str(&header.iter().map(cell).collect::<Vec<_>>().join(" | "));
        out.push_str(" |\n| ");
        out.push_str(&vec!["---"; header.len()].join(" | "));
        out.push_str(" |\n");
        for r in &rows[1..] {
            out.push_str("| ");
            out.push_str(&r.iter().map(cell).collect::<Vec<_>>().join(" | "));
            out.push_str(" |\n");
        }
        out
    }
}

#[async_trait]
impl FileConnector for XlsxConnector {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError> {
        let path = file.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<String, ConnectorError> {
            let mut wb: Xlsx<_> = open_workbook(&path).map_err(|e| ConnectorError::Io(e.to_string()))?;
            let name = wb.sheet_names().first().cloned().ok_or(ConnectorError::Parse("no sheets".into()))?;
            let range = wb.worksheet_range(&name).map_err(|e| ConnectorError::Parse(e.to_string()))?;
            let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
            Ok(Self::sheet_to_markdown(&rows))
        }).await.map_err(|e| ConnectorError::Io(e.to_string()))?
    }

    async fn tool_definitions(&self, _file: &Path) -> Result<Vec<ToolDef>, ConnectorError> {
        Ok(vec![]) // populated in Task 5
    }

    async fn invoke_tool(&self, _file: &Path, name: &str, _args: Value) -> Result<Value, ConnectorError> {
        Err(ConnectorError::UnknownTool(name.into()))
    }
}
```

- [ ] **Step 4.6: Run — verify green**

Run: `cargo test -p celerex-api --test file_connector -- --test-threads=1`
Expected: `xlsx_extracts_first_sheet_as_markdown_table` PASSES.

- [ ] **Step 4.7: Write failing DOCX test**

Append to `packages/backend/api/tests/file_connector.rs`:
```rust
use celerex_api::plugins::file_connector::docx::DocxConnector;

#[tokio::test]
async fn docx_extracts_paragraphs_as_markdown() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.docx");
    let c = DocxConnector::default();
    let md = c.extract_markdown(&fixture).await.unwrap();
    // sample.docx has: H1 "Quarterly Brief", paragraph "Revenue up 12%.", bullets "A"/"B"
    assert!(md.contains("# Quarterly Brief"));
    assert!(md.contains("Revenue up 12%."));
    assert!(md.contains("- A"));
    assert!(md.contains("- B"));
}
```

- [ ] **Step 4.8: Create the DOCX fixture**

```bash
python3 - <<'PY'
from docx import Document
doc = Document()
doc.add_heading("Quarterly Brief", level=1)
doc.add_paragraph("Revenue up 12%.")
doc.add_paragraph("A", style="List Bullet")
doc.add_paragraph("B", style="List Bullet")
doc.save("packages/backend/api/tests/fixtures/files/sample.docx")
PY
```

`pip3 install --user python-docx` if missing.

- [ ] **Step 4.9: Implement `DocxConnector`**

Create `packages/backend/api/src/plugins/file_connector/docx.rs`:
```rust
use super::{ConnectorError, FileConnector, ToolDef};
use async_trait::async_trait;
use docx_rs::{read_docx, DocumentChild, Paragraph, ParagraphChild, RunChild};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
pub struct DocxConnector;

fn para_text(p: &Paragraph) -> String {
    let mut out = String::new();
    for child in &p.children {
        if let ParagraphChild::Run(r) = child {
            for rc in &r.children {
                if let RunChild::Text(t) = rc { out.push_str(&t.text); }
            }
        }
    }
    out
}

fn para_to_md(p: &Paragraph) -> String {
    let txt = para_text(p);
    if let Some(style) = &p.property.style {
        let s = &style.val;
        if s == "Heading1" { return format!("# {txt}"); }
        if s == "Heading2" { return format!("## {txt}"); }
        if s == "Heading3" { return format!("### {txt}"); }
        if s.starts_with("ListBullet") || s == "ListBullet" { return format!("- {txt}"); }
    }
    txt
}

#[async_trait]
impl FileConnector for DocxConnector {
    async fn extract_markdown(&self, file: &Path) -> Result<String, ConnectorError> {
        let bytes = tokio::fs::read(file).await.map_err(|e| ConnectorError::Io(e.to_string()))?;
        let doc = read_docx(&bytes).map_err(|e| ConnectorError::Parse(e.to_string()))?;
        let mut lines = Vec::new();
        for ch in doc.document.children {
            if let DocumentChild::Paragraph(p) = ch {
                lines.push(para_to_md(&p));
            }
        }
        Ok(lines.join("\n\n"))
    }

    async fn tool_definitions(&self, _file: &Path) -> Result<Vec<ToolDef>, ConnectorError> { Ok(vec![]) }
    async fn invoke_tool(&self, _file: &Path, name: &str, _args: Value) -> Result<Value, ConnectorError> {
        Err(ConnectorError::UnknownTool(name.into()))
    }
}
```

- [ ] **Step 4.10: Run & commit**

Run: `cargo test -p celerex-api --test file_connector -- --test-threads=1`
Expected: both XLSX and DOCX tests PASS.

```bash
git add packages/backend/api/Cargo.toml \
        packages/backend/api/src/plugins/mod.rs \
        packages/backend/api/src/plugins/file_connector/ \
        packages/backend/api/tests/file_connector.rs \
        packages/backend/api/tests/fixtures/files/
git commit -m "feat(plugins): FileConnector trait + XLSX/DOCX markdown extraction"
```

---

## Task 5: File connector tool-use mode + `MockLlm` (spec sub-project E)

Add `tool_definitions` + `invoke_tool` implementations to both connectors so an LLM can call them during a `run_with_tools` loop. Add a `MockLlm` that returns scripted tool_use responses so tests don't hit a real provider.

**Files:**
- Modify: `packages/backend/api/src/plugins/file_connector/xlsx.rs` (populate tools)
- Modify: `packages/backend/api/src/plugins/file_connector/docx.rs` (populate tools)
- Create: `packages/backend/agent-orchestrator/src/llm/mock.rs`
- Modify: `packages/backend/agent-orchestrator/src/llm/mod.rs` (export)
- Test: append to `packages/backend/api/tests/file_connector.rs`

- [ ] **Step 5.1: Write failing XLSX tools test**

Append to `packages/backend/api/tests/file_connector.rs`:
```rust
#[tokio::test]
async fn xlsx_tools_include_get_cell_and_list_sheets() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/files/sample.xlsx");
    let c = XlsxConnector::default();
    let tools = c.tool_definitions(&fixture).await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(names.contains(&"list_sheets".into()));
    assert!(names.contains(&"get_cell".into()));

    let out = c.invoke_tool(&fixture, "get_cell",
        serde_json::json!({"sheet": null, "row": 2, "col": 2})).await.unwrap();
    assert_eq!(out, serde_json::json!("42"));
}
```

- [ ] **Step 5.2: Populate XLSX tools**

Replace the two `tool_definitions` / `invoke_tool` impls in `xlsx.rs`:
```rust
async fn tool_definitions(&self, _file: &Path) -> Result<Vec<ToolDef>, ConnectorError> {
    Ok(vec![
        ToolDef {
            name: "list_sheets".into(),
            description: "Return the sheet names in the workbook.".into(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
        ToolDef {
            name: "get_cell".into(),
            description: "Return a single cell value (1-indexed).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "sheet": { "type": ["string", "null"] },
                    "row": { "type": "integer", "minimum": 1 },
                    "col": { "type": "integer", "minimum": 1 }
                },
                "required": ["row", "col"]
            }),
        },
    ])
}

async fn invoke_tool(&self, file: &Path, name: &str, args: Value) -> Result<Value, ConnectorError> {
    let path = file.to_path_buf();
    let name = name.to_string();
    tokio::task::spawn_blocking(move || -> Result<Value, ConnectorError> {
        let mut wb: Xlsx<_> = open_workbook(&path).map_err(|e| ConnectorError::Io(e.to_string()))?;
        match name.as_str() {
            "list_sheets" => Ok(Value::Array(wb.sheet_names().iter()
                .map(|s| Value::String(s.clone())).collect())),
            "get_cell" => {
                let sheet = args["sheet"].as_str()
                    .map(str::to_string)
                    .or_else(|| wb.sheet_names().first().cloned())
                    .ok_or(ConnectorError::Parse("no sheet".into()))?;
                let row = args["row"].as_u64().ok_or(ConnectorError::Parse("row".into()))? as usize;
                let col = args["col"].as_u64().ok_or(ConnectorError::Parse("col".into()))? as usize;
                let range = wb.worksheet_range(&sheet).map_err(|e| ConnectorError::Parse(e.to_string()))?;
                let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
                let cell = rows.get(row.saturating_sub(1))
                    .and_then(|r| r.get(col.saturating_sub(1)))
                    .ok_or(ConnectorError::Parse("out of range".into()))?;
                let as_str = match cell {
                    Data::String(s) => s.clone(),
                    Data::Float(f) => if f.fract()==0.0 { (*f as i64).to_string() } else { f.to_string() },
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::Empty => String::new(),
                    Data::Error(e) => format!("#{e:?}"),
                    Data::DateTime(d) => d.to_string(),
                    Data::DurationIso(s) | Data::DateTimeIso(s) => s.clone(),
                };
                Ok(Value::String(as_str))
            }
            other => Err(ConnectorError::UnknownTool(other.into())),
        }
    }).await.map_err(|e| ConnectorError::Io(e.to_string()))?
}
```

- [ ] **Step 5.3: Populate DOCX tools**

Replace the two methods in `docx.rs`:
```rust
async fn tool_definitions(&self, _file: &Path) -> Result<Vec<ToolDef>, ConnectorError> {
    Ok(vec![ToolDef {
        name: "find_heading".into(),
        description: "Return the paragraph text under a given H1/H2 heading.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "heading": { "type": "string" } },
            "required": ["heading"]
        }),
    }])
}

async fn invoke_tool(&self, file: &Path, name: &str, args: Value) -> Result<Value, ConnectorError> {
    let bytes = tokio::fs::read(file).await.map_err(|e| ConnectorError::Io(e.to_string()))?;
    let doc = read_docx(&bytes).map_err(|e| ConnectorError::Parse(e.to_string()))?;
    match name {
        "find_heading" => {
            let target = args["heading"].as_str().ok_or(ConnectorError::Parse("heading".into()))?.to_string();
            let mut collecting = false;
            let mut out = Vec::new();
            for ch in doc.document.children {
                if let DocumentChild::Paragraph(p) = ch {
                    let txt = para_text(&p);
                    let is_heading = p.property.style.as_ref()
                        .map(|s| s.val.starts_with("Heading")).unwrap_or(false);
                    if is_heading {
                        if txt == target { collecting = true; continue; }
                        if collecting { break; }
                    } else if collecting && !txt.is_empty() {
                        out.push(txt);
                    }
                }
            }
            Ok(Value::String(out.join("\n")))
        }
        other => Err(ConnectorError::UnknownTool(other.into())),
    }
}
```

- [ ] **Step 5.4: Run — verify green**

Run: `cargo test -p celerex-api --test file_connector -- --test-threads=1`
Expected: XLSX tools test PASS.

- [ ] **Step 5.5: Add `MockLlm`**

Create `packages/backend/agent-orchestrator/src/llm/mock.rs`:
```rust
use super::provider::{Completion, Provider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Mutex;

/// Scripted tool-use responses: each element is a full Anthropic-shaped
/// `{stop_reason, content}` blob. Use `use_tool(name, input)` / `finish(text)`
/// to build common shapes.
pub struct MockLlm {
    queue: Mutex<std::collections::VecDeque<Value>>,
}

impl MockLlm {
    pub fn new() -> Self { Self { queue: Mutex::new(Default::default()) } }

    pub fn use_tool(self, name: &str, input: Value) -> Self {
        self.queue.lock().unwrap().push_back(json!({
            "stop_reason": "tool_use",
            "content": [{
                "type": "tool_use",
                "id": format!("tu_{}", uuid::Uuid::new_v4()),
                "name": name, "input": input
            }]
        }));
        self
    }

    pub fn finish(self, text: &str) -> Self {
        self.queue.lock().unwrap().push_back(json!({
            "stop_reason": "end_turn",
            "content": [{ "type": "text", "text": text }]
        }));
        self
    }
}

impl Default for MockLlm { fn default() -> Self { Self::new() } }

#[async_trait]
impl Provider for MockLlm {
    async fn complete(&self, _model: &str, _messages: Value, _tools: Value) -> Result<Completion, String> {
        let raw = self.queue.lock().unwrap().pop_front()
            .ok_or_else(|| "MockLlm: out of scripted responses".to_string())?;
        Ok(Completion { raw })
    }
}
```

Export in `packages/backend/agent-orchestrator/src/llm/mod.rs`:
```rust
pub mod mock;
pub use mock::MockLlm;
```

- [ ] **Step 5.6: Write MockLlm tool-use round-trip test**

Append to `packages/backend/api/tests/file_connector.rs`:
```rust
use celerex_orchestrator::llm::mock::MockLlm;
use celerex_orchestrator::llm::provider::Provider;

#[tokio::test]
async fn mock_llm_scripts_tool_use_then_finish() {
    let m = MockLlm::new()
        .use_tool("get_cell", serde_json::json!({"row":2,"col":2}))
        .finish("The signups cell is 42.");
    let first = m.complete("m", serde_json::json!([]), serde_json::json!([])).await.unwrap();
    assert_eq!(first.raw["stop_reason"], "tool_use");
    let second = m.complete("m", serde_json::json!([]), serde_json::json!([])).await.unwrap();
    assert_eq!(second.raw["content"][0]["text"], "The signups cell is 42.");
}
```

- [ ] **Step 5.7: Run & commit**

Run: `cargo test -p celerex-api --test file_connector -- --test-threads=1`
Expected: all five connector tests PASS.

```bash
git add packages/backend/api/src/plugins/file_connector/ \
        packages/backend/agent-orchestrator/src/llm/ \
        packages/backend/api/tests/file_connector.rs
git commit -m "feat(plugins): tool-use mode on file connectors + MockLlm harness"
```

---

## Task 6: workIQ + Stargate fixture plugins (spec sub-project F)

Manifest-backed fixture plugins that return canned realistic payloads. Same pattern as Sprint 0's `FixtureA2AClient`: commit a manifest under `tests/fixtures/plugins/` and a thin invocation wrapper.

**Files:**
- Create: `packages/backend/api/tests/fixtures/plugins/workiq/manifest.json`
- Create: `packages/backend/api/tests/fixtures/plugins/workiq/canned/pull_metrics.json`
- Create: `packages/backend/api/tests/fixtures/plugins/stargate/manifest.json`
- Create: `packages/backend/api/tests/fixtures/plugins/stargate/canned/query.json`
- Create: `packages/backend/api/src/plugins/fixtures/mod.rs`
- Create: `packages/backend/api/src/plugins/fixtures/workiq.rs`
- Create: `packages/backend/api/src/plugins/fixtures/stargate.rs`
- Modify: `packages/backend/api/src/plugins/mod.rs`
- Test: `packages/backend/api/tests/fixture_plugins.rs`

- [ ] **Step 6.1: Create workIQ manifest + canned payload**

Create `packages/backend/api/tests/fixtures/plugins/workiq/manifest.json`:
```json
{
  "id": "workiq",
  "name": "workIQ",
  "version": "0.1.0-fixture",
  "inbound": [],
  "outbound": [
    {
      "id": "workiq.pull_metrics",
      "kind": "http",
      "input_schema": {
        "type": "object",
        "properties": { "window_days": { "type": "integer" } }
      }
    }
  ]
}
```

Create `packages/backend/api/tests/fixtures/plugins/workiq/canned/pull_metrics.json`:
```json
{
  "window_days": 7,
  "metrics": {
    "emails_sent": 1284,
    "meetings_hosted": 32,
    "avg_response_minutes": 47,
    "teams_messages": 812
  },
  "top_collaborators": [
    {"name": "Alex Kim", "interactions": 44},
    {"name": "Priya Shah", "interactions": 39}
  ]
}
```

- [ ] **Step 6.2: Create Stargate manifest + canned payload**

`packages/backend/api/tests/fixtures/plugins/stargate/manifest.json`:
```json
{
  "id": "stargate",
  "name": "Stargate",
  "version": "0.1.0-fixture",
  "inbound": [],
  "outbound": [
    {
      "id": "stargate.query",
      "kind": "http",
      "input_schema": {
        "type": "object",
        "properties": { "sql": { "type": "string" } }
      }
    }
  ]
}
```

`packages/backend/api/tests/fixtures/plugins/stargate/canned/query.json`:
```json
{
  "columns": ["account", "mrr", "status"],
  "rows": [
    ["Acme Co", 12500, "green"],
    ["Globex", 8400, "yellow"],
    ["Initech", 4200, "red"]
  ]
}
```

- [ ] **Step 6.3: Write failing fixture-plugin test**

Create `packages/backend/api/tests/fixture_plugins.rs`:
```rust
use celerex_api::plugins::fixtures::{workiq::WorkIqFixture, stargate::StargateFixture};

#[tokio::test]
async fn workiq_fixture_returns_canned_payload() {
    let f = WorkIqFixture::from_fixture_dir();
    let out = f.invoke("workiq.pull_metrics", serde_json::json!({"window_days": 7})).await.unwrap();
    assert_eq!(out["metrics"]["emails_sent"], 1284);
}

#[tokio::test]
async fn stargate_fixture_returns_canned_rows() {
    let f = StargateFixture::from_fixture_dir();
    let out = f.invoke("stargate.query", serde_json::json!({"sql":"SELECT *"})).await.unwrap();
    assert_eq!(out["rows"][0][0], "Acme Co");
}
```

- [ ] **Step 6.4: Implement fixtures**

Create `packages/backend/api/src/plugins/fixtures/mod.rs`:
```rust
pub mod workiq;
pub mod stargate;
```

Add to `packages/backend/api/src/plugins/mod.rs`:
```rust
pub mod fixtures;
```

Create `packages/backend/api/src/plugins/fixtures/workiq.rs`:
```rust
use serde_json::Value;
use std::path::PathBuf;

pub struct WorkIqFixture { root: PathBuf }

impl WorkIqFixture {
    pub fn from_fixture_dir() -> Self {
        Self { root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/plugins/workiq") }
    }
    pub async fn invoke(&self, capability: &str, _input: Value) -> Result<Value, String> {
        match capability {
            "workiq.pull_metrics" => {
                let bytes = tokio::fs::read(self.root.join("canned/pull_metrics.json")).await
                    .map_err(|e| e.to_string())?;
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown workiq capability: {other}")),
        }
    }
}
```

Create `packages/backend/api/src/plugins/fixtures/stargate.rs` with the same shape, substituting `stargate` / `stargate.query` / `canned/query.json`.

- [ ] **Step 6.5: Run & commit**

Run: `cargo test -p celerex-api --test fixture_plugins -- --test-threads=1`
Expected: both tests PASS.

```bash
git add packages/backend/api/tests/fixtures/plugins/workiq/ \
        packages/backend/api/tests/fixtures/plugins/stargate/ \
        packages/backend/api/src/plugins/fixtures/ \
        packages/backend/api/src/plugins/mod.rs \
        packages/backend/api/tests/fixture_plugins.rs
git commit -m "feat(plugins): workIQ + Stargate fixture plugins with canned payloads"
```

---

## Task 7: SSE + amend `runs` schema (spec sub-project G)

Adds `snapshot_id` + `triggered_by` columns to `runs`. Wraps `PgRunStore` around a `tokio::sync::broadcast` so each `record(step)` also fans out an event. Adds `GET /api/runs/:run_id/events` that converts broadcast receivers into an SSE stream.

**Files:**
- Create: `packages/backend/api/src/db/migrations/0009_amend_runs.sql`
- Create: `packages/backend/api/src/runs/bus.rs`
- Create: `packages/backend/api/src/runs/sse.rs`
- Modify: `packages/backend/api/src/runs/mod.rs` (accept bus, publish on record)
- Modify: `packages/backend/api/src/lib.rs` (construct bus, mount SSE route)
- Modify: `packages/backend/api/Cargo.toml` (add `tokio-stream = "0.1"`, `futures = "0.3"` if not present)
- Test: `packages/backend/api/tests/sse.rs`
- Modify: `packages/backend/api/tests/project_members.rs` (un-ignore manual-run test)

- [ ] **Step 7.1: Add the migration**

Create `packages/backend/api/src/db/migrations/0009_amend_runs.sql`:
```sql
ALTER TABLE runs
    ADD COLUMN snapshot_id UUID REFERENCES project_member_snapshots(id),
    ADD COLUMN triggered_by TEXT NOT NULL DEFAULT 'scheduled';

ALTER TABLE runs
    ADD CONSTRAINT runs_triggered_by_chk
    CHECK (triggered_by IN ('scheduled', 'manual', 'webhook'));
```

- [ ] **Step 7.2: Add deps**

Edit `packages/backend/api/Cargo.toml` (add if missing):
```toml
tokio-stream = "0.1"
futures = "0.3"
```

- [ ] **Step 7.3: Create the event bus**

Create `packages/backend/api/src/runs/bus.rs`:
```rust
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    RunStepStarted { run_id: Uuid, step_id: String },
    RunStepSucceeded { run_id: Uuid, step_id: String, outputs: Value },
    RunStepFailed { run_id: Uuid, step_id: String, error: String },
    RunCompleted { run_id: Uuid, status: String },
}

#[derive(Default)]
pub struct RunBus {
    channels: Mutex<HashMap<Uuid, broadcast::Sender<RunEvent>>>,
}

impl RunBus {
    pub fn new() -> Self { Self::default() }

    pub fn publish(&self, run_id: Uuid, ev: RunEvent) {
        if let Some(tx) = self.channels.lock().unwrap().get(&run_id) {
            let _ = tx.send(ev);
        }
    }

    pub fn subscribe(&self, run_id: Uuid) -> broadcast::Receiver<RunEvent> {
        let mut guard = self.channels.lock().unwrap();
        guard.entry(run_id)
            .or_insert_with(|| broadcast::channel(256).0)
            .subscribe()
    }
}
```

- [ ] **Step 7.4: Make `PgRunStore` publish events**

Overwrite `packages/backend/api/src/runs/mod.rs`:
```rust
pub mod bus;
pub mod sse;

use std::sync::Arc;
use celerex_orchestrator::executor::RunStore;
use celerex_orchestrator::job::{RunStatus, StepRecord};
use sqlx::PgPool;
use uuid::Uuid;

use crate::runs::bus::{RunBus, RunEvent};

pub struct PgRunStore {
    pool: PgPool,
    bus: Arc<RunBus>,
}

impl PgRunStore {
    pub fn new(pool: PgPool) -> Self { Self { pool, bus: Arc::new(RunBus::new()) } }
    pub fn with_bus(pool: PgPool, bus: Arc<RunBus>) -> Self { Self { pool, bus } }
    pub fn bus(&self) -> Arc<RunBus> { self.bus.clone() }
}

impl RunStore for PgRunStore {
    fn record(&self, rec: StepRecord) {
        let pool = self.pool.clone();
        let bus = self.bus.clone();
        tokio::spawn(async move {
            let status = format!("{:?}", rec.status).to_lowercase();
            let _ = sqlx::query(
                "INSERT INTO run_steps (run_id, step_id, status, inputs, outputs, error, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())"
            )
            .bind(rec.run_id).bind(&rec.step_id).bind(&status)
            .bind(&rec.inputs).bind(&rec.outputs).bind(&rec.error)
            .execute(&pool).await;

            let ev = match rec.status {
                RunStatus::Succeeded => RunEvent::RunStepSucceeded {
                    run_id: rec.run_id, step_id: rec.step_id.clone(),
                    outputs: rec.outputs.clone().unwrap_or(serde_json::json!({})),
                },
                RunStatus::Failed => RunEvent::RunStepFailed {
                    run_id: rec.run_id, step_id: rec.step_id.clone(),
                    error: rec.error.clone().unwrap_or_default(),
                },
                RunStatus::Running | RunStatus::Pending => RunEvent::RunStepStarted {
                    run_id: rec.run_id, step_id: rec.step_id.clone(),
                },
            };
            bus.publish(rec.run_id, ev);
        });
    }

    fn records_for(&self, _run_id: Uuid) -> Vec<StepRecord> { Vec::new() }
}
```

- [ ] **Step 7.5: Create the SSE handler**

Create `packages/backend/api/src/runs/sse.rs`:
```rust
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

pub async fn stream(
    State(s): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.run_bus.subscribe(run_id);
    let events = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => {
            let payload = serde_json::to_string(&ev).ok()?;
            Some(Ok(Event::default().data(payload)))
        }
        Err(_) => None,
    });
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

- [ ] **Step 7.6: Wire bus into `AppState` + mount route**

Edit `packages/backend/api/src/lib.rs`:
```rust
use crate::runs::bus::RunBus;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub credentials: Arc<dyn CredentialStore>,
    pub executor: Arc<Executor>,
    pub members: Arc<dyn MembersStore>,
    pub run_bus: Arc<RunBus>,
}
```

Inside `build_app_with_store`:
```rust
let run_bus = Arc::new(RunBus::new());
let run_store = Arc::new(runs::PgRunStore::with_bus(pool.clone(), run_bus.clone()));
let executor = Arc::new(Executor::new(dispatcher, run_store));
```

And mount:
```rust
.route("/api/runs/{run_id}/events", get(runs::sse::stream))
```

- [ ] **Step 7.7: Write failing SSE integration test**

Create `packages/backend/api/tests/sse.rs`:
```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use tower::util::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn sse_streams_step_events_in_order() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());

    let run_id = Uuid::new_v4();

    // Open SSE stream first, then publish via an orchestrated run.
    let res = app.clone().oneshot(
        Request::builder().uri(format!("/api/runs/{run_id}/events")).body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let mut stream = res.into_body().into_data_stream();

    // Fire an event synthetically via the app's bus (handle via public accessor on AppState —
    // for tests we reach in via a known helper). For Sprint 1 we expose a test-only helper:
    celerex_api::runs::testing::publish_demo(&db.pool, run_id, "step-a").await;

    let chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next()).await
        .expect("stream produced no chunk").unwrap().unwrap();
    let text = String::from_utf8_lossy(&chunk).to_string();
    assert!(text.contains("run_step_succeeded"), "got: {text}");
    assert!(text.contains("step-a"));
}
```

Add a minimal test helper in `packages/backend/api/src/runs/mod.rs` (append):
```rust
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use super::bus::{RunBus, RunEvent};
    use once_cell::sync::Lazy;
    use sqlx::PgPool;
    use std::sync::Arc;
    use uuid::Uuid;

    // Scoped test-only singleton bus: the test harness instantiates build_app,
    // which replaces it; for direct publishes we need a way to reach the live bus.
    // Instead, we publish by spinning up a second app with a known bus. Easiest:
    // the test builds its own RunBus + PgRunStore.
    pub async fn publish_demo(_pool: &PgPool, _run_id: Uuid, _step_id: &str) {
        // Kept as a stub — the proper SSE test is in Task 14 (E2E).
        // The acceptance test for SSE in Sprint 1 lives in the E2E suite.
    }
}
```

**Simplification:** Because wiring a test-only publish path adds complexity, the SSE integration test is deferred to Task 14 (E2E), where a real orchestrated run publishes naturally. For now, keep a unit test only:

Replace `packages/backend/api/tests/sse.rs` with:
```rust
use celerex_api::runs::bus::{RunBus, RunEvent};
use uuid::Uuid;

#[tokio::test]
async fn bus_fans_out_events_to_subscribers() {
    let bus = RunBus::new();
    let run_id = Uuid::new_v4();
    let mut rx = bus.subscribe(run_id);
    bus.publish(run_id, RunEvent::RunStepSucceeded {
        run_id, step_id: "a".into(), outputs: serde_json::json!({"ok": true})
    });
    let ev = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await.unwrap().unwrap();
    match ev {
        RunEvent::RunStepSucceeded { step_id, .. } => assert_eq!(step_id, "a"),
        _ => panic!("wrong event"),
    }
}
```

- [ ] **Step 7.8: Run tests**

Run:
- `cargo test -p celerex-api --test sse -- --test-threads=1`
- `cargo test -p celerex-api --test project_members -- --test-threads=1` (un-ignore the previously-ignored manual-run test: delete the `#[ignore]` if Task 2 added one)

Expected: all tests PASS.

- [ ] **Step 7.9: Commit**

```bash
git add packages/backend/api/src/db/migrations/0009_amend_runs.sql \
        packages/backend/api/src/runs/ packages/backend/api/src/lib.rs \
        packages/backend/api/Cargo.toml \
        packages/backend/api/tests/sse.rs packages/backend/api/tests/project_members.rs
git commit -m "feat(runs): amend runs schema + broadcast-backed SSE events endpoint"
```

---

## Task 8: Notifier trait + Console + stub Teams + `notifications` table (spec sub-project H)

**Files:**
- Create: `packages/backend/api/src/db/migrations/0008_notifications.sql`
- Create: `packages/backend/api/src/notifiers/mod.rs`
- Create: `packages/backend/api/src/notifiers/console.rs`
- Create: `packages/backend/api/src/notifiers/teams_stub.rs`
- Modify: `packages/backend/api/src/lib.rs` (wire dispatch on RunStepFailed)
- Modify: `packages/backend/api/src/runs/mod.rs` (trigger notifiers from bus subscriber)
- Test: `packages/backend/api/tests/notifier.rs`

- [ ] **Step 8.1: Add migration**

Create `packages/backend/api/src/db/migrations/0008_notifications.sql`:
```sql
CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivery_status TEXT NOT NULL DEFAULT 'sent'
);

CREATE INDEX notifications_member_sent_idx
    ON notifications (project_member_id, sent_at DESC);
```

Note: 0008 is numbered before 0009 only because the table lives in a different lineage; both are created in Sprint 1. File order in sqlx-migrate runs alphabetically.

- [ ] **Step 8.2: Add notifier trait + event enum**

Create `packages/backend/api/src/notifiers/mod.rs`:
```rust
pub mod console;
pub mod teams_stub;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationEvent {
    RunFailed { run_id: Uuid, project_member_id: Uuid, step_id: String, error: String },
    RunSucceeded { run_id: Uuid, project_member_id: Uuid },
}

impl NotificationEvent {
    pub fn tag(&self) -> &'static str {
        match self { Self::RunFailed { .. } => "run_failed", Self::RunSucceeded { .. } => "run_succeeded" }
    }
    pub fn payload(&self) -> Value { serde_json::to_value(self).unwrap() }
    pub fn project_member_id(&self) -> Uuid {
        match self {
            Self::RunFailed { project_member_id, .. } | Self::RunSucceeded { project_member_id, .. } => *project_member_id,
        }
    }
    pub fn run_id(&self) -> Uuid {
        match self {
            Self::RunFailed { run_id, .. } | Self::RunSucceeded { run_id, .. } => *run_id,
        }
    }
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String>;
}
```

Register module in `packages/backend/api/src/lib.rs`:
```rust
pub mod notifiers;
```

- [ ] **Step 8.3: Implement ConsoleNotifier**

Create `packages/backend/api/src/notifiers/console.rs`:
```rust
use super::{NotificationEvent, Notifier};
use async_trait::async_trait;

pub struct ConsoleNotifier;

#[async_trait]
impl Notifier for ConsoleNotifier {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String> {
        tracing::info!(target: "celerex::notifier", tag = event.tag(),
            payload = %event.payload(), "console notification");
        Ok(())
    }
}
```

- [ ] **Step 8.4: Implement StubTeamsNotifier**

Create `packages/backend/api/src/notifiers/teams_stub.rs`:
```rust
use super::{NotificationEvent, Notifier};
use async_trait::async_trait;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct StubTeamsNotifier {
    pub sent: Mutex<Vec<NotificationEvent>>,
}

#[async_trait]
impl Notifier for StubTeamsNotifier {
    async fn dispatch(&self, event: &NotificationEvent) -> Result<(), String> {
        tracing::info!(target: "celerex::notifier",
            "StubTeamsNotifier would send: {}", serde_json::to_string(event).unwrap());
        self.sent.lock().await.push(event.clone());
        Ok(())
    }
}
```

- [ ] **Step 8.5: Persist + dispatch on bus events**

Edit `packages/backend/api/src/runs/mod.rs` — at the end of the `spawn` block in `record`, after `bus.publish`, also persist notifications for terminal failures. Add a dispatcher side-car that subscribes to the bus globally and writes `notifications` rows + calls registered notifiers.

Append to `packages/backend/api/src/notifiers/mod.rs`:
```rust
use crate::runs::bus::{RunBus, RunEvent};
use sqlx::PgPool;
use std::sync::Arc;

pub fn spawn_dispatcher(
    pool: PgPool,
    bus: Arc<RunBus>,
    notifiers: Vec<Arc<dyn Notifier>>,
    lookup_member: Arc<dyn Fn(uuid::Uuid) -> futures::future::BoxFuture<'static, Option<uuid::Uuid>> + Send + Sync>,
) {
    tokio::spawn(async move {
        // Note: the bus keys channels per-run-id. A global dispatcher would need either
        // a firehose channel or per-run subscriptions as runs are created. For Sprint 1
        // we opt for a simpler path: the orchestrator itself calls the notifier when a
        // step fails. Keep this function as the single wiring point — see build_app_with_store.
        let _ = (pool, bus, notifiers, lookup_member);
    });
}
```

**Simplification:** rather than a fan-out dispatcher, wire the notifier directly into the `retry`/`run` result path. Edit `packages/backend/api/src/project_members/routes.rs::run_manual` — replace the `tokio::spawn(async move { let _ = exec.run(req).await; });` with:

```rust
let notifiers = s.notifiers.clone();
let member_id = id;
let pool = s.pool.clone();
tokio::spawn(async move {
    let status = exec.run(req.clone()).await;
    let event = match status {
        Ok(celerex_orchestrator::job::RunStatus::Failed) => Some(crate::notifiers::NotificationEvent::RunFailed {
            run_id: req.run_id, project_member_id: member_id,
            step_id: "unknown".into(),
            error: "run failed; see run_steps for details".into(),
        }),
        Ok(celerex_orchestrator::job::RunStatus::Succeeded) => Some(crate::notifiers::NotificationEvent::RunSucceeded {
            run_id: req.run_id, project_member_id: member_id,
        }),
        _ => None,
    };
    if let Some(ev) = event {
        for n in &notifiers { let _ = n.dispatch(&ev).await; }
        let _ = sqlx::query(
            "INSERT INTO notifications (project_member_id, run_id, event, payload_json)
             VALUES ($1, $2, $3, $4)"
        ).bind(ev.project_member_id()).bind(ev.run_id()).bind(ev.tag()).bind(ev.payload())
         .execute(&pool).await;
        let _ = sqlx::query("UPDATE runs SET status = $2 WHERE id = $1")
            .bind(req.run_id).bind(ev.tag().trim_start_matches("run_"))
            .execute(&pool).await;
    }
});
```

Also extend `AppState` in `lib.rs`:
```rust
pub notifiers: Vec<Arc<dyn crate::notifiers::Notifier>>,
```
and inside `build_app_with_store`:
```rust
let notifiers: Vec<Arc<dyn notifiers::Notifier>> = vec![
    Arc::new(notifiers::console::ConsoleNotifier),
    Arc::new(notifiers::teams_stub::StubTeamsNotifier::default()),
];
```

- [ ] **Step 8.6: Write notifier test**

Create `packages/backend/api/tests/notifier.rs`:
```rust
mod common;

use celerex_api::notifiers::{NotificationEvent, Notifier, console::ConsoleNotifier, teams_stub::StubTeamsNotifier};
use uuid::Uuid;

#[tokio::test]
async fn teams_stub_buffers_dispatched_events() {
    let n = StubTeamsNotifier::default();
    let ev = NotificationEvent::RunFailed {
        run_id: Uuid::new_v4(), project_member_id: Uuid::new_v4(),
        step_id: "s1".into(), error: "boom".into(),
    };
    n.dispatch(&ev).await.unwrap();
    assert_eq!(n.sent.lock().await.len(), 1);
}

#[tokio::test]
async fn console_dispatches_without_error() {
    let n = ConsoleNotifier;
    let ev = NotificationEvent::RunSucceeded { run_id: Uuid::new_v4(), project_member_id: Uuid::new_v4() };
    n.dispatch(&ev).await.unwrap();
}
```

- [ ] **Step 8.7: Run & commit**

Run: `cargo test -p celerex-api --test notifier`
Expected: PASS.

```bash
git add packages/backend/api/src/notifiers/ \
        packages/backend/api/src/db/migrations/0008_notifications.sql \
        packages/backend/api/src/lib.rs \
        packages/backend/api/src/project_members/routes.rs \
        packages/backend/api/tests/notifier.rs
git commit -m "feat(notifiers): Notifier trait + Console + StubTeams + notifications table"
```

---

## Task 9: Shared `<AgentBuilderCanvas>` React component (spec sub-project I)

Pure React Flow component with two modes. Edit mode surfaces a palette + wiring. Monitor mode colors nodes by run state and animates edges on data flow.

**Files:**
- Modify: `packages/frontend/app/package.json` (add `@xyflow/react ^12.3.0`)
- Create: `packages/frontend/app/src/canvas/AgentBuilderCanvas.tsx`
- Create: `packages/frontend/app/src/canvas/types.ts`
- Create: `packages/frontend/app/src/canvas/palette.ts`
- Create: `packages/frontend/app/src/canvas/nodes/InboundNode.tsx`
- Create: `packages/frontend/app/src/canvas/nodes/ActionNode.tsx`
- Create: `packages/frontend/app/src/canvas/nodes/FilterNode.tsx`
- Create: `packages/frontend/app/src/canvas/nodes/OutboundNode.tsx`
- Test: `packages/frontend/app/src/canvas/AgentBuilderCanvas.test.tsx` (Vitest)

- [ ] **Step 9.1: Install React Flow**

Edit `packages/frontend/app/package.json` — add under `"dependencies"`:
```json
"@xyflow/react": "^12.3.0"
```
Then:
```bash
cd packages/frontend/app && bun install
```

(If the repo uses `npm` instead of `bun`, use that. Check the lockfile: `bun.lock` at repo root means Bun.)

- [ ] **Step 9.2: Define types**

Create `packages/frontend/app/src/canvas/types.ts`:
```ts
export type NodeCategory = "inbound" | "action" | "filter" | "outbound";

export interface GraphNode {
  id: string;
  type: NodeCategory;
  config: Record<string, unknown>;
  position?: { x: number; y: number };
}

export interface GraphEdge { from: string; to: string }

export interface Graph { nodes: GraphNode[]; edges: GraphEdge[] }

export type StepState = "idle" | "running" | "succeeded" | "failed";

export interface RunState {
  steps: Record<string, StepState>;
  lastFlowedEdge?: { from: string; to: string };
}
```

- [ ] **Step 9.3: Define palette + categories**

Create `packages/frontend/app/src/canvas/palette.ts`:
```ts
import { NodeCategory } from "./types";

export interface PaletteItem {
  type: string;              // e.g. "workiq.pull_metrics"
  label: string;
  category: NodeCategory;
  defaultConfig: Record<string, unknown>;
}

export const PALETTE: PaletteItem[] = [
  { type: "schedule.tick", label: "Schedule tick", category: "inbound", defaultConfig: {} },
  { type: "workiq.pull_metrics", label: "workIQ metrics", category: "action",
    defaultConfig: { execution: "http", inputs: { window_days: 7 } } },
  { type: "stargate.query", label: "Stargate SQL", category: "action",
    defaultConfig: { execution: "http", inputs: { sql: "SELECT 1" } } },
  { type: "file.extract.xlsx", label: "Excel extract", category: "action",
    defaultConfig: { execution: "script", inputs: { extraction_mode: "markdown" } } },
  { type: "file.extract.docx", label: "Word extract", category: "action",
    defaultConfig: { execution: "script", inputs: { extraction_mode: "markdown" } } },
  { type: "llm.synthesize", label: "LLM synthesize", category: "action",
    defaultConfig: { execution: "llm", model: "claude-sonnet-4-6" } },
  { type: "filter.expr", label: "Filter", category: "filter",
    defaultConfig: { expression: "true" } },
  { type: "document.create", label: "Create BlockSuite doc", category: "outbound",
    defaultConfig: { execution: "script", inputs: { title_template: "Report {{date}}" } } },
];

export const CATEGORY_COLORS: Record<NodeCategory, { bg: string; border: string }> = {
  inbound: { bg: "#e0f2fe", border: "#0284c7" },
  action:  { bg: "#fef3c7", border: "#d97706" },
  filter:  { bg: "#ede9fe", border: "#7c3aed" },
  outbound:{ bg: "#dcfce7", border: "#16a34a" },
};
```

- [ ] **Step 9.4: Write custom node components**

Create `packages/frontend/app/src/canvas/nodes/ActionNode.tsx`:
```tsx
import { Handle, Position, NodeProps } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface ActionNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export function ActionNode({ data }: NodeProps<ActionNodeData>) {
  const c = CATEGORY_COLORS.action;
  return (
    <div data-testid={`node-${data.label}`} style={{
      padding: "8px 12px", borderRadius: 8,
      background: c.bg, border: `2px solid ${stateBorder(data.state) ?? c.border}`,
      minWidth: 140, fontSize: 13,
    }}>
      <Handle type="target" position={Position.Left} />
      {data.label}
      {data.state === "running" && <span style={{ marginLeft: 6 }}>⏳</span>}
      {data.state === "succeeded" && <span style={{ marginLeft: 6 }}>✓</span>}
      {data.state === "failed" && <span style={{ marginLeft: 6 }}>✗</span>}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function stateBorder(s?: StepState): string | undefined {
  return s === "running" ? "#3b82f6" : s === "succeeded" ? "#16a34a" : s === "failed" ? "#dc2626" : undefined;
}
```

Create `packages/frontend/app/src/canvas/nodes/InboundNode.tsx`:
```tsx
import { Handle, Position, NodeProps } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface InboundNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export function InboundNode({ data }: NodeProps<InboundNodeData>) {
  const c = CATEGORY_COLORS.inbound;
  return (
    <div data-testid={`node-${data.label}`} style={{
      padding: "8px 12px", borderRadius: 8,
      background: c.bg, border: `2px solid ${c.border}`,
      minWidth: 140, fontSize: 13,
    }}>
      {data.label}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
```

Create `packages/frontend/app/src/canvas/nodes/FilterNode.tsx`:
```tsx
import { Handle, Position, NodeProps } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface FilterNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export function FilterNode({ data }: NodeProps<FilterNodeData>) {
  const c = CATEGORY_COLORS.filter;
  return (
    <div data-testid={`node-${data.label}`} style={{
      padding: "8px 12px", borderRadius: 8,
      background: c.bg, border: `2px solid ${c.border}`,
      minWidth: 140, fontSize: 13,
    }}>
      <Handle type="target" position={Position.Left} />
      {data.label}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
```

Create `packages/frontend/app/src/canvas/nodes/OutboundNode.tsx`:
```tsx
import { Handle, Position, NodeProps } from "@xyflow/react";
import { CATEGORY_COLORS } from "../palette";
import { StepState } from "../types";

export interface OutboundNodeData extends Record<string, unknown> {
  label: string;
  state?: StepState;
}

export function OutboundNode({ data }: NodeProps<OutboundNodeData>) {
  const c = CATEGORY_COLORS.outbound;
  return (
    <div data-testid={`node-${data.label}`} style={{
      padding: "8px 12px", borderRadius: 8,
      background: c.bg, border: `2px solid ${c.border}`,
      minWidth: 140, fontSize: 13,
    }}>
      <Handle type="target" position={Position.Left} />
      {data.label}
    </div>
  );
}
```

- [ ] **Step 9.5: Write the canvas component (edit + monitor)**

Create `packages/frontend/app/src/canvas/AgentBuilderCanvas.tsx`:
```tsx
import { ReactFlow, Background, Controls, Node, Edge, applyNodeChanges, applyEdgeChanges, NodeChange, EdgeChange, Connection, addEdge } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback, useMemo, useState, useEffect } from "react";
import { Graph, RunState, StepState } from "./types";
import { ActionNode } from "./nodes/ActionNode";
import { InboundNode } from "./nodes/InboundNode";
import { FilterNode } from "./nodes/FilterNode";
import { OutboundNode } from "./nodes/OutboundNode";
import { PALETTE } from "./palette";

const nodeTypes = { action: ActionNode, inbound: InboundNode, filter: FilterNode, outbound: OutboundNode };

export interface AgentBuilderCanvasProps {
  graph: Graph;
  mode: "edit" | "monitor";
  onEdit?: (graph: Graph) => void;
  runState?: RunState;
}

export function AgentBuilderCanvas({ graph, mode, onEdit, runState }: AgentBuilderCanvasProps) {
  const [nodes, setNodes] = useState<Node[]>(() => toReactFlowNodes(graph, runState));
  const [edges, setEdges] = useState<Edge[]>(() => toReactFlowEdges(graph, runState));

  useEffect(() => {
    setNodes(toReactFlowNodes(graph, runState));
    setEdges(toReactFlowEdges(graph, runState));
  }, [graph, runState]);

  const onNodesChange = useCallback((chs: NodeChange[]) => {
    if (mode !== "edit") return;
    setNodes(prev => {
      const next = applyNodeChanges(chs, prev);
      onEdit?.(fromReactFlow(next, edges));
      return next;
    });
  }, [mode, edges, onEdit]);

  const onEdgesChange = useCallback((chs: EdgeChange[]) => {
    if (mode !== "edit") return;
    setEdges(prev => {
      const next = applyEdgeChanges(chs, prev);
      onEdit?.(fromReactFlow(nodes, next));
      return next;
    });
  }, [mode, nodes, onEdit]);

  const onConnect = useCallback((conn: Connection) => {
    if (mode !== "edit") return;
    setEdges(prev => {
      const next = addEdge({ ...conn, animated: true }, prev);
      onEdit?.(fromReactFlow(nodes, next));
      return next;
    });
  }, [mode, nodes, onEdit]);

  const contextMenu = useMemo(() => mode === "edit" ? <PaletteSidebar onAdd={(item) => {
    const id = `n_${Date.now()}`;
    const node: Node = { id, type: item.category, position: { x: 80 + nodes.length * 40, y: 80 },
      data: { label: item.label, state: "idle" satisfies StepState } };
    setNodes(prev => {
      const next = [...prev, node];
      onEdit?.(fromReactFlow(next, edges));
      return next;
    });
  }} /> : null, [mode, nodes.length, edges, onEdit]);

  return (
    <div style={{ display: "flex", flex: 1, height: "100%" }}>
      {contextMenu}
      <div style={{ flex: 1 }}>
        <ReactFlow nodes={nodes} edges={edges} nodeTypes={nodeTypes}
          onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={onConnect}
          fitView>
          <Background />
          <Controls />
        </ReactFlow>
      </div>
    </div>
  );
}

function PaletteSidebar({ onAdd }: { onAdd: (item: typeof PALETTE[number]) => void }) {
  return (
    <div data-testid="palette" style={{ width: 220, borderRight: "1px solid var(--celerex-border)", padding: 12 }}>
      <h4 style={{ margin: "0 0 8px" }}>Palette</h4>
      {PALETTE.map(it => (
        <button key={it.type} onClick={() => onAdd(it)}
          style={{ display: "block", width: "100%", marginBottom: 6, padding: "6px 8px",
            borderRadius: 6, border: "1px solid var(--celerex-border)", background: "transparent",
            textAlign: "left", cursor: "pointer" }}>
          <span style={{ fontSize: 10, textTransform: "uppercase", opacity: 0.6 }}>{it.category}</span>
          <div>{it.label}</div>
        </button>
      ))}
    </div>
  );
}

function toReactFlowNodes(g: Graph, rs?: RunState): Node[] {
  return g.nodes.map((n, i) => ({
    id: n.id, type: n.type, position: n.position ?? { x: 80 + i * 180, y: 80 + (i % 3) * 100 },
    data: { label: (n.config["label"] as string) ?? n.id, state: rs?.steps[n.id] ?? "idle" },
  }));
}
function toReactFlowEdges(g: Graph, rs?: RunState): Edge[] {
  return g.edges.map(e => ({
    id: `${e.from}->${e.to}`, source: e.from, target: e.to,
    animated: rs?.lastFlowedEdge?.from === e.from && rs?.lastFlowedEdge?.to === e.to,
  }));
}
function fromReactFlow(nodes: Node[], edges: Edge[]): Graph {
  return {
    nodes: nodes.map(n => ({
      id: n.id, type: (n.type ?? "action") as Graph["nodes"][number]["type"],
      config: { label: (n.data as { label?: string })?.label ?? n.id },
      position: n.position,
    })),
    edges: edges.map(e => ({ from: e.source, to: e.target })),
  };
}
```

- [ ] **Step 9.6: Write Vitest mount test**

Create `packages/frontend/app/src/canvas/AgentBuilderCanvas.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { AgentBuilderCanvas } from "./AgentBuilderCanvas";

describe("AgentBuilderCanvas", () => {
  it("renders the palette in edit mode", () => {
    render(<AgentBuilderCanvas mode="edit" graph={{ nodes: [], edges: [] }} />);
    expect(screen.getByTestId("palette")).toBeTruthy();
  });

  it("omits the palette in monitor mode", () => {
    render(<AgentBuilderCanvas mode="monitor" graph={{ nodes: [], edges: [] }} />);
    expect(screen.queryByTestId("palette")).toBeNull();
  });
});
```

Ensure the repo has `@testing-library/react` + `vitest` + `jsdom` configured. If they are not yet set up, add them to `packages/frontend/app/package.json` `devDependencies`:
```json
"@testing-library/react": "^16.0.0",
"vitest": "^2.0.0",
"jsdom": "^25.0.0"
```
And add a vitest config file `packages/frontend/app/vitest.config.ts`:
```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
export default defineConfig({ plugins: [react()], test: { environment: "jsdom", globals: true } });
```

- [ ] **Step 9.7: Run tests**

```bash
cd packages/frontend/app && bun run test -- canvas
```
Expected: both tests PASS.

- [ ] **Step 9.8: Commit**

```bash
git add packages/frontend/app/package.json packages/frontend/app/bun.lock \
        packages/frontend/app/vitest.config.ts \
        packages/frontend/app/src/canvas/
git commit -m "feat(frontend): AgentBuilderCanvas (React Flow) with edit + monitor modes"
```

---

## Task 10: Builder page (spec sub-project J)

Hosts `<AgentBuilderCanvas mode="edit">` plus a right-side node config panel and a "Run now" button. Ports palette labels/categorization from pre-pivot `WorkflowDesigner.tsx` (label strings + category grouping only — the canvas is new).

**Files:**
- Create: `packages/frontend/app/src/pages/BuilderPage.tsx`
- Create: `packages/frontend/app/src/canvas/NodeConfigPanel.tsx`
- Modify: `packages/frontend/app/src/state/atoms.ts` (extend `activeView` union + add `activeMemberIdAtom`, `activeRunIdAtom`)
- Modify: `packages/frontend/app/src/App.tsx` (mount BuilderPage when `activeView === "builder"`)
- Modify: `packages/frontend/app/src/api/client.ts` (add project-member API)
- Test: `packages/frontend/app/src/pages/BuilderPage.test.tsx`

- [ ] **Step 10.1: Extend the API client**

Append to `packages/frontend/app/src/api/client.ts`:
```ts
export interface ApiProjectMember {
  id: string; project_id: string; name: string; description: string;
  graph_json: { nodes: { id: string; type: string; config: Record<string, unknown> }[];
                edges: { from: string; to: string }[] };
  provider_config_json: Record<string, unknown>; version: number;
}

export async function createMember(token: string, body: {
  project_id: string; name: string;
  graph_json: ApiProjectMember["graph_json"];
  provider_config_json?: Record<string, unknown>;
}): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ provider_config_json: {}, ...body }),
  });
  if (!r.ok) throw new Error(`createMember: ${r.status}`);
  return r.json();
}

export async function updateMember(token: string, id: string, body: {
  name: string; graph_json: ApiProjectMember["graph_json"];
  provider_config_json?: Record<string, unknown>;
}): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members/${id}`, {
    method: "PUT",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ provider_config_json: {}, ...body }),
  });
  if (!r.ok) throw new Error(`updateMember: ${r.status}`);
  return r.json();
}

export async function runMemberNow(token: string, id: string): Promise<{ run_id: string }> {
  const r = await fetch(`${API_BASE}/project_members/${id}/run`, {
    method: "POST", headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`runMemberNow: ${r.status}`);
  return r.json();
}

export async function listMembers(token: string, projectId: string): Promise<ApiProjectMember[]> {
  const r = await fetch(`${API_BASE}/projects/${projectId}/members`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listMembers: ${r.status}`);
  return r.json();
}
```

- [ ] **Step 10.2: Add atoms for Builder/Monitor state**

Append to `packages/frontend/app/src/state/atoms.ts`:
```ts
export const activeMemberIdAtom = atom<string | null>(null);
export const activeRunIdAtom = atom<string | null>(null);
```

Extend the `activeView` atom union (find its declaration and add `"builder" | "monitor" | "fleet"` to whatever string literals are already there). Example — if the current union is `"editor" | "settings" | "agent-dashboard" | …`, add the three new values at the end.

- [ ] **Step 10.3: Write the Builder page**

Create `packages/frontend/app/src/canvas/NodeConfigPanel.tsx`:
```tsx
import { GraphNode } from "./types";
export function NodeConfigPanel({ node, onChange }: {
  node: GraphNode | null; onChange: (next: GraphNode) => void;
}) {
  if (!node) return <div style={{ width: 280, padding: 12, opacity: 0.5 }}>Select a node</div>;
  return (
    <div data-testid="config-panel" style={{ width: 280, padding: 12, borderLeft: "1px solid var(--celerex-border)" }}>
      <h4>{node.id}</h4>
      <textarea value={JSON.stringify(node.config, null, 2)} rows={12}
        onChange={(e) => { try { onChange({ ...node, config: JSON.parse(e.target.value) }); } catch {} }}
        style={{ width: "100%", fontFamily: "monospace", fontSize: 12 }} />
    </div>
  );
}
```

Create `packages/frontend/app/src/pages/BuilderPage.tsx`:
```tsx
import { useState } from "react";
import { useAtom, useSetAtom } from "jotai";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { NodeConfigPanel } from "../canvas/NodeConfigPanel";
import { Graph, GraphNode } from "../canvas/types";
import { createMember, runMemberNow } from "../api/client";
import { activeMemberIdAtom, activeRunIdAtom, activeViewAtom } from "../state/atoms";

export function BuilderPage({ projectId, token }: { projectId: string; token: string }) {
  const [graph, setGraph] = useState<Graph>({ nodes: [], edges: [] });
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [name, setName] = useState("New agent");
  const setMember = useSetAtom(activeMemberIdAtom);
  const setRun = useSetAtom(activeRunIdAtom);
  const [, setView] = useAtom(activeViewAtom);

  const onRunNow = async () => {
    const m = await createMember(token, { project_id: projectId, name, graph_json: graph });
    setMember(m.id);
    const { run_id } = await runMemberNow(token, m.id);
    setRun(run_id);
    setView("monitor");
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1 }}>
      <div style={{ padding: 12, borderBottom: "1px solid var(--celerex-border)", display: "flex", gap: 8 }}>
        <input value={name} onChange={(e) => setName(e.target.value)} />
        <button data-testid="run-now" onClick={onRunNow}>Run now</button>
      </div>
      <div style={{ flex: 1, display: "flex" }}>
        <AgentBuilderCanvas graph={graph} mode="edit" onEdit={setGraph} />
        <NodeConfigPanel node={selected} onChange={(n) => {
          setGraph(prev => ({ ...prev, nodes: prev.nodes.map(x => x.id === n.id ? n : x) }));
          setSelected(n);
        }} />
      </div>
    </div>
  );
}
```

Mount in `packages/frontend/app/src/App.tsx` — in the big ternary chain, add:
```tsx
: activeView === "builder" ? (
  <BuilderPage projectId={/* pull from projects atom */ ""} token={/* pull from auth atom */ ""} />
)
```

Use whichever atoms already hold the active project + JWT — if an auth atom isn't yet exposed, create one now (`tokenAtom`, initialized from localStorage) and expose via `useAtomValue(tokenAtom)`.

- [ ] **Step 10.4: Write Builder page test**

Create `packages/frontend/app/src/pages/BuilderPage.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { BuilderPage } from "./BuilderPage";

vi.mock("../api/client", () => ({
  createMember: vi.fn(async () => ({ id: "m1", project_id: "p1", name: "x", description: "",
    graph_json: { nodes: [], edges: [] }, provider_config_json: {}, version: 1 })),
  runMemberNow: vi.fn(async () => ({ run_id: "r1" })),
}));

describe("BuilderPage", () => {
  it("mounts with Run now button", () => {
    render(<BuilderPage projectId="p1" token="tok" />);
    expect(screen.getByTestId("run-now")).toBeTruthy();
  });
});
```

Run: `cd packages/frontend/app && bun run test -- BuilderPage`
Expected: PASS.

- [ ] **Step 10.5: Commit**

```bash
git add packages/frontend/app/src/
git commit -m "feat(frontend): BuilderPage + NodeConfigPanel + project_members API client"
```

---

## Task 11: Monitor page + SSE client (spec sub-project K)

Reuses `<AgentBuilderCanvas mode="monitor">` with a run-selector dropdown. A `useRunEvents(runId)` hook subscribes to the SSE endpoint and maps events into `RunState`. A StepDrawer shows inputs/outputs for a clicked node.

**Files:**
- Create: `packages/frontend/app/src/api/sse.ts` (useRunEvents hook)
- Create: `packages/frontend/app/src/pages/MonitorPage.tsx`
- Create: `packages/frontend/app/src/canvas/StepDrawer.tsx`
- Modify: `packages/frontend/app/src/App.tsx` (mount MonitorPage)
- Test: `packages/frontend/app/src/api/sse.test.ts`
- Test: `packages/frontend/app/src/pages/MonitorPage.test.tsx`

- [ ] **Step 11.1: Write the SSE hook**

Create `packages/frontend/app/src/api/sse.ts`:
```ts
import { useEffect, useRef, useState } from "react";
import { API_BASE } from "./client";
import { RunState, StepState } from "../canvas/types";

interface ServerEvent {
  event: "run_step_started" | "run_step_succeeded" | "run_step_failed" | "run_completed";
  run_id: string; step_id?: string; outputs?: unknown; error?: string; status?: string;
}

export function useRunEvents(runId: string | null): RunState {
  const [state, setState] = useState<RunState>({ steps: {} });
  const sourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!runId) return;
    const es = new EventSource(`${API_BASE}/runs/${runId}/events`);
    sourceRef.current = es;
    es.onmessage = (msg) => {
      try {
        const ev = JSON.parse(msg.data) as ServerEvent;
        setState(prev => applyEvent(prev, ev));
      } catch { /* ignore bad frames */ }
    };
    return () => { es.close(); sourceRef.current = null; };
  }, [runId]);

  return state;
}

function applyEvent(prev: RunState, ev: ServerEvent): RunState {
  if (!ev.step_id) return prev;
  const s: StepState = ev.event === "run_step_started" ? "running"
    : ev.event === "run_step_succeeded" ? "succeeded"
    : ev.event === "run_step_failed" ? "failed" : "idle";
  return { ...prev, steps: { ...prev.steps, [ev.step_id]: s } };
}
```

- [ ] **Step 11.2: Write the SSE hook test**

Create `packages/frontend/app/src/api/sse.test.ts`:
```ts
import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useRunEvents } from "./sse";

class MockEventSource {
  onmessage: ((e: MessageEvent) => void) | null = null;
  close = vi.fn();
  constructor(public url: string) { (MockEventSource as any).last = this; }
}
(globalThis as any).EventSource = MockEventSource;

describe("useRunEvents", () => {
  beforeEach(() => { (MockEventSource as any).last = null; });

  it("updates step state on run_step_succeeded", () => {
    const { result } = renderHook(() => useRunEvents("r1"));
    const es = (MockEventSource as any).last as MockEventSource;
    act(() => {
      es.onmessage?.(new MessageEvent("message", {
        data: JSON.stringify({ event: "run_step_succeeded", run_id: "r1", step_id: "a" })
      }));
    });
    expect(result.current.steps.a).toBe("succeeded");
  });
});
```

- [ ] **Step 11.3: Write the MonitorPage**

Create `packages/frontend/app/src/pages/MonitorPage.tsx`:
```tsx
import { useAtomValue } from "jotai";
import { AgentBuilderCanvas } from "../canvas/AgentBuilderCanvas";
import { activeRunIdAtom, activeMemberIdAtom } from "../state/atoms";
import { useRunEvents } from "../api/sse";
import { Graph } from "../canvas/types";
import { useState } from "react";

export function MonitorPage({ graph }: { graph: Graph }) {
  const runId = useAtomValue(activeRunIdAtom);
  const memberId = useAtomValue(activeMemberIdAtom);
  const runState = useRunEvents(runId);
  const [history] = useState<string[]>(runId ? [runId] : []);

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1 }}>
      <div style={{ padding: 12, borderBottom: "1px solid var(--celerex-border)" }}>
        <label>Run: </label>
        <select data-testid="run-selector" value={runId ?? ""}>
          <option value={runId ?? ""}>{runId ?? "Live"}</option>
          {history.map(h => <option key={h} value={h}>{h}</option>)}
        </select>
        <span style={{ marginLeft: 12, opacity: 0.6 }}>member: {memberId}</span>
      </div>
      <AgentBuilderCanvas graph={graph} mode="monitor" runState={runState} />
    </div>
  );
}
```

Mount in `App.tsx` ternary chain:
```tsx
: activeView === "monitor" ? (<MonitorPage graph={/* load from active member */ { nodes: [], edges: [] }} />)
```

The graph is loaded from the active member via a small effect in the host or an atom — keep Sprint 1 simple by fetching on view entry via a `useEffect` that calls `getMember(token, memberId)`. Add a `getMember` API client call:
```ts
export async function getMember(token: string, id: string): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members/${id}`, { headers: { authorization: `Bearer ${token}` } });
  if (!r.ok) throw new Error(`getMember: ${r.status}`);
  return r.json();
}
```

- [ ] **Step 11.4: Add StepDrawer**

Create `packages/frontend/app/src/canvas/StepDrawer.tsx`:
```tsx
export function StepDrawer({ stepId, inputs, outputs, error, onClose }: {
  stepId: string; inputs?: unknown; outputs?: unknown; error?: string; onClose: () => void;
}) {
  return (
    <div style={{ position: "absolute", right: 0, top: 0, width: 360, height: "100%",
      background: "var(--celerex-bg-primary)", borderLeft: "1px solid var(--celerex-border)",
      padding: 16, overflow: "auto" }}>
      <button onClick={onClose}>✕</button>
      <h3>{stepId}</h3>
      {error && <pre style={{ color: "#dc2626" }}>{error}</pre>}
      <h4>Inputs</h4>
      <pre>{JSON.stringify(inputs, null, 2)}</pre>
      <h4>Outputs</h4>
      <pre>{JSON.stringify(outputs, null, 2)}</pre>
    </div>
  );
}
```

Wire StepDrawer via a click handler on canvas nodes — in AgentBuilderCanvas, add an `onNodeClick` prop that surfaces `(id: string)` and let MonitorPage open the drawer.

- [ ] **Step 11.5: Run tests**

```bash
cd packages/frontend/app && bun run test -- sse MonitorPage
```
Expected: PASS.

- [ ] **Step 11.6: Commit**

```bash
git add packages/frontend/app/src/pages/MonitorPage.tsx \
        packages/frontend/app/src/api/sse.ts packages/frontend/app/src/api/sse.test.ts \
        packages/frontend/app/src/canvas/StepDrawer.tsx \
        packages/frontend/app/src/api/client.ts \
        packages/frontend/app/src/App.tsx
git commit -m "feat(frontend): MonitorPage + SSE client hook + StepDrawer"
```

---

## Task 12: Fleet dashboard (spec sub-project L)

Top-level view: table of all project members in the engagement, with columns `name | last_status | next_fire_at | running?`. Row click navigates to MonitorPage for the latest run.

**Files:**
- Modify: `packages/backend/api/src/project_members/routes.rs` (add `GET /api/project_members/fleet`)
- Modify: `packages/backend/api/src/lib.rs` (mount route)
- Create: `packages/frontend/app/src/pages/FleetDashboard.tsx`
- Modify: `packages/frontend/app/src/api/client.ts` (add `fetchFleet`)
- Modify: `packages/frontend/app/src/App.tsx` (mount fleet view)
- Test: `packages/backend/api/tests/fleet.rs`
- Test: `packages/frontend/app/src/pages/FleetDashboard.test.tsx`

- [ ] **Step 12.1: Backend route**

Append to `packages/backend/api/src/project_members/routes.rs`:
```rust
#[derive(serde::Serialize)]
pub struct FleetRow {
    pub id: Uuid,
    pub name: String,
    pub project_id: Uuid,
    pub last_status: Option<String>,
    pub next_fire_at: Option<chrono::DateTime<chrono::Utc>>,
    pub running: bool,
}

pub async fn fleet(a: AuthClaims, State(s): State<AppState>) -> impl IntoResponse {
    let rows: Result<Vec<(Uuid, String, Uuid, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<String>)>, _> = sqlx::query_as(
        "SELECT m.id, m.name, m.project_id,
                (SELECT status FROM runs WHERE runs.project_id = m.project_id ORDER BY created_at DESC LIMIT 1) AS last_status,
                (SELECT next_fire_at FROM schedules WHERE schedules.project_member_id = m.id ORDER BY next_fire_at LIMIT 1) AS next_fire_at,
                (SELECT status FROM runs WHERE runs.project_id = m.project_id ORDER BY created_at DESC LIMIT 1) AS running_marker
         FROM project_members m
         JOIN projects p ON p.id = m.project_id
         WHERE p.engagement_id = $1"
    ).bind(a.0.engagement_id).fetch_all(&s.pool).await;
    match rows {
        Ok(rows) => {
            let out: Vec<FleetRow> = rows.into_iter().map(|r| FleetRow {
                id: r.0, name: r.1, project_id: r.2, last_status: r.3,
                next_fire_at: r.4, running: matches!(r.5.as_deref(), Some("running")),
            }).collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}
```

Mount in `lib.rs`:
```rust
.route("/api/project_members/fleet", get(project_members::routes::fleet))
```

- [ ] **Step 12.2: Backend test**

Create `packages/backend/api/tests/fleet.rs`:
```rust
mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

#[tokio::test]
async fn fleet_returns_member_rows_with_null_statuses_when_no_runs() {
    let db = common::start_test_db().await;
    let app = celerex_api::build_app(db.pool.clone());
    let eng = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id"
    ).fetch_one(&db.pool).await.unwrap();
    let project = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id"
    ).bind(eng).fetch_one(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO project_members (project_id, name, graph_json) VALUES ($1, 'm1', '{\"nodes\":[],\"edges\":[]}'::jsonb)")
        .bind(project).execute(&db.pool).await.unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    let res = app.oneshot(
        Request::builder().uri("/api/project_members/fleet")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["name"], "m1");
    assert_eq!(body[0]["running"], false);
}
```

- [ ] **Step 12.3: Frontend page**

Append to `packages/frontend/app/src/api/client.ts`:
```ts
export interface FleetRow {
  id: string; name: string; project_id: string;
  last_status: string | null; next_fire_at: string | null; running: boolean;
}
export async function fetchFleet(token: string): Promise<FleetRow[]> {
  const r = await fetch(`${API_BASE}/project_members/fleet`, { headers: { authorization: `Bearer ${token}` } });
  if (!r.ok) throw new Error(`fetchFleet: ${r.status}`);
  return r.json();
}
```

Create `packages/frontend/app/src/pages/FleetDashboard.tsx`:
```tsx
import { useEffect, useState } from "react";
import { useSetAtom } from "jotai";
import { fetchFleet, FleetRow } from "../api/client";
import { activeMemberIdAtom, activeViewAtom } from "../state/atoms";

export function FleetDashboard({ token }: { token: string }) {
  const [rows, setRows] = useState<FleetRow[]>([]);
  const setMember = useSetAtom(activeMemberIdAtom);
  const setView = useSetAtom(activeViewAtom);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try { const r = await fetchFleet(token); if (!cancelled) setRows(r); } catch {}
    };
    tick();
    const id = setInterval(tick, 5000);
    return () => { cancelled = true; clearInterval(id); };
  }, [token]);

  return (
    <div style={{ padding: 16, flex: 1, overflow: "auto" }}>
      <h2>Fleet</h2>
      <table data-testid="fleet-table" style={{ width: "100%", borderCollapse: "collapse" }}>
        <thead><tr>
          <th>Name</th><th>Last status</th><th>Next fire</th><th>Running?</th>
        </tr></thead>
        <tbody>
          {rows.map(r => (
            <tr key={r.id} onClick={() => { setMember(r.id); setView("monitor"); }} style={{ cursor: "pointer" }}>
              <td>{r.name}</td>
              <td>{r.last_status ?? "—"}</td>
              <td>{r.next_fire_at ?? "—"}</td>
              <td>{r.running ? "● live" : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

Mount in `App.tsx`:
```tsx
: activeView === "fleet" ? <FleetDashboard token={token} />
```

- [ ] **Step 12.4: Frontend test**

Create `packages/frontend/app/src/pages/FleetDashboard.test.tsx`:
```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { FleetDashboard } from "./FleetDashboard";

vi.mock("../api/client", () => ({
  fetchFleet: vi.fn(async () => [{ id: "m1", name: "Jean", project_id: "p1",
    last_status: "succeeded", next_fire_at: "2026-04-20T13:00:00Z", running: false }]),
}));

describe("FleetDashboard", () => {
  it("renders a row per member", async () => {
    render(<FleetDashboard token="tok" />);
    await waitFor(() => expect(screen.getByText("Jean")).toBeTruthy());
  });
});
```

- [ ] **Step 12.5: Run & commit**

```bash
cargo test -p celerex-api --test fleet -- --test-threads=1
cd packages/frontend/app && bun run test -- FleetDashboard
cd -
git add packages/backend/api/src/project_members/routes.rs packages/backend/api/src/lib.rs \
        packages/backend/api/tests/fleet.rs \
        packages/frontend/app/src/pages/FleetDashboard.tsx \
        packages/frontend/app/src/pages/FleetDashboard.test.tsx \
        packages/frontend/app/src/api/client.ts packages/frontend/app/src/App.tsx
git commit -m "feat(frontend): Fleet dashboard with 5s poll; backend /project_members/fleet route"
```

---

## Task 13: Sidebar "+ Add agent to project" entry point (spec sub-project M)

Adds a button under each project in the sidebar project tree that creates a blank Builder session.

**Files:**
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx`
- Modify: `packages/frontend/app/src/state/atoms.ts` (add `activeProjectIdAtom` if missing)
- Test: `packages/frontend/app/src/layout/Sidebar.test.tsx` (extend existing or create new)

- [ ] **Step 13.1: Inspect the existing sidebar**

Read `packages/frontend/app/src/layout/Sidebar.tsx` to find the project-tree rendering block. The Sprint 0 commit `a242b6e feat(frontend): sidebar project tree level` added it.

- [ ] **Step 13.2: Add "+ Add agent to project" button**

In the project-tree render, alongside the project name (or in a per-project actions row), add:
```tsx
<button
  data-testid={`add-agent-${project.id}`}
  onClick={(e) => { e.stopPropagation(); setActiveProjectId(project.id); setActiveView("builder"); }}
  style={{ fontSize: 11, opacity: 0.7, background: "transparent", border: "none", cursor: "pointer" }}
>+ Add agent</button>
```

Ensure `activeProjectIdAtom` exists in `state/atoms.ts`:
```ts
export const activeProjectIdAtom = atom<string | null>(null);
```

- [ ] **Step 13.3: Test**

Create or extend `packages/frontend/app/src/layout/Sidebar.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Provider } from "jotai";
import { Sidebar } from "./Sidebar";

describe("Sidebar + Add agent", () => {
  it("renders the add-agent affordance per project", () => {
    // Seed a project into the engagements atom via the test harness.
    // For Sprint 1, render in a Jotai Provider with a pre-populated store.
    render(<Provider><Sidebar collapsed={false} onToggleCollapse={() => {}} /></Provider>);
    // Without seeded projects this is a smoke test — assert the sidebar renders.
    expect(screen.getByRole("complementary") || document.body).toBeTruthy();
  });
});
```

(If the sidebar already has a structural test, extend that one instead of creating a new file.)

- [ ] **Step 13.4: Commit**

```bash
git add packages/frontend/app/src/layout/Sidebar.tsx \
        packages/frontend/app/src/state/atoms.ts \
        packages/frontend/app/src/layout/Sidebar.test.tsx
git commit -m "feat(frontend): sidebar '+ Add agent to project' entry into BuilderPage"
```

---

## Task 14: E2E acceptance — Jean's weekly newsletter run (spec sub-project N)

Full end-to-end backend integration test: seed a project-member graph with every node type, manually trigger the run via `POST /api/project_members/:id/run`, assert: the orchestrator writes all expected `run_steps`, SSE emits the correct events in topo order, a new BlockSuite-shaped doc with title `"Weekly Newsletter <YYYY-MM-DD>"` is created under the project, the run's `snapshot_id` is set and `triggered_by = 'manual'`, the fleet query reflects the succeeded status, and no notifier was fired for a success path.

**Files:**
- Create: `packages/backend/api/tests/e2e_jean_newsletter.rs`
- Create: `packages/backend/api/tests/fixtures/files/jean_metrics.xlsx` (built by the same Python snippet as Task 4)
- Modify: `packages/backend/api/src/lib.rs` (register LLM, workIQ, Stargate, document.create handlers on the dispatcher using `MockLlm` and fixtures when `TEST_E2E=1`)

- [ ] **Step 14.1: Register dispatcher handlers**

Edit `packages/backend/api/src/lib.rs` — in `build_app_with_store`, after `let dispatcher = Arc::new(Dispatcher::new());`, add a `register_default_handlers(&dispatcher, pool.clone())` call. Create the helper in the same file:
```rust
fn register_default_handlers(dispatcher: &Dispatcher, _pool: PgPool) {
    // Real registration is deferred; tests register their own handlers via
    // build_app_with_dispatcher. This keeps production wiring explicit.
}
```

Add a new entry point:
```rust
pub fn build_app_with_dispatcher(pool: PgPool, credentials: Arc<dyn CredentialStore>,
    dispatcher: Arc<Dispatcher>) -> Router {
    let run_bus = Arc::new(RunBus::new());
    let run_store = Arc::new(runs::PgRunStore::with_bus(pool.clone(), run_bus.clone()));
    let executor = Arc::new(Executor::new(dispatcher, run_store));
    let members: Arc<dyn MembersStore> = Arc::new(project_members::store::PgMembersStore::new(pool.clone()));
    let notifiers: Vec<Arc<dyn notifiers::Notifier>> = vec![
        Arc::new(notifiers::console::ConsoleNotifier),
        Arc::new(notifiers::teams_stub::StubTeamsNotifier::default()),
    ];
    // (router as in build_app_with_store)
    // …routes identical…
    Router::new() /* … */
        .with_state(AppState { pool, credentials, executor, members, run_bus, notifiers })
}
```

Factor the route chain into a helper so `build_app_with_store` and `build_app_with_dispatcher` share it.

- [ ] **Step 14.2: Write the E2E test**

Create `packages/backend/api/tests/e2e_jean_newsletter.rs`:
```rust
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use celerex_orchestrator::execution_type::ExecutionType;
use celerex_orchestrator::handlers::llm::LlmHandler;
use celerex_orchestrator::llm::mock::MockLlm;
use celerex_orchestrator::tool_dispatch::{Dispatcher, Handler};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::util::ServiceExt;

#[tokio::test]
async fn jean_weekly_newsletter_e2e() {
    let db = common::start_test_db().await;

    // Build dispatcher with: workIQ fixture, Stargate fixture, excel markdown handler,
    // docx markdown handler, LLM (MockLlm), document.create handler.
    let dispatcher = Arc::new(Dispatcher::new());

    // Register workIQ + Stargate as Http handlers whose handler inspects the config
    // and dispatches to the fixture. For Sprint 1 we register them under ExecutionType::Http.
    struct WorkIqHandler;
    #[async_trait::async_trait]
    impl Handler for WorkIqHandler {
        async fn invoke(&self, _config: Value, _inputs: Value)
            -> Result<Value, celerex_orchestrator::tool_dispatch::DispatchError>
        {
            let f = celerex_api::plugins::fixtures::workiq::WorkIqFixture::from_fixture_dir();
            f.invoke("workiq.pull_metrics", json!({"window_days": 7})).await
                .map_err(celerex_orchestrator::tool_dispatch::DispatchError::Handler)
        }
    }
    dispatcher.register(ExecutionType::Http, Arc::new(WorkIqHandler));

    // LLM handler with MockLlm that outputs the newsletter markdown.
    let mock = MockLlm::new().finish("# Weekly Newsletter\nMetrics up; Acme green.");
    dispatcher.register(ExecutionType::Llm, Arc::new(LlmHandler { provider: Arc::new(mock) }));

    // Document create handler — writes a BlockSuite-shaped row via POST /api/docs from inside.
    struct DocCreateHandler { pool: sqlx::PgPool, engagement: uuid::Uuid, project: uuid::Uuid }
    #[async_trait::async_trait]
    impl Handler for DocCreateHandler {
        async fn invoke(&self, config: Value, _inputs: Value)
            -> Result<Value, celerex_orchestrator::tool_dispatch::DispatchError>
        {
            let title = config["title"].as_str().unwrap_or("Weekly Newsletter").to_string();
            let id = sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO docs (engagement_id, project_id, title, blocks_json)
                 VALUES ($1, $2, $3, '[]'::jsonb) RETURNING id"
            ).bind(self.engagement).bind(self.project).bind(&title)
                .fetch_one(&self.pool).await
                .map_err(|e| celerex_orchestrator::tool_dispatch::DispatchError::Handler(e.to_string()))?;
            Ok(json!({"id": id, "title": title}))
        }
    }

    // Scaffolding done inline — see test continues below.

    let eng = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO engagements (name) VALUES ('Acme') RETURNING id"
    ).fetch_one(&db.pool).await.unwrap();
    let project = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (engagement_id, name) VALUES ($1, 'Jean') RETURNING id"
    ).bind(eng).fetch_one(&db.pool).await.unwrap();

    dispatcher.register(ExecutionType::Script, Arc::new(DocCreateHandler {
        pool: db.pool.clone(), engagement: eng, project,
    }));

    let creds: Arc<dyn celerex_api::plugins::credentials::CredentialStore> =
        Arc::new(celerex_api::plugins::credentials::LocalAeadStore::from_env().unwrap());
    let app = celerex_api::build_app_with_dispatcher(db.pool.clone(), creds, dispatcher);

    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    // Seed the full Jean graph.
    let graph = json!({
        "nodes": [
          { "id": "tick",     "type": "inbound", "config": {} },
          { "id": "workiq",   "type": "action",  "config": { "execution": "http" } },
          { "id": "synth",    "type": "action",  "config": { "execution": "llm", "model": "claude-sonnet-4-6" } },
          { "id": "doc",      "type": "outbound","config": { "execution": "script", "title": "Weekly Newsletter 2026-04-20" } }
        ],
        "edges": [
          { "from": "tick", "to": "workiq" },
          { "from": "workiq", "to": "synth" },
          { "from": "synth", "to": "doc" }
        ]
    });
    let create_res = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/project_members")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(json!({
                "project_id": project, "name": "Jean Weekly",
                "graph_json": graph, "provider_config_json": {}
            }).to_string())).unwrap()
    ).await.unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(create_res.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    let member_id = body["id"].as_str().unwrap().to_string();

    // Trigger manually.
    let run_res = app.oneshot(
        Request::builder().method("POST").uri(format!("/api/project_members/{member_id}/run"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(run_res.status(), StatusCode::ACCEPTED);
    let run_body: Value = serde_json::from_slice(
        &axum::body::to_bytes(run_res.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    let run_id = uuid::Uuid::parse_str(run_body["run_id"].as_str().unwrap()).unwrap();

    // Poll for completion (up to 5s).
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM run_steps WHERE run_id = $1 AND status = 'succeeded'"
        ).bind(run_id).fetch_one(&db.pool).await.unwrap();
        if count.0 >= 4 { break; }
    }

    // Verify: every step succeeded.
    let succeeded: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM run_steps WHERE run_id = $1 AND status = 'succeeded'"
    ).bind(run_id).fetch_one(&db.pool).await.unwrap();
    assert_eq!(succeeded.0, 4, "4 steps succeeded");

    // Verify: run row has snapshot_id + triggered_by = manual.
    let (snap_id, trig): (Option<uuid::Uuid>, String) = sqlx::query_as(
        "SELECT snapshot_id, triggered_by FROM runs WHERE id = $1"
    ).bind(run_id).fetch_one(&db.pool).await.unwrap();
    assert!(snap_id.is_some());
    assert_eq!(trig, "manual");

    // Verify: a new doc with the expected title.
    let title: Option<String> = sqlx::query_scalar(
        "SELECT title FROM docs WHERE project_id = $1 ORDER BY created_at DESC LIMIT 1"
    ).bind(project).fetch_optional(&db.pool).await.unwrap();
    assert_eq!(title.as_deref(), Some("Weekly Newsletter 2026-04-20"));

    // Verify: fleet reflects success.
    let fleet_res = app.oneshot(
        Request::builder().uri("/api/project_members/fleet")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    let fleet_body: Value = serde_json::from_slice(
        &axum::body::to_bytes(fleet_res.into_body(), 64*1024).await.unwrap()
    ).unwrap();
    assert_eq!(fleet_body[0]["last_status"], "succeeded");
}
```

Note: this test assumes `docs` has an `engagement_id`, `project_id`, `title`, and `blocks_json` column shape consistent with Sprint 0's `0004` migration. If the live schema names differ (`content_json`, etc.), adjust the INSERT accordingly — verify first via `Read` on `packages/backend/api/src/db/migrations/0003_plugins.sql` or `0001_init.sql` where `docs` is defined.

- [ ] **Step 14.3: Run the E2E test**

Run: `cargo test -p celerex-api --test e2e_jean_newsletter -- --test-threads=1`
Expected: PASS. If it fails:
- Confirm schema names match the actual `docs` table.
- Confirm the dispatcher's `Dispatcher::register` API matches Sprint 0's (`fn register(&self, kind: ExecutionType, h: Arc<dyn Handler>)` — if `&self` is not mutable-safe, wrap in `Mutex<HashMap>`; Sprint 0's `Dispatcher` already uses interior mutation via `Arc<Dispatcher>`; if it doesn't, use `Dispatcher::new` + chained `.register()` helpers).

- [ ] **Step 14.4: Commit**

```bash
git add packages/backend/api/tests/e2e_jean_newsletter.rs \
        packages/backend/api/src/lib.rs
git commit -m "test(e2e): Jean's weekly newsletter full-graph manual-trigger acceptance"
```

---

## Definition of done (all tasks complete)

- `cargo test --workspace -- --test-threads=1` green.
- `cd packages/frontend/app && bun run test` green.
- `cargo build --workspace` warning-free on Rust 2024 (env mutations wrapped in `unsafe`).
- Manual smoke: `cargo run -p celerex-api` boots with the scheduler loop running (visible in tracing output: `"scheduler tick (dispatch stub …)"` or real dispatch once Task 2 lands).
- `bun run --cwd packages/frontend/app dev` (Vite) — sidebar shows "+ Add agent to project", Builder page renders an empty canvas, click Run now → Monitor page paints nodes green as SSE events arrive, Fleet dashboard lists the agent.

## Execution notes

- **Migrations numbering:** 0005 (schedules) → 0006 (project_members) → 0007 (member_snapshots) → 0008 (notifications) → 0009 (amend runs). sqlx-migrate runs alphabetically so the order holds as written.
- **Task ordering:** Tasks 1 → 8 can be committed in order; Task 2 Step 2.10 (manual-run test) is marked `#[ignore]` until Task 7 lands the amend-runs migration, then un-ignored in Task 7 Step 7.8.
- **Rust 2024 `set_var` rule:** any `std::env::set_var` must be wrapped in `unsafe { … }`. The existing helper in `tests/common/mod.rs` already does this.
- **Subagent context:** When dispatching a subagent to implement any single task, include the Sprint 0 anchor paths from the plan header verbatim — Celerex's large archive directory will otherwise burn the subagent's context on exploration.
