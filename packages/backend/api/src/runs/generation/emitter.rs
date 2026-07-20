//! SSE run-event emitter (ported from the daemon's
//! apps/daemon/src/api-generation/run-event-emitter.ts). Thin typed wrapper
//! over the run's SSE channel that emits the exact event vocabulary the
//! shipped web SPA already consumes (see apps/web/src/providers/daemon.ts,
//! `translateAgentEvent` + the `start`/`error`/`end` branches of
//! `consumeDaemonRun`): top-level events `start` / `agent` / `error` / `end`,
//! with `agent` carrying a nested `type` discriminant of `status`,
//! `text_delta`, `tool_use`, `tool_result`, `usage`, or `live_artifact`.
//!
//! Every event's JSON shape is built by a small private pure function
//! (`*_frame`) returning the `(event-name, json-body)` pair. The async
//! methods below are thin wrappers that call the matching builder and send
//! the result — this is the seam the test module exercises directly (axum's
//! `Event` has no public getters, so asserting against the built `Event`
//! after the fact isn't possible; asserting against the pair *before* it's
//! wrapped is the single source of truth for the wire shape).

use std::convert::Infallible;

use axum::response::sse::Event;
use serde_json::{Value, json};
use tokio::sync::mpsc::Sender;

/// Thin typed wrapper over the run's SSE channel emitting the daemon's exact
/// event vocabulary (start / agent{status,text_delta,tool_use,tool_result,
/// usage,live_artifact} / error / end). Port of daemon `run-event-emitter.ts`.
pub struct RunEventEmitter {
    tx: Sender<Result<Event, Infallible>>,
}

fn start_frame(bin: &str, project_id: &str, model: &str) -> (&'static str, Value) {
    (
        "start",
        json!({ "bin": bin, "projectId": project_id, "model": model }),
    )
}

fn status_frame(label: &str, detail: Option<&str>) -> (&'static str, Value) {
    let data = match detail {
        Some(d) => json!({ "type": "status", "label": label, "detail": d }),
        None => json!({ "type": "status", "label": label }),
    };
    ("agent", data)
}

fn text_delta_frame(delta: &str) -> (&'static str, Value) {
    ("agent", json!({ "type": "text_delta", "delta": delta }))
}

fn tool_use_frame(id: &str, name: &str, input: &Value) -> (&'static str, Value) {
    (
        "agent",
        json!({ "type": "tool_use", "id": id, "name": name, "input": input }),
    )
}

fn tool_result_frame(tool_use_id: &str, content: &str, is_error: bool) -> (&'static str, Value) {
    let data = if is_error {
        json!({ "type": "tool_result", "toolUseId": tool_use_id, "content": content, "isError": true })
    } else {
        json!({ "type": "tool_result", "toolUseId": tool_use_id, "content": content })
    };
    ("agent", data)
}

fn usage_frame(input_tokens: Option<u64>, output_tokens: Option<u64>) -> (&'static str, Value) {
    (
        "agent",
        json!({ "type": "usage", "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens } }),
    )
}

fn live_artifact_frame(project_id: &str, artifact_id: &str, title: &str) -> (&'static str, Value) {
    (
        "agent",
        json!({ "type": "live_artifact", "action": "created", "projectId": project_id, "artifactId": artifact_id, "title": title }),
    )
}

fn error_frame(message: &str) -> (&'static str, Value) {
    // Flat `{ message }` — matches daemon run-event-emitter.ts errorEvent()
    // exactly (`this.bus.emit(this.run, 'error', { message })`). NOT nested
    // under an `error` key: that shape (createSseErrorPayload, in runs.ts)
    // belongs to the terminal bus.fail() path, which this method is not.
    ("error", json!({ "message": message }))
}

/// `resumable` is hardcoded `false`: daemon parity is `run.resumable ?? false`
/// (`apps/daemon/src/runs.ts::finish`), and `run.resumable` is only ever set
/// `true` by the CLI-spawn retry-decision heuristic
/// (`finishWithRetryDecision` in `server.ts`) — a mechanism specific to
/// child-process-driven runs that this HTTP-turn-loop port does not have and
/// is out of scope to add. `signal` IS a real parameter (not hardcoded)
/// because `StopReason::Canceled` is a reachable loop outcome: the loop
/// cancels when a frame send fails (the SSE receiver dropped, i.e. the client
/// disconnected), and `run_generation` maps that to the daemon-parity triple
/// `end("canceled", 1, Some("SIGTERM"))`. Every other stop reason passes
/// `None`.
fn end_frame(status: &str, code: i32, signal: Option<&str>) -> (&'static str, Value) {
    (
        "end",
        json!({ "status": status, "code": code, "signal": signal, "resumable": false }),
    )
}

/// Pure builder of the (event-name, json-body) pair — single source of truth
/// for the wire shape. Test-visible.
pub fn frame_parts(name: &str, data: Value) -> (String, Value) {
    (name.to_string(), data)
}

fn named(name: &str, data: Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

impl RunEventEmitter {
    pub fn new(tx: Sender<Result<Event, Infallible>>) -> RunEventEmitter {
        RunEventEmitter { tx }
    }

    async fn send(&self, name: &str, data: Value) {
        let (name, data) = frame_parts(name, data);
        let _ = self.tx.send(Ok(named(&name, data))).await;
    }

    pub async fn start(&self, bin: &str, project_id: &str, model: &str) {
        let (name, data) = start_frame(bin, project_id, model);
        self.send(name, data).await;
    }

    pub async fn status(&self, label: &str, detail: Option<&str>) {
        let (name, data) = status_frame(label, detail);
        self.send(name, data).await;
    }

    pub async fn text_delta(&self, delta: &str) {
        let (name, data) = text_delta_frame(delta);
        self.send(name, data).await;
    }

    pub async fn tool_use(&self, id: &str, name: &str, input: &Value) {
        let (event_name, data) = tool_use_frame(id, name, input);
        self.send(event_name, data).await;
    }

    pub async fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        let (name, data) = tool_result_frame(tool_use_id, content, is_error);
        self.send(name, data).await;
    }

    pub async fn usage(&self, input_tokens: Option<u64>, output_tokens: Option<u64>) {
        let (name, data) = usage_frame(input_tokens, output_tokens);
        self.send(name, data).await;
    }

    pub async fn live_artifact(&self, project_id: &str, artifact_id: &str, title: &str) {
        let (name, data) = live_artifact_frame(project_id, artifact_id, title);
        self.send(name, data).await;
    }

    pub async fn error_event(&self, message: &str) {
        let (name, data) = error_frame(message);
        self.send(name, data).await;
    }

    /// Terminal frame. Carries `signal`/`resumable` alongside `status`/`code`
    /// — the daemon's `bus.finish`/`bus.fail` shape
    /// (`apps/daemon/src/runs.ts`), which the shipped web SPA
    /// (`celerex-design/apps/web/src/providers/daemon.ts`, `endResumable` /
    /// `markErrorResumable`) reads to decide whether to offer a "Continue"
    /// affordance on a failed run. See `end_frame` for why `resumable` is
    /// always `false` here.
    pub async fn end(&self, status: &str, code: i32, signal: Option<&str>) {
        let (name, data) = end_frame(status, code, signal);
        self.send(name, data).await;
    }
}

#[cfg(test)]
pub(crate) async fn collect(
    mut rx: tokio::sync::mpsc::Receiver<Result<Event, Infallible>>,
) -> Vec<Event> {
    let mut out = Vec::new();
    while let Ok(Ok(e)) = rx.try_recv() {
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_builds_named_json_event_body() {
        // The public `frame_parts(name, data)` helper is the single source of
        // truth for what goes on the wire; assert its (name, data) output
        // directly since axum's `Event` exposes no public getters to inspect
        // after the fact.
        let (name, data) = frame_parts("agent", json!({ "type": "text_delta", "delta": "hi" }));
        assert_eq!(name, "agent");
        assert_eq!(data, json!({ "type": "text_delta", "delta": "hi" }));
    }

    // --- Per-event vocabulary pins -------------------------------------
    // Each test below asserts the exact (event-name, JSON-field-name) pair
    // against LITERAL strings pinned from the daemon's
    // apps/daemon/src/api-generation/run-event-emitter.ts (the file this
    // module ports) and the SPA's translateAgentEvent / consumeDaemonRun in
    // apps/web/src/providers/daemon.ts. A field-name typo here (e.g.
    // `tool_use_id` instead of `toolUseId`) would silently break the shipped
    // UI, so these compare against inline literals rather than constants
    // defined in this same module (which would just be tautological).

    #[test]
    fn start_frame_matches_daemon_vocabulary() {
        let (name, data) = start_frame("copilot/gpt-4o", "proj-1", "gpt-4o");
        assert_eq!(name, "start");
        assert_eq!(
            data,
            json!({ "bin": "copilot/gpt-4o", "projectId": "proj-1", "model": "gpt-4o" })
        );
    }

    #[test]
    fn status_frame_omits_detail_key_when_none() {
        let (name, data) = status_frame("running", None);
        assert_eq!(name, "agent");
        assert_eq!(data, json!({ "type": "status", "label": "running" }));
        assert!(data.get("detail").is_none());
    }

    #[test]
    fn status_frame_includes_detail_when_some() {
        // "artifact-error" and "reached step limit" are the two special
        // labels the daemon passes through this same generic method (see
        // index.ts / loop.ts) — pinned here as literal strings, since they
        // are not special-cased inside the emitter itself.
        let (name, data) =
            status_frame("artifact-error", Some("script elements are not supported"));
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "status", "label": "artifact-error", "detail": "script elements are not supported" })
        );
    }

    #[test]
    fn text_delta_frame_matches_vocabulary() {
        let (name, data) = text_delta_frame("hi");
        assert_eq!(name, "agent");
        assert_eq!(data, json!({ "type": "text_delta", "delta": "hi" }));
    }

    #[test]
    fn tool_use_frame_matches_vocabulary() {
        let (name, data) = tool_use_frame("c1", "write_file", &json!({ "path": "index.html" }));
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "tool_use", "id": "c1", "name": "write_file", "input": { "path": "index.html" } })
        );
    }

    #[test]
    fn tool_result_frame_omits_is_error_key_when_false() {
        let (name, data) = tool_result_frame("c1", "wrote index.html (1 bytes)", false);
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "tool_result", "toolUseId": "c1", "content": "wrote index.html (1 bytes)" })
        );
        assert!(data.get("isError").is_none());
    }

    #[test]
    fn tool_result_frame_sets_is_error_true_when_error() {
        let (name, data) = tool_result_frame("c1", "boom", true);
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "tool_result", "toolUseId": "c1", "content": "boom", "isError": true })
        );
    }

    #[test]
    fn usage_frame_matches_vocabulary() {
        let (name, data) = usage_frame(Some(10), Some(4));
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "usage", "usage": { "input_tokens": 10, "output_tokens": 4 } })
        );
    }

    #[test]
    fn live_artifact_frame_matches_vocabulary() {
        let (name, data) = live_artifact_frame("proj-1", "la-x", "Title");
        assert_eq!(name, "agent");
        assert_eq!(
            data,
            json!({ "type": "live_artifact", "action": "created", "projectId": "proj-1", "artifactId": "la-x", "title": "Title" })
        );
    }

    #[test]
    fn error_frame_is_flat_message_matching_daemon_error_event() {
        // daemon run-event-emitter.ts:
        //   errorEvent(message: string): void {
        //     this.bus.emit(this.run, 'error', { message });
        //   }
        // Flat — NOT nested under an `error` key. The nested
        // `{ error: { code, message, ... } }` shape (createSseErrorPayload)
        // belongs to the *terminal* bus.fail() path in runs.ts, which this
        // method does not implement (mid-stream notices only, per index.ts's
        // "terminal outcomes go through bus.finish/bus.fail, never
        // emitter.errorEvent" comment).
        let (name, data) = error_frame("script elements are not supported");
        assert_eq!(name, "error");
        assert_eq!(
            data,
            json!({ "message": "script elements are not supported" })
        );
    }

    #[test]
    fn end_frame_matches_vocabulary() {
        let (name, data) = end_frame("succeeded", 0, None);
        assert_eq!(name, "end");
        assert_eq!(
            data,
            json!({ "status": "succeeded", "code": 0, "signal": null, "resumable": false })
        );
    }

    #[test]
    fn end_frame_includes_signal_when_some() {
        // Daemon parity: `bus.finish(run, 'canceled', null, 'SIGTERM')`. This
        // is the shape a real production caller emits: `run_generation`'s
        // `terminal_end_params` maps `StopReason::Canceled` (reached when a
        // frame send fails, i.e. the client disconnected) to exactly this
        // triple.
        let (name, data) = end_frame("canceled", 1, Some("SIGTERM"));
        assert_eq!(name, "end");
        assert_eq!(
            data,
            json!({ "status": "canceled", "code": 1, "signal": "SIGTERM", "resumable": false })
        );
    }

    // --- Public-API integration ------------------------------------------
    // Proves the async methods actually wire through to the channel without
    // silently dropping frames. Per-field vocabulary is covered by the
    // `*_frame` tests above (which the async methods delegate to), so this
    // only needs to check names + count.
    #[tokio::test]
    async fn emits_agent_frames_with_exact_vocabulary() {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let em = RunEventEmitter::new(tx);
        em.start("copilot/gpt-4o", "proj-1", "gpt-4o").await;
        em.text_delta("building").await;
        em.tool_use("c1", "write_file", &json!({ "path": "index.html" }))
            .await;
        em.tool_result("c1", "wrote index.html (1 bytes)", false)
            .await;
        em.usage(Some(10), Some(4)).await;
        em.live_artifact("proj-1", "la-x", "Title").await;
        em.status("artifact-error", Some("script elements are not supported"))
            .await;
        em.end("succeeded", 0, None).await;
        drop(em);
        // Assert we sent 8 frames (one per call), proving none silently dropped.
        let received = collect(rx).await;
        assert_eq!(received.len(), 8);
    }
}
