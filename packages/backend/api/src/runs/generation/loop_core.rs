//! Orchestration core of the server-side design-generation loop. Port of
//! daemon `loop.ts`: loop one provider turn at a time, emitting SSE for
//! text/tool-use/tool-result/usage, dispatching tool calls through the
//! `ToolRegistry` and feeding results back into the conversation. Terminates
//! on natural stop (model returns no tool calls), a `max_turns` safety cap, an
//! already-set cancel flag, or the SSE consumer having disconnected. Tool
//! errors are fed back to the model as the tool_result content (`is_error:
//! true`) and never abort the loop — matching the daemon's non-abort
//! semantics. Every dispatched tool_call unconditionally gets both an emitted
//! `tool_result` SSE frame and a `role:"tool"` transcript entry (even on
//! error), avoiding orphan tool_call_ids that some providers reject with a
//! 400.
//!
//! **Disconnect detection.** A closed browser tab drops the SSE response
//! body, which drops the `Receiver` half of the channel `RunEventEmitter`
//! sends into; the paired `Sender::is_closed()` then reports `true` from that
//! instant on. The loop checks it at the top of every turn (alongside the
//! pre-existing `canceled` flag) and stops with `StopReason::Canceled` rather
//! than placing another (billable) provider call into the void. This is not a
//! heuristic: for a `tokio::sync::mpsc` channel, `is_closed()` and a failed
//! `send` share exactly one cause — every receiver dropped — so a real
//! provider/tool error is never misclassified as a cancellation.
//!
//! This module does NOT itself emit the "artifact-error" / "reached step
//! limit" status literals — matching the daemon's separation of concerns,
//! where `loop.ts` only returns a `stopReason` and `index.ts` (here, the
//! Task 9 caller) inspects it after the loop returns to decide what to emit.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::response::sse::Event;
use celerex_orchestrator::llm::Provider;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

use super::emitter::RunEventEmitter;
use super::file_store::FileStore;
use super::provider_turn::run_provider_turn;
use super::registry::ToolRegistry;

pub use super::system_prompt::{
    ComposedPrompt, FragmentSlot, MAX_PROMPT_FRAGMENT_CHARS, PromptFragment, build_system_prompt,
    compose_system_prompt,
};

#[derive(Debug, Clone, Copy)]
pub enum StopReason {
    Natural,
    MaxTurns,
    Canceled,
}

pub struct LoopResult {
    pub final_text: String,
    pub produced_html: Option<String>,
    pub stop_reason: StopReason,
}

/// Orchestration core: loop one provider turn at a time, emitting SSE for
/// text/tool-use/tool-result/usage, dispatching tool calls and feeding results
/// back. Terminates on natural stop (no tool calls), a maxTurns cap, an
/// already-set cancel flag, or the SSE sender closing (client disconnect).
/// Tool errors are fed back as tool_result content (is_error=true) and never
/// abort the loop. Port of daemon `loop.ts`.
// The original 8-parameter signature was the fixed cross-task interface
// (Task 9 calls this directly; see the task brief's `Interfaces` section)
// rather than an accidental pile-up; `tx` was added as a 9th purely for
// disconnect detection (see the module doc's "Disconnect detection"
// paragraph) — still a deliberate, named parameter list, not a pile-up, so
// this keeps the default clippy threshold rather than wrapping the params in
// a bespoke struct that only this one function would use.
#[allow(clippy::too_many_arguments)]
pub async fn run_generation_loop(
    seed: Vec<Value>,
    provider: Arc<dyn Provider>,
    model: &str,
    registry: &ToolRegistry,
    emitter: &RunEventEmitter,
    file_store: &FileStore,
    max_turns: usize,
    canceled: &AtomicBool,
    tx: &Sender<Result<Event, Infallible>>,
) -> Result<LoopResult, String> {
    let schemas = registry.schemas().await;
    let mut convo = seed;
    let mut final_text = String::new();
    let mut stop_reason = StopReason::MaxTurns;

    for _turn in 0..max_turns {
        // `tx.is_closed()` is true iff every `Receiver` for this channel has
        // been dropped — for a browser-tab-closed SSE consumer, that happens
        // when axum drops the response body and, with it, the `Receiver`
        // `RunEventEmitter` sends into. Checked here (not just the `canceled`
        // flag) so the loop notices a disconnect BEFORE placing another
        // (billable) provider call — see the module doc's "Disconnect
        // detection" paragraph for why this can't misclassify a real error.
        if canceled.load(Ordering::Relaxed) || tx.is_closed() {
            stop_reason = StopReason::Canceled;
            break;
        }

        let result =
            run_provider_turn(&*provider, model, Value::Array(convo.clone()), &schemas).await?;
        if !result.text.is_empty() {
            emitter.text_delta(&result.text).await;
            final_text = result.text.clone();
        }
        if result.input_tokens.is_some() || result.output_tokens.is_some() {
            emitter
                .usage(result.input_tokens, result.output_tokens)
                .await;
        }

        if result.tool_calls.is_empty() {
            stop_reason = StopReason::Natural;
            break;
        }

        // Assistant turn carrying the tool calls (OpenAI-shaped, mirroring loop.ts).
        let assistant_tool_calls: Vec<Value> = result
            .tool_calls
            .iter()
            .map(|tc| json!({ "id": tc.id, "type": "function", "function": { "name": tc.name, "arguments": tc.args.to_string() } }))
            .collect();
        convo.push(json!({
            "role": "assistant",
            "content": if result.text.is_empty() { Value::Null } else { Value::String(result.text.clone()) },
            "tool_calls": assistant_tool_calls,
        }));

        for call in &result.tool_calls {
            emitter.tool_use(&call.id, &call.name, &call.args).await;
            let tool_result = registry.dispatch(&call.name, &call.args).await;
            emitter
                .tool_result(&call.id, &tool_result.content, tool_result.is_error)
                .await;
            convo.push(
                json!({ "role": "tool", "tool_call_id": call.id, "content": tool_result.content }),
            );
        }
    }

    let produced_html = file_store.read_index_html();
    Ok(LoopResult {
        final_text,
        produced_html,
        stop_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::generation::emitter::RunEventEmitter;
    use crate::runs::generation::file_store::{FileStore, FileToolSource};
    use crate::runs::generation::registry::ToolRegistry;
    use crate::runs::generation::tools::{ToolResult, ToolSource, ToolSpec};
    use celerex_orchestrator::llm::stub::StubProvider;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn reg_and_store() -> (ToolRegistry, Arc<FileStore>) {
        let store = FileStore::new();
        let reg = ToolRegistry::new(vec![Arc::new(FileToolSource::new(store.clone()))]);
        (reg, store)
    }

    /// Test-only `ToolSource` wrapper that pauses AFTER delegating a dispatch
    /// to the real work (so its effect — e.g. a file write — has already
    /// happened) but BEFORE returning, using a `Notify` handshake the test
    /// controls. This is what makes "disconnect happens strictly between
    /// turn 1 finishing its work and turn 2 starting" a deterministic,
    /// race-free test rather than a bet on task-scheduling order: `reached`
    /// fires the instant this dispatch pauses, synchronously followed by
    /// awaiting `gate` with no intervening `.await` the runtime could use to
    /// run anything else first — so once the test's `reached.notified()`
    /// resolves, the spawned loop is PROVABLY suspended right here, and it's
    /// safe to `drop(rx)` before releasing `gate`. Only the FIRST dispatch is
    /// gated (`gated_once`); if the bug regressed and a second dispatch
    /// happened, it must run unimpeded rather than hang the test forever —
    /// the test should fail on a wrong assertion, not time out.
    struct GateFirstDispatch {
        inner: Arc<dyn ToolSource>,
        reached: Arc<tokio::sync::Notify>,
        gate: Arc<tokio::sync::Notify>,
        gated_once: std::sync::atomic::AtomicBool,
    }

    #[async_trait::async_trait]
    impl ToolSource for GateFirstDispatch {
        fn id(&self) -> &str {
            self.inner.id()
        }
        async fn list_tools(&self) -> Vec<ToolSpec> {
            self.inner.list_tools().await
        }
        async fn execute(&self, name: &str, args: &Value) -> ToolResult {
            let result = self.inner.execute(name, args).await;
            if !self
                .gated_once
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                self.reached.notify_one();
                self.gate.notified().await;
            }
            result
        }
    }

    #[tokio::test]
    async fn terminates_naturally_and_captures_produced_html() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "building",
                "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>hi</h1>\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "done" } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(
            vec![json!({ "role": "user", "content": "make a page" })],
            provider,
            "m",
            &reg,
            &em,
            &store,
            20,
            &canceled,
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::Natural));
        assert_eq!(res.produced_html.as_deref(), Some("<h1>hi</h1>"));
    }

    #[tokio::test]
    async fn stops_at_max_turns() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        // Three identical tool-call turns; maxTurns=3 → MaxTurns.
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(
            vec![json!({ "role": "user", "content": "go" })],
            provider,
            "m",
            &reg,
            &em,
            &store,
            3,
            &canceled,
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::MaxTurns));
    }

    #[tokio::test]
    async fn tool_error_is_fed_back_and_loop_continues() {
        let (reg, store) = reg_and_store();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        // First turn calls write_file with no args → tool error result; then done.
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "e1", "type": "function", "function": { "name": "write_file", "arguments": "{}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "ok" } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        let res = run_generation_loop(
            vec![json!({ "role": "user", "content": "go" })],
            provider,
            "m",
            &reg,
            &em,
            &store,
            20,
            &canceled,
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::Natural));
        // Drain and confirm at least one tool_result frame was emitted.
        let mut saw_tool_result = false;
        while let Ok(Ok(_ev)) = rx.try_recv() {
            saw_tool_result = true;
        }
        assert!(saw_tool_result);
    }

    #[tokio::test]
    async fn already_canceled_stops_before_first_turn() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        let provider = Arc::new(StubProvider::new(vec![]));
        let canceled = AtomicBool::new(true);
        let res = run_generation_loop(
            vec![json!({ "role": "user", "content": "go" })],
            provider,
            "m",
            &reg,
            &em,
            &store,
            20,
            &canceled,
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::Canceled));
    }

    // --- Important 4: disconnect detection -------------------------------
    // A disconnected SSE consumer (closed browser tab) drops the channel
    // `Receiver`; `Sender::is_closed()` then reports `true`. Nothing else
    // ever sets the pre-existing `canceled` flag in these two tests, so a
    // `Canceled` result here can ONLY be produced by the new tx-based check
    // — proving it, not the old flag, is responsible.

    #[tokio::test]
    async fn disconnected_before_loop_starts_stops_immediately_without_consuming_turns() {
        let (reg, store) = reg_and_store();
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        // Turns queued that must NEVER be consumed if the check works: a
        // stub that still had them consumed would either finish `Natural`
        // (wrong) or need to reach into a 3rd, unqueued response (Err).
        let provider = Arc::new(StubProvider::new(vec![
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<should-not-run/>\"}" } }] } }] }),
            json!({ "choices": [{ "message": { "content": "done" } }] }),
        ]));
        let canceled = AtomicBool::new(false);
        // Simulate a client that had already disconnected before this run's
        // stream ever started draining (e.g. a tab closed during a lazy
        // resolve step upstream of the loop).
        drop(rx);

        let res = run_generation_loop(
            vec![json!({ "role": "user", "content": "go" })],
            provider,
            "m",
            &reg,
            &em,
            &store,
            20,
            &canceled,
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::Canceled));
        // Zero turns ran: no index.html was ever written.
        assert_eq!(res.produced_html, None);
    }

    #[tokio::test]
    async fn disconnected_mid_flight_stops_before_next_turn_reaches_paid_provider() {
        let store = FileStore::new();
        let file_source: Arc<dyn ToolSource> = Arc::new(FileToolSource::new(store.clone()));
        let reached = Arc::new(tokio::sync::Notify::new());
        let gate = Arc::new(tokio::sync::Notify::new());
        let gated: Arc<dyn ToolSource> = Arc::new(GateFirstDispatch {
            inner: file_source,
            reached: reached.clone(),
            gate: gate.clone(),
            gated_once: std::sync::atomic::AtomicBool::new(false),
        });
        let reg = ToolRegistry::new(vec![gated]);

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx.clone());
        let provider = Arc::new(StubProvider::new(vec![
            // Turn 1: writes index.html and asks for another turn.
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<turn-1/>\"}" } }] } }] }),
            // Turn 2 must NEVER run once the receiver is dropped mid-flight
            // — it would overwrite index.html with different content, which
            // the assertion below would catch.
            json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "c2", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<turn-2-should-not-run/>\"}" } }] } }] }),
        ]));

        let handle = tokio::spawn(async move {
            let canceled = AtomicBool::new(false);
            run_generation_loop(
                vec![json!({ "role": "user", "content": "go" })],
                provider,
                "m",
                &reg,
                &em,
                &store,
                20,
                &canceled,
                &tx,
            )
            .await
        });

        // Deterministic (no sleeps, no channel-timing guesswork): turn 1's
        // tool dispatch signals `reached` the instant it pauses on `gate`,
        // with no intervening await the runtime could use to run turn 2
        // first — so once this resolves, the loop is PROVABLY paused right
        // after turn 1's write and before it could ever reach turn 2.
        reached.notified().await;

        // Simulate the browser tab closing mid-run: drop the SSE receiver
        // strictly between turn 1's work finishing and turn 2 starting.
        drop(rx);

        // Let turn 1 finish (its tool_result emit now fails silently — the
        // whole point) and the loop proceed to its top-of-turn disconnect
        // check.
        gate.notify_one();

        let res = handle
            .await
            .expect("loop task panicked")
            .expect("loop returned Err instead of a LoopResult");
        assert!(matches!(res.stop_reason, StopReason::Canceled));
        // Turn 2 never ran: index.html still holds turn 1's content, proving
        // the loop stopped BEFORE placing the next (billable) provider call
        // rather than running on to max_turns.
        assert_eq!(res.produced_html.as_deref(), Some("<turn-1/>"));
    }
}
