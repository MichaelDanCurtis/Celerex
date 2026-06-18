//! Living Documents v1 — slot instruction + slots-read endpoints.
//!
//! Each handler is `DeviceAuth`-authenticated and engagement-scoped: it first
//! verifies the template belongs to the caller's engagement (`own_template`)
//! before touching any `template_slot_fills` rows. A template the caller does
//! not own returns 404 (mirroring `design::get_template`) — never leaking its
//! existence or its slot instructions/values.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use celerex_orchestrator::llm::ProviderRegistry;

use crate::AppState;
use crate::auth::extractors::DeviceAuth;
use crate::design::slot_fills;
use crate::providers::store::ProviderKeysStore;

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

/// Default assistant provider/model when a slot carries no explicit override.
/// Mirrors `assistant::chat` exactly so a slot fill behaves like a one-shot chat.
const DEFAULT_PROVIDER_ID: &str = "anthropic";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Extract the completion text from a provider's `raw` response. Supports both
/// the Anthropic shape (`content: [{ text }]`) — the default provider — and the
/// OpenAI/chat-completions shape (`choices[0].message.content`).
fn extract_completion_text(raw: &Value) -> String {
    if let Some(arr) = raw.get("content").and_then(|c| c.as_array()) {
        let joined: String = arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect();
        if !joined.is_empty() {
            return joined;
        }
    }
    raw["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Run a single one-shot completion for a slot. Mirrors `assistant::chat`'s
/// provider-acquisition policy: prefer the engagement's vaulted key, fall back to
/// the house registry only when `CELEREX_HOUSE_MODE` is on, else 409 (no key).
/// A provider/transport error maps to 502 — never a panic.
async fn run_fill(
    provider_keys: &Arc<dyn ProviderKeysStore>,
    registry: &Arc<ProviderRegistry>,
    engagement_id: Uuid,
    instruction: &str,
    slot_type: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<String, (StatusCode, String)> {
    let pid = provider_id.unwrap_or(DEFAULT_PROVIDER_ID).to_string();
    let model = model.unwrap_or(DEFAULT_MODEL).to_string();

    let provider = match crate::assistant::resolve_engagement_provider_with(
        provider_keys,
        engagement_id,
        &pid,
    )
    .await?
    {
        Some(provider) => provider,
        None if crate::assistant::house_mode_enabled() => registry
            .get(&pid)
            .ok_or((StatusCode::BAD_REQUEST, format!("no provider '{pid}'")))?,
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!("no key configured for provider '{pid}'"),
            ));
        }
    };

    let sys = format!(
        "You fill a document slot of type '{slot_type}'. Return ONLY the slot \
         content (for a table or list, return markdown). No preamble, no quotes, \
         no explanation."
    );
    let messages = json!([
        { "role": "system", "content": sys },
        { "role": "user", "content": instruction },
    ]);
    let completion = provider
        .complete(&model, messages, json!([]))
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(extract_completion_text(&completion.raw))
}

/// Look up a slot's declared type from `design_templates.slots` (a JSONB array of
/// `{ name, type, required }`). Defaults to "text" when absent/unparseable.
async fn slot_type(pool: &PgPool, template_id: Uuid, slot: &str) -> String {
    let slots: Option<Value> =
        sqlx::query_scalar::<_, Value>("SELECT slots FROM design_templates WHERE id = $1")
            .bind(template_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    slots
        .as_ref()
        .and_then(|s| s.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|el| el.get("name").and_then(|n| n.as_str()) == Some(slot))
        })
        .and_then(|el| el.get("type").and_then(|t| t.as_str()))
        .unwrap_or("text")
        .to_string()
}

/// Substitute each `{{slot:NAME}}` marker in `template_html` with the slot's
/// last filled value (or the empty string when a slot has no value yet).
fn substitute_slots(template_html: &str, fills: &[slot_fills::SlotFill]) -> String {
    let mut html = template_html.to_string();
    for f in fills {
        let marker = format!("{{{{slot:{}}}}}", f.slot_name);
        let value = f.last_value.as_deref().unwrap_or("");
        html = html.replace(&marker, value);
    }
    html
}

/// `POST /api/design/templates/{template_id}/slots/{slot}/fill` — run a one-shot
/// completion for a single slot and persist the result. 409 if the slot has no
/// instruction.
pub async fn fill_slot(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path((template_id, slot)): Path<(Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let fill = slot_fills::get_slot_fill(&state.pool, template_id, &slot)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "slot has no instruction".into()))?;
    let stype = slot_type(&state.pool, template_id, &slot).await;
    let value = run_fill(
        &state.provider_keys,
        &state.providers_registry,
        claims.engagement_id,
        &fill.instruction,
        &stype,
        fill.provider_id.as_deref(),
        fill.model.as_deref(),
    )
    .await?;
    slot_fills::set_value(&state.pool, template_id, &slot, &value)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "value": value })))
}

/// Fill every instruction'd slot of a template and persist each `last_value`.
///
/// The DRY core shared by the v1 HTTP endpoint (`fill_all`) and the v2 executor
/// (`SlotFillHandler`): lists the template's slot fills, runs each slot's
/// server-held instruction as a one-shot completion (the v1 `run_fill` policy —
/// engagement key first, house fallback when enabled), persists the result via
/// `slot_fills::set_value`, and returns the `(slot_name, value)` pairs in stable
/// (slot-name) order.
///
/// Takes only the collaborators an executor can hold — `pool`, `provider_keys`,
/// `registry` — never `&AppState`. Errors are stringified (the executor maps them
/// to `DispatchError`; the HTTP endpoint maps them to a 500-class response).
pub(crate) async fn fill_template_slots(
    pool: &PgPool,
    provider_keys: &Arc<dyn ProviderKeysStore>,
    registry: &Arc<ProviderRegistry>,
    engagement_id: Uuid,
    template_id: Uuid,
) -> Result<Vec<(String, String)>, String> {
    // Ownership gate (self-contained — the executor path reads `template_id` from
    // node config, so this fn cannot trust the caller to have verified it). A
    // template the run's engagement does not own — or one that doesn't exist —
    // both resolve to "not found" so we never disclose a foreign template's slot
    // instructions/values or overwrite them. Absent here = cross-tenant IDOR.
    match slot_fills::template_engagement(pool, template_id).await? {
        Some(owner) if owner == engagement_id => {}
        _ => return Err("template not found".into()),
    }
    let fills = slot_fills::list_slot_fills(pool, template_id).await?;
    let mut out = Vec::with_capacity(fills.len());
    for f in fills {
        let stype = slot_type(pool, template_id, &f.slot_name).await;
        let value = run_fill(
            provider_keys,
            registry,
            engagement_id,
            &f.instruction,
            &stype,
            f.provider_id.as_deref(),
            f.model.as_deref(),
        )
        .await
        .map_err(|(_, msg)| msg)?;
        slot_fills::set_value(pool, template_id, &f.slot_name, &value).await?;
        out.push((f.slot_name, value));
    }
    Ok(out)
}

/// `POST /api/design/templates/{template_id}/fill` — fill every slot that has an
/// instruction. Each row in `list_slot_fills` has a NOT NULL instruction, so all
/// listed slots are fillable.
pub async fn fill_all(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let pairs = fill_template_slots(
        &state.pool,
        &state.provider_keys,
        &state.providers_registry,
        claims.engagement_id,
        template_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let filled: Vec<Value> = pairs
        .into_iter()
        .map(|(slot, value)| json!({ "slot": slot, "value": value }))
        .collect();
    Ok(Json(json!({ "filled": filled })))
}

/// `GET /api/design/templates/{template_id}/render` — substitute the stored slot
/// values into the template's `{{slot:name}}` markers and return the filled HTML.
pub async fn render(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let template_html =
        sqlx::query_scalar::<_, String>("SELECT template_html FROM design_templates WHERE id = $1")
            .bind(template_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let fills = slot_fills::list_slot_fills(&state.pool, template_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let html = substitute_slots(&template_html, &fills);
    Ok(Json(json!({ "html": html })))
}
