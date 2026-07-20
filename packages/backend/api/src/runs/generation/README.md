# Server-Side Design-Generation Engine — Module Runbook

This module (`runs::generation`) is a Rust port of the TypeScript daemon's
`apps/daemon/src/api-generation/` pipeline. It replaces the old `design_chat`
bare-completion stub with a real agentic tool loop: the model writes an
`index.html` file server-side using tool calls, the result is validated by an
HTML-template guard, persisted as one artifact row, and served back to the
**unchanged** web SPA over the **unchanged** runs SSE channel. No daemon, no
local CLI process, no HTTP hop to a second internal service — the provider
call happens in-process.

This document is for whoever next has to operate, debug, or extend this
engine. Every claim below was checked against the code in this directory (and,
for daemon-parity claims, against the actual daemon/SPA source in the sibling
`celerex-design` repo) at the time this was written — not copied from the
implementation plan unread. Where the plan's own "Global Constraints" text
undersells or drifts from what the code actually does, that is called out
explicitly rather than silently repeated.

## 1. Module map

| File | Responsibility |
|---|---|
| `mod.rs` | The orchestrator, `run_generation(...)` — the `index.ts` analog. Wires the registry/emitter/file-store, composes the system prompt, runs the loop, validates + persists the produced HTML, emits `live_artifact`/`artifact-error`, emits the terminal `end` frame. |
| `tools.rs` | `ToolSpec`, `ToolResult`, the `ToolSource` trait, `namespace_tool_name` / `owns_tool_name` (bare names for `file`, `mcp__<server>__<tool>` for `mcp:<server>`, `<sourceId>__<tool>` otherwise). |
| `file_store.rs` | `FileStore` — an in-memory, per-run `Mutex<HashMap<String,String>>` working set (replaces the daemon's on-disk project directory) — plus `FileToolSource` (`read_file`/`write_file`/`edit_file`/`list_files`) and `validate_project_path` (path-safety gate at the tool boundary). |
| `registry.rs` | `ToolRegistry` — flattens every registered `ToolSource` into one namespaced catalogue and routes dispatch back to the owning source. |
| `template_guard.rs` | `render_html_template_v1` / `TemplateGuardError` — the `html_template_v1` validator (script/iframe/handler/`javascript:`/raw-interpolation denylist + `{{ data.* }}` binding check). |
| `provider_turn.rs` | `ToolCall`, `ProviderTurnResult`, `build_tool_aliases`, `normalize_raw_completion`, `run_provider_turn` — builds the OpenAI function-tool wire envelope, calls the in-process `Provider`, normalizes the response, reverse-maps tool-call names. |
| `loop_core.rs` | `run_generation_loop`, `LoopResult`, `StopReason` — the turn-by-turn orchestration core (port of daemon `loop.ts`). Also **re-exports** the `system_prompt` public items (see note below). |
| `system_prompt.rs` | `PromptFragment`, `FragmentSlot`, `MAX_PROMPT_FRAGMENT_CHARS`, `compose_system_prompt`, `build_system_prompt` — the base prompt plus the ordered (Brand → Motion → Task) fragment composer. |
| `emitter.rs` | `RunEventEmitter` — the only place that builds SSE frame JSON. Every event is a pure `*_frame(...)` builder function plus a thin async `send` wrapper; see §3. |
| `artifact_store.rs` | `DesignArtifactRow`, `generate_artifact_id`, `insert_artifact` (upsert), `get_artifact` — the `design_artifacts` Postgres access layer. |
| `artifact_routes.rs` | `list_live_artifacts`, `get_artifact_meta`, `get_artifact_preview` — the three `GET /api/live-artifacts...` handlers, including the CSP-parity security headers. |

Not in this directory but load-bearing to how it's wired:

- `packages/backend/api/src/runs/design_chat.rs` — `POST /api/runs` registers a `PendingDesignRun`; `GET /api/runs/{id}/events` (via `runs/sse.rs`) calls `stream_pending`, which resolves the provider and calls `run_generation`.
- `packages/backend/api/src/runs/sse.rs` — the single SSE-stream entry point; dispatches to `design_chat::stream_pending` for a pending design run, or to the unrelated flow-canvas `RunBus`/`RunEvent` path otherwise (two entirely different run types share one route). `claim_pending_design_run` authorizes BEFORE removing the single-shot entry (checked under one lock acquisition) — a caller from a different engagement holding/guessing a valid v4 UUID gets `403` without destroying the legitimate owner's pending run; an earlier version removed first and checked after, so a wrong-engagement probe still 404'd the real owner's own subsequent connect.
- `packages/backend/api/src/lib.rs` — mounts `/api/live-artifacts`, `/api/live-artifacts/{artifact_id}`, and `/api/live-artifacts/{artifact_id}/preview` (lines ~137–139).
- `packages/backend/api/src/db/migrations/0024_design_artifacts.sql` — the one schema addition (additive only; 0023 was the prior highest).
- `packages/backend/api/src/assistant/mod.rs` — `resolve_engagement_provider` / `resolve_engagement_provider_with`, the vaulted-key → `Arc<dyn Provider>` resolution `design_chat.rs` calls before handing off to `run_generation`.

**Re-export note:** `mod.rs` imports `PromptFragment` and `compose_system_prompt` from `self::loop_core`, not `self::system_prompt`, even though they are *defined* in `system_prompt.rs`. This works because `loop_core.rs` does `pub use super::system_prompt::{ComposedPrompt, FragmentSlot, MAX_PROMPT_FRAGMENT_CHARS, PromptFragment, build_system_prompt, compose_system_prompt};`. Not a bug — just know which file to open if you go looking for the definition versus the import.

## 2. Request lifecycle

```
POST /api/runs (design_chat::create)
  -> registers a PendingDesignRun { engagement_id, provider_id, model, project_id, user_message }
     in AppState.design_runs (in-memory map), returns { run_id }
     (a 60s TTL sweep reclaims it if no one ever opens the SSE stream)

GET /api/runs/{run_id}/events (runs/sse.rs::stream -> claim_pending_design_run)
  -> 403 if a pending entry exists but the caller's engagement doesn't match
     the creator's — checked BEFORE removal, so a wrong-engagement probe
     cannot destroy the legitimate owner's pending run as a side effect
  -> otherwise removes the pending entry (single-shot; race-free by
     construction — the completion cannot start before a consumer is
     subscribed), both checked and removed under one lock acquisition
  -> design_chat::stream_pending(state, run_id, pending)
       -> sends the `start` frame directly (not via RunEventEmitter)
       -> resolve_engagement_provider(state, engagement_id, provider_id)
            Ok(Some(provider))  -> continue
            Ok(None)            -> nested `error` frame + failed `end`, return (fail-closed, §6)
            Err(store error)    -> generic `error` frame + failed `end`, return (detail -> tracing::error!)
       -> runs::generation::run_generation(tx, provider, provider_id, model,
            engagement_id, project_id, run_id, user_message, pool,
            max_turns=20, fragments=&[])
            -> composes the system prompt (empty fragments today, see §7)
            -> run_generation_loop(...) turn by turn:
                 run_provider_turn -> provider.complete(...) (in-process, §6)
                 emits text_delta / tool_use / tool_result / usage per turn
                 dispatches tool calls through ToolRegistry -> FileStore
            -> validates+persists produced_html (§5), emits live_artifact or artifact-error
            -> emits `reached step limit` status if MaxTurns
            -> emits terminal `end`: "succeeded" (natural stop or MaxTurns),
               "failed" (the loop/provider itself errored), or "canceled"
               (the SSE consumer disconnected mid-run — §3, §7)

GET /api/live-artifacts?projectId=...                          (artifact_routes::list_live_artifacts)
GET /api/live-artifacts/{artifact_id}?projectId=...            (artifact_routes::get_artifact_meta)
GET /api/live-artifacts/{artifact_id}/preview?projectId=...&variant=...  (artifact_routes::get_artifact_preview)
  -> all three AuthClaims-gated; the list route scopes by the query bind,
     the other two by (engagement_id, project_id, artifact_id) in the same
     SQL WHERE clause (a cross-engagement request 404s; it never gets a 403
     that would confirm the row exists elsewhere)
```

`max_turns` is currently a hardcoded `20`, set at the one production call site
in `design_chat.rs`; it is not part of `CreateRunRequest` and not
client-configurable today.

## 3. SSE event vocabulary — the hard compatibility contract

The web SPA (`apps/web/src/providers/daemon.ts`'s `consumeDaemonRun` +
`translateAgentEvent`, and `apps/web/src/components/FileViewer.tsx`) is
**unchanged** and was written against the daemon. This is the plan's own
"Global Constraints" list, reproduced verbatim because it is the actual
compatibility contract this engine must not break:

> - Top-level named SSE events (`event:` field): `start`, `agent`, `error`, `end`.
> - `start` data: `{ "bin": "<provider>/<model>", "projectId": <string>, "model": <string> }`.
> - `agent` data `type` field, one of: `status` (`{type:"status",label,detail?}`), `text_delta` (`{type:"text_delta",delta}`), `tool_use` (`{type:"tool_use",id,name,input}`), `tool_result` (`{type:"tool_result",toolUseId,content,isError?}`), `usage` (`{type:"usage",usage:{input_tokens,output_tokens}}`), `live_artifact` (`{type:"live_artifact",action:"created",projectId,artifactId,title}`).
> - `error` data: `{ "error": { "message": <string> } }`.
> - `end` data: `{ "status": "succeeded"|"failed", "code": <int> }`.
> - **`artifact-error` is a mid-stream `agent`/`status` frame** (`{type:"status",label:"artifact-error",detail:<message>}`) and the run STILL finishes `end {status:"succeeded", code:0}`. This exact label is load-bearing.
> - **`reached step limit`** is emitted as `agent`/`status` (`label:"reached step limit"`) when the loop hits `maxTurns`, then the run finishes `succeeded`.

Two things the list above doesn't spell out, verified against the shipped code
on both sides (`emitter.rs` here; `daemon.ts` in `celerex-design`):

- **`end` actually carries two more keys**: `{ "status", "code", "signal", "resumable": false }`. `resumable` is always hardcoded `false` (nothing in this HTTP-turn-loop port can ever set it `true` — the daemon's `true` case is a CLI-spawn retry heuristic with no equivalent here). `signal` is `None`/`null` for `"succeeded"`/`"failed"`, and `Some("SIGTERM")` for `"canceled"` — `mod.rs::terminal_end_params` is the one place that maps a loop `StopReason` to this `(status, code, signal)` triple. Both keys exist because the shipped SPA (`daemon.ts` lines ~1063–1089) reads `event.data.signal` / `event.data.resumable` off the `end` frame to decide whether to offer a "Continue" affordance on a failed run — omitting them would silently break that affordance, not just leave it inert. That same SPA code treats `status: "canceled"` as a graceful completion (`handlers.onDone(...)`, not `onError`) — fitting, since a canceled run's own `end` frame typically fails to actually reach anyone: see the cancellation note in §7.
- **The `error` event has two different valid shapes**, depending on which failure path emits it, and the SPA is written to accept both (`daemon.ts`: `String(data.error?.message ?? data.message ?? 'daemon error')`):
  - **Nested** `{ "error": { "message": <string> } }` — emitted by `design_chat.rs::send_error`, used only for *pre-loop* failures (no vaulted key, key-store error) before `run_generation`/the emitter even exists.
  - **Flat** `{ "message": <string> }` — emitted by `RunEventEmitter::error_event`, used for *mid-loop* failures (a provider/loop turn failed inside `run_generation`). This matches the daemon's own `run-event-emitter.ts` `errorEvent()` exactly; the nested shape belongs to the daemon's separate terminal `bus.fail()` path.

  If you ever "fix" this into one canonical shape, you must update both
  `design_chat.rs::send_error` and `emitter.rs::error_frame` together, and
  confirm the SPA still parses whichever shape you keep — don't assume this
  is dead code drift; it's two intentionally-different call sites.

`emitter.rs`'s own `RunEventEmitter::start` method is unused in production —
production sends the `start` frame directly from `design_chat.rs::stream_pending`
*before* the emitter (and hence `run_generation`) even exists. It's only
exercised by `emitter.rs`'s own unit test.

## 4. Storage model

**In-memory working set — `FileStore` (`file_store.rs`).** A `Mutex<HashMap<String, String>>`, one per run, created fresh in `run_generation` and dropped when the run ends. `write_file`/`read_file`/`edit_file`/`list_files` are the only write path in; every path goes through `validate_project_path` first (rejects empty/whitespace, embedded NUL, absolute paths, Windows drive letters, and any `.`/`..`/empty `/`-segment — ported from the daemon's `validateProjectPath`, closing a bug where `write_file("./index.html", …)` used to succeed silently under a key `read_index_html()`'s hardcoded `"index.html"` lookup would never find). **Only the exact key `"index.html"` is ever promoted out of this map** (`FileStore::read_index_html()`); anything else the model writes under another path (`write_file`/`edit_file` accept any path that passes `validate_project_path` — e.g. `assets/style.css`, used as a valid-path example in this module's own test matrix) is discarded with the rest of the map when the run ends. Multi-file output is not persisted or servable today — the system prompt itself tells the model to build one self-contained document.

**Persisted artifact — `design_artifacts` (Postgres, migration `0024_design_artifacts.sql`).** One row per successfully-guarded artifact: `(engagement_id, project_id, artifact_id, title, template_html, rendered_html, created_by_run_id, created_at)`, unique on `(engagement_id, project_id, artifact_id)`, `ON DELETE CASCADE` on the engagement FK. `artifact_store::insert_artifact` is an upsert (`ON CONFLICT ... DO UPDATE`); `title` comes from `derive_title(user_message)` in `mod.rs` (whitespace-collapsed, first 80 chars, or `"Generated design"` if empty) and `generate_artifact_id` slugifies it (`la-<slug>-<12 hex>`, random suffix via `ring`'s `SystemRandom`).

**Three GET routes (`artifact_routes.rs`), all `AuthClaims`-gated and engagement-scoped:**
- `GET /api/live-artifacts?projectId=...` — every artifact for one project, most-recently-created first (`list_live_artifacts`, scoped by the query bind). This is the route the shipped web SPA actually calls to populate the artifact/workspace tab (`ProjectView.tsx` → `fetchLiveArtifacts(projectId)` → `registry.ts`'s `{ artifacts: [...] }`); the `live_artifact` SSE payload alone never opens the tab on its own.
- `GET /api/live-artifacts/{artifact_id}?projectId=...` — artifact metadata (`get_artifact_meta`, scoped in the SQL `WHERE` clause). Returns `{ "artifact": { "id", "projectId", "title", "refreshStatus": "idle", "preview": { "type": "html", "entry": "index.html" } } }`. `refreshStatus` is a **hardcoded literal**, not backed by any real refresh mechanism.
- `GET /api/live-artifacts/{artifact_id}/preview?projectId=...&variant=...` — the HTML the FileViewer iframes (`get_artifact_preview`, scoped in the SQL `WHERE` clause). `variant=template` returns the raw author template; anything else (including `rendered`, an unrecognized value, or the param entirely absent) returns the rendered/guarded HTML. This route is also reachable from a bare iframe load, not only an authenticated `apiFetch` — its auth posture for that case (`PreviewAuth`) is documented directly on the handler, not repeated here.

All three responses carry the CSP-parity security header set (`live_artifact_security_headers()` — `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and an 11-directive `Content-Security-Policy` that is directive-for-directive byte-identical, in the same order, to the daemon's `setLiveArtifactPreviewHeaders` — verified directly against `celerex-design/apps/daemon/src/server.ts`). This is deliberately applied to **all three** routes; the daemon only applies it to the preview route. `script-src 'none'` plus the script-less `sandbox` directive are what make the guard's known denylist gaps (below) inert on serve — the CSP is defense-in-depth, not decorative.

**Scope boundary, confirmed by grep, not assumed:** the web SPA's client (`apps/web/src/providers/registry.ts` in `celerex-design`) also has functions for `refreshLiveArtifact` (`POST .../refresh`), listing refreshes (`GET .../refreshes`), and `deleteLiveArtifact` (`DELETE ...`). **None of these three routes exist server-side** — only the three GET routes above are mounted in `lib.rs`. If a UI affordance ever calls one of those against this Agent, it will 404. This remains the deliberate scope of the port (the brief's own storage decision: "engagement-authenticated GET routes... that the existing web already calls" for the *generation* flow specifically), not an oversight — unlike the list route above, which this document itself previously failed to enumerate here at all (neither as mounted nor as a gap) despite the SPA depending on it; that omission is now corrected.

## 5. The `artifact-error` → `succeeded` guard contract

In `mod.rs::run_generation`, after the loop returns:

```rust
if let Some(html) = &result.produced_html {
    match render_html_template_v1(html) {
        Ok(rendered) => { /* insert_artifact + emit live_artifact, or artifact-error on a DB failure */ }
        Err(err) => emitter.status("artifact-error", Some(&err.0)).await,
    }
}
// ... then unconditionally:
emitter.end("succeeded", 0, None).await;
```

Three things worth being precise about:

1. **A guard rejection never fails the run.** `<script>`, a stray `{{ name }}`, an `<svg/onload=...>`, etc. all produce an `artifact-error` status frame with a human-readable `detail`, and the run still ends `succeeded, code 0`. The web shows no error banner; it just never gets a `live_artifact` event, so no artifact tab populates.
2. **This block only runs at all if the model actually wrote `index.html`.** If the loop ends naturally (or hits max-turns) without ever writing `index.html`, `produced_html` is `None`, the whole block is skipped — no `artifact-error`, no `live_artifact` — and the run still proceeds straight to the terminal `succeeded` frame. A purely conversational turn (no design produced) is indistinguishable, on the wire, from "the guard rejected it," except by the absence of *any* artifact-related status frame.
3. **A DB persist failure also produces `artifact-error`, not a failed run** — same status label, different detail string (`"failed to persist artifact"`), logged server-side via `tracing::error!`.

`reached step limit` (emitted only when `result.stop_reason` is `StopReason::MaxTurns`) and `artifact-error` are independent — a run can emit both (max-turns reached *and* the last-written HTML was rejected by the guard) or either alone. Both still end `succeeded`.

## 6. Provider call semantics — in-process, fail-closed

The turn layer never makes an HTTP request to this server's own `/api/assistant/chat` or any other internal endpoint. `provider_turn::run_provider_turn` calls `provider.complete(model, messages, wire_tools).await` directly on the `Arc<dyn Provider>` trait object `design_chat.rs` already resolved — a plain in-process async call. (The `Provider` implementation itself, e.g. `OpenAiProvider`, of course still makes the one real outbound HTTP call to the upstream vendor API — there is just no *internal* proxy hop in between.)

**Fail-closed, no house-mode fallback.** `design_chat.rs::stream_pending` resolves the provider via `assistant::resolve_engagement_provider`, which only ever returns the engagement's own vaulted key or `Ok(None)`/`Err`. Unlike `assistant::chat`/`chat_stream` (which fall back to the shared `CELEREX_HOUSE_MODE` registry when no engagement key exists), the design-chat path treats `Ok(None)` as terminal: a nested `error` frame (`"no key configured for provider '<id>'"`) plus a failed `end`, before `run_generation` is even entered. There is no code path by which a design run executes against the operator's own key.

**Error sanitization boundary.** A provider/loop failure inside the loop (network error, non-2xx, malformed response) propagates as a plain `Result<_, String>` via `?` all the way from `Provider::complete` through `run_provider_turn` and `run_generation_loop`. Some providers embed the raw upstream response body in that string (`openai_compat.rs`: `"openai-compat status {status}: {raw}"`; `copilot.rs`: `"copilot chat status {status}: {raw}"`). `run_generation`'s `Err(message)` arm is the one place this is sanitized: the full string goes to `tracing::warn!(engagement, run_id, error, ...)` server-side, and the client-visible SSE `error` frame always gets the fixed string `"generation failed"` — daemon parity with `provider-turn.ts`'s `` `Agent generation failed (status ${status})` `` (status discarded here too; the Rust error string's shape isn't structured enough to reliably re-extract a status code, so a constant message was chosen over a fragile parse). This is a regression-tested invariant (`run_generation_never_leaks_upstream_error_body_onto_error_frame` in `tests/design_generation.rs`) — it closed a real leak that was briefly shipped and caught before merge; don't reintroduce it by piping a raw provider error string to `emitter.error_event` directly.

## 7. Known limitations and follow-ups

**Tools go on the wire as the OpenAI function-tool envelope (by design, not a gap).** `provider_turn::build_tool_aliases` wraps every `ToolSpec` as `{type:"function",function:{name,description,parameters}}` and sanitizes/aliases the name to `[a-zA-Z0-9_-]` via a reversible, injective map — because the provider clients forward `tools` verbatim (`openai.rs`, `kimi.rs`, `openai_compat.rs` all do `if !tools.is_empty() { body["tools"] = tools }`). This gives full parity for every OpenAI-compatible chat/completions provider (openai, copilot, kimi, glm, kimi-code, and xai's OpenAI-compat layer) — the daemon did exactly the same thing. `normalize_raw_completion` *can* parse an Anthropic-shaped response (`content[]` blocks) on the way in, but no provider-specific *outbound* tool-schema adaptation exists — Anthropic/Gemini get the same OpenAI-shaped tool envelope as everyone else. `run_generation`'s `provider_id` parameter is accepted but currently unused (`let _ = provider_id; // reserved for future per-provider tool-shape adaptation`) — that's the seam a native-schema follow-up would hook into.

**`complete_stream()` in `llm/openai.rs` / `llm/kimi.rs` still forwards `tools` unconditionally**, even when empty (unlike their own `complete()`, which gates it). This engine never calls `complete_stream()` (`run_provider_turn` always calls `complete()`), so it is confirmed unused by anything in this module — but it's a live inconsistency for `assistant::chat_stream`, the only caller of `complete_stream()`. Worth fixing alongside any future streaming work on this engine, not because this engine is affected.

**`tc.args` null-coalescing gap vs. the daemon's `?? {}`.** In `loop_core.rs`, the assistant tool-call message is reconstructed as `"arguments": tc.args.to_string()`. If a `ToolCall.args` is JSON `Value::Null` — reachable via `provider_turn.rs`'s Anthropic branch, `block.get("input").cloned().unwrap_or(json!({}))`, which only defaults when the `input` key is *absent*, not when it's present-and-`null` — this serializes to the literal string `"null"` echoed back to the provider, where the daemon's `?? {}` would send `"{}"`. Not observed in practice (no real Anthropic response sends an explicit `null` input), but latent.

**`slugify` (`artifact_store.rs`) is ASCII-only**, lowercasing and keeping `[a-z0-9]` with `-` as separator. The daemon NFKD-normalizes first, so e.g. `"Café"` slugifies to `caf` here vs. `cafe` in the daemon — the accented letter and everything after the stripped combining mark become a dropped/collapsed separator. Cosmetic: only affects the human-readable segment of `artifact_id`; uniqueness comes from the random hex suffix regardless.

**`get_artifact_preview`'s `variant` handling collapses everything but the literal `"template"` to `rendered`** (including `"rendered-source"`, garbage, or the param being absent). The daemon 400s an unrecognized variant. Inert today because the shipped `FileViewer.tsx` only ever requests `variant=rendered` or omits it — but a new client-side variant added later would silently get `rendered` instead of an error here.

**`StopReason::Canceled` now has a real, reachable producer: SSE-consumer disconnect (resolved — previously unreachable).** `run_generation_loop` still checks the `canceled` `AtomicBool` at the top of every turn (unit-tested: `already_canceled_stops_before_first_turn`), and `run_generation` still constructs it fresh, local, and never explicitly flips it — there is no user-initiated cancel API yet. What changed: the loop *also* checks the SSE channel's `Sender::is_closed()` at that same point (`loop_core.rs` tests `disconnected_before_loop_starts_stops_immediately_without_consuming_turns` and `disconnected_mid_flight_stops_before_next_turn_reaches_paid_provider`). A closed browser tab drops axum's SSE response body, which drops the channel's `Receiver`, making `is_closed()` true; the loop then stops with `Canceled` before placing another (billable) provider call, and `mod.rs::terminal_end_params` reports it as `end("canceled", 1, Some("SIGTERM"))` (see §3). This closes a real cost/resource leak: previously, closing the tab left the loop running silently for up to `max_turns` (20) more provider calls with nobody consuming the output.

**`GET /api/runs/{runId}` — the web's status-fallback route — does not exist for design-chat runs; documented here rather than faked.** The web's `consumeDaemonRun` (`daemon.ts`'s `fetchChatRunStatus`) calls this route in two situations: after a mid-stream `error` frame (to judge whether the failure is retryable), and when the event stream ends without ever having seen a terminal `end` frame (reconnect budget exhausted, or some other stream break). This crate has no handler for a bare `GET /api/runs/{run_id}` — the three existing `/api/runs*` handlers (`runs::routes::list`, `recent`, `steps`) all read the Postgres `runs` table directly, populated only by the unrelated flow-canvas run type. Design-chat runs are never rows in that table: `PendingDesignRun` lives only in the in-memory `design_runs` registry (`AppState`), and that entry is removed the instant its SSE stream starts (single-shot — see `sse.rs`, which now checks ownership before that removal rather than after). A real implementation would need either a persisted design-chat run-status record (piggybacking on `runs`, or a new store) plus a new route registered in `lib.rs`, or comparable new state — outside this fix's file scope, and not a "cheap correct" change, so it was assessed and documented rather than implemented. Consequence: the mid-stream-`error` fallback resolves harmlessly today (a 404 there already reads as "status unreachable," which the SPA already treats as "surface the structured error" — the correct outcome anyway). The no-`end`-frame fallback is the one with a real, pre-existing cost: the SPA cannot distinguish "the run actually finished server-side but its `end` frame never arrived" from "the run is gone," so it can surface a spurious `daemon stream disconnected before run completed` error even for a run that completed cleanly. This is unrelated to, and not made worse by, the disconnect-cancellation behavior above — a client that has already disconnected was never going to be the one calling this fallback for itself.

**The system-prompt fragment composer ships empty in production.** `compose_system_prompt`/`build_system_prompt`/`PromptFragment`/`FragmentSlot` (Brand → Motion → Task, `MAX_PROMPT_FRAGMENT_CHARS = 8_000`, truncate-last) are fully implemented and unit-tested, but the one production call site (`design_chat.rs`) always passes `&[]`, so the composed prompt is byte-identical to the pre-composer base prompt. This is plumbing for a future producer (e.g. a Brand Kit feature) to pass real fragments — not a bug, just not live yet. If you add a fragment producer, the `prompt-fragment-truncated` status literal is already wired end-to-end (`run_generation` checks `composed.truncated` and emits it) and has a regression test (`run_generation_truncated_fragment_emits_truncation_status_literal`).

**The base system prompt and the guard must be kept in sync by hand.** `system_prompt.rs::base_prompt` explicitly tells the model not to use `<script>` or `{{ }}`; `template_guard.rs` is what actually enforces that. They are two separate files with no shared constant — if you change what the guard rejects, update the prompt text too (and vice versa), or the model will keep trying things the guard silently turns into `artifact-error`.

## 8. Deliberate divergences from the daemon (documented in-code, worth surfacing here too)

- **Guard hardening beyond the daemon's own denylist** (`template_guard.rs`): the `on*=` handler pattern is widened to match a `/` separator as well as whitespace before `on` (`[\s/]on[a-z][a-z0-9_-]*\s*=`), catching the `<svg/onload=...>` / `<img/onerror=...>` class the daemon's own `\son...=` pattern misses. This is a **known, deliberate over-reject**: it also flags ordinary URL text like `url(/images/online=1.png)` (locked in by `hardening_tests::known_limitation_url_containing_slash_on_word_equals_is_over_rejected`) — do not narrow this pattern without re-solving `<svg/onload=` detection first; a visible over-reject was judged better than an executable-content miss, and the CSP (§4) is defense-in-depth, not a reason to loosen this. `<foreignObject>` is also rejected outright (the HTML-in-SVG script bridge; no legitimate static-chart use). The `data.*` template-binding check requires the *literal* first dot-segment `data` (rejecting `database.foo`, `dataset.x`, `dataXYZ.foo`, and bare `data`), matching the daemon's actual `segments.shift() !== 'data'` semantics rather than a looser `starts_with("data")`.
- **Path validation is stricter than the daemon on doubled separators** (`file_store.rs::validate_project_path`): the daemon's own `normalized.split('/').filter(Boolean)` drops empty segments *before* checking its forbidden-segment regex, so a raw `"a//b"` silently collapses to the accepted `"a/b"` in the daemon's actual runtime. This port checks segments *before* dropping empties, so `"a//b"` is rejected loudly instead. Strictly stricter, never looser. The daemon's separate `.live-artifacts`-segment reservation (an on-disk sentinel) is deliberately not ported — there is no on-disk state here to protect.
- **No `requireLocalDaemonRequest` gate.** The daemon required requests to originate from its own local-network daemon process; these routes have no equivalent. This is the direct, intended consequence of the locked server-authoritative architecture (there is no local-daemon-network concept server-side) — `AuthClaims`-based engagement scoping is what replaces it, not an omission.
- **CSP-parity headers applied to all three live-artifact routes**, not just the preview route the daemon applies them to — a deliberate widening (defense-in-depth on the metadata/list JSON responses too), not a gap.

## 9. Running the tests

```bash
# Unit tests (no external services): 151 passing as of this writing.
cargo test -p celerex-api --lib

# Integration tests (needs Docker — testcontainers spins up real Postgres),
# in tests/design_generation.rs. Count intentionally not pinned here — this
# file gains coverage often enough that a hardcoded number goes stale fast;
# run it locally for the current count.
cargo test -p celerex-api --test design_generation

# A single test, with output:
cargo test -p celerex-api --test design_generation <test_name> -- --nocapture

# Gated live E2E — makes a REAL network call with REAL credentials against
# a real provider, and asserts a real artifact row was persisted. #[ignore]d,
# so a normal `cargo test` run (and CI) never executes it:
OMNIVYX_LIVE_KEY=... \
  [OMNIVYX_LIVE_PROVIDER=copilot] [OMNIVYX_LIVE_MODEL=gpt-4o] \
  cargo test -p celerex-api --test design_generation_live \
  -- --ignored --nocapture
```

Notes on the live test (`tests/design_generation_live.rs`): it resolves the
provider through `assistant::resolve_engagement_provider_with` — the exact
vault-to-client path production uses — after vaulting `OMNIVYX_LIVE_KEY`
through a real (testcontainers) Postgres, so it also proves the vault-wiring,
not just the loop in isolation. It skips (prints to stderr, does not panic)
if `OMNIVYX_LIVE_KEY` is unset. `OMNIVYX_LIVE_PROVIDER` defaults to `copilot`
and `OMNIVYX_LIVE_MODEL` to `gpt-4o`; both are overridable to any id in
`providers::store::SUPPORTED_PROVIDERS`. **Caveat not currently spelled out in
the test's own doc comment:** `codex` and `xai` are blob-credentialed
(`from_blob`), so `OMNIVYX_LIVE_KEY` would need to be a full JSON credential
blob for those two specifically, not a plain API key string — the default
(`copilot`) and the other OpenAI-compat-style providers take a plain key.

Also useful: after any `cargo build`/`cargo test`, grep the *unpiped* output
for `^error` (anchored) rather than trusting a piped command's exit code —
this crate has pre-existing warnings elsewhere that can visually bury a real
new compile error, and a `cargo | tee` pipeline's exit status reflects `tee`,
not `cargo`.

## 10. Manual browser-verification runbook

After any change here, it's worth verifying the full path end-to-end against
the real SPA (the web side is unchanged, so this is really verifying the
Agent's half of the contract):

1. **Build the SPA export** and point the Agent at it via `CELEREX_WEB_DIR=<path to the design app's `out/` export>` (see `web_service()` in `lib.rs`).
2. **Boot the Agent** with a DB and the crypto env:
   ```bash
   CELEREX_CRED_KEY=0123456789abcdef0123456789abcdef \
   CELEREX_JWT_SECRET=dev-secret \
   PORT=3001 cargo run -p celerex-api
   ```
   Do **not** set `CELEREX_HOUSE_MODE` — the design-chat path is fail-closed regardless, but leaving it unset keeps the whole environment honest to production.
3. **Seed a real vaulted key** for your engagement via the provider-keys endpoint (or the design Settings UI): store an OpenAI-compatible provider key (e.g. `copilot`) for the engagement you'll sign in as.
4. **Open the SPA** at `http://localhost:3001`, sign in, open/create a project, select the matching `<provider>/<model>`, and send a design prompt like *"Build a centered hero with a headline and subtitle."*
5. **Watch the chat stream:** assistant status → text → tool-use/tool-result activity (the `agent` frames), then the artifact tab populates (backed by `GET /api/live-artifacts?projectId=...` — §4).
6. **Confirm the artifact renders:** the FileViewer iframe shows the generated page. In the network panel, confirm the web fetched `GET /api/live-artifacts/{artifactId}?projectId=...` (200 JSON) and `GET /api/live-artifacts/{artifactId}/preview?projectId=...&variant=rendered` (200 HTML) from the Agent. (`/preview` is reachable from a bare iframe load, not only an authenticated `apiFetch`; its auth posture for that case is documented directly on the `get_artifact_preview` handler, not repeated here.)
7. **Confirm the guard path:** send a prompt that coerces a `<script>` into `index.html` (or temporarily relax the system prompt). The chat should show an `artifact-error` status but the run should still finish `succeeded` (no error banner), and no artifact row should be served.
8. **Confirm isolation:** with a second engagement that has **no** vaulted key, sending a design prompt should surface `no key configured for provider '<id>'` and finish `failed` (fail-closed, §6).
9. **DB check:** `SELECT engagement_id, project_id, artifact_id, title FROM design_artifacts ORDER BY created_at DESC LIMIT 5;` — the successful run's row is present and scoped to your engagement.
10. **Confirm disconnect-cancellation (§7):** send a design prompt, then close the browser tab (or otherwise abort the `GET /api/runs/{runId}/events` request) right after the first `tool_use`/`tool_result` activity appears. There is no client-visible confirmation of this — the whole point is nobody is listening any more — so watch the Agent's own terminal/logs instead: provider-call activity for that run should stop immediately rather than continuing for the full `max_turns` (20).
