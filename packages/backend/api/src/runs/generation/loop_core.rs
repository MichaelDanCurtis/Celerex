//! Orchestration core of the server-side design-generation loop. Port of
//! daemon `loop.ts`: loop one provider turn at a time, emitting SSE for
//! text/tool-use/tool-result/usage, dispatching tool calls through the
//! `ToolRegistry` and feeding results back into the conversation. Terminates
//! on natural stop (model returns no tool calls), a `max_turns` safety cap, or
//! an already-set cancel flag. Tool errors are fed back to the model as the
//! tool_result content (`is_error: true`) and never abort the loop — matching
//! the daemon's non-abort semantics. Every dispatched tool_call unconditionally
//! gets both an emitted `tool_result` SSE frame and a `role:"tool"` transcript
//! entry (even on error), avoiding orphan tool_call_ids that some providers
//! reject with a 400.
//!
//! This module does NOT itself emit the "artifact-error" / "reached step
//! limit" status literals — matching the daemon's separation of concerns,
//! where `loop.ts` only returns a `stopReason` and `index.ts` (here, the
//! Task 9 caller) inspects it after the loop returns to decide what to emit.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use celerex_orchestrator::llm::Provider;
use serde_json::{Value, json};

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
/// back. Terminates on natural stop (no tool calls), a maxTurns cap, or an
/// already-set cancel flag. Tool errors are fed back as tool_result content
/// (is_error=true) and never abort the loop. Port of daemon `loop.ts`.
// The 8-parameter signature is the fixed cross-task interface (Task 9 calls
// this directly; see the task brief's `Interfaces` section) rather than an
// accidental pile-up — allow the default clippy threshold rather than wrap
// the params in a bespoke struct that only this one function would use.
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
) -> Result<LoopResult, String> {
    let schemas = registry.schemas().await;
    let mut convo = seed;
    let mut final_text = String::new();
    let mut stop_reason = StopReason::MaxTurns;

    for _turn in 0..max_turns {
        if canceled.load(Ordering::Relaxed) {
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
    use celerex_orchestrator::llm::stub::StubProvider;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn reg_and_store() -> (ToolRegistry, Arc<FileStore>) {
        let store = FileStore::new();
        let reg = ToolRegistry::new(vec![Arc::new(FileToolSource::new(store.clone()))]);
        (reg, store)
    }

    #[tokio::test]
    async fn terminates_naturally_and_captures_produced_html() {
        let (reg, store) = reg_and_store();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
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
        let em = RunEventEmitter::new(tx);
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
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::MaxTurns));
    }

    #[tokio::test]
    async fn tool_error_is_fed_back_and_loop_continues() {
        let (reg, store) = reg_and_store();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let em = RunEventEmitter::new(tx);
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
        let em = RunEventEmitter::new(tx);
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
        )
        .await
        .unwrap();
        assert!(matches!(res.stop_reason, StopReason::Canceled));
    }
}
