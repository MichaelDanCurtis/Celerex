# Celerex Connectors & Federation

Three external-system connectors, each a first-class flow node
(`api/src/connectors/`, registered on the prod dispatcher). Drag them from the
**Memory & federation** palette group in the Flow Builder.

## 1. galvo-memory (Neo4j agent memory) — `execution: "memory"` ✅ live

Federates Celerex with the galvo-memory sidecar (`lucidity/galvo-memory`).

- **Nodes:** *Memory: recall* (`op: search`) and *Memory: store* (`op: store`).
- **Config:** `op`, `mem_label` (Decision / Pattern / Belief / Artifact / Session
  / …), `scope` (default `project:celerex`), `query`+`limit` (search) or `fields`
  (store).
- **Auto-memory:** every flow run is recorded as a `:Session` node
  (`run_job::enqueue_run` → `connectors::memory::log_run_session`, best-effort).
- **Env:** `GALVO_MEMORY_URL` (default `http://localhost:7575`),
  `CELEREX_MEMORY_SCOPE` (default `project:celerex`).
- **Run the sidecar:** `cd lucidity/galvo-memory/docker && docker compose up -d`
  (Neo4j `:7687`/`:7474` + sidecar `:7575`).
- **Verified (2026-06-10):** a flow with store→search ran `succeeded`; the Decision
  was written to Neo4j and recalled semantically (score 0.77); the run logged a
  `:Session`. Entity count grew on the live graph.

## 2. Microsoft 365 / Graph — `execution: "m365"` 🧪 unit-tested

Implements the "Build your first Copilot connector" flow: publish Celerex
documents into Microsoft 365 Copilot, and read Microsoft Graph.

- **Nodes:** *M365 Copilot publish* (`op: copilot_publish`) and *M365 fetch*
  (`op: graph_get`). Also `op: copilot_setup` (create connection + register schema).
- **copilot_publish:** `PUT /external/connections/{id}/items/{itemId}` with
  `acl` / `properties{title,url}` / `content{value,type}` → the doc becomes
  Copilot-searchable.
- **graph_get:** authenticated `GET {graph}/{path}` (e.g. `/users/{id}/messages`,
  `/sites/{id}/drive`).
- **Auth:** app-only client credentials. **Env:** `AZURE_TENANT_ID`,
  `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` (or per-node config). Needs an Entra
  app with `ExternalConnection.ReadWrite.OwnedBy` + `ExternalItem.ReadWrite.OwnedBy`
  (admin-consented) to run live.
- **Starter template:** "Publish to M365 Copilot" (LLM draft → doc → publish).

## 3. Galvo agent federation — `execution: "galvo"` 🧪 unit-tested

Invoke a Galvo agent (`lucidity/Galvo`) from a Celerex flow via the Forge
control plane (the shipping surface; the mTLS Hub A2A path is a separate, larger
integration left for later).

- **Node:** *Galvo agent* (`op: invoke`) → `POST {base}/agents/{agent}/run`
  with `{ "input": … }` and a Bearer token.
- **Env:** `GALVO_CONTROL_PLANE_URL` (default `http://localhost:8787`),
  `GALVO_CONTROL_PLANE_TOKEN`.
- **Run a control plane:** a Galvo instance via the Python SDK
  (`async with galvo.serve(port=8787): …`). Not running by default, so this is
  build-+unit-tested; point `GALVO_CONTROL_PLANE_URL` at a live instance to use it.

## Tests
`cargo test -p celerex-api --lib connectors` — 8 wiremock unit tests (memory
store/search, galvo invoke, m365 token→publish / graph_get / missing-creds).
