# Document Servicers Phase 2 — Agent-Type Model Defaults

Shipped: 2026-05-20 — `6e19413` (the `apply_agent_type_defaults` half)

## Problem

Phase 1 (`6ac87b1`) added the `agent_type` column to `project_members` and accepted `"reasoning"` / `"writer"` on `POST /api/project_members`, but nothing in the runtime consumed the column — every LLM node still routed through whatever `model` / `provider_id` the user happened to put in the graph, and unset fields hit the executor with no defaulting. The product principle (see `celerex_agent_types` memory) is that Project Members (reasoning co-workers) get Sonnet-class models and Document Servicers (writers) get Haiku-class models *by default*, with explicit overrides always winning. Phase 2 actually wires that routing.

## Design

A single in-memory transformation applied at run-start, before graph compilation:

```rust
pub fn apply_agent_type_defaults(graph: &mut serde_json::Value, agent_type: &str)
```

Lives in `packages/backend/api/src/project_members/run_job.rs`. Contract:

1. For each entry in `graph.nodes[]` where `config.execution == "llm"`:
   - If `config.model` is missing or null, set it to:
     - `"claude-haiku-4-5"` when `agent_type == "writer"`
     - `"claude-sonnet-4-6"` otherwise (reasoning + unknown values fall through to Sonnet)
   - If `config.provider_id` is missing or null, set it to `"default"` (the registry alias for the house provider).
2. Values the user explicitly set are NEVER overwritten — `"model": "claude-opus-4-7"` on a writer node stays put.
3. Non-LLM nodes (execution `http`, `script`, `doc`, etc.) are left strictly alone.
4. Malformed graphs (no `nodes` key, `nodes` not an array, node without a `config`, `config` not an object) are silent no-ops — `apply_agent_type_defaults` never panics, and the executor will reject obviously-bad input downstream with a clearer error.

Constants live at the top of the file so the writer/reasoning split is visible at a glance:

```rust
pub const DEFAULT_REASONING_MODEL: &str = "claude-sonnet-4-6";
pub const DEFAULT_WRITER_MODEL:    &str = "claude-haiku-4-5";
pub const DEFAULT_PROVIDER_ID:     &str = "default";
```

### Injection point

In `enqueue_run`, after fetching the member and snapshot, before `graph::compile(...)`:

```rust
let mut graph = member.graph_json.clone();
apply_agent_type_defaults(&mut graph, &member.agent_type);
let steps = graph::compile(&graph)?;
```

`enqueue_run` is the single shared entry point used by both the manual `POST /api/project_members/{id}/run` route and the scheduler's dispatch closure (see `build_app_with_dispatcher_inner`), so writer routing applies uniformly to both manual and scheduled invocations.

### Audit-friendly mutation

The snapshot row inserted into `runs.snapshot_id` still references the original, untouched `member.graph_json` — `apply_agent_type_defaults` mutates only the local `graph` value handed to the compiler. Replays and audits show what the user actually wrote; the inserted defaults exist purely in the live `JobRequest::steps` pipeline. This matters for "why did my Haiku run cost what it did?" forensics: the snapshot doesn't lie about whether the user asked for Haiku explicitly or got it by default.

## Test coverage

`run_job::tests` (8 unit tests, all on `apply_agent_type_defaults` directly — no DB needed):

- `writer_gets_haiku_default_for_llm_nodes`
- `reasoning_gets_sonnet_default_for_llm_nodes`
- `existing_model_is_not_overwritten` — writer member with explicit `"claude-opus-4-7"` + `"openrouter"` stays put
- `non_llm_nodes_are_left_alone` — http/script/doc nodes never get a `model` field added
- `missing_provider_id_filled_with_default` — preserves user-supplied model, only fills `provider_id`
- `null_model_is_treated_as_missing` — explicit `null` is replaced (matches the "missing key" branch)
- `unknown_agent_type_falls_back_to_sonnet` — `"alien"` hits the `_ =>` arm
- `malformed_graph_does_not_panic` — exhaustively covers no-nodes-key, nodes-as-string, no-config, config-as-string

## What's not done yet

- No per-engagement model defaults — every engagement uses the same global Sonnet/Haiku pair.
- No per-provider model defaults — e.g. a writer routed through `provider_id: "kimi"` still gets `claude-haiku-4-5` requested against the Kimi endpoint, which will 4xx. The right fix is a `(provider_id, agent_type) → model` table, but that's deferred until the multi-provider UI surfaces it.
- No "show me what defaults will apply" preview on the Save path — the user only finds out by running.
- Phase 3 (deferred): differing LLM tool-use behaviour for writers vs reasoners — writer tool sets more constrained (doc append/replace only), reasoner tool sets broader (web search, A2A, plugin actions).
- No telemetry on how often defaults trigger vs. user-supplied values — useful for evaluating whether the defaults match real usage.
