# `feat/cloud-frontend-ui` Branch Audit

Status: read-only audit, 2026-05-20.

## Branch state

`feat/cloud-frontend-ui` is **57 commits ahead of main** (last commit `c1e60f0`). All commits predate the product pivot from "agentic dev workspace" to "document-centric agentic automation for business users" (see `celerex_vision` memory).

## Topic buckets

### Backend
- **LLM providers** — Anthropic + OpenAI implementations (pre-pivot, less complete than main's current registry)
- **Cloud infrastructure** — Azure VM provisioning, cloud credentials, Docker executor (`bollard`), ACI/Functions executors
- **Pipeline orchestration engine** — real execution (superseded by main's `celerex-orchestrator::Executor`)
- **Code indexing** — tree-sitter-based codebase indexing with token-efficient symbol retrieval + transactional DB persistence
- **Tenant isolation** — JWT engagement-scoped middleware (relevant; main has Claims pattern but the middleware shape might be reusable)
- **MCP code-index server** — stdio JSON-RPC server (per `celerex_plugin_strategy` memory, MCP is case-by-case)
- **Memory store** — confidence-tracked, namespace-filtered project memory (the Memory Explorer view backs onto this)

### Frontend
- **Cloud VMs view** — Azure provisioning UI, credential management
- **Memory Explorer view** — namespace filtering, confidence display
- **Code Index view** — file tree + symbol browser
- **Pipeline Monitor with live WebSocket** — Yjs document sync, diff-style state comparison, breakpoint UI
- **Agent Console redesign** — single TUI + tab strip pattern

## Salvageable (worth deeper review)

| Area | Why | Notes |
|---|---|---|
| Code indexing (tree-sitter) | Not in main; useful for Document Servicers tooling | Big bundle — would need its own DB tables + migration |
| Tenant isolation middleware | Security layer beyond current Claims-based 403 checks | Worth comparing to main's per-handler `engagement_id_for` pattern |
| MCP code-index server | Bidirectional plugin per memory; not yet implemented in main | Standalone binary, minimal main-tree impact |
| Yjs doc-sync relay | Could enable real-time collab on docs | Speculative — out of current product scope |

## Obsolete (superseded by main)

| Branch artifact | Replaced by |
|---|---|
| Anthropic/OpenAI provider scaffolding | `a29df05` + `ae2861f` + `6462309` (full 4-provider registry with streaming) |
| Workflow Designer | `AgentBuilderCanvas` |
| Engagement/project CRUD | `b19d20f` (`EngagementManager`) |
| Pipeline execution engine | `celerex-orchestrator::Executor` |
| Basic chat / LLM tool dispatch | `0d1668f` + `ca4e78e` (streaming + assistant chat endpoint) |
| Workflow Monitor live WS | `e97bd5c` (MonitorPage SSE wiring) |

## Recommendation

1. **Tag the tip** as `archive/cloud-frontend-ui-2026-pre-pivot` so the branch is durable but not in the active branch list:
   ```sh
   git tag archive/cloud-frontend-ui-2026-pre-pivot c1e60f0
   git push origin archive/cloud-frontend-ui-2026-pre-pivot
   ```
2. **Delete the branch + worktree:**
   ```sh
   git worktree remove .worktrees/cloud-frontend-ui
   git branch -D feat/cloud-frontend-ui
   git push origin --delete feat/cloud-frontend-ui
   ```
3. **Defer cherry-picks** until the relevant slice is in scope:
   - Code indexing — when Document Servicers Phase 3 needs file-aware writers
   - Tenant middleware — during a security hardening pass
   - MCP server — when an external client needs to consume Celerex via MCP

Don't merge the branch wholesale — it predates the product pivot and 80%+ of the surface area is now obsolete or actively conflicting with main's design.

## What's NOT in this audit

This audit is based on commit message summaries only (per cost constraint mid-session). Each "salvageable" item above needs a deeper file-diff review before cherry-picking — the surface area might be tangled with obsolete code (e.g., the tenant middleware may depend on the old Provider trait shape).
