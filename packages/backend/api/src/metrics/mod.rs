//! Workspace metrics summary endpoint.
//!
//! `GET /api/metrics/summary` returns a point-in-time aggregation of
//! engagement-scoped counts (projects, members by agent_type, schedules,
//! runs in the last 24h, docs, plugin installs, configured providers).
//!
//! Engagement scoping comes from the JWT claims (`AuthClaims`). Every count
//! filters on `engagement_id = claims.engagement_id`; joining through the
//! parent table where the counted table doesn't store engagement directly
//! (e.g. `project_members` joins through `projects`, `schedules` through
//! `project_members → projects`).
//!
//! This is intentionally a single point-in-time aggregation — no histograms,
//! no time buckets, no telemetry pipeline. It's the data behind a small
//! dashboard card grid, nothing more.
//!
//! Implementation notes
//! --------------------
//! * One small query per metric, each fully engagement-scoped.
//! * `members_by_agent_type` does a `GROUP BY agent_type` and surfaces the
//!   result as a `HashMap<String, i64>`; absent agent_types are simply
//!   missing keys (so empty engagements return an empty map).
//! * `providers_configured` is read straight from `AppState.providers`, not
//!   the database — it reflects which provider clients booted successfully
//!   in this process, which is the same shape as `GET /api/providers`.
//!
//! All counts are `i64` because Postgres `COUNT(*)` returns `BIGINT`.

use std::collections::HashMap;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{AppState, auth::AuthClaims};

/// Response shape for `GET /api/metrics/summary`.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSummary {
    pub engagements_total: i64,
    pub projects_total: i64,
    pub members_total: i64,
    pub members_by_agent_type: HashMap<String, i64>,
    pub schedules_total: i64,
    pub schedules_enabled: i64,
    pub runs_24h: i64,
    pub runs_succeeded_24h: i64,
    pub runs_failed_24h: i64,
    pub docs_total: i64,
    pub plugins_installed: i64,
    pub providers_configured: Vec<String>,
}

async fn count_projects(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM projects WHERE engagement_id = $1",
    )
    .bind(engagement_id)
    .fetch_one(pool)
    .await
}

async fn count_members(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM project_members pm
         JOIN projects p ON p.id = pm.project_id
         WHERE p.engagement_id = $1",
    )
    .bind(engagement_id)
    .fetch_one(pool)
    .await
}

async fn members_grouped_by_agent_type(
    pool: &PgPool,
    engagement_id: Uuid,
) -> sqlx::Result<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT pm.agent_type, COUNT(*) FROM project_members pm
         JOIN projects p ON p.id = pm.project_id
         WHERE p.engagement_id = $1
         GROUP BY pm.agent_type",
    )
    .bind(engagement_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

async fn count_schedules(
    pool: &PgPool,
    engagement_id: Uuid,
    enabled_only: bool,
) -> sqlx::Result<i64> {
    let sql = if enabled_only {
        "SELECT COUNT(*) FROM schedules s
         JOIN project_members pm ON pm.id = s.project_member_id
         JOIN projects p ON p.id = pm.project_id
         WHERE p.engagement_id = $1 AND s.enabled = TRUE"
    } else {
        "SELECT COUNT(*) FROM schedules s
         JOIN project_members pm ON pm.id = s.project_member_id
         JOIN projects p ON p.id = pm.project_id
         WHERE p.engagement_id = $1"
    };
    sqlx::query_scalar::<_, i64>(sql)
        .bind(engagement_id)
        .fetch_one(pool)
        .await
}

/// Count runs in the last 24h for `engagement_id`. If `status` is `Some`,
/// further filter by `runs.status = $2`.
async fn count_runs_24h(
    pool: &PgPool,
    engagement_id: Uuid,
    status: Option<&str>,
) -> sqlx::Result<i64> {
    match status {
        Some(s) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM runs
                 WHERE engagement_id = $1
                   AND created_at > NOW() - INTERVAL '24 hours'
                   AND status = $2",
            )
            .bind(engagement_id)
            .bind(s)
            .fetch_one(pool)
            .await
        }
        None => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM runs
                 WHERE engagement_id = $1
                   AND created_at > NOW() - INTERVAL '24 hours'",
            )
            .bind(engagement_id)
            .fetch_one(pool)
            .await
        }
    }
}

async fn count_docs(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM documents WHERE engagement_id = $1",
    )
    .bind(engagement_id)
    .fetch_one(pool)
    .await
}

async fn count_plugins(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM plugin_installs WHERE engagement_id = $1",
    )
    .bind(engagement_id)
    .fetch_one(pool)
    .await
}

/// Handler for `GET /api/metrics/summary`. Engagement-scoped via JWT claims.
pub async fn summary(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let engagement_id = claims.engagement_id;
    let pool = &state.pool;

    let projects_total = match count_projects(pool, engagement_id).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let members_total = match count_members(pool, engagement_id).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let members_by_agent_type =
        match members_grouped_by_agent_type(pool, engagement_id).await {
            Ok(m) => m,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    let schedules_total = match count_schedules(pool, engagement_id, false).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let schedules_enabled = match count_schedules(pool, engagement_id, true).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let runs_24h = match count_runs_24h(pool, engagement_id, None).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let runs_succeeded_24h =
        match count_runs_24h(pool, engagement_id, Some("succeeded")).await {
            Ok(n) => n,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };
    let runs_failed_24h = match count_runs_24h(pool, engagement_id, Some("failed")).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let docs_total = match count_docs(pool, engagement_id).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let plugins_installed = match count_plugins(pool, engagement_id).await {
        Ok(n) => n,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let providers_configured: Vec<String> = (*state.providers).clone();

    let body = MetricsSummary {
        engagements_total: 1,
        projects_total,
        members_total,
        members_by_agent_type,
        schedules_total,
        schedules_enabled,
        runs_24h,
        runs_succeeded_24h,
        runs_failed_24h,
        docs_total,
        plugins_installed,
        providers_configured,
    };
    (StatusCode::OK, Json(body)).into_response()
}
