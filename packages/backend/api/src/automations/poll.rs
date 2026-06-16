//! Poll loop: fires `enqueue_run` for every new connector item that passes
//! the optional substring filter. Uses an at-least-once cursor: the cursor
//! advances per item, so a failed `enqueue_run` stops the advance for that
//! batch and the next poll retries from the last safe position.

use crate::automations::http_api::HttpApiEventSource;
use crate::automations::source::EventSource;
use crate::automations::store::{AutomationTrigger, AutomationTriggersStore};
use crate::notifiers::Notifier;
use crate::project_members::run_job::{self, TRIGGERED_BY_CONNECTOR};
use crate::project_members::store::MembersStore;
use crate::providers::store::ProviderKeysStore;
use celerex_orchestrator::executor::Executor;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

/// Returns `true` when `item` satisfies `filter`.
///
/// * `filter` is `Null` or not an object → matches everything.
/// * `{ "field": "<f>", "contains": "<needle>" }` → `item[f]` (stringified)
///   contains `needle` (case-sensitive substring).
/// * Missing `field` key in item → does **not** match.
pub fn item_matches_filter(item: &Value, filter: &Value) -> bool {
    let obj = match filter.as_object() {
        Some(o) => o,
        None => return true,
    };
    let field = match obj.get("field").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return true,
    };
    let needle = obj.get("contains").and_then(|v| v.as_str()).unwrap_or("");
    let hay = match item.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => return false,
    };
    hay.contains(needle)
}

// ---------------------------------------------------------------------------
// Event source factory
// ---------------------------------------------------------------------------

fn event_source_for(
    connector_id: &str,
    api_key: Option<String>,
) -> Result<Box<dyn EventSource>, String> {
    match connector_id {
        "http-api" => Ok(Box::new(HttpApiEventSource { api_key })),
        other => Err(format!("no event source for connector '{other}'")),
    }
}

// ---------------------------------------------------------------------------
// Poll loop
// ---------------------------------------------------------------------------

pub struct AutomationPollLoop {
    pub store: Arc<dyn AutomationTriggersStore>,
    pub members: Arc<dyn MembersStore>,
    pub provider_keys: Arc<dyn ProviderKeysStore>,
    pub pool: PgPool,
    pub executor: Arc<Executor>,
    pub notifiers: Arc<Vec<Arc<dyn Notifier>>>,
    pub tick: Duration,
}

impl AutomationPollLoop {
    /// Spawn a background task that polls on every `tick` interval.
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.tick);
            loop {
                interval.tick().await;
                let now = Utc::now();
                let due = match self.store.due(now).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::error!(?e, "automation store.due");
                        continue;
                    }
                };
                for t in due {
                    self.poll_one(&t, now).await;
                }
            }
        })
    }

    /// Drive a single trigger: poll → filter → enqueue → advance cursor.
    ///
    /// Public so integration tests can drive a single poll without starting
    /// the ticker.
    pub async fn poll_one(&self, t: &AutomationTrigger, now: DateTime<Utc>) {
        // Resolve engagement_id (needed for enqueue_run + provider key lookup).
        let engagement_id = match self.members.engagement_id_for(t.project_member_id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(trigger=%t.id, error=%e, "resolve engagement");
                return;
            }
        };

        // Look up an optional per-engagement API key for this connector.
        let api_key = self
            .provider_keys
            .get(engagement_id, &t.connector_id)
            .await
            .ok()
            .flatten();

        let source = match event_source_for(&t.connector_id, api_key) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(trigger=%t.id, error=%e, "event source");
                return;
            }
        };

        let (items, new_cursor) = match source.poll(&t.source_config, t.cursor.as_deref()).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(trigger=%t.id, error=%e, "poll failed; will retry next tick");
                return;
            }
        };

        let filter = t.filter.clone().unwrap_or(Value::Null);
        // At-least-once: cursor advances item-by-item. A failed enqueue stops
        // the advance so the item is retried on the next poll.
        let mut advanced: Option<String> = t.cursor.clone();
        for item in items {
            if item_matches_filter(&item.payload, &filter) {
                if let Err(e) = run_job::enqueue_run(
                    self.pool.clone(),
                    self.members.clone(),
                    self.executor.clone(),
                    self.notifiers.clone(),
                    t.project_member_id,
                    engagement_id,
                    TRIGGERED_BY_CONNECTOR,
                )
                .await
                {
                    tracing::error!(trigger=%t.id, error=%e, "enqueue failed; stop advancing cursor");
                    break;
                }
            }
            // Advance past this item regardless of whether it matched the filter.
            advanced = Some(item.id);
        }

        // If the source returned a high-water cursor but no items arrived (e.g.
        // all were already past the cursor), use that as the final position.
        let final_cursor = advanced.or(new_cursor);
        if let Err(e) = self.store.mark_polled(t.id, now, final_cursor).await {
            tracing::error!(trigger=%t.id, error=%e, "mark_polled");
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests (pure — no DB, no network)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_substring_on_field() {
        let item = serde_json::json!({"from":"alice@x.com","subject":"hi"});
        assert!(item_matches_filter(
            &item,
            &serde_json::json!({"field":"from","contains":"alice"})
        ));
        assert!(!item_matches_filter(
            &item,
            &serde_json::json!({"field":"from","contains":"bob"})
        ));
    }

    #[test]
    fn no_filter_matches_all() {
        assert!(item_matches_filter(
            &serde_json::json!({"a":1}),
            &serde_json::Value::Null
        ));
    }

    #[test]
    fn missing_field_does_not_match() {
        assert!(!item_matches_filter(
            &serde_json::json!({"a":1}),
            &serde_json::json!({"field":"from","contains":"x"})
        ));
    }
}
