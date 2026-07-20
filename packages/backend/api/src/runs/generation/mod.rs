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
/// to offer a "Continue" affordance on a failed run. `terminal_end_params`
/// below is the single place that maps a `StopReason` to those three values.
///
/// `canceled` (the `AtomicBool` passed to `run_generation_loop`) is still
/// constructed fresh here and never flipped by anything in THIS function —
/// there is no explicit user-initiated cancel API yet. `StopReason::Canceled`
/// nonetheless has a real, reachable producer as of the disconnect-detection
/// fix: `run_generation_loop` also checks the SSE sender's `is_closed()` at
/// the top of every turn, independent of the `canceled` flag. When the
/// browser tab closes mid-run, axum drops the SSE response body, which drops
/// the channel's `Receiver`, which makes `is_closed()` true from that instant
/// on — the loop notices before it would otherwise place another (billable)
/// provider call and stops with `Canceled`. `terminal_end_params` reports
/// that as `end("canceled", 1, Some("SIGTERM"))` (daemon parity: `bus.finish(
/// run, 'canceled', null, 'SIGTERM')`). The frame itself typically still
/// fails to send in exactly this case — the whole point is nobody is
/// listening any more — but the value is in stopping wasted provider calls
/// promptly, not in this specific frame being delivered.
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
        tx,
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
    let (status, code, signal) = terminal_end_params(result.stop_reason);
    emitter.end(status, code, signal).await;
}

/// Pure mapping from a loop's `StopReason` to the terminal `end` frame's
/// `(status, code, signal)`. Test-visible (mirrors `emitter.rs`'s own
/// `*_frame` pure-builder pattern — see its module doc) since the emitted
/// `Event` itself exposes no public getters to assert against after the
/// fact, and this function needs no `PgPool`/`RunEventEmitter` to exercise.
/// `Natural` and `MaxTurns` both report daemon-parity success; `MaxTurns`
/// additionally gets the `"reached step limit"` status frame (above, not
/// here — that is a distinct mid-stream frame, not part of the terminal
/// triple). `Canceled` reports daemon-parity `bus.finish(run, 'canceled',
/// null, 'SIGTERM')` — see this module's doc comment above `run_generation`
/// for how a `Canceled` result reaches here (SSE-consumer disconnect).
fn terminal_end_params(stop_reason: StopReason) -> (&'static str, i32, Option<&'static str>) {
    match stop_reason {
        StopReason::Natural | StopReason::MaxTurns => ("succeeded", 0, None),
        StopReason::Canceled => ("canceled", 1, Some("SIGTERM")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_end_params_natural_and_max_turns_both_report_succeeded() {
        assert_eq!(
            terminal_end_params(StopReason::Natural),
            ("succeeded", 0, None)
        );
        assert_eq!(
            terminal_end_params(StopReason::MaxTurns),
            ("succeeded", 0, None)
        );
    }

    // Important 4 outcome: `Canceled` (an SSE-consumer disconnect, detected
    // by `run_generation_loop` via the sender's `is_closed()`) must map to
    // the daemon-parity `end("canceled", 1, Some("SIGTERM"))` triple, not to
    // the unconditional `end("succeeded", 0, None)` this function replaced.
    #[test]
    fn terminal_end_params_canceled_maps_to_daemon_parity_signal() {
        assert_eq!(
            terminal_end_params(StopReason::Canceled),
            ("canceled", 1, Some("SIGTERM"))
        );
    }
}
