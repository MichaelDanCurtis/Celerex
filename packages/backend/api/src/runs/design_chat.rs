//! Server-side, provider-plane design-chat Run.
//!
//! The web design app posts a conversation turn to `POST /api/runs` and then
//! consumes `GET /api/runs/{id}/events` as an SSE stream (see the web
//! `consumeDaemonRun` in apps/web/src/providers/daemon.ts). Historically that
//! stream was served by the local daemon, which spawned a code-agent CLI. On
//! the one server (the Agent) a Run instead runs the server-side generation
//! engine (`runs::generation::run_generation`, Task 9: an ordered tool-call
//! loop against the per-engagement vaulted provider — the same
//! vaulted-key resolution the assistant chat endpoint uses) and emits the
//! same web event vocabulary (`start` -> `agent`{`text_delta`/`tool_use`/
//! `tool_result`/`usage`/`live_artifact`} -> `end`). No daemon, no CLI.
//!
//! Race-free by construction: `POST` only REGISTERS the pending run and returns
//! its id; the run starts lazily when the client connects to the SSE stream, so
//! a fast run can never finish before the consumer has subscribed. The
//! registry entry is single-shot — removed when the stream starts — so a
//! mid-stream disconnect cannot be reattached (acceptable for a short,
//! non-resumable run; the web only reconnects on a dropped stream, which a
//! one-shot run does not produce).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::AppState;
use crate::assistant::resolve_engagement_provider;
use crate::auth::extractors::AuthClaims;

/// A run registered by `POST /api/runs`, awaiting its SSE consumer. Carries only
/// what `run_generation` needs; `engagement_id` is captured from the creator's
/// claims and re-checked against the streamer's claims for cross-engagement
/// isolation.
#[derive(Clone)]
pub struct PendingDesignRun {
    pub engagement_id: Uuid,
    pub provider_id: String,
    pub model: String,
    pub project_id: String,
    /// The user's turn (flattened transcript) seeded into the generation loop.
    pub user_message: String,
}

/// In-memory registry of pending design-chat runs, keyed by run id. Entries are
/// removed when their SSE stream starts (single-shot). Held in `AppState`.
pub type DesignRuns = Arc<Mutex<HashMap<Uuid, PendingDesignRun>>>;

pub fn new_registry() -> DesignRuns {
    Arc::new(Mutex::new(HashMap::new()))
}

/// How long a registered-but-unstreamed run lingers before the registry drops
/// it. The web opens the SSE stream immediately after POST, so this only
/// reclaims abandoned runs — a client crash, a navigation away, or an abusive
/// POST loop — keeping the in-memory map bounded for multi-tenant safety.
const PENDING_RUN_TTL: Duration = Duration::from_secs(60);

/// Subset of the web `ChatRequest` the provider-plane run consumes. The extra
/// fields the daemon used (skills, attachments, plugin snapshots, ...) are
/// ignored on the v1 completion path; unknown keys are dropped by serde.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunRequest {
    /// Full flattened transcript (daemon `buildDaemonTranscript`). Used as the
    /// user turn so prior conversation context is preserved.
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub current_prompt: Option<String>,
    /// Unified-plane provider id (`anthropic` / `openai` / `gemini` / `kimi` /
    /// `copilot` / `codex`). The web sends this from the active
    /// `<provider>/<model>` selection.
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Client-supplied logical project identifier for this generation run
    /// (persisted on `design_artifacts` + used to scope live-artifact
    /// lookups). Falls back to the run id when absent, so every run still
    /// gets a stable, unique project scope.
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunResponse {
    pub run_id: String,
}

/// `POST /api/runs` — register a provider-plane design-chat run and return its
/// id. Does NOT start the completion (see module docs: lazy-on-connect avoids
/// the POST->GET race). Auth: engagement captured from the caller's claims.
pub async fn create(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Json(body): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, (StatusCode, String)> {
    let provider_id = body
        .provider_id
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "anthropic".to_string());
    let model = body
        .model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    // Fail fast on a provider the plane can't resolve (rather than registering a
    // run that is guaranteed to error at stream time).
    if !crate::providers::store::SUPPORTED_PROVIDERS.contains(&provider_id.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported provider '{provider_id}'"),
        ));
    }

    // The flattened transcript is the richest context; fall back to the latest
    // user turn, then empty. Seeded as the user turn in `run_generation`'s
    // conversation — the exact shape `/api/assistant/chat` proves works across
    // every provider.
    let content = body
        .message
        .filter(|m| !m.trim().is_empty())
        .or(body.current_prompt)
        .unwrap_or_default();

    let run_id = Uuid::new_v4();
    let project_id = body
        .project_id
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| run_id.to_string());

    state.design_runs.lock().await.insert(
        run_id,
        PendingDesignRun {
            engagement_id: claims.engagement_id,
            provider_id,
            model,
            project_id,
            user_message: content,
        },
    );

    // Reclaim the entry if its SSE consumer never connects (bounds the registry
    // against client crashes / abusive POST loops). Streaming removes the entry
    // first, so this is a no-op for the normal POST-then-GET path.
    let registry = state.design_runs.clone();
    tokio::spawn(async move {
        tokio::time::sleep(PENDING_RUN_TTL).await;
        registry.lock().await.remove(&run_id);
    });

    Ok(Json(CreateRunResponse {
        run_id: run_id.to_string(),
    }))
}

/// Drive a registered run to completion through the server-side generation
/// engine (Task 9 `run_generation`: the ordered tool-call loop, port of the
/// daemon's `api-generation/index.ts`) and stream the result over the web
/// `consumeDaemonRun` event vocabulary. Called from `runs::sse::stream` once
/// it has matched (and engagement-checked) the run id.
pub fn stream_pending(
    state: AppState,
    run_id: Uuid,
    pending: PendingDesignRun,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let bin = format!("{}/{}", pending.provider_id, pending.model);
        if tx
            .send(Ok(named(
                "start",
                json!({ "bin": bin, "projectId": pending.project_id, "model": pending.model }),
            )))
            .await
            .is_err()
        {
            return;
        }

        // Resolve the engagement's vaulted credential through the unified plane.
        let provider =
            match resolve_engagement_provider(&state, pending.engagement_id, &pending.provider_id)
                .await
            {
                Ok(Some(p)) => p,
                // Fail closed: a design-chat Run requires the engagement's OWN
                // vaulted key. Unlike `/api/assistant/chat`, it intentionally
                // does NOT fall back to the shared house registry
                // (CELEREX_HOUSE_MODE) — the multi-tenancy hardening is removing
                // that fallback, and a per-engagement Run must never silently
                // run on the operator's key.
                Ok(None) => {
                    send_error(
                        &tx,
                        format!("no key configured for provider '{}'", pending.provider_id),
                    )
                    .await;
                    return;
                }
                // Don't leak key-store / sqlx internals (table, column, or
                // constraint names) into a client-readable SSE frame: log
                // server-side and surface a generic message.
                Err((_, e)) => {
                    tracing::error!(
                        engagement = %pending.engagement_id,
                        provider = %pending.provider_id,
                        error = %e,
                        "design-chat run: provider key store error"
                    );
                    send_error(&tx, "provider key store error".to_string()).await;
                    return;
                }
            };

        crate::runs::generation::run_generation(
            &tx,
            provider,
            &pending.provider_id,
            &pending.model,
            pending.engagement_id,
            &pending.project_id,
            run_id,
            &pending.user_message,
            &state.pool,
            20,
            &[], // v1: system-prompt fragment composer ships EMPTY (Brand Kit is the first producer)
        )
        .await;
    });

    let sse_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Sse::new(sse_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// A named SSE event carrying a JSON object body, matching the daemon frames the
/// web `parseSseFrame` + `consumeDaemonRun` expect.
fn named(name: &str, data: Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

/// Emit the terminal failure pair the web understands: a structured `error`
/// frame followed by a `failed` `end` frame. Used only for the pre-loop
/// failures above (no engagement key / key-store error) — once
/// `run_generation` takes over, ITS OWN terminal `end` frame (via
/// `RunEventEmitter::end`) is what ends the stream. The `end` body here
/// carries the same `signal`/`resumable` keys `run_generation`'s does (always
/// `null`/`false` — no cancellation applies before the loop even starts), so
/// the shipped web SPA's resume affordance behaves identically regardless of
/// which stage the run failed in.
async fn send_error(tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>, message: String) {
    let _ = tx
        .send(Ok(named(
            "error",
            json!({ "error": { "message": message } }),
        )))
        .await;
    let _ = tx
        .send(Ok(named(
            "end",
            json!({ "status": "failed", "code": 1, "signal": null, "resumable": false }),
        )))
        .await;
}
