//! Server-side design-generation engine (ported from the daemon's
//! apps/daemon/src/api-generation/). Runs an agentic tool loop against the
//! engagement's in-process vaulted provider, writes files server-side, and
//! streams the daemon SSE vocabulary to the unchanged web SPA.

pub mod artifact_routes;
pub mod artifact_store;
pub mod emitter;
pub mod file_store;
pub mod loop_core;
pub mod provider_turn;
pub mod registry;
pub mod system_prompt;
pub mod template_guard;
pub mod tools;

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::response::sse::Event;
use celerex_orchestrator::llm::Provider;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use self::artifact_store::{generate_artifact_id, insert_artifact};
use self::emitter::RunEventEmitter;
use self::file_store::{FileStore, FileToolSource};
use self::loop_core::{PromptFragment, StopReason, compose_system_prompt, run_generation_loop};
use self::registry::ToolRegistry;
use self::template_guard::render_html_template_v1;
use self::tools::ToolSource;

const DEFAULT_TITLE: &str = "Generated design";

fn derive_title(user_message: &str) -> String {
    let trimmed: String = user_message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if trimmed.is_empty() {
        DEFAULT_TITLE.to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// The `index.ts` analog: wire the registry/emitter/file-store, seed messages,
/// run the loop, validate + persist the produced HTML as one artifact row and
/// emit `live_artifact`, then emit the terminal `end` frame. The caller
/// (`design_chat::stream_pending`) has already emitted `start` and resolved the
/// provider. An artifact-guard rejection surfaces as an `artifact-error` status
/// while the run STILL finishes `succeeded` — load-bearing for the web UI.
///
/// `fragments` are the ordered system-prompt fragments (Brand → Motion → Task),
/// composed by `compose_system_prompt` under `MAX_PROMPT_FRAGMENT_CHARS`
/// (truncate-last). v1 always passes `&[]` (composer empty; base prompt only) —
/// this parameter is plumbing for Brand Kit, the first real producer (B1).
///
/// Terminal-frame parity with the daemon's `runs.ts::finish`/`fail` (the "bus"
/// the daemon's `api-generation/index.ts` terminates through, NOT this crate's
/// unrelated `runs::bus::RunBus` — that's a different run type's event
/// vocabulary entirely): every `end` frame carries `signal`/`resumable`
/// alongside `status`/`code` (see `emitter::end`), which the shipped web SPA
/// (`celerex-design/apps/web/src/providers/daemon.ts`) reads to decide whether
/// to offer a "Continue" affordance on a failed run. `signal` is always `None`
/// on both branches below because v1 wires no cancellation: `canceled` is
/// constructed fresh and never shared, so `StopReason::Canceled` can never
/// actually be returned by the loop here. A future cancellation-wiring task
/// must add a `StopReason::Canceled => emitter.end("canceled", 1,
/// Some("SIGTERM"))` arm once it threads a real, externally-settable flag
/// through — left undone deliberately rather than adding an untested,
/// currently-unreachable branch.
#[allow(clippy::too_many_arguments)]
pub async fn run_generation(
    tx: &Sender<Result<Event, Infallible>>,
    provider: Arc<dyn Provider>,
    provider_id: &str,
    model: &str,
    engagement_id: Uuid,
    project_id: &str,
    run_id: Uuid,
    user_message: &str,
    pool: &PgPool,
    max_turns: usize,
    fragments: &[PromptFragment],
) {
    let _ = provider_id; // reserved for future per-provider tool-shape adaptation
    let emitter = RunEventEmitter::new(tx.clone());
    let file_store = FileStore::new();
    let file_source: Arc<dyn ToolSource> = Arc::new(FileToolSource::new(file_store.clone()));
    let registry = ToolRegistry::new(vec![file_source]);

    let tool_names: Vec<String> = registry
        .schemas()
        .await
        .into_iter()
        .map(|s| s.name)
        .collect();
    // Compose the system prompt from the ordered fragments (slot order, not
    // insertion order; truncate-last to the combined budget). v1 passes an empty
    // list, so `composed.text` is byte-identical to the pre-composer base prompt.
    let composed = compose_system_prompt(&tool_names, fragments);
    if composed.truncated {
        emitter
            .status(
                "prompt-fragment-truncated",
                Some("system-prompt fragments exceeded the combined budget; the last fragment was truncated"),
            )
            .await;
    }
    let seed: Vec<Value> = vec![
        json!({ "role": "system", "content": composed.text }),
        json!({ "role": "user", "content": user_message }),
    ];

    let canceled = AtomicBool::new(false);
    let result = match run_generation_loop(
        seed,
        provider,
        model,
        &registry,
        &emitter,
        &file_store,
        max_turns,
        &canceled,
    )
    .await
    {
        Ok(r) => r,
        Err(message) => {
            // Provider/loop failure: `message` may embed the raw upstream
            // response body verbatim — some providers (`openai_compat.rs`,
            // `copilot.rs`) format their non-2xx errors as
            // `"... status {status}: {raw}"`, and that string propagates
            // unchanged via `?` all the way up from `Provider::complete`
            // through `run_provider_turn`/`run_generation_loop` (see
            // `provider_turn.rs`'s doc comment: "callers own the policy for
            // how much of it to surface"). This IS that policy boundary, so
            // log the full detail server-side only and put a fixed, generic
            // message on the client-visible SSE `error` frame — daemon parity:
            // `ProviderTurnError` in
            // apps/daemon/src/api-generation/provider-turn.ts discards the
            // response body entirely (`Agent generation failed (status
            // ${status})` — status only, no body). We don't attempt to
            // re-extract a status code from the string here: its shape isn't
            // structured and varies per provider, so a best-effort parse would
            // be fragile — a constant message is preferable to a leak.
            tracing::warn!(
                engagement = %engagement_id,
                run_id = %run_id,
                error = %message,
                "design run: provider/loop turn failed"
            );
            emitter.error_event("generation failed").await;
            emitter.end("failed", 1, None).await;
            return;
        }
    };

    if let Some(html) = &result.produced_html {
        match render_html_template_v1(html) {
            Ok(rendered) => {
                let title = derive_title(user_message);
                let artifact_id = generate_artifact_id(&title);
                match insert_artifact(
                    pool,
                    engagement_id,
                    project_id,
                    &artifact_id,
                    &title,
                    html,
                    &rendered,
                    run_id,
                )
                .await
                {
                    Ok(()) => {
                        emitter
                            .live_artifact(project_id, &artifact_id, &title)
                            .await
                    }
                    Err(e) => {
                        tracing::error!(engagement = %engagement_id, error = %e, "design run: artifact persist failed");
                        emitter
                            .status("artifact-error", Some("failed to persist artifact"))
                            .await;
                    }
                }
            }
            // Guard rejection (e.g. <script>, stray {{ }}): surface artifact-error
            // but DO NOT fail the run.
            Err(err) => emitter.status("artifact-error", Some(&err.0)).await,
        }
    }

    if matches!(result.stop_reason, StopReason::MaxTurns) {
        emitter.status("reached step limit", None).await;
    }
    emitter.end("succeeded", 0, None).await;
}
