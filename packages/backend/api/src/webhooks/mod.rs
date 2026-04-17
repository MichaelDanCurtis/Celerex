pub mod signature;

use axum::{extract::{Path, State}, http::{HeaderMap, StatusCode}, response::IntoResponse, body::Bytes, Json};
use serde_json::json;
use crate::AppState;

pub async fn ingest(
    State(s): State<AppState>,
    Path((plugin_id, event)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let event_id = headers.get("x-event-id").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(id) = &event_id {
        let dup_key = format!("{plugin_id}:{event}:{id}");
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM webhook_dedupe WHERE id = $1")
            .bind(&dup_key).fetch_optional(&s.pool).await.unwrap_or(None);
        if exists.is_some() {
            return (StatusCode::OK, Json(json!({ "deduped": true }))).into_response();
        }
        let _ = sqlx::query("INSERT INTO webhook_dedupe (id) VALUES ($1)")
            .bind(&dup_key).execute(&s.pool).await;
    }
    let trigger = json!({
        "kind": "event",
        "plugin_id": plugin_id,
        "event": event,
        "payload": serde_json::from_slice::<serde_json::Value>(&body).unwrap_or(json!(null)),
    });
    let _ = sqlx::query(
        "INSERT INTO runs (engagement_id, trigger)
         VALUES ((SELECT engagement_id FROM plugin_installs WHERE plugin_id = $1 LIMIT 1), $2)"
    ).bind(&plugin_id).bind(&trigger).execute(&s.pool).await;
    (StatusCode::ACCEPTED, Json(json!({ "received": true }))).into_response()
}
