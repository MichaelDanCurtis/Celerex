use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

pub async fn stream(
    a: crate::auth::extractors::AuthClaims,
    State(s): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    // Provider-plane design-chat run? Serve the web `consumeDaemonRun` event
    // vocabulary instead of the flow-canvas `RunEvent` frames. Single-shot:
    // taking the entry both starts the completion and prevents a second
    // consumer (or the flow lookup below) from matching this id.
    if let Some(pending) = s.design_runs.lock().await.remove(&run_id) {
        if pending.engagement_id != a.0.engagement_id {
            return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response();
        }
        return crate::runs::design_chat::stream_pending(s.clone(), pending).into_response();
    }
    match crate::db::engagement_for_run(&s.pool, run_id).await {
        Ok(Some(eng)) if eng == a.0.engagement_id => {}
        Ok(Some(_)) => return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response(),
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "run not found").into_response(),
        Err(e) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
    let rx = s.run_bus.subscribe(run_id);
    let events = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => {
            let payload = serde_json::to_string(&ev).ok()?;
            Some(Ok::<_, std::convert::Infallible>(Event::default().data(payload)))
        }
        Err(_) => None,
    });
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
