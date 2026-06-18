//! Living Documents v2 — the runtime behind `ExecutionType::SlotFill` nodes.
//!
//! This makes the publish scaffold's placeholder `fill` node real. A `slot_fill`
//! node fills a design template's Living-Doc slots by running each slot's
//! server-held instruction as a one-shot completion (reusing v1's per-slot
//! completion via `living::fill_template_slots`), persists each `last_value`, and
//! emits a flat `{ slot_name: value }` object.
//!
//! Output shape contract (matches the `render` handler): the downstream Render
//! node config is `inputs: { data: "{{steps.fill.outputs}}" }`. The template
//! resolver replaces `{{steps.fill.outputs}}` with this node's WHOLE output, so
//! Render sees `inputs.data == { slot_name: value, ... }` and substitutes each
//! `{{slot:NAME}}` via `data.get("NAME")`. Therefore this handler returns the
//! flat object directly — NOT wrapped in a `{ data: ... }` envelope.
//!
//! Config contract:
//!   { "execution": "slot_fill",
//!     "template_id":   "<uuid>",   // the live template whose slots to fill
//!     "engagement_id": "<uuid>" }  // injected by `apply_doc_engagement`
//!
//! `engagement_id` is the run's TRUE engagement (the executor never trusts a
//! flow-author-pinned value — `apply_doc_engagement` always overwrites it), so a
//! flow cannot fill another tenant's template.

use std::sync::Arc;

use async_trait::async_trait;
use celerex_orchestrator::llm::ProviderRegistry;
use celerex_orchestrator::tool_dispatch::{DispatchError, Handler};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::providers::store::ProviderKeysStore;

pub struct SlotFillHandler {
    pool: PgPool,
    provider_keys: Arc<dyn ProviderKeysStore>,
    registry: Arc<ProviderRegistry>,
}

impl SlotFillHandler {
    pub fn new(
        pool: PgPool,
        provider_keys: Arc<dyn ProviderKeysStore>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            pool,
            provider_keys,
            registry,
        }
    }
}

fn handler_err(msg: impl Into<String>) -> DispatchError {
    DispatchError::Handler(msg.into())
}

fn parse_uuid(config: &Value, key: &str) -> Result<Uuid, DispatchError> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| handler_err(format!("slot_fill: missing `{key}`")))
        .and_then(|s| {
            Uuid::parse_str(s)
                .map_err(|e| handler_err(format!("slot_fill: `{key}` not a uuid: {e}")))
        })
}

#[async_trait]
impl Handler for SlotFillHandler {
    async fn invoke(&self, config: Value, _inputs: Value) -> Result<Value, DispatchError> {
        let template_id = parse_uuid(&config, "template_id")?;
        let engagement_id = parse_uuid(&config, "engagement_id")?;
        let pairs = crate::design::living::fill_template_slots(
            &self.pool,
            &self.provider_keys,
            &self.registry,
            engagement_id,
            template_id,
        )
        .await
        .map_err(|e| handler_err(format!("slot_fill: {e}")))?;
        let mut data = serde_json::Map::with_capacity(pairs.len());
        for (slot, value) in pairs {
            data.insert(slot, json!(value));
        }
        // Flat `{ slot: value }` — becomes the Render node's `inputs.data`.
        Ok(Value::Object(data))
    }
}
