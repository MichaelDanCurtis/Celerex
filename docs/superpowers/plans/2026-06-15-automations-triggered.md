# Automations (triggered) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship connector-event Automations: a flow (`project_member`) that fires when a polled connector surfaces a new item, hosted always-on on the Agent — the triggered counterpart to scheduled Rituals.

**Architecture:** An Automation = a `project_member` + an `automation_triggers` binding (connector source + poll config + cursor + filter). An Agent poll loop (sibling to the scheduler loop) ticks, polls each due trigger's connector via an `EventSource` adapter (first impl: http-api), and fires the member via `enqueue_run(triggered_by='connector')` per new item, advancing the cursor at-least-once. Surfaced through `@celerex/connect` + a daemon `/api/automations` proxy + a web Automations section, mirroring Rituals.

**Tech Stack:** Rust/Axum + sqlx/Postgres (Agent); TypeScript/bun (`@celerex/connect`); TypeScript/Node (daemon) + React/Vite (web).

**Spec:** `docs/superpowers/specs/2026-06-15-automations-triggered-design.md`
**Repos:** Celerex (Agent), celerex-design (seam/daemon/web). Each stage = its own branch + PR off main (GitHub Actions stays disabled).

**Template note:** The merged **Rituals** implementation is the structural template throughout. Reference files:
- Agent: `packages/backend/api/src/design/rituals.rs` (handlers + RitualSpec→graph), `scheduler/{loop,store,advance}.rs` (loop), `design/mod.rs::rituals_project` (container project), `lib.rs` (route mounting ~line 121, scheduler spawn ~256-305).
- Seam: `celerex-design/packages/celerex-connect/src/rituals.ts`.
- Daemon: `celerex-design/apps/daemon/src/routes/ritual.ts` + its registration in `server.ts`.
- Web: `celerex-design/apps/web/src/state/rituals.ts` + the Rituals section in `TasksView.tsx`.

---

# STAGE 1 — Agent: data model + adapter + poll loop

> Branch `feat/automations-agent-core` off main (Celerex). Run from `packages/backend/api`. Tests use the existing Postgres/testcontainers harness (`cargo test`).

## Task 1: Migration — `automation_triggers` + `triggered_by='connector'`

**Files:**
- Create: `packages/backend/api/src/db/migrations/00NN_automation_triggers.sql` (use the next sequential number — `ls src/db/migrations/` and increment)
- Test: `packages/backend/api/tests/automations.rs`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE automation_triggers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_member_id UUID NOT NULL REFERENCES project_members(id) ON DELETE CASCADE,
    connector_id TEXT NOT NULL,
    source_config JSONB NOT NULL,
    filter JSONB,
    poll_interval_secs INT NOT NULL DEFAULT 300,
    cursor TEXT,
    last_polled_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_automation_triggers_due ON automation_triggers(enabled, last_polled_at);

ALTER TABLE runs DROP CONSTRAINT runs_triggered_by_chk;
ALTER TABLE runs ADD CONSTRAINT runs_triggered_by_chk
    CHECK (triggered_by IN ('scheduled', 'manual', 'webhook', 'connector'));
```

- [ ] **Step 2: Write a failing test that the table exists + accepts a 'connector' run**

Create `packages/backend/api/tests/automations.rs` (use the same harness helpers the sibling `design_rituals` / scheduler integration tests use — copy the test-DB + token bootstrap):

```rust
// NOTE: reuse the existing integration harness (spawn_app/device_token or the
// testcontainers pool helper the sibling tests use). This test only needs the pool.
#[tokio::test]
async fn migration_creates_automation_triggers_and_allows_connector_runs() {
    let app = spawn_app().await; // existing helper; gives a migrated pool
    // table exists:
    let cols: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns WHERE table_name='automation_triggers'",
    )
    .fetch_one(&app.pool).await.unwrap();
    assert!(cols >= 8);
    // 'connector' is an accepted triggered_by (constraint check): insert a runs row with it.
    // (uses a throwaway engagement; adapt to the harness's seeding helper.)
}
```

- [ ] **Step 3: Run it — migrations run on pool build**

Run: `cargo test --test automations migration_creates_automation_triggers_and_allows_connector_runs`
Expected: PASS once the migration file is picked up (`sqlx::migrate!("./src/db/migrations")` runs on pool build). If the harness caches a pre-migrated DB, ensure a fresh container.

- [ ] **Step 4: Commit**

```bash
git add packages/backend/api/src/db/migrations/ packages/backend/api/tests/automations.rs
git commit -m "feat(automations): automation_triggers table + triggered_by=connector"
```

## Task 2: `EventSource` trait + `PolledItem`

**Files:**
- Create: `packages/backend/api/src/automations/mod.rs` (+ `pub mod automations;` in `lib.rs`)
- Create: `packages/backend/api/src/automations/source.rs`
- Test: `packages/backend/api/tests/automations.rs` (append unit test) OR `#[cfg(test)]` in `source.rs`

- [ ] **Step 1: Write the failing test (in `source.rs` under `#[cfg(test)]`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polled_item_carries_id_and_payload() {
        let it = PolledItem { id: "42".into(), payload: serde_json::json!({"x":1}) };
        assert_eq!(it.id, "42");
        assert_eq!(it.payload["x"], 1);
    }
}
```

- [ ] **Step 2: Run — FAIL (module missing)**

Run: `cargo test -p celerex-api automations::source`
Expected: FAIL (unresolved module).

- [ ] **Step 3: Implement the trait module**

`packages/backend/api/src/automations/mod.rs`:
```rust
//! Automations: connector-event-triggered project_members. A poll loop (sibling
//! to the scheduler) detects new connector items and fires the member's flow.
//! See docs/superpowers/specs/2026-06-15-automations-triggered-design.md
pub mod source;
pub mod store;
pub mod poll;
```
`packages/backend/api/src/automations/source.rs`:
```rust
use async_trait::async_trait;
use serde_json::Value;

/// One new item surfaced by a connector poll.
#[derive(Debug, Clone)]
pub struct PolledItem {
    pub id: String,
    pub payload: Value,
}

/// A connector-specific event source. `cursor` is the last-seen item id; `poll`
/// returns items strictly newer than it (oldest→newest) and the new high-water cursor.
#[async_trait]
pub trait EventSource: Send + Sync {
    async fn poll(
        &self,
        source_config: &Value,
        cursor: Option<&str>,
    ) -> Result<(Vec<PolledItem>, Option<String>), String>;
}
```
Add `pub mod automations;` to `lib.rs` (with the other `pub mod` lines).

- [ ] **Step 4: Run — PASS.** `cargo test -p celerex-api automations::source`

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/automations/ packages/backend/api/src/lib.rs
git commit -m "feat(automations): EventSource trait + PolledItem"
```

## Task 3: `HttpApiEventSource`

**Files:**
- Create: `packages/backend/api/src/automations/http_api.rs` (+ `pub mod http_api;` in `automations/mod.rs`)
- Test: `#[cfg(test)]` in `http_api.rs` (uses `wiremock` — already a dev-dependency)

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    #[tokio::test]
    async fn returns_only_items_past_the_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    {"id": "1", "t": "a"},
                    {"id": "2", "t": "b"},
                    {"id": "3", "t": "c"}
                ]
            })))
            .mount(&server).await;
        let src = HttpApiEventSource { api_key: None };
        let cfg = serde_json::json!({ "url": server.uri(), "items_path": "items", "id_field": "id" });
        let (items, cursor) = src.poll(&cfg, Some("1")).await.unwrap();
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["2", "3"]);
        assert_eq!(cursor.as_deref(), Some("3"));
    }

    #[tokio::test]
    async fn empty_when_nothing_new() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "items": [{"id":"1"}] })))
            .mount(&server).await;
        let src = HttpApiEventSource { api_key: None };
        let cfg = serde_json::json!({ "url": server.uri(), "items_path": "items", "id_field": "id" });
        let (items, cursor) = src.poll(&cfg, Some("1")).await.unwrap();
        assert!(items.is_empty());
        assert_eq!(cursor.as_deref(), Some("1")); // cursor unchanged when no new items
    }
}
```

- [ ] **Step 2: Run — FAIL.** `cargo test -p celerex-api automations::http_api`

- [ ] **Step 3: Implement**

`packages/backend/api/src/automations/http_api.rs`:
```rust
use async_trait::async_trait;
use serde_json::Value;
use crate::automations::source::{EventSource, PolledItem};

/// Polls a JSON endpoint. `source_config`: { url, items_path, id_field }. New =
/// item whose `id_field` value (as a string) is lexicographically/numerically
/// greater than `cursor`. Cursor = the max id seen. Items are returned in input
/// order, filtered to those past the cursor.
pub struct HttpApiEventSource {
    /// Optional vaulted api_key for the `http-api` connector; sent as a bearer.
    pub api_key: Option<String>,
}

fn cmp_ids(a: &str, b: &str) -> std::cmp::Ordering {
    // Numeric compare when both parse as i128; else lexicographic.
    match (a.parse::<i128>(), b.parse::<i128>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

#[async_trait]
impl EventSource for HttpApiEventSource {
    async fn poll(
        &self,
        source_config: &Value,
        cursor: Option<&str>,
    ) -> Result<(Vec<PolledItem>, Option<String>), String> {
        let url = source_config.get("url").and_then(|v| v.as_str())
            .ok_or("source_config.url required")?;
        let items_path = source_config.get("items_path").and_then(|v| v.as_str()).unwrap_or("items");
        let id_field = source_config.get("id_field").and_then(|v| v.as_str()).unwrap_or("id");

        let client = reqwest::Client::new();
        let mut req = client.get(url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        let arr = body.get(items_path).and_then(|v| v.as_array())
            .ok_or(format!("items_path '{items_path}' is not an array"))?;

        let mut new_items = Vec::new();
        let mut high = cursor.map(|s| s.to_string());
        for item in arr {
            let id = match item.get(id_field) {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string().trim_matches('"').to_string(),
                None => continue,
            };
            let is_new = match cursor {
                Some(c) => cmp_ids(&id, c) == std::cmp::Ordering::Greater,
                None => true,
            };
            if is_new {
                new_items.push(PolledItem { id: id.clone(), payload: item.clone() });
            }
            // track the high-water mark across ALL seen ids
            high = Some(match high {
                Some(h) if cmp_ids(&id, &h) != std::cmp::Ordering::Greater => h,
                _ => id,
            });
        }
        Ok((new_items, high))
    }
}
```
Add `pub mod http_api;` to `automations/mod.rs`.

> Note `wiremock` is in `[dev-dependencies]`; the `#[cfg(test)]` block uses it fine. `reqwest` + `async_trait` + `serde_json` are normal deps.

- [ ] **Step 4: Run — PASS.** `cargo test -p celerex-api automations::http_api`

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/automations/
git commit -m "feat(automations): http-api EventSource adapter (cursor by id)"
```

## Task 4: `AutomationTriggersStore`

**Files:**
- Create: `packages/backend/api/src/automations/store.rs`
- Test: `packages/backend/api/tests/automations.rs` (append; needs the pool)

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn store_due_filters_by_interval_and_advances_cursor() {
    let app = spawn_app().await;
    use celerex_api::automations::store::{PgAutomationTriggersStore, AutomationTriggersStore, AutomationTrigger};
    // seed an engagement + project + member (reuse the harness's member-seeding helper)
    let member_id = app.seed_member().await; // existing helper, or insert directly
    let store = PgAutomationTriggersStore::new(app.pool.clone());
    let id = store.create(AutomationTrigger {
        id: uuid::Uuid::new_v4(),
        project_member_id: member_id,
        connector_id: "http-api".into(),
        source_config: serde_json::json!({"url":"http://x","items_path":"items","id_field":"id"}),
        filter: None,
        poll_interval_secs: 300,
        cursor: None,
        last_polled_at: None,
        enabled: true,
    }).await.unwrap();

    let due = store.due(chrono::Utc::now()).await.unwrap();
    assert!(due.iter().any(|t| t.id == id)); // never-polled trigger is due

    store.mark_polled(id, chrono::Utc::now(), Some("5")).await.unwrap();
    let due2 = store.due(chrono::Utc::now()).await.unwrap();
    assert!(!due2.iter().any(|t| t.id == id)); // just polled → not due within interval
    let got = store.get(id).await.unwrap().unwrap();
    assert_eq!(got.cursor.as_deref(), Some("5"));
}
```

- [ ] **Step 2: Run — FAIL.** `cargo test --test automations store_due`

- [ ] **Step 3: Implement** `packages/backend/api/src/automations/store.rs` mirroring `scheduler/store.rs` (`Schedule`/`SchedulesStore`/`PgSchedulesStore`):

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrigger {
    pub id: Uuid,
    pub project_member_id: Uuid,
    pub connector_id: String,
    pub source_config: Value,
    pub filter: Option<Value>,
    pub poll_interval_secs: i32,
    pub cursor: Option<String>,
    pub last_polled_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

#[async_trait]
pub trait AutomationTriggersStore: Send + Sync {
    async fn create(&self, t: AutomationTrigger) -> Result<Uuid, String>;
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<AutomationTrigger>, String>;
    async fn mark_polled(&self, id: Uuid, at: DateTime<Utc>, cursor: Option<String>) -> Result<(), String>;
    async fn get(&self, id: Uuid) -> Result<Option<AutomationTrigger>, String>;
    async fn list_for_engagement(&self, engagement_id: Uuid) -> Result<Vec<AutomationTrigger>, String>;
    async fn update(&self, id: Uuid, source_config: &Value, filter: Option<&Value>, poll_interval_secs: i32, enabled: bool) -> Result<(), String>;
    async fn delete(&self, id: Uuid) -> Result<(), String>;
}

pub struct PgAutomationTriggersStore { pool: PgPool }
impl PgAutomationTriggersStore { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[async_trait]
impl AutomationTriggersStore for PgAutomationTriggersStore {
    async fn create(&self, t: AutomationTrigger) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO automation_triggers (id, project_member_id, connector_id, source_config, filter, poll_interval_secs, cursor, enabled)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
            .bind(t.id).bind(t.project_member_id).bind(&t.connector_id).bind(&t.source_config)
            .bind(&t.filter).bind(t.poll_interval_secs).bind(&t.cursor).bind(t.enabled)
            .fetch_one(&self.pool).await.map_err(|e| e.to_string())
    }
    async fn due(&self, now: DateTime<Utc>) -> Result<Vec<AutomationTrigger>, String> {
        sqlx::query_as::<_, (Uuid,Uuid,String,Value,Option<Value>,i32,Option<String>,Option<DateTime<Utc>>,bool)>(
            "SELECT id, project_member_id, connector_id, source_config, filter, poll_interval_secs, cursor, last_polled_at, enabled
             FROM automation_triggers
             WHERE enabled = TRUE AND (last_polled_at IS NULL OR last_polled_at + (poll_interval_secs || ' seconds')::interval <= $1)")
            .bind(now).fetch_all(&self.pool).await
            .map(|rows| rows.into_iter().map(row_to_trigger).collect())
            .map_err(|e| e.to_string())
    }
    async fn mark_polled(&self, id: Uuid, at: DateTime<Utc>, cursor: Option<String>) -> Result<(), String> {
        sqlx::query("UPDATE automation_triggers SET last_polled_at=$2, cursor=COALESCE($3, cursor) WHERE id=$1")
            .bind(id).bind(at).bind(cursor).execute(&self.pool).await.map(|_| ()).map_err(|e| e.to_string())
    }
    async fn get(&self, id: Uuid) -> Result<Option<AutomationTrigger>, String> {
        sqlx::query_as::<_, (Uuid,Uuid,String,Value,Option<Value>,i32,Option<String>,Option<DateTime<Utc>>,bool)>(
            "SELECT id, project_member_id, connector_id, source_config, filter, poll_interval_secs, cursor, last_polled_at, enabled FROM automation_triggers WHERE id=$1")
            .bind(id).fetch_optional(&self.pool).await.map(|o| o.map(row_to_trigger)).map_err(|e| e.to_string())
    }
    async fn list_for_engagement(&self, engagement_id: Uuid) -> Result<Vec<AutomationTrigger>, String> {
        sqlx::query_as::<_, (Uuid,Uuid,String,Value,Option<Value>,i32,Option<String>,Option<DateTime<Utc>>,bool)>(
            "SELECT a.id, a.project_member_id, a.connector_id, a.source_config, a.filter, a.poll_interval_secs, a.cursor, a.last_polled_at, a.enabled
             FROM automation_triggers a JOIN project_members m ON m.id=a.project_member_id JOIN projects p ON p.id=m.project_id
             WHERE p.engagement_id=$1 ORDER BY a.created_at DESC")
            .bind(engagement_id).fetch_all(&self.pool).await
            .map(|rows| rows.into_iter().map(row_to_trigger).collect()).map_err(|e| e.to_string())
    }
    async fn update(&self, id: Uuid, source_config: &Value, filter: Option<&Value>, poll_interval_secs: i32, enabled: bool) -> Result<(), String> {
        sqlx::query("UPDATE automation_triggers SET source_config=$2, filter=$3, poll_interval_secs=$4, enabled=$5 WHERE id=$1")
            .bind(id).bind(source_config).bind(filter).bind(poll_interval_secs).bind(enabled)
            .execute(&self.pool).await.map(|_| ()).map_err(|e| e.to_string())
    }
    async fn delete(&self, id: Uuid) -> Result<(), String> {
        sqlx::query("DELETE FROM automation_triggers WHERE id=$1").bind(id).execute(&self.pool).await.map(|_| ()).map_err(|e| e.to_string())
    }
}

#[allow(clippy::type_complexity)]
fn row_to_trigger(r: (Uuid,Uuid,String,Value,Option<Value>,i32,Option<String>,Option<DateTime<Utc>>,bool)) -> AutomationTrigger {
    AutomationTrigger { id: r.0, project_member_id: r.1, connector_id: r.2, source_config: r.3, filter: r.4, poll_interval_secs: r.5, cursor: r.6, last_polled_at: r.7, enabled: r.8 }
}
```

- [ ] **Step 4: Run — PASS.** `cargo test --test automations store_due`

- [ ] **Step 5: Commit**

```bash
git add packages/backend/api/src/automations/store.rs packages/backend/api/tests/automations.rs
git commit -m "feat(automations): AutomationTriggersStore (due/mark_polled/cursor)"
```

## Task 5: Poll loop + filter

**Files:**
- Create: `packages/backend/api/src/automations/poll.rs`
- Test: `#[cfg(test)]` in `poll.rs` (fake EventSource + counter) + an integration test in `tests/automations.rs`

- [ ] **Step 1: Write the failing unit test (in `poll.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_matches_substring_on_field() {
        let item = serde_json::json!({"from":"alice@x.com","subject":"hi"});
        assert!(item_matches_filter(&item, &serde_json::json!({"field":"from","contains":"alice"})));
        assert!(!item_matches_filter(&item, &serde_json::json!({"field":"from","contains":"bob"})));
    }
    #[test]
    fn no_filter_matches_all() {
        assert!(item_matches_filter(&serde_json::json!({"a":1}), &serde_json::Value::Null));
    }
}
```

- [ ] **Step 2: Run — FAIL.** `cargo test -p celerex-api automations::poll`

- [ ] **Step 3: Implement the filter + the loop**

`packages/backend/api/src/automations/poll.rs`:
```rust
use crate::automations::http_api::HttpApiEventSource;
use crate::automations::source::EventSource;
use crate::automations::store::{AutomationTrigger, AutomationTriggersStore};
use crate::project_members::run_job::{self, TRIGGERED_BY_CONNECTOR};
use crate::project_members::store::MembersStore;
use crate::providers::store::ProviderKeysStore;
use crate::runs::Executor;  // adjust path to the real Executor type used by enqueue_run
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// True when `item` passes `filter`. `filter` Null/None → matches all. A filter
/// `{field, contains}` matches when `item[field]` (stringified) contains `contains`.
pub fn item_matches_filter(item: &Value, filter: &Value) -> bool {
    let obj = match filter.as_object() { Some(o) => o, None => return true };
    let field = match obj.get("field").and_then(|v| v.as_str()) { Some(f) => f, None => return true };
    let needle = obj.get("contains").and_then(|v| v.as_str()).unwrap_or("");
    let hay = match item.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => return false,
    };
    hay.contains(needle)
}

/// Resolve the EventSource for a connector id (only http-api for now). `api_key`
/// is the vaulted credential for that connector (None for keyless/internal).
fn event_source_for(connector_id: &str, api_key: Option<String>) -> Result<Box<dyn EventSource>, String> {
    match connector_id {
        "http-api" => Ok(Box::new(HttpApiEventSource { api_key })),
        other => Err(format!("no event source for connector '{other}'")),
    }
}

pub struct AutomationPollLoop {
    pub store: Arc<dyn AutomationTriggersStore>,
    pub members: Arc<dyn MembersStore>,
    pub provider_keys: Arc<dyn ProviderKeysStore>,
    pub pool: PgPool,
    pub executor: Arc<Executor>,
    pub notifiers: Arc<Vec<Arc<dyn crate::notifiers::Notifier>>>,
    pub tick: Duration,
}

impl AutomationPollLoop {
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.tick);
            loop {
                interval.tick().await;
                let now = Utc::now();
                let due = match self.store.due(now).await {
                    Ok(v) => v,
                    Err(e) => { tracing::error!(?e, "automation store.due"); continue; }
                };
                for t in due {
                    self.poll_one(&t, now).await;
                }
            }
        })
    }

    async fn poll_one(&self, t: &AutomationTrigger, now: chrono::DateTime<Utc>) {
        let engagement_id = match self.members.engagement_id_for(t.project_member_id).await {
            Ok(v) => v,
            Err(e) => { tracing::error!(trigger=%t.id, error=%e, "resolve engagement"); return; }
        };
        let api_key = self.provider_keys.get(engagement_id, &t.connector_id).await.ok().flatten();
        let source = match event_source_for(&t.connector_id, api_key) {
            Ok(s) => s,
            Err(e) => { tracing::error!(trigger=%t.id, error=%e, "event source"); return; }
        };
        let (items, new_cursor) = match source.poll(&t.source_config, t.cursor.as_deref()).await {
            Ok(v) => v,
            Err(e) => { tracing::warn!(trigger=%t.id, error=%e, "poll failed; will retry"); return; }
        };
        // Fire per matching new item, advancing the cursor only past fired items (at-least-once).
        let filter = t.filter.clone().unwrap_or(Value::Null);
        let mut advanced: Option<String> = t.cursor.clone();
        for item in items {
            if item_matches_filter(&item.payload, &filter) {
                if let Err(e) = run_job::enqueue_run(
                    self.pool.clone(), self.members.clone(), self.executor.clone(),
                    self.notifiers.clone(), t.project_member_id, engagement_id, TRIGGERED_BY_CONNECTOR,
                ).await {
                    tracing::error!(trigger=%t.id, error=%e, "enqueue failed; stop advancing");
                    break; // leave cursor at last successfully-fired item → retry next tick
                }
            }
            advanced = Some(item.id); // advance past processed (fired or filtered-out) item
        }
        // If no items at all, keep the source's high-water cursor so we don't re-scan.
        let final_cursor = advanced.or(new_cursor);
        if let Err(e) = self.store.mark_polled(t.id, now, final_cursor).await {
            tracing::error!(trigger=%t.id, error=%e, "mark_polled");
        }
    }
}
```
Add `pub const TRIGGERED_BY_CONNECTOR: &str = "connector";` to `project_members/run_job.rs`.

> VERIFY paths: `Executor`, `ProviderKeysStore`, `MembersStore`, `notifiers::Notifier`, and `provider_keys.get(engagement, connector_id) -> Result<Option<String>, _>` against the real modules (the Rituals/scheduler wiring + `design/mod.rs` use these exact types). Fix imports to match.

- [ ] **Step 4: Run the filter unit tests — PASS.** `cargo test -p celerex-api automations::poll`

- [ ] **Step 5: Integration test — a fake source fires a run + advances cursor**

Append to `tests/automations.rs` a test that: seeds a member + an `automation_triggers` row whose `source_config` points at a `wiremock` endpoint returning 2 items, builds the store + a real executor (or assert via the `runs` table), runs ONE `poll_one` (expose it or drive a single tick), and asserts a `runs` row with `triggered_by='connector'` exists for the member and the trigger's cursor advanced. (Mirror how `design_rituals`' run-now test asserts a `runs` row.)

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/automations/poll.rs packages/backend/api/src/project_members/run_job.rs packages/backend/api/tests/automations.rs
git commit -m "feat(automations): poll loop + substring filter + at-least-once cursor"
```

---

# STAGE 2 — Agent: `/api/design/automations` API + startup wiring

> Same branch (or `feat/automations-agent-api` off the Stage 1 branch). Mirrors `design/rituals.rs` almost line-for-line, swapping `schedules`→`automation_triggers` and adding source/filter fields.

## Task 6: `AutomationSpec` + create handler

**Files:**
- Create: `packages/backend/api/src/design/automations.rs` (+ `pub mod automations;` in `design/mod.rs`)
- Modify: `packages/backend/api/src/design/mod.rs` — add an `automations_project` helper (copy `rituals_project`, name "Automations")
- Modify: `packages/backend/api/src/lib.rs` — mount routes
- Test: `packages/backend/api/tests/design_automations.rs`

- [ ] **Step 1: Recon + copy the Rituals handler shape.** Read `design/rituals.rs` fully — `RitualBody` (`agent_run`/`flow_ref`), `build_ritual_graph`, `create_ritual` (member create + schedule insert + rollback), `RitualView`, `owned_schedule`. The Automations handlers are the same with `automation_triggers` instead of `schedules` + the extra `connector_id`/`source_config`/`filter`/`poll_interval_secs` fields.

- [ ] **Step 2: Write the failing create test** in `tests/design_automations.rs` (mirror `design_rituals`'s create test + harness):

```rust
#[tokio::test]
async fn create_automation_makes_member_and_trigger() {
    let app = spawn_app().await;
    let token = app.device_token().await;
    let res = app.client.post(format!("{}/api/design/automations", app.addr))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "Watch API",
            "connector_id": "http-api",
            "source_config": {"url":"http://x","items_path":"items","id_field":"id"},
            "poll_interval_secs": 120,
            "body": {"kind":"agent_run","prompt":"handle the new item"}
        })).send().await.unwrap();
    assert_eq!(res.status(), 201);
    let v: serde_json::Value = res.json().await.unwrap();
    assert!(v["automation_id"].is_string());
    assert!(v["project_member_id"].is_string());
    assert_eq!(v["connector_id"], "http-api");
}
```

- [ ] **Step 3: Run — FAIL (route missing).** `cargo test --test design_automations create_automation`

- [ ] **Step 4: Implement `automations.rs`** — `AutomationSpec` / `AutomationBody` (reuse the `agent_run`/`flow_ref` shape; you may import `build_ritual_graph` from `design::rituals` for the `agent_run` graph, or copy it), `AutomationView { automation_id, project_member_id, name, connector_id, source_config, filter, poll_interval_secs, enabled, cursor, last_polled_at }`, and `create_automation` (resolve `automations_project`, `members.create(..., "writer")`, insert the `automation_triggers` row via `PgAutomationTriggersStore`; roll back the member on trigger-insert failure for an `agent_run` body). Use `DeviceAuth`. Add `automations_project` to `design/mod.rs`. Mount `POST /api/design/automations` in `lib.rs`.

- [ ] **Step 5: Run — PASS.** `cargo test --test design_automations create_automation`

- [ ] **Step 6: Commit**

```bash
git add packages/backend/api/src/design/automations.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/design_automations.rs
git commit -m "feat(automations): POST /api/design/automations create handler"
```

## Task 7: list / update / delete / run-now / runs handlers

**Files:** modify `design/automations.rs`, `lib.rs`; append to `tests/design_automations.rs`.

- [ ] Mirror Rituals Task 4+5 exactly (the merged `design/rituals.rs` `list_rituals`/`delete_ritual`/`update_ritual`/`run_ritual_now`/`list_ritual_runs` + `owned_*` guard), swapping the store + view. `run_automation_now` uses `TRIGGERED_BY_MANUAL`. Write the analogous tests (list→delete lifecycle; update source/filter/interval/enabled; run-now→202 + runs non-empty), run them green, mount the routes (`GET /api/design/automations`, `{id}` PUT/DELETE, `{id}/run`, `{id}/runs`), commit `feat(automations): list/update/delete/run/runs handlers`.

## Task 8: Wire the poll loop into startup

**Files:** modify `packages/backend/api/src/lib.rs` (the `build_app` spawn block ~256-305, beside the scheduler spawn).

- [ ] **Step 1:** After the `if spawn_scheduler { … }` block, add an analogous spawn for the automation poll loop (gated by the same flag, or a new `spawn_automation_poll`). Build `PgAutomationTriggersStore`, pass `members`/`provider_keys`/`pool`/`executor`/`notifiers` clones + a `tick` (e.g. `Duration::from_secs(30)`), and call `AutomationPollLoop { … }.spawn()`. `provider_keys` is already on `AppState` (`build_provider_keys_store`).
- [ ] **Step 2:** `cargo build -p celerex-api` clean; `cargo test -p celerex-api` green (the test harness uses `spawn_scheduler=false`, so the loop won't interfere — confirm the automation test that drives `poll_one` does so directly, not via the spawned loop).
- [ ] **Step 3:** Commit `feat(automations): spawn the poll loop at startup`.

## Task 9: Stage 1+2 full-suite + PR

- [ ] `cargo test -p celerex-api` green. Push `feat/automations-agent-core`; `gh pr create --fill --base main --title "feat(automations): Agent core — triggers, http-api poll, /api/design/automations (2d)"`.

---

# STAGE 3 — Seam (`@celerex/connect`)

> Repo celerex-design. Branch `feat/automations-seam` off main. Mirror `packages/celerex-connect/src/rituals.ts`. **Rebuild dist after** (`pnpm --filter @celerex/connect build`) — the daemon imports the built output.

## Task 10: `automations.ts` client

- [ ] **Step 1:** Read `src/rituals.ts` + `test/rituals.test.ts`.
- [ ] **Step 2:** Create `src/automations.ts` mirroring it: types `AutomationSpec` (`{name, enabled?, connector_id, source_config, filter?, poll_interval_secs?, body}`), `AutomationView`, `AutomationRun`; functions `listAutomations`/`createAutomation`/`updateAutomation`/`deleteAutomation`/`runAutomationNow`/`getAutomationRuns` against `/api/design/automations*`, using the same `api()`/`asError()` helpers + `FetchFn`. Add explicit exports to `src/index.ts` (named-export style).
- [ ] **Step 3:** `test/automations.test.ts` mirroring `rituals.test.ts` (create posts the spec incl. `connector_id`/`source_config`; list returns the array; delete issues a DELETE). Run green (`bun test test/automations.test.ts`).
- [ ] **Step 4:** `pnpm --filter @celerex/connect build`. Commit `feat(automations): @celerex/connect automation client`. Push + PR.

---

# STAGE 4 — Daemon proxy

> Branch `feat/automations-daemon` off main (after Stage 3 merges + dist rebuilt). Mirror `apps/daemon/src/routes/ritual.ts`.

## Task 11: `/api/automations` proxy routes

- [ ] **Step 1:** Read `apps/daemon/src/routes/ritual.ts` + its registration in `server.ts` + `tests/ritual-routes.test.ts`.
- [ ] **Step 2:** Create `apps/daemon/src/routes/automation.ts` mirroring `ritual.ts` (read pairing via the same accessor; call the seam `listAutomations`/`createAutomation`/etc.). Register `GET/POST /api/automations`, `PUT/DELETE /api/automations/:id`, `POST /api/automations/:id/run`, `GET /api/automations/:id/runs` in `server.ts` next to the ritual routes. Same 403/409/400/502 mapping. Light body-shape check on POST (`name`, `connector_id`, `source_config`, `body.kind`).
- [ ] **Step 3:** `tests/automation-routes.test.ts` mirroring `ritual-routes.test.ts` (stub Agent; pair; assert each route proxies with the bearer token). Run green; `pnpm --filter @celerex/daemon typecheck`.
- [ ] **Step 4:** Commit `feat(automations): daemon routes proxy to Agent automation API`. Push + PR.

---

# STAGE 5 — Web Automations section

> Branch `feat/automations-web` off main (after Stages 3-4 merge). Replaces the "Automations — coming soon" stub.

## Task 12: `state/automations.ts` + list section

- [ ] **Step 1:** Read `apps/web/src/state/rituals.ts` + the Rituals section + the `automations-stub-section` in `TasksView.tsx`.
- [ ] **Step 2:** Create `apps/web/src/state/automations.ts`: `Automation` type (matching `AutomationView`) + `fetchAutomations(fetchFn?)` (GET `/api/automations` → `.automations`), `createAutomation`, `updateAutomation`, `deleteAutomation`, `runAutomationNow`, `getAutomationRuns`. Add an `Automation`/`AutomationRun` type to `apps/web/src/types.ts`. Tests in `apps/web/tests/state/automations.test.ts` (mirror `rituals.test.ts`).
- [ ] **Step 3:** Replace the stub section in `TasksView.tsx` with a real **Automations** section listing automations (name, connector source, enabled, last run) from `fetchAutomations`, with run-now + enable-disable. Update the TasksView tests.
- [ ] **Step 4:** `pnpm --filter @celerex/web typecheck` + web tests (baseline minus the known 3 `default-background` failures). Commit `feat(automations): web Automations section reads from the Agent`.

## Task 13: Create form

- [ ] **Step 1:** Add a minimal create form (modal or inline) in the Automations section: fields name, connector (http-api fixed for now), `url`, `items_path`, `id_field`, optional filter (`field`+`contains`), `poll_interval_secs`, and an `agent_run` `prompt`. On submit → `createAutomation({ name, connector_id:'http-api', source_config:{url,items_path,id_field}, filter, poll_interval_secs, body:{kind:'agent_run', prompt} })` → refresh the list.
- [ ] **Step 2:** Test: the form posts a well-formed `AutomationSpec` (mock the daemon route). Typecheck + tests green.
- [ ] **Step 3:** Commit `feat(automations): minimal create form for http-api automations`. Push + PR.

## Task 14: Live verification (mirrors 2f/2g)

- [ ] With the in-cluster Agent + daemon + web running (the 2g stack), create an http-api Automation pointed at a stub JSON endpoint, add a new item to the endpoint, and confirm (via `/api/automations/:id/runs` and the Agent `runs` table) that a `triggered_by='connector'` run fired. Record the result.

---

## Self-review notes (for the executor)

- **Spec coverage:** §4.1 table+`triggered_by` → Task 1. §4.2 EventSource+http-api → Tasks 2-3. §4.3 poll loop+filter+at-least-once → Task 5 (+ store Task 4). §4.4 device API → Tasks 6-7. wiring → Task 8. §4.5 seam → Task 10. §4.6 daemon → Task 11. §4.7 web section+create form → Tasks 12-13. §7 testing → each task is TDD. §6 error handling → poll/enqueue failure (Task 5), partial-create rollback + bad-connector 400 (Task 6).
- **Type consistency:** `AutomationTrigger`/`AutomationTriggersStore` (Rust) ↔ `AutomationSpec`/`AutomationView`/`AutomationRun` (seam) ↔ `Automation`/`AutomationRun` (web). `connector_id`/`source_config`/`filter`/`poll_interval_secs` field names identical across all layers. Body discriminant `kind: agent_run|flow_ref` matches Rituals. `TRIGGERED_BY_CONNECTOR = "connector"` matches the migration's CHECK value.
- **In-situ confirmations (not placeholders):** the test-harness helpers (`spawn_app`/`device_token`/`seed_member`) — copy from the sibling `design_rituals`/`automations` integration tests; the exact `Executor`/`ProviderKeysStore`/`Notifier` import paths (Task 5) — verify against the scheduler/Rituals wiring; the daemon pairing accessor + web stub-section location — read the Rituals equivalents first.
- **Known baseline:** 3 pre-existing web `default-background` failures + 5 daemon Claude-CLI-auth failures are acceptable; no new failures.
