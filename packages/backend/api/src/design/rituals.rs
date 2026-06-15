//! Rituals: scheduled agent work, hosted on the Agent. A Ritual is a
//! `project_member` (the body) plus a scheduler entry that fires it on cron.
//! The device seam speaks Design's vocabulary (`RitualSpec`); this module
//! translates it into the engine's `project_member` graph. See
//! docs/superpowers/specs/2026-06-14-rituals-via-agent-design.md.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::auth::extractors::DeviceAuth;
use crate::scheduler::routes::compute_next_fire_at;
use crate::scheduler::store::{PgSchedulesStore, Schedule, SchedulesStore};

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

// ===========================================================================
// HTTP handlers
// ===========================================================================

/// What we return for a Ritual: the scheduler entry id + the member it fires.
#[derive(Debug, Serialize)]
pub struct RitualView {
    pub ritual_id: Uuid,
    pub project_member_id: Uuid,
    pub name: String,
    pub cron_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub next_fire_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
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
    let next_fire_at = match compute_next_fire_at(&spec.schedule.cron_expr, &spec.schedule.timezone) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };

    let member_id = match &spec.body {
        RitualBody::FlowRef { project_member_id } => {
            match state.members.engagement_id_for(*project_member_id).await {
                Ok(eng) if eng == claims.engagement_id => *project_member_id,
                Ok(_) => {
                    return (StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" }))).into_response();
                }
                Err(_) => {
                    return (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" }))).into_response();
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
                .create(
                    project_id,
                    &spec.name,
                    "Ritual (scheduled agent run).",
                    &graph,
                    &provider_config,
                    "writer",
                )
                .await
            {
                Ok(m) => m.id,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
                }
            }
        }
    };

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
        // Rollback the project_member we just created for AgentRun bodies.
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

/// Row shape for the Ritual list query: schedule fields plus the member name,
/// in SELECT order. Kept as an alias so the `query_as` mapping is auditable.
type RitualRow = (Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, bool, String);

/// `GET /api/design/rituals` — all Rituals for the caller's engagement.
pub async fn list_rituals(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, RitualRow>(
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

/// `DELETE /api/design/rituals/{id}` — remove the schedule; also remove the
/// member iff it lives in the engagement's "Rituals" project (i.e. we created it
/// for an agent_run). FlowRef members live in the user's own project and are kept.
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
    if let Err(e) = sqlx::query(
        "DELETE FROM project_members m USING projects p
         WHERE m.id = $1 AND m.project_id = p.id
           AND p.engagement_id = $2 AND p.name = 'Rituals'",
    )
    .bind(schedule.project_member_id)
    .bind(claims.engagement_id)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(project_member_id = %schedule.project_member_id, error = %e, "ritual member cleanup failed; schedule deleted, member row may be orphaned");
    }
    StatusCode::NO_CONTENT.into_response()
}
