//! Automations: connector-triggered agent work, hosted on the Agent. An
//! Automation is a `project_member` (the body) plus an `automation_triggers`
//! entry that polls a connector and fires the member when new items arrive.
//! The device seam speaks Design's vocabulary (`AutomationSpec`); this module
//! translates it into the engine's `project_member` graph. See
//! docs/superpowers/specs/2026-06-15-automations-triggered-design.md.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::extractors::DeviceAuth;
use crate::automations::store::{AutomationTrigger, AutomationTriggersStore, PgAutomationTriggersStore};
use crate::project_members::run_job::{self, TRIGGERED_BY_MANUAL};

// Re-use the RitualBody enum + graph builder verbatim: the agent node shape is
// identical (trigger → agent, prompt embedded). AutomationBody is a separate
// type alias so the Design vocabulary stays independent of the scheduler.
pub use super::rituals::RitualBody as AutomationBody;
use super::rituals::build_ritual_graph;

// ===========================================================================
// Spec types
// ===========================================================================

fn default_enabled() -> bool {
    true
}

fn default_poll_interval() -> i32 {
    300
}

/// The full create payload from the daemon for a connector-triggered automation.
#[derive(Debug, Clone, Deserialize)]
pub struct AutomationSpec {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub connector_id: String,
    pub source_config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: i32,
    pub body: AutomationBody,
}

/// What we return for an Automation: the trigger id + the member it fires.
#[derive(Debug, Serialize)]
pub struct AutomationView {
    pub automation_id: Uuid,
    pub project_member_id: Uuid,
    pub name: String,
    pub connector_id: String,
    pub source_config: Value,
    pub filter: Option<Value>,
    pub poll_interval_secs: i32,
    pub enabled: bool,
    pub cursor: Option<String>,
    pub last_polled_at: Option<DateTime<Utc>>,
}

fn view_from(trigger: &AutomationTrigger, name: String) -> AutomationView {
    AutomationView {
        automation_id: trigger.id,
        project_member_id: trigger.project_member_id,
        name,
        connector_id: trigger.connector_id.clone(),
        source_config: trigger.source_config.clone(),
        filter: trigger.filter.clone(),
        poll_interval_secs: trigger.poll_interval_secs,
        enabled: trigger.enabled,
        cursor: trigger.cursor.clone(),
        last_polled_at: trigger.last_polled_at,
    }
}

// ===========================================================================
// HTTP handlers
// ===========================================================================

/// `POST /api/design/automations` — create an Automation (member body + trigger entry).
pub async fn create_automation(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Json(spec): Json<AutomationSpec>,
) -> impl IntoResponse {
    // Validate connector_id is non-empty.
    if spec.connector_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "connector_id must not be empty" }))).into_response();
    }
    // Validate source_config is an object.
    if !spec.source_config.is_object() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "source_config must be a JSON object" }))).into_response();
    }

    let member_id = match &spec.body {
        AutomationBody::FlowRef { project_member_id } => {
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
        AutomationBody::AgentRun { .. } => {
            let project_id = match super::automations_project(&state, claims.engagement_id).await {
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
                    "Automation (connector-triggered).",
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

    let trigger = AutomationTrigger {
        id: Uuid::new_v4(),
        project_member_id: member_id,
        connector_id: spec.connector_id.clone(),
        source_config: spec.source_config.clone(),
        filter: spec.filter.clone(),
        poll_interval_secs: spec.poll_interval_secs,
        cursor: None,
        last_polled_at: None,
        enabled: spec.enabled,
    };
    let store = PgAutomationTriggersStore::new(state.pool.clone());
    match store.create(trigger.clone()).await {
        Ok(_) => {}
        Err(e) => {
            // Rollback the project_member we just created for AgentRun bodies.
            if matches!(spec.body, AutomationBody::AgentRun { .. }) {
                let _ = sqlx::query("DELETE FROM project_members WHERE id = $1")
                    .bind(member_id)
                    .execute(&state.pool)
                    .await;
            }
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
        }
    }

    (StatusCode::CREATED, Json(view_from(&trigger, spec.name))).into_response()
}

/// `GET /api/design/automations` — all Automations for the caller's engagement.
pub async fn list_automations(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Join automation_triggers → project_members → projects to get name + scope.
    type AutoRow = (Uuid, Uuid, String, Value, Option<Value>, i32, Option<String>, Option<DateTime<Utc>>, bool, String);
    let rows = sqlx::query_as::<_, AutoRow>(
        "SELECT a.id, a.project_member_id, a.connector_id, a.source_config, a.filter,
                a.poll_interval_secs, a.cursor, a.last_polled_at, a.enabled, m.name
         FROM automation_triggers a
         JOIN project_members m ON m.id = a.project_member_id
         JOIN projects p ON p.id = m.project_id
         WHERE p.engagement_id = $1
         ORDER BY a.created_at DESC",
    )
    .bind(claims.engagement_id)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let automations: Vec<AutomationView> = rows
                .into_iter()
                .map(|r| AutomationView {
                    automation_id: r.0,
                    project_member_id: r.1,
                    connector_id: r.2,
                    source_config: r.3,
                    filter: r.4,
                    poll_interval_secs: r.5,
                    cursor: r.6,
                    last_polled_at: r.7,
                    enabled: r.8,
                    name: r.9,
                })
                .collect();
            (StatusCode::OK, Json(json!({ "automations": automations }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

/// Guard: load a trigger and confirm its member is in the caller's engagement.
async fn owned_automation(
    state: &AppState,
    engagement_id: Uuid,
    automation_id: Uuid,
) -> Result<AutomationTrigger, (StatusCode, Json<Value>)> {
    let store = PgAutomationTriggersStore::new(state.pool.clone());
    let trigger = store
        .get(automation_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))))?
        .ok_or((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))))?;
    match state.members.engagement_id_for(trigger.project_member_id).await {
        Ok(eng) if eng == engagement_id => Ok(trigger),
        Ok(_) => Err((StatusCode::FORBIDDEN, Json(json!({ "error": "forbidden" })))),
        Err(_) => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })))),
    }
}

/// Payload for `PUT /api/design/automations/{id}`.
#[derive(Debug, Deserialize)]
pub struct UpdateAutomationBody {
    pub source_config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// `PUT /api/design/automations/{id}` — update the trigger (source_config/filter/interval/enabled).
pub async fn update_automation(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(automation_id): Path<Uuid>,
    Json(b): Json<UpdateAutomationBody>,
) -> impl IntoResponse {
    let trigger = match owned_automation(&state, claims.engagement_id, automation_id).await {
        Ok(v) => v,
        Err((s, body)) => return (s, body).into_response(),
    };
    let store = PgAutomationTriggersStore::new(state.pool.clone());
    if let Err(e) = store
        .update(
            automation_id,
            &b.source_config,
            b.filter.as_ref(),
            b.poll_interval_secs,
            b.enabled,
        )
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
    }
    let updated = AutomationTrigger {
        source_config: b.source_config,
        filter: b.filter,
        poll_interval_secs: b.poll_interval_secs,
        enabled: b.enabled,
        ..trigger
    };
    let name = state
        .members
        .get(updated.project_member_id)
        .await
        .ok()
        .flatten()
        .map(|m| m.name)
        .unwrap_or_default();
    (StatusCode::OK, Json(view_from(&updated, name))).into_response()
}

/// `POST /api/design/automations/{id}/run` — fire once, off-schedule.
pub async fn run_automation_now(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(automation_id): Path<Uuid>,
) -> impl IntoResponse {
    let trigger = match owned_automation(&state, claims.engagement_id, automation_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    match run_job::enqueue_run(
        state.pool.clone(),
        state.members.clone(),
        state.executor.clone(),
        state.notifiers.clone(),
        trigger.project_member_id,
        claims.engagement_id,
        TRIGGERED_BY_MANUAL,
    )
    .await
    {
        Ok(run_id) => (StatusCode::ACCEPTED, Json(json!({ "run_id": run_id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    }
}

/// `GET /api/design/automations/{id}/runs` — recent runs for this Automation's member.
pub async fn list_automation_runs(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(automation_id): Path<Uuid>,
) -> impl IntoResponse {
    let trigger = match owned_automation(&state, claims.engagement_id, automation_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    let rows = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
        "SELECT id, status, triggered_by, created_at FROM runs
         WHERE project_member_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(trigger.project_member_id)
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

/// `DELETE /api/design/automations/{id}` — remove the trigger; also remove the
/// member iff it lives in the engagement's "Automations" project (i.e. we
/// created it for an agent_run body). FlowRef members live in the user's own
/// project and are kept.
pub async fn delete_automation(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(automation_id): Path<Uuid>,
) -> impl IntoResponse {
    let trigger = match owned_automation(&state, claims.engagement_id, automation_id).await {
        Ok(v) => v,
        Err((s, b)) => return (s, b).into_response(),
    };
    let store = PgAutomationTriggersStore::new(state.pool.clone());
    if let Err(e) = store.delete(trigger.id).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
    }
    if let Err(e) = sqlx::query(
        "DELETE FROM project_members m USING projects p
         WHERE m.id = $1 AND m.project_id = p.id
           AND p.engagement_id = $2 AND p.name = 'Automations'",
    )
    .bind(trigger.project_member_id)
    .bind(claims.engagement_id)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(
            project_member_id = %trigger.project_member_id,
            error = %e,
            "automation member cleanup failed; trigger deleted, member row may be orphaned"
        );
    }
    StatusCode::NO_CONTENT.into_response()
}
