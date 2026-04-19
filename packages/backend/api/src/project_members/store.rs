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
    async fn create(
        &self,
        project_id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
    ) -> Result<Member, String>;
    async fn get(&self, id: Uuid) -> Result<Option<Member>, String>;
    async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Member>, String>;
    async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
    ) -> Result<Member, String>;
    async fn snapshot_id_for_current(&self, id: Uuid) -> Result<Uuid, String>;
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
    ) -> Result<Member, String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: (Uuid, i32) = sqlx::query_as(
            "INSERT INTO project_members (project_id, name, description, graph_json, provider_config_json)
             VALUES ($1, $2, $3, $4, $5) RETURNING id, version",
        )
        .bind(project_id)
        .bind(name)
        .bind(description)
        .bind(graph)
        .bind(provider_config)
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
        })
    }

    async fn get(&self, id: Uuid) -> Result<Option<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version
             FROM project_members WHERE id = $1",
        )
        .bind(id)
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
            })
        })
    }

    async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Member>, String> {
        sqlx::query_as::<_, (Uuid, Uuid, String, String, Value, Value, i32)>(
            "SELECT id, project_id, name, description, graph_json, provider_config_json, version
             FROM project_members WHERE project_id = $1 ORDER BY created_at",
        )
        .bind(project_id)
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
                })
                .collect()
        })
    }

    async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: &str,
        graph: &Value,
        provider_config: &Value,
    ) -> Result<Member, String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let new_version: i32 = sqlx::query_scalar(
            "UPDATE project_members
               SET name = $2, description = $3, graph_json = $4,
                   provider_config_json = $5, version = version + 1, updated_at = NOW()
             WHERE id = $1 RETURNING version",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(graph)
        .bind(provider_config)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let _ = self
            .write_snapshot(&mut tx, id, new_version, graph, provider_config)
            .await?;
        tx.commit().await.map_err(|e| e.to_string())?;
        let member = self.get(id).await?.ok_or("member vanished")?;
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
}
