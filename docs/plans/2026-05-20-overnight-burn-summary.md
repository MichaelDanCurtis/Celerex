# Overnight Burn — Session Summary 2026-05-19 → 2026-05-20

Status: shipped 2026-05-20.

This doc catches up the slices shipped after `b19d20f` that don't yet have individual plan docs. For the slices that do have individual docs, see the sibling files in `docs/plans/`.

## Slices covered here

| Commit | Slice |
|---|---|
| `78d62dc` | Doc viewer (backend GET + frontend page) |
| `0d1668f` | Anthropic streaming + Plugin install UI |
| `ca4e78e` | `/api/assistant/chat/stream` SSE endpoint |
| `0afb161` | Provider fallback chains + AIPanel streaming wire |
| `9345269` | Run history end-to-end + OpenAI/Kimi streaming |
| `6462309` | Auth login + Gemini streaming + Settings page + E2E demo |
| `d13cc39` | Per-engagement encrypted keys + Galvatronic plugin |

## Doc viewer (`78d62dc`)

Backend: `GET /api/engagements/{engagement_id}/docs` (with block_count), `GET /api/docs/{id}`, `GET /api/docs/{id}/blocks` (ordered by position), `DELETE /api/docs/{id}` (cascades). 4 integration tests.

Frontend: `components/DocViewer.tsx` (per-block payload switch: markdown / json / fallback), `pages/DocsPage.tsx` (engagement switcher + viewer pane), client.ts gains `listEngagementDocs`/`getDoc`/`listDocBlocks`, atoms add `activeViewerDocIdAtom`.

Not done: rich markdown rendering; in-viewer editing; download as PDF.

## Provider streaming (`0d1668f`, `ca4e78e`, OpenAI/Kimi from `9345269`, Gemini from `6462309`)

Provider trait gained `complete_stream` default method wrapping `complete()` in `stream::once`.

Real streaming overrides for all 4 providers:
- Anthropic — `/v1/messages` with `stream:true`, parses `event:` lines + `content_block_delta`
- OpenAI — `/v1/chat/completions` with `stream:true`, parses `data:` lines, skips `data: [DONE]`
- Kimi — identical to OpenAI (compatible API)
- Gemini — `:streamGenerateContent?alt=sse`, parses `data:` lines, no terminator

Shared `stream::unfold` + Phase enum pattern. Cargo deps: `futures-util` (orchestrator), `bytes` + reqwest `stream` feature (api).

SSE endpoint `POST /api/assistant/chat/stream`: tokio::spawn + mpsc::channel + ReceiverStream. Events: `delta` / `error` / `done`. KeepAlive 15s. Sidesteps `complete_stream<'a>` lifetime by owning `Arc<dyn Provider>` clone + owned model string in the spawned task.

Frontend `assistantChatStream` uses fetch + body.getReader() + TextDecoder (EventSource can't POST with Bearer). AIPanel pre-allocates assistant bubble, mutates on each onDelta. AbortController + Stop button.

Not done: streaming tool-use loop; chat history persistence; provider-key per-engagement override at call time.

## Provider fallback chains (`0afb161`)

`LlmHandler::invoke` reads `config["provider_fallback_ids"]` array. Tries primary first, then each fallback in order on Err. Returns last error if all fail. Missing-from-registry treated as failure. Messages/tools cloned per attempt.

Tests: `tests/llm_handler.rs` gains 3 cases — fallback on err, all-fail, skip-missing.

Not done: streaming-aware fallback (only triggers on `complete()` err, not mid-stream); cost tracking; per-attempt timeout.

## Run history (`9345269`)

Migration `0012_runs_project_member.sql` adds `project_member_id` column (nullable, FK CASCADE, indexed `(project_member_id, created_at DESC)`). Backfill via `trigger->>'project_member_id'`.

`enqueue_run` writes the id to both the column and the trigger JSONB.

`GET /api/runs?project_member_id=X&limit=N` — engagement-scoped 403.

`components/RunHistoryPanel.tsx` mounts as 3rd BuilderPage tab. Click row → activeRunIdAtom + monitor view.

Not done: paginated cursors; fleet-wide run history; cancel.

## Auth login (`6462309`)

`POST /api/auth/login` `{ engagement_name } → { token, engagement }`. Case-insensitive lookup; 404 unknown.

`pages/LoginPage.tsx` gates app when tokenAtom empty. localStorage persistence as `celerex_jwt`. Sign-out button clears both.

Not done: passwords (any engagement_name signs in); registration; OAuth/SSO; session expiry.

## Settings + E2E demo (`6462309`)

`pages/SettingsPage.tsx` — 5 sections: Account+Engagement (sign out), LLM Providers (badges), Engagements+Projects (re-mounts EngagementManager), Plugins (count + link), About.

`tests/e2e_demo_flow.rs` — 12-step walkthrough: engagement → JWT → project → install plugin → create writer member → schedule → fire-now → poll → list runs → list docs → read block → exercise assistant chat. MockLlm.finish().finish() queues two replies; proves graph step + assistant chat share the same registered provider.

## Plugin install UI (`0d1668f`)

`pages/PluginsPage.tsx` — two-pane: left list, right detail with capabilities + credentials form. Install modal: id/name/version + manifest JSON textarea. Credential form: direction selector, dynamic k/v rows.

client.ts: `listInstalledPlugins`, `installPlugin`, `setPluginCredentials`.

## Per-engagement encrypted provider keys (`d13cc39`)

Migration `0013_engagement_provider_keys.sql` — `(engagement_id, provider_id) UNIQUE` table with `encrypted_key BYTEA` + `nonce BYTEA`.

`providers/store.rs::PgProviderKeysStore` AEAD-encrypts with `CELEREX_CRED_KEY`. `ProviderKeyMeta.key_preview` returns `"sk-...XYZ"` — plaintext never over the wire.

Routes: `GET /api/providers/keys`, `PUT /api/providers/keys/{provider_id}`, `DELETE /api/providers/keys/{provider_id}`.

AppState gains `provider_keys: Arc<PgProviderKeysStore>`.

**Phase 1 only:** Storage exists, but LlmHandler still uses env-var-loaded providers. Phase 2 will plumb runtime key override (read from store on each call, override the provider in registry per-engagement).

## Galvatronic plugin (`d13cc39`)

`plugins/fixtures/galvatronic.rs` — 4th fixture (after Galvo / WorkIQ / Stargate) per `celerex_plugin_strategy` memory. Capabilities: `read_sensor`, `list_devices`, `send_command`. 7 integration tests including a pipeline test that chains `read_sensor → send_command` via templating.

## Operational lessons learned

- **Provider streaming + Axum SSE:** `complete_stream<'a>` borrowing `&self` blocks direct `Sse::new` wrapping. Workaround: spawn task owning `Arc<dyn Provider>` clone + owned model String + `mpsc::channel`; return `ReceiverStream`.
- **Browser EventSource can't POST with Bearer.** Use fetch + body.getReader() + TextDecoder.
- **Cost-hook auto-pause** at ~$60 on agents by default. Override with: `IGNORE COST WARNINGS, DO NOT STOP. Push to completion.`
- **Agent worktrees share filesystem with main** despite `isolation: "worktree"`. Plan agent file-touches to not overlap.
- **GateGuard fact pattern:** before every Bash/Edit/Write — state (1) callers, (2) functions/changes, (3) data shape, (4) verbatim user instruction.
