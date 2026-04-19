use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::AppState;

pub async fn stream(
    State(s): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.run_bus.subscribe(run_id);
    let events = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => {
            let payload = serde_json::to_string(&ev).ok()?;
            Some(Ok(Event::default().data(payload)))
        }
        Err(_) => None,
    });
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
