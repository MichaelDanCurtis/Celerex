//! Living Documents v1 — slot instruction + slots-read endpoints.
//!
//! Each handler is `DeviceAuth`-authenticated and engagement-scoped: it first
//! verifies the template belongs to the caller's engagement (`own_template`)
//! before touching any `template_slot_fills` rows. A template the caller does
//! not own returns 404 (mirroring `design::get_template`) — never leaking its
//! existence or its slot instructions/values.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::extractors::DeviceAuth;
use crate::design::slot_fills;

/// Verify the template exists AND belongs to the caller's engagement.
/// Returns 404 for both "no such template" and "cross-tenant" so a caller can't
/// probe for templates in other engagements (mirrors `design::get_template`).
async fn own_template(
    state: &AppState,
    claims_eng: Uuid,
    template_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    match slot_fills::template_engagement(&state.pool, template_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    {
        Some(eng) if eng == claims_eng => Ok(()),
        _ => Err((StatusCode::NOT_FOUND, "template not found".into())),
    }
}

#[derive(Deserialize)]
pub struct InstructionBody {
    pub instruction: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// `PUT /api/design/templates/{template_id}/slots/{slot}/instruction` — set (or
/// replace) the server-held fill instruction for a slot.
pub async fn set_instruction(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path((template_id, slot)): Path<(Uuid, String)>,
    Json(body): Json<InstructionBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    if body.instruction.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "instruction required".into()));
    }
    slot_fills::upsert_instruction(
        &state.pool,
        template_id,
        &slot,
        &body.instruction,
        body.provider_id.as_deref(),
        body.model.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /api/design/templates/{template_id}/slots` — list the slot fills
/// (instruction + last value) for the template.
pub async fn list_slots(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let fills = slot_fills::list_slot_fills(&state.pool, template_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "slots": fills })))
}
