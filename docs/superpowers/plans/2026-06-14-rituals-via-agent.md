# Rituals via Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-home OmniVyx Design's scheduled work ("Rituals") onto the always-on Agent, so Rituals fire even when the desktop is closed; unify the routines/Orbit vocabulary under "Ritual".

**Architecture:** A Ritual *is* a scheduled `project_member` on the Agent. The Agent already has the cron scheduler (`scheduler/`) that fires a `project_member` via `run_job::enqueue_run`, and a flow engine (`project_members.graph_json`). We add a Design-shaped device-seam surface (`/api/design/rituals`) that translates a `RitualSpec` into a `project_member` + scheduler entry, a `@celerex/connect` client for it, and rewire the Design UI to it. The local daemon scheduler is retired last, after the Agent path is proven.

**Tech Stack:** Rust/Axum + sqlx/Postgres (Agent); TypeScript/bun (`@celerex/connect` seam); TypeScript/Node (daemon) + React/Vite (web).

**Spec:** `docs/superpowers/specs/2026-06-14-rituals-via-agent-design.md`

**Branch:** `docs/rituals-via-agent` (this plan + spec). Each implementation stage lands on its own branch off `main` and is PR'd separately (GitHub Actions stays disabled).

---

## File Structure

**Stage 1 — Agent Ritual API (Celerex repo):**
- Create: `packages/backend/api/src/design/rituals.rs` — RitualSpec types, the 6 device-seam handlers, RitualSpec→graph translation.
- Modify: `packages/backend/api/src/design/mod.rs` — add `pub mod rituals;` and re-export handlers; expose the `design_deliveries_project` helper (or add a sibling `rituals_project`).
- Modify: `packages/backend/api/src/scheduler/routes.rs` — make `compute_next_fire_at` `pub` so the rituals module reuses it.
- Modify: `packages/backend/api/src/lib.rs` — mount the 6 routes.
- Test: `packages/backend/api/tests/design_rituals.rs` — Postgres-backed integration tests.

**Stage 2 — Seam (celerex-design repo):**
- Create: `packages/celerex-connect/src/rituals.ts` — types + 6 client functions.
- Modify: `packages/celerex-connect/src/index.ts` — export the new module.
- Test: `packages/celerex-connect/test/rituals.test.ts` — HTTP-stub tests (bun).

**Stage 3 — Design rename + rewire (celerex-design repo):**
- Modify (daemon): `apps/daemon/src/server.ts` (+ a new `apps/daemon/src/routes/ritual.ts`) — expose `/api/rituals*` proxying to the Agent via the seam, using the stored pairing (base URL + device token).
- Modify (web): `apps/web/src/components/TasksView.tsx` → Rituals surface; `apps/web/src/types.ts` (`OrbitConfig`→ daily-briefing Ritual template type); `apps/web/src/state/config.ts` (`DEFAULT_ORBIT`); `apps/web/src/components/SettingsDialog.tsx`; i18n strings.
- Test: daemon vitest for the ritual routes; web vitest for the rename + seam wiring.

**Stage 4 — Retire local scheduler (celerex-design repo):**
- Modify/Delete: `apps/daemon/src/routines.ts`, `apps/daemon/src/orbit.ts`, `apps/daemon/src/routes/routine.ts` — remove local scheduling once Agent Rituals proven; migrate scheduled live-artifact refreshers to Rituals.

---

# STAGE 1 — Agent Ritual API

> Branch: `feat/rituals-agent-api` off `main`. PR when stage complete.
> Run tests from `packages/backend/api`. The repo's integration tests use a real Postgres (same harness as `tests/` for scheduler/design). Use `cargo test --test design_rituals`.

## Task 1: Expose `compute_next_fire_at`

**Files:**
- Modify: `packages/backend/api/src/scheduler/routes.rs:46`

- [ ] **Step 1: Make the helper public**

Change the signature in `packages/backend/api/src/scheduler/routes.rs`:

```rust
pub fn compute_next_fire_at(cron_expr: &str, timezone: &str) -> Result<DateTime<Utc>, String> {
```

(only `fn` → `pub fn`; body unchanged.)

- [ ] **Step 2: Verify it still compiles**

Run: `cargo check -p celerex-api`
Expected: compiles (no other change).

- [ ] **Step 3: Commit**

```bash
git add packages/backend/api/src/scheduler/routes.rs
git commit -m "refactor(scheduler): expose compute_next_fire_at for reuse"
```

## Task 2: RitualSpec types + graph translation

**Files:**
- Create: `packages/backend/api/src/design/rituals.rs`
- Modify: `packages/backend/api/src/design/mod.rs:1` (add `pub mod rituals;`)

- [ ] **Step 1: Write the failing test for graph translation**

Create `packages/backend/api/tests/design_rituals.rs`:

```rust
// Unit-level coverage for RitualSpec → graph translation. Full HTTP/Postgres
// tests live further down this file (added in later tasks).
use celerex_api::design::rituals::{build_ritual_graph, RitualBody};

#[test]
fn agent_run_body_builds_a_valid_minimal_graph() {
    let body = RitualBody::AgentRun {
        prompt: "Summarize my unread mail".into(),
        skill_template_id: Some("orbit-general".into()),
        target: None,
    };
    let graph = build_ritual_graph(&body);
    // graph must pass the engine's own validator.
    celerex_api::project_members::graph::validate(&graph).expect("ritual graph must validate");
    // carries the prompt so the run is self-contained.
    assert!(graph.to_string().contains("Summarize my unread mail"));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test design_rituals agent_run_body_builds_a_valid_minimal_graph`
Expected: FAIL — `design::rituals` does not exist.

- [ ] **Step 3: Create the module with types + translation**

Create `packages/backend/api/src/design/rituals.rs`:

```rust
//! Rituals: scheduled agent work, hosted on the Agent. A Ritual is a
//! `project_member` (the body) plus a scheduler entry that fires it on cron.
//! The device seam speaks Design's vocabulary (`RitualSpec`); this module
//! translates it into the engine's `project_member` graph. See
//! docs/superpowers/specs/2026-06-14-rituals-via-agent-design.md.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::extractors::DeviceAuth;
use crate::project_members::run_job::{self, TRIGGERED_BY_SCHEDULED};
use crate::scheduler::routes::compute_next_fire_at;
use crate::scheduler::store::{PgSchedulesStore, Schedule, SchedulesStore};
use crate::AppState;

/// The schedule half of a Ritual.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RitualSchedule {
    pub cron_expr: String,
    pub timezone: String,
}

/// The work half of a Ritual — Design's vocabulary, not raw graph JSON.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RitualBody {
    /// A scheduled agent conversation (today's routine / Orbit shape).
    AgentRun {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skill_template_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// An existing flow becomes the Ritual body directly.
    FlowRef { project_member_id: Uuid },
}

/// The full create/update payload from the daemon.
#[derive(Debug, Clone, Deserialize)]
pub struct RitualSpec {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub schedule: RitualSchedule,
    pub body: RitualBody,
}

fn default_enabled() -> bool {
    true
}

/// Build a minimal, self-contained `project_member` graph for an `agent_run`
/// Ritual: a trigger feeding a single agent action that carries the prompt.
pub fn build_ritual_graph(body: &RitualBody) -> Value {
    match body {
        RitualBody::AgentRun {
            prompt,
            skill_template_id,
            target,
        } => json!({
            "nodes": [
                { "id": "trigger", "type": "inbound", "config": { "execution": "input" } },
                {
                    "id": "agent",
                    "type": "action",
                    "config": {
                        "execution": "agent",
                        "prompt": prompt,
                        "skill_template_id": skill_template_id,
                        "target": target,
                        "inputs": "{{steps.trigger.outputs}}"
                    }
                }
            ],
            "edges": [ { "from": "trigger", "to": "agent" } ]
        }),
        // FlowRef bodies reuse an existing graph; this is never called for them.
        RitualBody::FlowRef { .. } => json!({ "nodes": [], "edges": [] }),
    }
}
```

Add to `packages/backend/api/src/design/mod.rs` (top, with the other `pub mod`/`mod` lines — put it after the module's doc comment):

```rust
pub mod rituals;
```

> If `crate::project_members::graph` is not already `pub`, confirm it is — the existing `routes.rs` calls `graph::validate`/`graph::compile`, and `project_members/mod.rs` should expose `pub mod graph;`. If the test can't see it, add `pub mod graph;` to `packages/backend/api/src/project_members/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --test design_rituals agent_run_body_builds_a_valid_minimal_graph`
Expected: PASS.

> If `graph::validate` rejects the node shape, align the node `type`/`config.execution` strings with what `scaffold::scaffold_live_flow` emits (`inbound`/`action`, `execution: "input"`) — those are known-valid. Adjust `build_ritual_graph` to match and re-run.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/rituals.rs packages/backend/api/src/design/mod.rs packages/backend/api/tests/design_rituals.rs
git commit -m "feat(rituals): RitualSpec types + agent_run graph translation"
```

## Task 3: Resolve the "Rituals" project + create handler

**Files:**
- Modify: `packages/backend/api/src/design/rituals.rs`
- Modify: `packages/backend/api/src/design/mod.rs` (make `design_deliveries_project` reusable, or add `rituals_project`)
- Test: `packages/backend/api/tests/design_rituals.rs`

- [ ] **Step 1: Add a Rituals project helper**

In `packages/backend/api/src/design/mod.rs`, add a sibling to `design_deliveries_project` (copy its body, change the name + make it `pub(crate)`):

```rust
/// Resolve the engagement's "Rituals" project, creating it on first use.
/// Ritual bodies (scheduled project_members) are grouped here.
pub(crate) async fn rituals_project(
    state: &AppState,
    engagement_id: Uuid,
) -> Result<Uuid, (StatusCode, String)> {
    const NAME: &str = "Rituals";
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects WHERE engagement_id = $1 AND name = $2 ORDER BY created_at LIMIT 1",
    )
    .bind(engagement_id)
    .bind(NAME)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Ok(id);
    }
    crate::db::create_project(&state.pool, engagement_id, NAME)
        .await
        .map(|p| p.id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
```

- [ ] **Step 2: Write the failing HTTP test for create**

Append to `packages/backend/api/tests/design_rituals.rs` (use the same test bootstrap the existing design/scheduler integration tests use — copy the harness helper that builds the app + a device-scoped token; mirror `tests/` files that already hit `/api/design/*`):

```rust
// NOTE: `spawn_app()` and `device_token(engagement_id)` are the existing test
// helpers used by other /api/design integration tests — reuse them. If they live
// in a shared module, import it the same way the sibling design tests do.
#[tokio::test]
async fn create_ritual_makes_a_member_and_a_schedule() {
    let app = spawn_app().await;
    let token = app.device_token().await; // design.device-scoped

    let res = app
        .client
        .post(format!("{}/api/design/rituals", app.addr))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "Morning briefing",
            "enabled": true,
            "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Summarize unread mail" }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["ritual_id"].is_string());
    assert!(body["project_member_id"].is_string());
    assert_eq!(body["enabled"], true);
    assert!(body["next_fire_at"].is_string());
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test --test design_rituals create_ritual_makes_a_member_and_a_schedule`
Expected: FAIL — route not mounted (404) / handler missing.

- [ ] **Step 4: Implement the create handler**

Append to `packages/backend/api/src/design/rituals.rs`:

```rust
/// What we return for a Ritual: the scheduler entry id + the member it fires.
#[derive(Debug, Serialize)]
pub struct RitualView {
    pub ritual_id: Uuid,          // = schedule id
    pub project_member_id: Uuid,
    pub name: String,
    pub cron_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub next_fire_at: chrono::DateTime<chrono::Utc>,
    pub last_fired_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn view_from(schedule: &Schedule, name: String) -> RitualView {
    RitualView {
        ritual_id: schedule.id,
        project_member_id: schedule.project_member_id,
        name,
        cron_expr: schedule.cron_expr.clone(),
        timezone: schedule.timezone.clone(),
        enabled: schedule.enabled,
        next_fire_at: schedule.next_fire_at,
        last_fired_at: schedule.last_fired_at,
    }
}

/// `POST /api/design/rituals` — create a Ritual (member body + scheduler entry).
pub async fn create_ritual(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(spec): Json<RitualSpec>,
) -> impl IntoResponse {
    // Validate the schedule up front so we don't create an orphan member.
    let next_fire_at = match compute_next_fire_at(&spec.schedule.cron_expr, &spec.schedule.timezone) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };

    // Resolve the member that will be fired.
    let member_id = match &spec.body {
        RitualBody::FlowRef { project_member_id } => {
            // Must belong to the caller's engagement.
            match state.members.engagement_id_for(*project_member_id).await {
                Ok(eng) if eng == claims.engagement_id => *project_member_id,
                Ok(_) => {
                    return (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" }))).into_response()
                }
                Err(_) => {
                    return (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" }))).into_response()
                }
            }
        }
        RitualBody::AgentRun { .. } => {
            let project_id = match super::rituals_project(&state, claims.engagement_id).await {
                Ok(v) => v,
                Err((s, e)) => return (s, Json(json!({ "error": e }))).into_response(),
            };
            let graph = build_ritual_graph(&spec.body);
            let provider_config = Value::Object(Default::default());
            match state
                .members
                .create(project_id, &spec.name, "Ritual (scheduled agent run).", &graph, &provider_config, "writer")
                .await
            {
                Ok(m) => m.id,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response()
                }
            }
        }
    };

    // Create the scheduler entry; roll back a freshly-created member on failure.
    let schedule = Schedule {
        id: Uuid::new_v4(),
        project_member_id: member_id,
        cron_expr: spec.schedule.cron_expr.clone(),
        timezone: spec.schedule.timezone.clone(),
        next_fire_at,
        last_fired_at: None,
        enabled: spec.enabled,
    };
    let store = PgSchedulesStore::new(state.pool.clone());
    if let Err(e) = store.create(schedule.clone()).await {
        if matches!(spec.body, RitualBody::AgentRun { .. }) {
            let _ = sqlx::query("DELETE FROM project_members WHERE id = $1")
                .bind(member_id)
                .execute(&state.pool)
                .await;
        }
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
    }

    (StatusCode::CREATED, Json(view_from(&schedule, spec.name))).into_response()
}
```

- [ ] **Step 5: Mount the route**

In `packages/backend/api/src/lib.rs`, after the connectors routes (around line 121), add:

```rust
        .route("/api/design/rituals", post(design::rituals::create_ritual))
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --test design_rituals create_ritual_makes_a_member_and_a_schedule`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/backend/api/src/design/rituals.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_rituals.rs
git commit -m "feat(rituals): POST /api/design/rituals create handler"
```

## Task 4: List + delete handlers

**Files:**
- Modify: `packages/backend/api/src/design/rituals.rs`, `packages/backend/api/src/lib.rs`
- Test: `packages/backend/api/tests/design_rituals.rs`

- [ ] **Step 1: Write the failing test**

Append to `packages/backend/api/tests/design_rituals.rs`:

```rust
#[tokio::test]
async fn list_then_delete_ritual() {
    let app = spawn_app().await;
    let token = app.device_token().await;

    let created: serde_json::Value = app.client
        .post(format!("{}/api/design/rituals", app.addr))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "Weekly digest",
            "schedule": { "cron_expr": "0 0 9 * * MON", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "Weekly digest" }
        }))
        .send().await.unwrap().json().await.unwrap();
    let ritual_id = created["ritual_id"].as_str().unwrap();

    let list: serde_json::Value = app.client
        .get(format!("{}/api/design/rituals", app.addr))
        .bearer_auth(&token).send().await.unwrap().json().await.unwrap();
    let rituals = list["rituals"].as_array().unwrap();
    assert!(rituals.iter().any(|r| r["ritual_id"] == ritual_id));

    let del = app.client
        .delete(format!("{}/api/design/rituals/{}", app.addr, ritual_id))
        .bearer_auth(&token).send().await.unwrap();
    assert_eq!(del.status(), 204);

    let list2: serde_json::Value = app.client
        .get(format!("{}/api/design/rituals", app.addr))
        .bearer_auth(&token).send().await.unwrap().json().await.unwrap();
    assert!(list2["rituals"].as_array().unwrap().iter().all(|r| r["ritual_id"] != ritual_id));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test design_rituals list_then_delete_ritual`
Expected: FAIL — list/delete routes missing.

- [ ] **Step 3: Implement list + delete**

Append to `packages/backend/api/src/design/rituals.rs`:

```rust
/// `GET /api/design/rituals` — all Rituals for the caller's engagement.
/// Joins each schedule to its member name; filters by engagement.
pub async fn list_rituals(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // schedules whose member belongs to this engagement.
    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, bool, String)>(
        "SELECT s.id, s.project_member_id, s.cron_expr, s.timezone, s.next_fire_at, s.last_fired_at, s.enabled, m.name
         FROM schedules s
         JOIN project_members m ON m.id = s.project_member_id
         JOIN projects p ON p.id = m.project_id
         WHERE p.engagement_id = $1
         ORDER BY s.next_fire_at",
    )
    .bind(claims.engagement_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let rituals: Vec<RitualView> = rows
                .into_iter()
                .map(|r| RitualView {
                    ritual_id: r.0,
                    project_member_id: r.1,
                    cron_expr: r.2,
                    timezone: r.3,
                    next_fire_at: r.4,
                    last_fired_at: r.5,
                    enabled: r.6,
                    name: r.7,
                })
                .collect();
            (StatusCode::OK, Json(json!({ "rituals": rituals }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

/// Guard: load a schedule and confirm its member is in the caller's engagement.
async fn owned_schedule(
    state: &AppState,
    engagement_id: Uuid,
    ritual_id: Uuid,
) -> Result<Schedule, (StatusCode, Json<Value>)> {
    let store = PgSchedulesStore::new(state.pool.clone());
    let schedule = store
        .get(ritual_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))))?
        .ok_or((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))))?;
    match state.members.engagement_id_for(schedule.project_member_id).await {
        Ok(eng) if eng == engagement_id => Ok(schedule),
        Ok(_) => Err((StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" })))),
        Err(_) => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))),
    }
}

/// `DELETE /api/design/rituals/{id}` — remove the schedule + its member.
/// (Deleting the member cascades the schedule, but we delete the schedule first
/// so a FlowRef member we did NOT create is never orphaned — for FlowRef we keep
/// the member and only drop the schedule.)
pub async fn delete_ritual(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(ritual_id): Path<Uuid>,
) -> impl IntoResponse {
    let schedule = match owned_schedule(&state, claims.engagement_id, ritual_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    let store = PgSchedulesStore::new(state.pool.clone());
    if let Err(e) = store.delete(schedule.id).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
    }
    // Drop the member only if it lives in the "Rituals" project (i.e. we made it
    // for an agent_run; FlowRef members belong to the user's own project).
    let _ = sqlx::query(
        "DELETE FROM project_members m USING projects p
         WHERE m.id = $1 AND m.project_id = p.id
           AND p.engagement_id = $2 AND p.name = 'Rituals'",
    )
    .bind(schedule.project_member_id)
    .bind(claims.engagement_id)
    .execute(&state.pool)
    .await;
    StatusCode::NO_CONTENT.into_response()
}
```

Mount in `packages/backend/api/src/lib.rs`:

```rust
        .route("/api/design/rituals", post(design::rituals::create_ritual).get(design::rituals::list_rituals))
        .route("/api/design/rituals/{id}", axum::routing::delete(design::rituals::delete_ritual))
```

(Replace the single `create_ritual` route line from Task 3 with the combined `.get(...)` line.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --test design_rituals list_then_delete_ritual`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/rituals.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_rituals.rs
git commit -m "feat(rituals): list + delete handlers (engagement-scoped)"
```

## Task 5: Update + run-now + runs handlers

**Files:**
- Modify: `packages/backend/api/src/design/rituals.rs`, `packages/backend/api/src/lib.rs`
- Test: `packages/backend/api/tests/design_rituals.rs`

- [ ] **Step 1: Write the failing tests**

Append to `packages/backend/api/tests/design_rituals.rs`:

```rust
#[tokio::test]
async fn update_changes_schedule_and_enabled() {
    let app = spawn_app().await;
    let token = app.device_token().await;
    let created: serde_json::Value = app.client
        .post(format!("{}/api/design/rituals", app.addr)).bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "X", "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "x" }
        })).send().await.unwrap().json().await.unwrap();
    let id = created["ritual_id"].as_str().unwrap();

    let res = app.client
        .put(format!("{}/api/design/rituals/{}", app.addr, id)).bearer_auth(&token)
        .json(&serde_json::json!({ "cron_expr": "0 0 18 * * *", "timezone": "UTC", "enabled": false }))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["enabled"], false);
    assert_eq!(body["cron_expr"], "0 0 18 * * *");
}

#[tokio::test]
async fn run_now_enqueues_a_run_visible_in_runs() {
    let app = spawn_app().await;
    let token = app.device_token().await;
    let created: serde_json::Value = app.client
        .post(format!("{}/api/design/rituals", app.addr)).bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "Y", "schedule": { "cron_expr": "0 0 7 * * *", "timezone": "UTC" },
            "body": { "kind": "agent_run", "prompt": "y" }
        })).send().await.unwrap().json().await.unwrap();
    let id = created["ritual_id"].as_str().unwrap();

    let run = app.client
        .post(format!("{}/api/design/rituals/{}/run", app.addr, id)).bearer_auth(&token)
        .send().await.unwrap();
    assert_eq!(run.status(), 202);
    let run_body: serde_json::Value = run.json().await.unwrap();
    assert!(run_body["run_id"].is_string());

    let runs: serde_json::Value = app.client
        .get(format!("{}/api/design/rituals/{}/runs", app.addr, id)).bearer_auth(&token)
        .send().await.unwrap().json().await.unwrap();
    assert!(!runs["runs"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test design_rituals update_changes_schedule_and_enabled run_now_enqueues_a_run_visible_in_runs`
Expected: FAIL — routes missing.

- [ ] **Step 3: Implement update, run-now, runs**

Append to `packages/backend/api/src/design/rituals.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateRitualBody {
    pub cron_expr: String,
    pub timezone: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// `PUT /api/design/rituals/{id}` — update the schedule (cron/tz/enabled).
pub async fn update_ritual(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(ritual_id): Path<Uuid>,
    Json(b): Json<UpdateRitualBody>,
) -> impl IntoResponse {
    let schedule = match owned_schedule(&state, claims.engagement_id, ritual_id).await {
        Ok(v) => v,
        Err((s, body)) => return (s, body).into_response(),
    };
    let next_fire_at = match compute_next_fire_at(&b.cron_expr, &b.timezone) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };
    let store = PgSchedulesStore::new(state.pool.clone());
    if let Err(e) = store.update(ritual_id, &b.cron_expr, &b.timezone, b.enabled, next_fire_at).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
    }
    let updated = Schedule {
        cron_expr: b.cron_expr,
        timezone: b.timezone,
        enabled: b.enabled,
        next_fire_at,
        ..schedule
    };
    // member name unchanged; fetch it for the view.
    let name = state.members.get(updated.project_member_id).await.ok().flatten().map(|m| m.name).unwrap_or_default();
    (StatusCode::OK, Json(view_from(&updated, name))).into_response()
}

/// `POST /api/design/rituals/{id}/run` — fire once, off-schedule.
pub async fn run_ritual_now(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(ritual_id): Path<Uuid>,
) -> impl IntoResponse {
    let schedule = match owned_schedule(&state, claims.engagement_id, ritual_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    match run_job::enqueue_run(
        state.pool.clone(),
        state.members.clone(),
        state.executor.clone(),
        state.notifiers.clone(),
        schedule.project_member_id,
        claims.engagement_id,
        TRIGGERED_BY_SCHEDULED,
    )
    .await
    {
        Ok(run_id) => (StatusCode::ACCEPTED, Json(json!({ "run_id": run_id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    }
}

/// `GET /api/design/rituals/{id}/runs` — recent runs for this Ritual's member.
pub async fn list_ritual_runs(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(ritual_id): Path<Uuid>,
) -> impl IntoResponse {
    let schedule = match owned_schedule(&state, claims.engagement_id, ritual_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    let rows = sqlx::query_as::<_, (Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, status, triggered_by, created_at FROM runs
         WHERE project_member_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(schedule.project_member_id)
    .fetch_all(&state.pool)
    .await;
    match rows {
        Ok(rows) => {
            let runs: Vec<Value> = rows
                .into_iter()
                .map(|r| json!({ "run_id": r.0, "status": r.1, "triggered_by": r.2, "created_at": r.3 }))
                .collect();
            (StatusCode::OK, Json(json!({ "runs": runs }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}
```

Mount in `packages/backend/api/src/lib.rs`:

```rust
        .route("/api/design/rituals/{id}", axum::routing::delete(design::rituals::delete_ritual).put(design::rituals::update_ritual))
        .route("/api/design/rituals/{id}/run", post(design::rituals::run_ritual_now))
        .route("/api/design/rituals/{id}/runs", get(design::rituals::list_ritual_runs))
```

(Merge the `.put(...)` onto the existing `/api/design/rituals/{id}` line from Task 4.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test design_rituals`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/design/rituals.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_rituals.rs
git commit -m "feat(rituals): update, run-now, and runs handlers"
```

## Task 6: Stage 1 full-suite check + PR

- [ ] **Step 1: Run the whole API test suite**

Run: `cargo test -p celerex-api`
Expected: green (no regressions in scheduler/design/members).

- [ ] **Step 2: Open the PR**

```bash
git push -u origin feat/rituals-agent-api
gh pr create --fill --base main --title "feat(rituals): Agent Ritual API (2b stage 1)"
```

---

# STAGE 2 — Seam (`@celerex/connect`)

> Repo: celerex-design. Branch: `feat/rituals-seam` off `main`. Run from `packages/celerex-connect`. Tests: bun (mirror `test/` files). **After landing, rebuild the dist (`pnpm --filter @celerex/connect build`) — the daemon imports the built output (gitignored).**

## Task 7: Ritual seam client

**Files:**
- Create: `packages/celerex-connect/src/rituals.ts`
- Modify: `packages/celerex-connect/src/index.ts`
- Test: `packages/celerex-connect/test/rituals.test.ts`

- [ ] **Step 1: Write the failing test**

Create `packages/celerex-connect/test/rituals.test.ts` (mirror `test/` connector test style — a Node http stub):

```ts
import http from "node:http";
import { afterAll, beforeAll, expect, test } from "bun:test";
import { createRitual, listRituals, deleteRitual } from "../src/rituals.js";

let url: string;
let server: http.Server;
const seen: { method: string; path: string; auth?: string; body?: unknown }[] = [];

beforeAll(async () => {
  server = http.createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      seen.push({
        method: req.method!,
        path: req.url!,
        auth: req.headers.authorization,
        body: raw ? JSON.parse(raw) : undefined,
      });
      res.setHeader("content-type", "application/json");
      if (req.method === "POST" && req.url === "/api/design/rituals") {
        res.statusCode = 201;
        res.end(JSON.stringify({ ritual_id: "r1", project_member_id: "m1", name: "X", cron_expr: "0 0 7 * * *", timezone: "UTC", enabled: true, next_fire_at: "2026-06-15T07:00:00Z", last_fired_at: null }));
      } else if (req.method === "GET" && req.url === "/api/design/rituals") {
        res.statusCode = 200;
        res.end(JSON.stringify({ rituals: [{ ritual_id: "r1", project_member_id: "m1", name: "X", cron_expr: "0 0 7 * * *", timezone: "UTC", enabled: true, next_fire_at: "2026-06-15T07:00:00Z", last_fired_at: null }] }));
      } else if (req.method === "DELETE" && req.url === "/api/design/rituals/r1") {
        res.statusCode = 204;
        res.end();
      } else {
        res.statusCode = 404;
        res.end("{}");
      }
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const a = server.address() as import("node:net").AddressInfo;
  url = `http://127.0.0.1:${a.port}`;
});
afterAll(() => new Promise<void>((r) => server.close(() => r())));

test("createRitual posts the spec and returns the view", async () => {
  const v = await createRitual(url, "tok", {
    name: "X",
    enabled: true,
    schedule: { cron_expr: "0 0 7 * * *", timezone: "UTC" },
    body: { kind: "agent_run", prompt: "summarize" },
  });
  expect(v.ritual_id).toBe("r1");
  const call = seen.find((s) => s.method === "POST")!;
  expect(call.auth).toBe("Bearer tok");
  expect((call.body as any).body.kind).toBe("agent_run");
});

test("listRituals returns the array", async () => {
  const list = await listRituals(url, "tok");
  expect(list).toHaveLength(1);
  expect(list[0]!.ritual_id).toBe("r1");
});

test("deleteRitual issues a DELETE", async () => {
  await deleteRitual(url, "tok", "r1");
  expect(seen.some((s) => s.method === "DELETE" && s.path === "/api/design/rituals/r1")).toBe(true);
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `bun test test/rituals.test.ts`
Expected: FAIL — `../src/rituals.js` not found.

- [ ] **Step 3: Implement the client**

Create `packages/celerex-connect/src/rituals.ts`:

```ts
// Ritual fetchers — thin wrappers over the Agent's /api/design/rituals surface.
// A Ritual is scheduled agent work hosted on the Agent. Mirrors connectors.ts.
import type { FetchFn } from "./pairing.js";

function api(baseUrl: string, path: string): string {
  return `${baseUrl.replace(/\/+$/, "")}${path}`;
}
async function asError(res: Response, action: string): Promise<Error> {
  return new Error(`celerex ${action} failed (${res.status}): ${await res.text().catch(() => "")}`.trim());
}

export interface RitualSchedule {
  cron_expr: string;
  timezone: string;
}

export type RitualBody =
  | { kind: "agent_run"; prompt: string; skill_template_id?: string; target?: string }
  | { kind: "flow_ref"; project_member_id: string };

export interface RitualSpec {
  name: string;
  enabled?: boolean;
  schedule: RitualSchedule;
  body: RitualBody;
}

export interface RitualView {
  ritual_id: string;
  project_member_id: string;
  name: string;
  cron_expr: string;
  timezone: string;
  enabled: boolean;
  next_fire_at: string;
  last_fired_at: string | null;
}

export interface RitualRun {
  run_id: string;
  status: string;
  triggered_by: string;
  created_at: string;
}

function auth(token: string): Record<string, string> {
  return { authorization: `Bearer ${token}`, "content-type": "application/json" };
}

export async function listRituals(baseUrl: string, token: string, fetchFn: FetchFn = fetch): Promise<RitualView[]> {
  const res = await fetchFn(api(baseUrl, "/api/design/rituals"), { headers: auth(token) });
  if (!res.ok) throw await asError(res, "list rituals");
  return ((await res.json()) as { rituals: RitualView[] }).rituals;
}

export async function createRitual(baseUrl: string, token: string, spec: RitualSpec, fetchFn: FetchFn = fetch): Promise<RitualView> {
  const res = await fetchFn(api(baseUrl, "/api/design/rituals"), { method: "POST", headers: auth(token), body: JSON.stringify(spec) });
  if (!res.ok) throw await asError(res, "create ritual");
  return (await res.json()) as RitualView;
}

export async function updateRitual(
  baseUrl: string, token: string, ritualId: string,
  patch: { cron_expr: string; timezone: string; enabled: boolean },
  fetchFn: FetchFn = fetch,
): Promise<RitualView> {
  const res = await fetchFn(api(baseUrl, `/api/design/rituals/${ritualId}`), { method: "PUT", headers: auth(token), body: JSON.stringify(patch) });
  if (!res.ok) throw await asError(res, "update ritual");
  return (await res.json()) as RitualView;
}

export async function deleteRitual(baseUrl: string, token: string, ritualId: string, fetchFn: FetchFn = fetch): Promise<void> {
  const res = await fetchFn(api(baseUrl, `/api/design/rituals/${ritualId}`), { method: "DELETE", headers: auth(token) });
  if (!res.ok && res.status !== 204) throw await asError(res, "delete ritual");
}

export async function runRitualNow(baseUrl: string, token: string, ritualId: string, fetchFn: FetchFn = fetch): Promise<{ run_id: string }> {
  const res = await fetchFn(api(baseUrl, `/api/design/rituals/${ritualId}/run`), { method: "POST", headers: auth(token) });
  if (!res.ok) throw await asError(res, "run ritual");
  return (await res.json()) as { run_id: string };
}

export async function getRitualRuns(baseUrl: string, token: string, ritualId: string, fetchFn: FetchFn = fetch): Promise<RitualRun[]> {
  const res = await fetchFn(api(baseUrl, `/api/design/rituals/${ritualId}/runs`), { headers: auth(token) });
  if (!res.ok) throw await asError(res, "ritual runs");
  return ((await res.json()) as { runs: RitualRun[] }).runs;
}
```

Add to `packages/celerex-connect/src/index.ts` (alongside the connectors export):

```ts
export * from "./rituals.js";
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `bun test test/rituals.test.ts`
Expected: PASS.

- [ ] **Step 5: Build the dist (daemon depends on it) + commit**

```bash
cd ../.. && pnpm --filter @celerex/connect build && cd packages/celerex-connect
git add packages/celerex-connect/src/rituals.ts packages/celerex-connect/src/index.ts packages/celerex-connect/test/rituals.test.ts
git commit -m "feat(rituals): @celerex/connect ritual client"
```

- [ ] **Step 6: Open the PR**

```bash
git push -u origin feat/rituals-seam
gh pr create --fill --base main --title "feat(rituals): seam client (2b stage 2)"
```

---

# STAGE 3 — Design rename + rewire

> Repo: celerex-design. Branch: `feat/rituals-design` off `main` (after stages 1–2 merge + dist rebuilt). `exactOptionalPropertyTypes: true` is on in the daemon — never pass explicit `undefined` to an optional prop.

## Task 8: Daemon ritual routes (proxy to Agent)

**Files:**
- Create: `apps/daemon/src/routes/ritual.ts`
- Modify: `apps/daemon/src/server.ts` (register routes; reuse the pairing base-URL + device-token accessor used by the connector/provider paths)
- Test: `apps/daemon/tests/ritual-routes.test.ts`

- [ ] **Step 1: Recon the existing pattern (read, don't write)**

Read how the daemon already exposes an Agent-backed resource to the web app and where the pairing base URL + device token are read:
- `apps/daemon/src/server.ts` — find the route table and the handler that calls `@celerex/connect` (e.g. the connect/provider path) to copy its base-URL/token access + error shape.
- `apps/daemon/src/routes/routine.ts` — the existing local routine routes; the new ritual routes replace these as the scheduled-work surface.

Note the exact helper names (base URL getter, token getter, JSON response helper) — the steps below assume `getPairing()` returns `{ baseUrl, deviceToken }`; rename to match what you find.

- [ ] **Step 2: Write the failing test**

Create `apps/daemon/tests/ritual-routes.test.ts` (mirror `tests/celerex-connect-connectors.test.ts`: start the daemon, stub the Agent, assert the daemon proxies):

```ts
import http from "node:http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { startServer } from "../src/server.js";

let baseUrl: string;
let daemon: http.Server;
let agent: http.Server;
let agentUrl: string;
const calls: string[] = [];

beforeAll(async () => {
  agent = http.createServer((req, res) => {
    calls.push(`${req.method} ${req.url}`);
    res.setHeader("content-type", "application/json");
    if (req.method === "GET" && req.url === "/api/design/rituals") {
      res.end(JSON.stringify({ rituals: [{ ritual_id: "r1", project_member_id: "m1", name: "X", cron_expr: "0 0 7 * * *", timezone: "UTC", enabled: true, next_fire_at: "2026-06-15T07:00:00Z", last_fired_at: null }] }));
    } else { res.statusCode = 404; res.end("{}"); }
  });
  await new Promise<void>((r) => agent.listen(0, "127.0.0.1", r));
  const a = agent.address() as import("node:net").AddressInfo;
  agentUrl = `http://127.0.0.1:${a.port}`;
  // Pair the daemon to the stub agent (use the same connect path the connector
  // test uses; set base URL + a device token in app-config).
  const started = (await startServer({ port: 0, returnServer: true })) as { url: string; server: http.Server };
  baseUrl = started.url;
  daemon = started.server;
  // … reuse the connector test's connect() helper to pair to agentUrl …
});
afterAll(async () => {
  await new Promise<void>((r) => daemon.close(() => r()));
  await new Promise<void>((r) => agent.close(() => r()));
});

describe("GET /api/rituals", () => {
  it("proxies the Agent's ritual list", async () => {
    const res = await fetch(`${baseUrl}/api/rituals`);
    expect(res.status).toBe(200);
    const json = (await res.json()) as { rituals: unknown[] };
    expect(json.rituals).toHaveLength(1);
    expect(calls).toContain("GET /api/design/rituals");
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

Run: `pnpm --filter @celerex/daemon test ritual-routes`
Expected: FAIL — `/api/rituals` 404.

- [ ] **Step 4: Implement the daemon ritual routes**

Create `apps/daemon/src/routes/ritual.ts` — a handler set that reads the pairing and calls the seam. Skeleton (adapt names to Step 1 findings):

```ts
import { listRituals, createRitual, updateRitual, deleteRitual, runRitualNow, getRitualRuns, type RitualSpec } from "@celerex/connect";
import { getPairing } from "../pairing-state.js"; // ← rename to the real accessor

// Each returns a plain object/throws; server.ts maps to HTTP.
export async function handleListRituals() {
  const { baseUrl, deviceToken } = getPairing();
  return { rituals: await listRituals(baseUrl, deviceToken) };
}
export async function handleCreateRitual(spec: RitualSpec) {
  const { baseUrl, deviceToken } = getPairing();
  return createRitual(baseUrl, deviceToken, spec);
}
export async function handleUpdateRitual(id: string, patch: { cron_expr: string; timezone: string; enabled: boolean }) {
  const { baseUrl, deviceToken } = getPairing();
  return updateRitual(baseUrl, deviceToken, id, patch);
}
export async function handleDeleteRitual(id: string) {
  const { baseUrl, deviceToken } = getPairing();
  await deleteRitual(baseUrl, deviceToken, id);
}
export async function handleRunRitual(id: string) {
  const { baseUrl, deviceToken } = getPairing();
  return runRitualNow(baseUrl, deviceToken, id);
}
export async function handleRitualRuns(id: string) {
  const { baseUrl, deviceToken } = getPairing();
  return { runs: await getRitualRuns(baseUrl, deviceToken, id) };
}
```

Register `GET/POST /api/rituals`, `PUT/DELETE /api/rituals/:id`, `POST /api/rituals/:id/run`, `GET /api/rituals/:id/runs` in `apps/daemon/src/server.ts`, copying the routing + error-handling shape of the existing connector/provider routes.

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm --filter @celerex/daemon test ritual-routes`
Expected: PASS.

- [ ] **Step 6: Type-check + commit**

```bash
pnpm --filter @celerex/daemon typecheck
git add apps/daemon/src/routes/ritual.ts apps/daemon/src/server.ts apps/daemon/tests/ritual-routes.test.ts
git commit -m "feat(rituals): daemon routes proxy to Agent ritual API"
```

## Task 9: Web — vocabulary rename routines/Orbit → Rituals

**Files:**
- Modify: `apps/web/src/types.ts` (`OrbitConfig` → keep type, retitle as the daily-briefing Ritual template; add Ritual view types matching the seam)
- Modify: `apps/web/src/state/config.ts` (`DEFAULT_ORBIT` → the built-in daily-briefing Ritual default)
- Modify: `apps/web/src/components/TasksView.tsx` (the `/automations` page → "Rituals" surface label/strings)
- Modify: `apps/web/src/components/SettingsDialog.tsx` (Orbit settings → Ritual settings copy)
- Modify: i18n string files (search for user-facing "Routine"/"Orbit"/"Automation" strings)

- [ ] **Step 1: Find every user-facing occurrence**

Run: `cd apps/web && grep -rni "orbit\|routine\|automation" src --include=*.ts --include=*.tsx -l`
Read each hit and classify: **product-facing string** (rename to Ritual) vs **internal identifier** (leave). Per the spec, internal `Routine`/`Orbit` *type names* may stay; only product-facing vocabulary changes.

- [ ] **Step 2: Add Ritual view types mirroring the seam**

In `apps/web/src/types.ts`, add (matching `RitualView`/`RitualRun` from `@celerex/connect`):

```ts
export interface Ritual {
  ritual_id: string;
  project_member_id: string;
  name: string;
  cron_expr: string;
  timezone: string;
  enabled: boolean;
  next_fire_at: string;
  last_fired_at: string | null;
}
export interface RitualRun {
  run_id: string;
  status: string;
  triggered_by: string;
  created_at: string;
}
```

- [ ] **Step 3: Rename product-facing strings**

For each product-facing hit from Step 1, change the displayed copy: "Routine"/"Orbit" → "Ritual"; the `/automations` page title/header → "Rituals". Keep route paths/identifiers stable unless trivially safe. Make the Orbit briefing present as the built-in "Daily briefing" Ritual.

- [ ] **Step 4: Type-check**

Run: `pnpm --filter @celerex/web typecheck`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src
git commit -m "refactor(rituals): rename routines/Orbit vocabulary to Rituals"
```

## Task 10: Web — Rituals surface talks to the daemon

**Files:**
- Modify: `apps/web/src/components/TasksView.tsx` (fetch from `/api/rituals` via the daemon)
- Create: `apps/web/src/state/rituals.ts` (a small data hook calling the daemon routes)
- Test: `apps/web/src/state/rituals.test.ts`

- [ ] **Step 1: Write the failing test for the data hook**

Create `apps/web/src/state/rituals.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { fetchRituals } from "./rituals";

describe("fetchRituals", () => {
  it("GETs /api/rituals and returns the array", async () => {
    const fetchFn = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ rituals: [{ ritual_id: "r1", name: "X" }] }),
    });
    const out = await fetchRituals(fetchFn as unknown as typeof fetch);
    expect(out).toHaveLength(1);
    expect(fetchFn).toHaveBeenCalledWith("/api/rituals", expect.anything());
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm --filter @celerex/web test rituals`
Expected: FAIL — `./rituals` not found.

- [ ] **Step 3: Implement the data hook**

Create `apps/web/src/state/rituals.ts`:

```ts
import type { Ritual } from "../types";

export async function fetchRituals(fetchFn: typeof fetch = fetch): Promise<Ritual[]> {
  const res = await fetchFn("/api/rituals", { headers: { accept: "application/json" } });
  if (!res.ok) throw new Error(`rituals failed (${res.status})`);
  return ((await res.json()) as { rituals: Ritual[] }).rituals;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm --filter @celerex/web test rituals`
Expected: PASS.

- [ ] **Step 5: Wire TasksView to fetchRituals**

In `apps/web/src/components/TasksView.tsx`, replace the local-routine data source with `fetchRituals()`; render the Ritual list (name, schedule, next_fire_at, enabled), and add a stub empty-state section labelled "Automations" with copy: "Triggered automations are coming soon" (no logic — placeholder per spec §4.3).

- [ ] **Step 6: Type-check + commit**

```bash
pnpm --filter @celerex/web typecheck
git add apps/web/src/state/rituals.ts apps/web/src/state/rituals.test.ts apps/web/src/components/TasksView.tsx
git commit -m "feat(rituals): Rituals surface reads from the Agent via the daemon"
```

## Task 11: Stage 3 verification + PR

- [ ] **Step 1: Run the affected suites**

Run: `pnpm --filter @celerex/daemon test && pnpm --filter @celerex/web test`
Expected: green.

- [ ] **Step 2: Verify in the running app (preview)**

Start the app, open the Rituals surface, confirm it lists Rituals from the Agent and the "Automations" stub renders. (Use the preview workflow; share a screenshot.)

- [ ] **Step 3: Open the PR**

```bash
git push -u origin feat/rituals-design
gh pr create --fill --base main --title "feat(rituals): Design rename + rewire to Agent (2b stage 3)"
```

---

# STAGE 4 — Retire the local scheduler

> Branch: `feat/rituals-retire-local` off `main`. **Only after stages 1–3 are merged and Agent Rituals are confirmed firing with the desktop closed.** This stage removes code; do it last.

## Task 12: Migrate scheduled live-artifact refreshers to Rituals

**Files:**
- Modify: wherever live-artifact refreshers are scheduled (search the daemon).

- [ ] **Step 1: Find the refreshers**

Run: `cd apps/daemon && grep -rni "refresh\|live.*artifact\|live.*template" src -l`
Read the hits; identify any scheduled (clock-driven) refresh registered with the local scheduler.

- [ ] **Step 2: Re-express each scheduled refresh as a Ritual**

For each, create it through the seam (`createRitual` with a `flow_ref` to the existing live-delivery flow, or `agent_run`) instead of the local scheduler. Add/adjust a daemon test asserting the refresh is registered as a Ritual, not a local routine.

- [ ] **Step 3: Commit**

```bash
git add apps/daemon/src
git commit -m "refactor(rituals): scheduled live-artifact refreshers run as Rituals"
```

## Task 13: Remove the local scheduler

**Files:**
- Delete: `apps/daemon/src/routines.ts`, `apps/daemon/src/orbit.ts`, `apps/daemon/src/routes/routine.ts`
- Modify: `apps/daemon/src/server.ts` (drop the routine route registration + scheduler startup)

- [ ] **Step 1: Confirm nothing else imports them**

Run: `cd apps/daemon && grep -rn "routines\|orbit\b\|routes/routine" src`
Expected: only the files being removed + their registration in `server.ts`.

- [ ] **Step 2: Remove the scheduler wiring**

Delete the three files; remove their imports, route registrations, and the scheduler boot call from `apps/daemon/src/server.ts`. Delete the now-dead local-routine tests.

- [ ] **Step 3: Type-check + full daemon suite**

Run: `pnpm --filter @celerex/daemon typecheck && pnpm --filter @celerex/daemon test`
Expected: green (no references to removed modules).

- [ ] **Step 4: Commit + PR**

```bash
git add -A apps/daemon
git commit -m "refactor(rituals): retire the local daemon scheduler (Agent-hosted now)"
git push -u origin feat/rituals-retire-local
gh pr create --fill --base main --title "refactor(rituals): retire local scheduler (2b stage 4)"
```

---

## Self-review notes (for the executor)

- **Spec coverage:** §3 architecture → Stage 1 (a Ritual = scheduled project_member). §4.1 endpoints → Tasks 3–5 (create/list/update/delete/run/runs). §4.2 seam → Task 7. §4.3 rename + rewire + Automations stub → Tasks 9–10. §4.4 retirement (additive-first, last) → Stage 4. §6 error handling → partial-create rollback (Task 3), cron 400 (Tasks 3/5), run failure recorded by `enqueue_run` (existing). §7 testing → each task is TDD.
- **Type consistency:** `RitualView`/`RitualBody`/`RitualSpec`/`RitualRun` names are identical across Rust (Stage 1), seam (Stage 2), and web types (Stage 3). The body discriminant is `kind` with `agent_run`/`flow_ref` everywhere.
- **Known confirmations the executor must make in-situ** (not placeholders — integration points): the Agent test harness helper names (`spawn_app`/`device_token`) — copy from the sibling `/api/design` integration tests; `project_members::graph` visibility; the daemon's real pairing accessor + route-registration style (Task 8 Step 1); the web app's daemon-fetch base path.
