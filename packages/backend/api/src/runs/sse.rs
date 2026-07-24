use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::AppState;
use crate::runs::design_chat::{DesignRuns, PendingDesignRun};

pub async fn stream(
    a: crate::auth::extractors::AuthClaims,
    State(s): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    // Provider-plane design-chat run? Serve the web `consumeDaemonRun` event
    // vocabulary instead of the flow-canvas `RunEvent` frames. Single-shot:
    // a successful claim both starts the completion and prevents a second
    // consumer (or the flow lookup below) from matching this id.
    match claim_pending_design_run(&s.design_runs, run_id, a.0.engagement_id).await {
        Ok(Some(pending)) => {
            return crate::runs::design_chat::stream_pending(s.clone(), run_id, pending)
                .into_response();
        }
        Err(()) => {
            return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response();
        }
        Ok(None) => {}
    }
    match crate::db::engagement_for_run(&s.pool, run_id).await {
        Ok(Some(eng)) if eng == a.0.engagement_id => {}
        Ok(Some(_)) => return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response(),
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, "run not found").into_response(),
        Err(e) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    }
    let rx = s.run_bus.subscribe(run_id);
    let events = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => {
            let payload = serde_json::to_string(&ev).ok()?;
            Some(Ok::<_, std::convert::Infallible>(
                Event::default().data(payload),
            ))
        }
        Err(_) => None,
    });
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

/// Claim a pending design-chat run for `run_id` on behalf of
/// `caller_engagement_id`. Authorization is checked BEFORE the entry is
/// consumed: a caller from a DIFFERENT engagement (a valid token, but the
/// wrong one, holding or guessing this run's v4 UUID) must not be able to
/// destroy the legitimate owner's pending run just by probing it — that
/// would 404 the real owner's own subsequent connect (Minor 6 fix; the prior
/// version of this function removed unconditionally and checked ownership
/// only afterward, so a wrong-engagement probe still consumed the victim's
/// entry on its way to returning `Err`). The whole check-then-remove
/// decision happens under a single lock acquisition, so two concurrent
/// legitimate claims for the same id can't race each other into a
/// double-remove either.
async fn claim_pending_design_run(
    design_runs: &DesignRuns,
    run_id: uuid::Uuid,
    caller_engagement_id: uuid::Uuid,
) -> Result<Option<PendingDesignRun>, ()> {
    let mut guard = design_runs.lock().await;
    match guard.get(&run_id) {
        Some(pending) if pending.engagement_id == caller_engagement_id => Ok(guard.remove(&run_id)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pending(engagement_id: Uuid) -> PendingDesignRun {
        PendingDesignRun {
            engagement_id,
            provider_id: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            project_id: "proj-1".to_string(),
            user_message: "hi".to_string(),
        }
    }

    #[tokio::test]
    async fn owner_claims_and_removes_single_shot() {
        let registry = crate::runs::design_chat::new_registry();
        let run_id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        registry.lock().await.insert(run_id, sample_pending(owner));

        let claimed = claim_pending_design_run(&registry, run_id, owner).await;
        assert!(matches!(claimed, Ok(Some(_))));
        // Single-shot: gone after the first successful claim.
        assert!(registry.lock().await.get(&run_id).is_none());
    }

    // Minor 6 regression: a caller from a DIFFERENT engagement (a valid
    // token, but the wrong one) probing/guessing another engagement's
    // v4-UUID run id must be forbidden WITHOUT destroying the victim's
    // pending run. Against the unconditional-remove-first code above, this
    // fails (RED) — `still_there` is `None` because the entry was already
    // removed before the ownership mismatch was even noticed.
    #[tokio::test]
    async fn wrong_engagement_is_forbidden_and_leaves_entry_intact() {
        let registry = crate::runs::design_chat::new_registry();
        let run_id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let attacker = Uuid::new_v4();
        registry.lock().await.insert(run_id, sample_pending(owner));

        let claimed = claim_pending_design_run(&registry, run_id, attacker).await;
        assert!(matches!(claimed, Err(())));
        let guard = registry.lock().await;
        let still_there = guard.get(&run_id);
        assert!(
            still_there.is_some(),
            "an unauthorized probe from another engagement must not consume the victim's pending run"
        );
        assert_eq!(still_there.unwrap().engagement_id, owner);
    }

    #[tokio::test]
    async fn missing_run_id_falls_through_for_flow_canvas_lookup() {
        let registry = crate::runs::design_chat::new_registry();
        let run_id = Uuid::new_v4();
        let claimed = claim_pending_design_run(&registry, run_id, Uuid::new_v4()).await;
        assert!(matches!(claimed, Ok(None)));
    }
}
