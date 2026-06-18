//! Living Documents v1 — the slot-fill store + template-ownership helper.
//!
//! A `template_slot_fills` row binds a `design_templates` slot to a server-held
//! instruction (the "AI spot") plus the last filled value. The handlers in
//! `design::living` verify `template_engagement(...) == claims.engagement_id`
//! before touching any of these rows (no cross-tenant leak).

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlotFill {
    pub slot_name: String,
    pub instruction: String,
    pub last_value: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

/// The engagement that owns a template (None if the template doesn't exist).
pub async fn template_engagement(pool: &PgPool, template_id: Uuid) -> Result<Option<Uuid>, String> {
    sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM design_templates WHERE id = $1")
        .bind(template_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_instruction(
    pool: &PgPool,
    template_id: Uuid,
    slot: &str,
    instruction: &str,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO template_slot_fills (template_id, slot_name, instruction, provider_id, model)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (template_id, slot_name) DO UPDATE SET instruction=$3, provider_id=$4, model=$5, updated_at=NOW()",
    )
    .bind(template_id)
    .bind(slot)
    .bind(instruction)
    .bind(provider_id)
    .bind(model)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub async fn get_slot_fill(
    pool: &PgPool,
    template_id: Uuid,
    slot: &str,
) -> Result<Option<SlotFill>, String> {
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>)>(
        "SELECT slot_name, instruction, last_value, provider_id, model FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_id)
    .bind(slot)
    .fetch_optional(pool)
    .await
    .map(|o| {
        o.map(|(slot_name, instruction, last_value, provider_id, model)| SlotFill {
            slot_name,
            instruction,
            last_value,
            provider_id,
            model,
        })
    })
    .map_err(|e| e.to_string())
}

pub async fn list_slot_fills(pool: &PgPool, template_id: Uuid) -> Result<Vec<SlotFill>, String> {
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>)>(
        "SELECT slot_name, instruction, last_value, provider_id, model FROM template_slot_fills WHERE template_id=$1 ORDER BY slot_name",
    )
    .bind(template_id)
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|(slot_name, instruction, last_value, provider_id, model)| SlotFill {
                slot_name,
                instruction,
                last_value,
                provider_id,
                model,
            })
            .collect()
    })
    .map_err(|e| e.to_string())
}

pub async fn set_value(
    pool: &PgPool,
    template_id: Uuid,
    slot: &str,
    value: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE template_slot_fills SET last_value=$3, updated_at=NOW() WHERE template_id=$1 AND slot_name=$2",
    )
    .bind(template_id)
    .bind(slot)
    .bind(value)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}
