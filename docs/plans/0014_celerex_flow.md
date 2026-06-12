# Celerex Flow — Overnight Build Plan

**Goal:** Turn the rough "Agent Builder" into a gorgeous, fully-wired **n8n / LangFlow-style
visual flow editor** that unifies docs, agents, plugins, and connectors — and make it run
end-to-end. Branch: `feat/celerex-flow-editor`.

## Key architectural facts (verified firsthand)
- A "flow" already exists as `project_members.graph_json` (`{nodes,edges}`, types
  `inbound|action|filter|outbound`). `graph::compile` → orchestrator `Step`s; `enqueue_run`
  runs them on the DAG `Executor`; step events stream via `RunBus` → `GET /api/runs/{id}/events`.
- LLM handler wired for `ExecutionType::Llm` (anthropic/openai/kimi/gemini from env).
- Frontend already has `@xyflow/react` canvas (`AgentBuilderCanvas`), `BuilderPage`,
  `MonitorPage` (live SSE via `useRunEvents`), an 8-item palette, BlockSuite doc editor.

## Confirmed gaps (what "half-assed" means here)
1. **No CORS** → Vite (5173) cannot call API (3001). HARD BLOCKER.
2. **`ExecutionType::Doc` has no handler** → `document.create` outbound nodes fail at runtime.
   This is the doc-builder ("one doc feeds another") story. MUST.
3. **SSE never emits `run_step_started` / `run_completed`** though the frontend expects them →
   live monitor can't show running/completion. MUST.
4. **Canvas config round-trip is lossy**: palette-add drops `type`+`defaultConfig`;
   `fromReactFlow` flattens `config` to `{label}`. → nodes built on canvas can't execute. MUST.
5. Filter nodes are a no-op (no branching). SHOULD.
6. Raw-JSON-textarea node config; no drag-drop; action-only live styling; no Flows landing. SHOULD.

## Waves
### Wave 1 — Backend (Rust): make nodes real + runnable
- [ ] CORS layer (tower-http) in `build_app_with_dispatcher_inner`.
- [ ] Doc handler (`ExecutionType::Doc`): ops create/append/read over documents+doc_blocks; register it.
- [ ] SSE completeness: `RunStore::started` + `RunStore::completed` defaults; executor calls them;
      `PgRunStore` publishes `RunStepStarted` / `RunCompleted` (+ updates runs.status on completion).
- [ ] Filter evaluation: evaluate `expression` over upstream outputs; gate/skip dependents.
- [ ] Tests for all; keep existing green.

### Wave 2 — Frontend: fix canvas round-trip (core bug)
- [ ] Thread full `config` + category through ReactFlow node `data`; preserve in `fromReactFlow`.
- [ ] Apply `defaultConfig`+`type` on palette-add.

### Wave 3 — Frontend: the gorgeous n8n/LangFlow experience
- [ ] Beautiful node cards (icons, colored headers, config summary, typed handles, live states on all types).
- [ ] Dark canvas, dotted grid, MiniMap, styled Controls, run toolbar, animated edges.
- [ ] Drag-and-drop palette → canvas; richer categorized palette (trigger/LLM/doc read+write/HTTP/filter/plugin/script).
- [ ] Structured config forms per node type w/ "insert upstream output" `{{steps.X.outputs.Y}}` helper.
- [ ] First-class **Flows** surface: landing/list + New Flow + nav entry + starter templates (3 personas).

### Wave 4 — Seed + verify end-to-end
- [ ] Seed engagement/project + starter flow.
- [ ] Run Postgres (docker) + API + Vite; browser-verify build → run → nodes light up.
- [ ] cargo test + vitest + frontend build all green.

## Verification gates
- `cargo check --workspace` and `cargo test` (Docker testcontainers) green.
- `bun run build` (tsc -b + vite) green; `vitest run` green.
- Manual browser run: open Flow Builder, add nodes via drag, wire, configure, Run, watch live.
