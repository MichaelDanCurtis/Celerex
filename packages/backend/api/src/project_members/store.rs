use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Default agent kind for newly created project members.
///
/// Project Members are reasoning co-workers (Sonnet/Opus-class) by default.
/// The `writer` variant is reserved for Document Servicers (format-bound
/// writers, Haiku-class). Phase 1 only persists the distinction — no LLM
/// routing changes yet.
pub const DEFAULT_AGENT_TYPE: &str = "reasoning";

/// Validate an agent_type string against the DB CHECK constraint.
/// Returns the validated string (still as `&str`) so callers can keep a thin
/// borrow without re-allocating.
pub fn validate_agent_type(s: &str) -> Result<&str, String> {
    match s {
        "reasoning" | "writer" => Ok(s),
        other => Err(format!(
            "invalid agent_type '{other}' (expected 'reasoning' or 'writer')"
        )),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Member {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub graph_json: Value,
    pub provider_config_json: Value,
    pub version: i32,
    pub agent_type: String,
}

#[async_trait]
pub trait MembersStore: Send + Sync {
    async fn create(
        &self,
        project_id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
        agent_type: &str,
    ) -> Result<Member, String>;
    async fn get(&self, id: Uuid, engagement_id: Uuid) -> Result<Option<Member>, String>;
    async fn list_for_project(
        &self,
        project_id: Uuid,
        engagement_id: Uuid,
    ) -> Result<Vec<Member>, String>;
    #[allow(clippy::too_many_arguments)] // engagement_id added for tenant-scoping; param-struct refactor deferred
    async fn update(
        &self,
        id: Uuid,
        engagement_id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
        agent_type: &str,
    ) -> Result<Member, String>;
    async fn snapshot_id_for_current(&self, id: Uuid) -> Result<Uuid, String>;
    async fn engagement_id_for(&self, id: Uuid) -> Result<Uuid, String>;
}

pub struct PgMembersStore {
    pool: PgPool,
}

impl PgMembersStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn write_snapshot(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        member_id: Uuid,
        version: i32,
        graph: &Value,
        provider_config: &Value,
    ) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO project_member_snapshots
               (project_member_id, version, graph_json, provider_config_json)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(member_id)
        .bind(version)
        .bind(graph)
        .bind(provider_config)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl MembersStore for PgMembersStore {
    async fn create(
        &self,
        project_id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
        agent_type: &str,
    ) -> Result<Member, String> {
        let agent_type = validate_agent_type(agent_type)?;
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: (Uuid, i32) = sqlx::query_as(
            "INSERT INTO project_members (project_id, name, description, graph_json, provider_config_json, agent_type)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, version",
        )
        .bind(project_id)
        .bind(name)
        .bind(description)
        .bind(graph)
        .bind(provider_config)
        .bind(agent_type)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let _ = self
            .write_snapshot(&mut tx, row.0, row.1, graph, provider_config)
            .await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(Member {
            id: row.0,
            project_id,
            name: name.into(),
            description: description.into(),
            graph_json: graph.clone(),
            provider_config_json: provider_config.clone(),
            version: row.1,
            agent_type: agent_type.into(),
        })
    }

    async fn get(&self, id: Uuid, engagement_id: Uuid) -> Result<Option<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32, String)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version, agent_type
             FROM project_members
             WHERE id = $1
               AND EXISTS (SELECT 1 FROM projects p WHERE p.id = project_members.project_id AND p.engagement_id = $2)",
        )
        .bind(id)
        .bind(engagement_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())
        .map(|opt| {
            opt.map(|r| Member {
                id: r.0,
                project_id: r.1,
                name: r.2,
                description: r.3,
                graph_json: r.4,
                provider_config_json: r.5,
                version: r.6,
                agent_type: r.7,
            })
        })
    }

    async fn list_for_project(
        &self,
        project_id: Uuid,
        engagement_id: Uuid,
    ) -> Result<Vec<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32, String)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version, agent_type
             FROM project_members
             WHERE project_id = $1
               AND EXISTS (SELECT 1 FROM projects p WHERE p.id = $1 AND p.engagement_id = $2)
             ORDER BY created_at",
        )
        .bind(project_id)
        .bind(engagement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| Member {
                    id: r.0,
                    project_id: r.1,
                    name: r.2,
                    description: r.3,
                    graph_json: r.4,
                    provider_config_json: r.5,
                    version: r.6,
                    agent_type: r.7,
                })
                .collect()
        })
    }

    #[allow(clippy::too_many_arguments)] // engagement_id added for tenant-scoping; param-struct refactor deferred
    async fn update(
        &self,
        id: Uuid,
        engagement_id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
        agent_type: &str,
    ) -> Result<Member, String> {
        let agent_type = validate_agent_type(agent_type)?;
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        // Scope the mutation to the caller's engagement. If the RETURNING yields
        // no row (cross-tenant id / no match), surface the store's not-found
        // error instead of silently succeeding or a generic sqlx error.
        let new_version: Option<i32> = sqlx::query_scalar(
            "UPDATE project_members
               SET name = $3, description = $4, graph_json = $5,
                   provider_config_json = $6, agent_type = $7,
                   version = version + 1, updated_at = NOW()
             WHERE id = $1
               AND EXISTS (SELECT 1 FROM projects p WHERE p.id = project_members.project_id AND p.engagement_id = $2)
             RETURNING version",
        )
        .bind(id)
        .bind(engagement_id)
        .bind(name)
        .bind(description)
        .bind(graph)
        .bind(provider_config)
        .bind(agent_type)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let new_version = new_version.ok_or_else(|| "not found".to_string())?;
        let _ = self
            .write_snapshot(&mut tx, id, new_version, graph, provider_config)
            .await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        let member = self
            .get(id, engagement_id)
            .await?
            .ok_or("member vanished")?;
        Ok(member)
    }

    async fn snapshot_id_for_current(&self, id: Uuid) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT s.id FROM project_member_snapshots s
             JOIN project_members m
               ON m.id = s.project_member_id AND m.version = s.version
             WHERE m.id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())
    }

    async fn engagement_id_for(&self, id: Uuid) -> Result<Uuid, String> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT p.engagement_id
             FROM project_members m
             JOIN projects p ON p.id = m.project_id
             WHERE m.id = $1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())
    }
}
