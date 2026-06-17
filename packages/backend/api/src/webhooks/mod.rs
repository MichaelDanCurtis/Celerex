pub mod signature;

use axum::{extract::{Path, State}, http::{HeaderMap, StatusCode}, response::IntoResponse, body::Bytes, Json};
use celerex_orchestrator::job::{JobRequest, Step};
use serde_json::json;
use uuid::Uuid;
use crate::AppState;

pub async fn ingest(
    State(s): State<AppState>,
    // `token` is the per-install inbound secret — treat as a credential: never log it.
    Path((token, event)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Resolve the install by its secret token. An unknown token yields no install row; this
    // also routes the webhook to the correct engagement (no cross-tenant `plugin_id LIMIT 1`).
    let install: Option<(Uuid, Uuid, serde_json::Value, String)> = sqlx::query_as(
        "SELECT id, engagement_id, manifest, plugin_id FROM plugin_installs WHERE webhook_token = $1"
    ).bind(&token).fetch_optional(&s.pool).await.unwrap_or(None);

    let Some((install_id, engagement_id, manifest, plugin_id)) = install else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "unknown webhook" }))).into_response();
    };

    let event_id = headers.get("x-event-id").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    if let Some(id) = &event_id {
        // Key the dedupe entry on the install id (not the secret token) so replays are
        // deduped without persisting the credential.
        let dup_key = format!("{install_id}:{event}:{id}");
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

    let run_id = Uuid::new_v4();
    let _ = sqlx::query("INSERT INTO runs (id, engagement_id, trigger) VALUES ($1, $2, $3)")
        .bind(run_id).bind(engagement_id).bind(&trigger).execute(&s.pool).await;

    if let Some(pipeline) = manifest.get("on_event_pipeline").and_then(|p| p.get(&event)) {
        let steps_json = pipeline.get("steps").cloned().unwrap_or(json!([]));
        if let Ok(steps) = serde_json::from_value::<Vec<Step>>(steps_json) {
            let req = JobRequest { run_id, engagement_id, project_id: None, steps };
            let exec = s.executor.clone();
            tokio::spawn(async move {
                let _ = exec.run(req).await;
            });
        }
    }

    (StatusCode::ACCEPTED, Json(json!({ "received": true }))).into_response()
}
