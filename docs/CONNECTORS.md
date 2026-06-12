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

## 4. Galvo Hub mTLS A2A federation — `execution: "galvo"` + `transport: "hub"` ✅ round-trip verified

Celerex is a first-class participant in Galvo's cross-org agent federation
(`connectors/galvo_hub.rs`), reusing Galvo's own `galvo-federation` crate for
all crypto — X25519 ECDH + HKDF-SHA256 + ChaCha20-Poly1305 sealing, Ed25519
sender sigs — so the wire format can never drift from the Hub.

- **Register** (`POST /api/galvo/hub/register`): generate A2A keys + a CSR,
  exchange for an issued client cert over the Hub's TLS listener, persist a
  keystore (`GALVO_HUB_KEYSTORE`, default `./galvo-hub-keys.json`). Optional
  `elevate_tier` raises the org's trust tier (bootstrap-gated) so it clears
  targets' default `Verified` policy.
- **Invoke** (flow node `galvo` with `transport: "hub"`, or
  `POST /api/galvo/hub/invoke`): lookup → request cap token → seal envelope →
  `hub.relay`. The target `agent` is a federation handle (`org/agent`).
- **Receive**: `GalvoHubClient::run_receiver` connects to `/push`, completes the
  Hub's signed-nonce challenge-response handshake, then decrypts each relayed
  envelope (verifying the sender's Ed25519 sig via `hub.lookup_agent`). This is
  what makes Celerex agents *callable* over A2A, not just callers.
- **Status**: `GET /api/galvo/hub/status`.
- **Run the Hub** (local): `HUB_BOOTSTRAP_TOKEN=… cargo run -p galvo-hub --
  --addr 127.0.0.1:8443 --data-dir ./hub-data [--plaintext-ws-port 8444]`. The
  CA lands at `./hub-data/ca.crt` (point `GALVO_HUB_CA_PATH` at it).
- **Verified (2026-06-10):** `tests/galvo_hub_federation.rs` boots the REAL Hub
  in-process and drives a full encrypted A2A round trip through Celerex's own
  client + receiver (register both ends → elevate → relay → decrypt → assert
  plaintext), plus a trust-policy rejection. The mTLS Hub A2A path is no longer
  deferred — it's live.

## Tests
- `cargo test -p celerex-api --lib connectors` — wiremock unit tests (memory
  store/search, galvo control-plane invoke, m365 token→publish / graph_get /
  missing-creds, galvo-hub CSR + keystore).
- `cargo test -p celerex-api --test galvo_hub_federation` — the in-process Hub
  mTLS A2A round trip + trust-gating rejection.
