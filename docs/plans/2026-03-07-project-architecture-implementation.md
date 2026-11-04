# Project Architecture Restructuring — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restructure Celerex from Engagement→Docs to Engagement→Project→Docs, add PostgreSQL persistence, project-level agent assignments, BMAD Solution Builder, and real-time execution views.

**Architecture:** Six phases building bottom-up: database layer first, then API, then frontend data model, then UI restructuring, then BMAD wizard, then execution views. Each phase is independently deployable and testable.

**Tech Stack:** Rust (Axum, sqlx, tokio), PostgreSQL, Docker Compose, React 19, Jotai, TypeScript, BlockSuite.

**Design Doc:** `docs/plans/2026-03-07-project-architecture-design.md`

---

# Phase 1: PostgreSQL Database Layer

## Task 1: Add PostgreSQL with Docker Compose and sqlx

**Files:**
- Create: `docker-compose.yml` (project root)
- Modify: `packages/backend/api/Cargo.toml`
- Create: `packages/backend/api/src/db.rs`
- Create: `packages/backend/api/migrations/001_initial_schema.sql`
- Modify: `packages/backend/api/src/main.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create docker-compose.yml at project root**

```yaml
# docker-compose.yml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: celerex
      POSTGRES_USER: celerex
      POSTGRES_PASSWORD: celerex_dev
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

**Step 2: Add sqlx dependency to workspace and api crate**

In root `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
```

In `packages/backend/api/Cargo.toml`, add:
```toml
sqlx.workspace = true
```

**Step 3: Create migration file**

Create `packages/backend/api/migrations/001_initial_schema.sql`:
```sql
-- Core hierarchy
CREATE TABLE IF NOT EXISTS engagements (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    engagement_id TEXT NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    code TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Global catalog
CREATE TABLE IF NOT EXISTS agent_defs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
    definition JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_defs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
    definition JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS trigger_defs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
    definition JSONB NOT NULL
);

-- Project-scoped assignments
CREATE TABLE IF NOT EXISTS project_agents (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_def_id TEXT NOT NULL REFERENCES agent_defs(id),
    overrides JSONB
);

CREATE TABLE IF NOT EXISTS pipelines (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    nodes JSONB NOT NULL DEFAULT '[]',
    edges JSONB NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS trigger_bindings (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    trigger_def_id TEXT NOT NULL REFERENCES trigger_defs(id),
    agent_id TEXT NOT NULL REFERENCES project_agents(id),
    config JSONB,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    pipeline_id TEXT REFERENCES pipelines(id),
    status TEXT NOT NULL,
    result JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL
);
```

**Step 4: Create database module**

Create `packages/backend/api/src/db.rs`:
```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let migration_sql = include_str!("../migrations/001_initial_schema.sql");
    sqlx::raw_sql(migration_sql).execute(pool).await?;
    tracing::info!("Database migrations applied");
    Ok(())
}
```

**Step 5: Update AppState and main.rs**

In `main.rs`, add `mod db;` and update `AppState` to include the pool:

```rust
// Replace the old AppState with:
#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
    copilot: Arc<Mutex<CopilotState>>,
}
```

Remove the in-memory `engagements: Arc<Mutex<Vec<Engagement>>>` field.

In `main()`, initialize the pool:
```rust
let database_url = env::var("DATABASE_URL")
    .unwrap_or_else(|_| "postgres://celerex:celerex_dev@localhost:5432/celerex".to_string());

let pool = db::init_pool(&database_url).await?;

let state = AppState {
    pool,
    copilot: Arc::new(Mutex::new(copilot_state)),
};
```

**Step 6: Verify it compiles**

Start Postgres first:
```bash
docker compose up -d postgres
```

Then: `cargo check -p celerex-api`

**Step 7: Commit**

```bash
git add docker-compose.yml Cargo.toml packages/backend/api/
git commit -m "feat: add PostgreSQL with docker-compose, sqlx pool, and initial schema migration"
```

---

## Task 2: Rewrite Engagement CRUD to use PostgreSQL

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Replace in-memory engagement handlers with sqlx queries**

```rust
async fn list_engagements(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, chrono::DateTime<Utc>)>(
        "SELECT id, name, created_at FROM engagements ORDER BY created_at DESC"
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let engagements: Vec<Value> = rows.iter().map(|(id, name, created_at)| {
                json!({"id": id, "name": name, "created_at": created_at.to_rfc3339()})
            }).collect();
            (StatusCode::OK, Json(json!(engagements)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn create_engagement(
    State(state): State<AppState>,
    Json(body): Json<CreateEngagement>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT INTO engagements (id, name) VALUES ($1, $2)"
    )
    .bind(&id)
    .bind(&body.name)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({"id": id, "name": body.name}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn delete_engagement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Check for projects first
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE engagement_id = $1")
        .bind(&id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or((0,));

    if count.0 > 0 {
        return (StatusCode::CONFLICT, Json(json!({"error": "Cannot delete engagement with projects"})));
    }

    let result = sqlx::query("DELETE FROM engagements WHERE id = $1")
        .bind(&id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Engagement not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 2: Remove the old in-memory domain types (Engagement, DocMeta structs) and seed data**

Delete the old `Engagement`, `DocMeta` structs and the default data creation in `main()`. Keep `CreateEngagement` request struct.

**Step 3: Verify it compiles and test**

```bash
cargo check -p celerex-api
```

Start API with Postgres running, then:
```bash
curl -X POST http://localhost:3001/api/engagements -H "Content-Type: application/json" -d '{"name":"Test"}'
curl http://localhost:3001/api/engagements
```

**Step 4: Commit**

```bash
git commit -m "feat: rewrite engagement CRUD to use PostgreSQL via sqlx"
```

---

## Task 3: Add Project CRUD endpoints

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Add request structs**

```rust
#[derive(Debug, Deserialize)]
struct CreateProject {
    name: String,
    code: String,
}

#[derive(Debug, Deserialize)]
struct UpdateProject {
    name: Option<String>,
    code: Option<String>,
}
```

**Step 2: Add project handlers**

```rust
async fn list_projects(
    State(state): State<AppState>,
    Path(engagement_id): Path<String>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, String, String, chrono::DateTime<Utc>)>(
        "SELECT id, name, code, engagement_id, created_at FROM projects WHERE engagement_id = $1 ORDER BY created_at"
    )
    .bind(&engagement_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let projects: Vec<Value> = rows.iter().map(|(id, name, code, eng_id, created_at)| {
                json!({"id": id, "name": name, "code": code, "engagement_id": eng_id, "created_at": created_at.to_rfc3339()})
            }).collect();
            (StatusCode::OK, Json(json!(projects)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn create_project(
    State(state): State<AppState>,
    Path(engagement_id): Path<String>,
    Json(body): Json<CreateProject>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();

    // Verify engagement exists
    let exists = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM engagements WHERE id = $1")
        .bind(&engagement_id)
        .fetch_one(&state.pool)
        .await
        .map(|r| r.0 > 0)
        .unwrap_or(false);

    if !exists {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Engagement not found"})));
    }

    let result = sqlx::query(
        "INSERT INTO projects (id, engagement_id, name, code) VALUES ($1, $2, $3, $4)"
    )
    .bind(&id)
    .bind(&engagement_id)
    .bind(&body.name)
    .bind(&body.code)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({
            "id": id, "name": body.name, "code": body.code, "engagement_id": engagement_id
        }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let row = sqlx::query_as::<_, (String, String, String, String, chrono::DateTime<Utc>)>(
        "SELECT id, name, code, engagement_id, created_at FROM projects WHERE id = $1"
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some((id, name, code, eng_id, created_at))) => {
            (StatusCode::OK, Json(json!({
                "id": id, "name": name, "code": code,
                "engagement_id": eng_id, "created_at": created_at.to_rfc3339()
            })))
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProject>,
) -> impl IntoResponse {
    if let Some(name) = &body.name {
        let _ = sqlx::query("UPDATE projects SET name = $1 WHERE id = $2")
            .bind(name).bind(&id).execute(&state.pool).await;
    }
    if let Some(code) = &body.code {
        let _ = sqlx::query("UPDATE projects SET code = $1 WHERE id = $2")
            .bind(code).bind(&id).execute(&state.pool).await;
    }
    (StatusCode::OK, Json(json!({"status": "updated"})))
}

async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(&id).execute(&state.pool).await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Project not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 3: Register routes**

```rust
.route("/api/engagements/{id}/projects", get(list_projects))
.route("/api/engagements/{id}/projects", post(create_project))
.route("/api/projects/{id}", get(get_project))
.route("/api/projects/{id}", put(update_project))
.route("/api/projects/{id}", delete(delete_project))
```

**Step 4: Verify and commit**

```bash
cargo check -p celerex-api
git commit -m "feat: add project CRUD endpoints with PostgreSQL"
```

---

## Task 4: Move Document CRUD under Projects

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Rewrite document handlers to use projects**

```rust
async fn list_docs(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, String, chrono::DateTime<Utc>)>(
        "SELECT id, title, project_id, created_at FROM documents WHERE project_id = $1 ORDER BY created_at"
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let docs: Vec<Value> = rows.iter().map(|(id, title, proj_id, created_at)| {
                json!({"id": id, "title": title, "project_id": proj_id, "created_at": created_at.to_rfc3339()})
            }).collect();
            (StatusCode::OK, Json(json!(docs)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn create_doc(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<CreateDoc>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT INTO documents (id, project_id, title) VALUES ($1, $2, $3)"
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&body.title)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({"id": id, "title": body.title, "project_id": project_id}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn update_doc(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateDoc>,
) -> impl IntoResponse {
    let result = sqlx::query("UPDATE documents SET title = $1 WHERE id = $2")
        .bind(&body.title)
        .bind(&id)
        .execute(&state.pool)
        .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"id": id, "title": body.title}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Document not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn delete_doc(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(&id).execute(&state.pool).await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Document not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 2: Update routes**

Replace old engagement-based doc routes with:
```rust
.route("/api/projects/{id}/docs", get(list_docs))
.route("/api/projects/{id}/docs", post(create_doc))
.route("/api/docs/{id}", put(update_doc))
.route("/api/docs/{id}", delete(delete_doc))
```

Remove old `/api/engagements/{id}/docs` and `/api/engagements/{id}/docs/{doc_id}` routes.

**Step 3: Verify and commit**

```bash
cargo check -p celerex-api
git commit -m "feat: move document CRUD under projects with PostgreSQL"
```

---

## Task 5: Add Project Agent CRUD endpoints

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Add handlers**

```rust
#[derive(Debug, Deserialize)]
struct AssignAgent {
    agent_def_id: String,
    #[serde(default)]
    overrides: Option<Value>,
}

async fn list_project_agents(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, String, Option<serde_json::Value>, String, String, serde_json::Value)>(
        "SELECT pa.id, pa.agent_def_id, pa.project_id, pa.overrides, ad.name, ad.category, ad.definition
         FROM project_agents pa
         JOIN agent_defs ad ON pa.agent_def_id = ad.id
         WHERE pa.project_id = $1"
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let agents: Vec<Value> = rows.iter().map(|(id, def_id, proj_id, overrides, name, cat, def)| {
                json!({
                    "id": id, "agent_def_id": def_id, "project_id": proj_id,
                    "overrides": overrides, "agent_name": name,
                    "agent_category": cat, "agent_definition": def,
                })
            }).collect();
            (StatusCode::OK, Json(json!(agents)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn assign_agent_to_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<AssignAgent>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT INTO project_agents (id, project_id, agent_def_id, overrides) VALUES ($1, $2, $3, $4)"
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&body.agent_def_id)
    .bind(&body.overrides)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({"id": id, "project_id": project_id, "agent_def_id": body.agent_def_id}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn update_project_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let result = sqlx::query("UPDATE project_agents SET overrides = $1 WHERE id = $2")
        .bind(&body)
        .bind(&id)
        .execute(&state.pool)
        .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "updated"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Project agent not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn remove_project_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM project_agents WHERE id = $1")
        .bind(&id).execute(&state.pool).await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "removed"}))),
        Ok(_) => (StatusCode::NOT_FOUND, Json(json!({"error": "Project agent not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 2: Register routes**

```rust
.route("/api/projects/{id}/agents", get(list_project_agents))
.route("/api/projects/{id}/agents", post(assign_agent_to_project))
.route("/api/project-agents/{id}", put(update_project_agent))
.route("/api/project-agents/{id}", delete(remove_project_agent))
```

**Step 3: Verify and commit**

```bash
cargo check -p celerex-api
git commit -m "feat: add project agent assignment endpoints"
```

---

## Task 6: Add Global Catalog endpoints (Agent Defs, Tool Defs, Trigger Defs)

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Add agent definition handlers**

```rust
async fn list_agent_defs(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, String, bool, serde_json::Value)>(
        "SELECT id, name, category, is_builtin, definition FROM agent_defs ORDER BY category, name"
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let defs: Vec<Value> = rows.iter().map(|(id, name, cat, builtin, def)| {
                json!({"id": id, "name": name, "category": cat, "is_builtin": builtin, "definition": def})
            }).collect();
            (StatusCode::OK, Json(json!(defs)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn create_agent_def(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let id = body.get("id").and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled");
    let category = body.get("category").and_then(|v| v.as_str()).unwrap_or("document-management");

    let result = sqlx::query(
        "INSERT INTO agent_defs (id, name, category, is_builtin, definition) VALUES ($1, $2, $3, FALSE, $4)
         ON CONFLICT (id) DO UPDATE SET name = $2, category = $3, definition = $4 WHERE agent_defs.is_builtin = FALSE"
    )
    .bind(&id)
    .bind(name)
    .bind(category)
    .bind(&body)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({"id": id, "name": name}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn delete_agent_def(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Only delete non-builtin
    let result = sqlx::query("DELETE FROM agent_defs WHERE id = $1 AND is_builtin = FALSE")
        .bind(&id).execute(&state.pool).await;
    match result {
        Ok(r) if r.rows_affected() > 0 => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Ok(_) => (StatusCode::FORBIDDEN, Json(json!({"error": "Cannot delete built-in agent or agent not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 2: Add tool_defs and trigger_defs handlers**

Same pattern as agent_defs — list, create (INSERT ON CONFLICT), delete (block built-ins).

**Step 3: Register routes**

```rust
.route("/api/agent-defs", get(list_agent_defs))
.route("/api/agent-defs", post(create_agent_def))
.route("/api/agent-defs/{id}", delete(delete_agent_def))
.route("/api/tool-defs", get(list_tool_defs))
.route("/api/tool-defs", post(create_tool_def))
.route("/api/tool-defs/{id}", delete(delete_tool_def))
.route("/api/trigger-defs", get(list_trigger_defs))
.route("/api/trigger-defs", post(create_trigger_def))
.route("/api/trigger-defs/{id}", delete(delete_trigger_def))
```

**Step 4: Verify and commit**

```bash
cargo check -p celerex-api
git commit -m "feat: add global catalog endpoints for agent, tool, and trigger definitions"
```

---

## Task 7: Add Pipeline CRUD and Settings endpoints

**Files:**
- Modify: `packages/backend/api/src/main.rs`

**Step 1: Add pipeline handlers**

```rust
#[derive(Debug, Deserialize)]
struct CreatePipeline {
    name: String,
    #[serde(default)]
    nodes: Value,
    #[serde(default)]
    edges: Value,
}

async fn list_pipelines(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, String, String, serde_json::Value, serde_json::Value)>(
        "SELECT id, project_id, name, nodes, edges FROM pipelines WHERE project_id = $1"
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let pipelines: Vec<Value> = rows.iter().map(|(id, proj_id, name, nodes, edges)| {
                json!({"id": id, "project_id": proj_id, "name": name, "nodes": nodes, "edges": edges})
            }).collect();
            (StatusCode::OK, Json(json!(pipelines)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn create_pipeline(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<CreatePipeline>,
) -> impl IntoResponse {
    let id = Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, nodes, edges) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&body.name)
    .bind(&body.nodes)
    .bind(&body.edges)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(json!({"id": id, "name": body.name, "project_id": project_id}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn update_pipeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        let _ = sqlx::query("UPDATE pipelines SET name = $1 WHERE id = $2")
            .bind(name).bind(&id).execute(&state.pool).await;
    }
    if let Some(nodes) = body.get("nodes") {
        let _ = sqlx::query("UPDATE pipelines SET nodes = $1 WHERE id = $2")
            .bind(nodes).bind(&id).execute(&state.pool).await;
    }
    if let Some(edges) = body.get("edges") {
        let _ = sqlx::query("UPDATE pipelines SET edges = $1 WHERE id = $2")
            .bind(edges).bind(&id).execute(&state.pool).await;
    }
    (StatusCode::OK, Json(json!({"status": "updated"})))
}

async fn delete_pipeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM pipelines WHERE id = $1")
        .bind(&id).execute(&state.pool).await;
    match result {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "deleted"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}
```

**Step 2: Add settings handlers**

```rust
async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT key, value FROM settings"
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let mut settings = json!({});
            for (key, value) in &rows {
                settings[key] = value.clone();
            }
            (StatusCode::OK, Json(settings))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(obj) = body.as_object() {
        for (key, value) in obj {
            let _ = sqlx::query(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = $2"
            )
            .bind(key)
            .bind(value)
            .execute(&state.pool)
            .await;
        }
    }
    (StatusCode::OK, Json(json!({"status": "saved"})))
}
```

**Step 3: Register routes**

```rust
.route("/api/projects/{id}/pipelines", get(list_pipelines))
.route("/api/projects/{id}/pipelines", post(create_pipeline))
.route("/api/pipelines/{id}", put(update_pipeline))
.route("/api/pipelines/{id}", delete(delete_pipeline))
.route("/api/settings", get(get_settings))
.route("/api/settings", put(update_settings))
```

**Step 4: Verify and commit**

```bash
cargo check -p celerex-api
git commit -m "feat: add pipeline CRUD and settings endpoints with PostgreSQL"
```

---

# Phase 2: Frontend Data Model & State

## Task 8: Update frontend types and API client

**Files:**
- Modify: `packages/frontend/app/src/state/atoms.ts`
- Modify: `packages/frontend/app/src/api/client.ts`

**Step 1: Update types in atoms.ts**

Replace old `DocMeta` and `Engagement` with:

```typescript
export interface Engagement {
  id: string;
  name: string;
  createdAt: string;
  projects: Project[];
}

export interface Project {
  id: string;
  name: string;
  code: string;
  engagementId: string;
  createdAt: string;
}

export interface DocMeta {
  id: string;
  title: string;
  projectId: string;
  createdAt: string;
}

export interface ProjectAgent {
  id: string;
  agentDefId: string;
  projectId: string;
  overrides?: {
    systemPrompt?: string;
    model?: string;
    tools?: string[];
    config?: Record<string, any>;
  };
  agentName?: string;
  agentCategory?: string;
}
```

**Step 2: Add project-related atoms**

```typescript
export const activeProjectIdAtom = atom<string | null>(null);
export const activeProjectAtom = atom((get) => {
  const id = get(activeProjectIdAtom);
  if (!id) return null;
  const engagements = get(engagementsAtom);
  for (const e of engagements) {
    const p = e.projects?.find(p => p.id === id);
    if (p) return p;
  }
  return null;
});

export const projectDocsAtom = atom<DocMeta[]>([]);
export const projectAgentsAtom = atom<ProjectAgent[]>([]);
export const projectPipelinesAtom = atom<any[]>([]);
```

**Step 3: Update activeViewAtom**

```typescript
export const activeViewAtom = atom<
  | "editor"
  | "agent-catalog"
  | "project-agents"
  | "workflow-designer"
  | "workflow-monitor"
  | "agent-runner"
  | "task-queue"
  | "agent-triggers"
  | "tools-manager"
  | "solution-builder"
  | "settings"
  | "timeline"
>("editor");
```

**Step 4: Update API client**

Add project, doc (under project), project-agent, pipeline, agent-def, settings endpoints to `client.ts`. See design doc for full API surface.

**Step 5: Commit**

```bash
git commit -m "feat: update frontend types, atoms, and API client for project hierarchy"
```

---

## Task 9: Update persistence and sync hooks

**Files:**
- Modify: `packages/frontend/app/src/hooks/useApiSync.ts`
- Modify: `packages/frontend/app/src/hooks/usePersistence.ts`
- Modify: `packages/frontend/app/src/persistence/storage.ts`

**Step 1: Update useApiSync to fetch projects within engagements**

Fetch engagements, then for each fetch projects. Populate atoms.

**Step 2: Update usePersistence for new atoms**

Add `activeProjectIdAtom` to persisted atoms.

**Step 3: Update IndexedDB schema for project hierarchy**

**Step 4: Commit**

```bash
git commit -m "feat: update sync and persistence for project hierarchy"
```

---

# Phase 3: UI Restructuring

## Task 10: Restructure Sidebar for Engagement → Project → Docs

**Files:**
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx`

Three-level tree: Engagement → Project (name + code badge) → Documents. "New Project" button in each engagement. "New Document" only inside a project. Clicking a project sets `activeProjectIdAtom`.

**Commit:** `git commit -m "feat: restructure sidebar for engagement/project/doc hierarchy"`

---

## Task 11: Rename views and update App.tsx routing

**Files:**
- Modify: `packages/frontend/app/src/App.tsx`
- Rename: `PipelineEditor.tsx` → `WorkflowDesigner.tsx`
- Rename: `AgentDashboard.tsx` → `AgentCatalog.tsx`
- Create: `packages/frontend/app/src/layout/ProjectAgents.tsx`

ProjectAgents view shows agents assigned to the current project with edit/remove/assign controls.

**Commit:** `git commit -m "feat: rename views, add ProjectAgents component, update App routing"`

---

## Task 12: Update WorkflowDesigner to use ProjectAgents

**Files:**
- Modify: `packages/frontend/app/src/layout/WorkflowDesigner.tsx`

Agent palette shows only project-assigned agents. Nodes reference ProjectAgent.id. Save/load pipelines via API.

**Commit:** `git commit -m "feat: update WorkflowDesigner to use project agents and API persistence"`

---

# Phase 4: BMAD Solution Builder

## Task 13: Create Solution Builder wizard component

**Files:**
- Create: `packages/frontend/app/src/layout/SolutionBuilder.tsx`
- Create: `packages/frontend/app/src/data/bmadPhases.ts`

Define 10 BMAD phases with personas and prompt templates. Build stepped wizard UI: phase selection → per-phase chat Q&A → artifact generation → agent suggestions → pipeline scaffolding.

**Commit:** `git commit -m "feat: add BMAD Solution Builder wizard with phase definitions"`

---

# Phase 5: Real-Time Execution Views

## Task 14: Create Workflow Monitor component

**Files:**
- Create: `packages/frontend/app/src/layout/WorkflowMonitor.tsx`

Read-only pipeline view with live node status colors (gray/blue/green/red), edge animations, click-to-inspect output panel. Data from polling jobs API.

**Commit:** `git commit -m "feat: add Workflow Monitor with live pipeline execution view"`

---

## Task 15: Create Agent Runner component

**Files:**
- Create: `packages/frontend/app/src/layout/AgentRunner.tsx`

TUI-style split-pane terminal view. Monospace font, dark background. Agent name + status header per pane. Auto-scroll with pause. Data from polling/SSE.

**Commit:** `git commit -m "feat: add Agent Runner with TUI-style output streaming"`

---

## Task 16: Enhance Task Queue for project scoping

**Files:**
- Modify: `packages/frontend/app/src/layout/TaskQueue.tsx`

Filter by current project. Add columns: pipeline name, agent name, duration. Job detail panel with logs. Retry/cancel buttons.

**Commit:** `git commit -m "feat: enhance Task Queue with project scoping and job details"`

---

# Phase 6: Seed Data & Integration

## Task 17: Seed built-in agents and tools into PostgreSQL on startup

**Files:**
- Modify: `packages/backend/api/src/db.rs`

Add `seed_builtins()` called after migrations. If `agent_defs` table is empty, insert all built-in agents/tools/triggers with `is_builtin = TRUE`. Use `INSERT ... ON CONFLICT DO NOTHING` for idempotency.

**Commit:** `git commit -m "feat: seed built-in agents, tools, and triggers into PostgreSQL on startup"`

---

## Task 18: Wire up full integration and cleanup

**Files:**
- Modify: `packages/frontend/app/src/App.tsx`
- Modify: `packages/frontend/app/src/data/agents.ts`
- Modify: `packages/frontend/app/src/data/tools.ts`
- Modify: `packages/frontend/app/src/data/triggers.ts`

Remove all localStorage persistence for agents, tools, triggers. Update AgentCatalog and ToolsManager to fetch from API. Final end-to-end integration test.

**Commit:** `git commit -m "feat: complete integration — API-backed persistence for all entities"`

---

## Future Tasks (Not In This Plan)

1. **Offline support** — IndexedDB as write-through cache with sync queue
2. **Document content persistence** — Store BlockSuite Y.Doc binary in PostgreSQL
3. **SSE streaming** — Real-time job output streaming from orchestrator
4. **BMAD workflow templates** — Pre-built engagement templates with BMAD phases pre-configured
5. **Multi-user auth** — User accounts, role-based access
6. **Custom BMAD phases** — User-defined phases with custom personas
7. **Kubernetes manifests** — Helm chart for full deployment
