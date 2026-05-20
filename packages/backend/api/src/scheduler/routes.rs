use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::scheduler::store::Schedule;
use crate::{AppState, auth::extractors::AuthClaims};

#[derive(Deserialize)]
pub struct CreateBody {
    pub project_member_id: Uuid,
    pub cron_expr: String,
    pub timezone: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub project_member_id: Uuid,
    pub cron_expr: String,
    pub timezone: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub project_member_id: Uuid,
}

fn default_enabled() -> bool {
    true
}

fn compute_next_fire_at(cron_expr: &str, timezone: &str) -> Result<DateTime<Utc>, String> {
    let tz: Tz = timezone
        .parse()
        .map_err(|_| format!("invalid timezone: {timezone}"))?;
    let sched =
        CronSchedule::from_str(cron_expr).map_err(|e| format!("invalid cron_expr: {e}"))?;
    let next = sched
        .upcoming(tz)
        .next()
        .ok_or_else(|| "cron has no upcoming occurrences".to_string())?;
    Ok(next.with_timezone(&Utc))
}

async fn engagement_for_member(
    s: &AppState,
    project_member_id: Uuid,
) -> Result<Uuid, (StatusCode, axum::Json<serde_json::Value>)> {
    s.members
        .engagement_id_for(project_member_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": e})),
            )
        })
}

async fn load_schedule_pg(
    s: &AppState,
    id: Uuid,
) -> Result<Schedule, (StatusCode, axum::Json<serde_json::Value>)> {
    let store = crate::scheduler::store::PgSchedulesStore::new(s.pool.clone());
    use crate::scheduler::store::SchedulesStore;
    match store.get(id).await {
        Ok(Some(sc)) => Ok(sc),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "not found"})))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )),
    }
}

pub async fn create(
    a: AuthClaims,
    State(s): State<AppState>,
    Json(b): Json<CreateBody>,
) -> impl IntoResponse {
    let engagement_id = match engagement_for_member(&s, b.project_member_id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    if engagement_id != a.0.engagement_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))).into_response();
    }
    let next_fire_at = match compute_next_fire_at(&b.cron_expr, &b.timezone) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    let sched = Schedule {
        id: Uuid::new_v4(),
        project_member_id: b.project_member_id,
        cron_expr: b.cron_expr,
        timezone: b.timezone,
        next_fire_at,
        last_fired_at: None,
        enabled: b.enabled,
    };
    let store = crate::scheduler::store::PgSchedulesStore::new(s.pool.clone());
    use crate::scheduler::store::SchedulesStore;
    match store.create(sched.clone()).await {
        Ok(_) => (StatusCode::CREATED, Json(sched)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn list(
    a: AuthClaims,
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let engagement_id = match engagement_for_member(&s, q.project_member_id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    if engagement_id != a.0.engagement_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))).into_response();
    }
    let store = crate::scheduler::store::PgSchedulesStore::new(s.pool.clone());
    use crate::scheduler::store::SchedulesStore;
    match store.list_for_project_member(q.project_member_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

pub async fn update(
    a: AuthClaims,
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<UpdateBody>,
) -> impl IntoResponse {
    let existing = match load_schedule_pg(&s, id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    let engagement_id = match engagement_for_member(&s, existing.project_member_id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    if engagement_id != a.0.engagement_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))).into_response();
    }
    let next_fire_at = match compute_next_fire_at(&b.cron_expr, &b.timezone) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    };
    let store = crate::scheduler::store::PgSchedulesStore::new(s.pool.clone());
    use crate::scheduler::store::SchedulesStore;
    if let Err(e) = store
        .update(id, &b.cron_expr, &b.timezone, b.enabled, next_fire_at)
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response();
    }
    let updated = Schedule {
        id,
        project_member_id: existing.project_member_id,
        cron_expr: b.cron_expr,
        timezone: b.timezone,
        next_fire_at,
        last_fired_at: existing.last_fired_at,
        enabled: b.enabled,
    };
    (StatusCode::OK, Json(updated)).into_response()
}

pub async fn delete(
    a: AuthClaims,
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let existing = match load_schedule_pg(&s, id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    let engagement_id = match engagement_for_member(&s, existing.project_member_id).await {
        Ok(v) => v,
        Err((status, body)) => return (status, body).into_response(),
    };
    if engagement_id != a.0.engagement_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))).into_response();
    }
    let store = crate::scheduler::store::PgSchedulesStore::new(s.pool.clone());
    use crate::scheduler::store::SchedulesStore;
    match store.delete(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}
