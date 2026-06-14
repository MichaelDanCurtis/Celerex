# Connectors via the Agent — Design Spec (Sub-project 2a)

**Date:** 2026-06-13
**Status:** Draft for review
**Scope:** Sub-project **2a** of the OmniVyx product re-architecture (spec #2). Replace the
Design app's Composio connectors with connectors served by the OmniVyx Agent as a
**credential vault**, executed **locally** in the Design daemon. Spans both repos.
**Repos:** `Celerex` (OmniVyx Agent — vault + catalog API) and `celerex-design`
(OmniVyx Design — daemon adopts creds + runs connectors locally).
**Builds on:** the provider gateway / device-pairing seam already shipped
(`/api/design/*`, `@celerex/connect`, `DeviceAuth`/`design.device` scope). See
[[celerex-design-provider-integration]], [[celerex_connectors]], [[celerex_plugin_strategy]].

---

## 1. Goal & the two execution planes

Replace Composio with our own connector layer, while **preserving how the Design app works
today**. Two AI/execution planes stay distinct:

- **Design's AI plane = local CLI / ACP** (Claude Code, Codex). Unchanged — this remains
  Design's "brain." The Agent does **not** and **cannot** use the local CLI.
- **Agent's plane = its own API providers + the credential vault.** The Agent holds API
  keys and connector credentials.

**The connector model (decided):** the Agent is a **key vault**. On pairing, the Design
daemon **adopts** connector credentials from the vault and **runs the connector calls
locally** — a direct extension of the provider auto-seed already built. Design's local-CLI
AI gains new local capabilities (connectors) without changing its execution model.
Server-side connector execution (the "run without my PC" case) is explicitly **out of
scope here** → sub-project **2e (Agent backplane / portable profile)**.

## 2. Decisions (locked in brainstorm 2026-06-13)

1. **v1 = framework + seed connectors, read-first.** Build the Agent's connector framework
   + ship 2-3 real connectors; do not replicate Composio's OAuth catalog.
2. **Daemon bridge over the device seam.** Design's daemon is the single integration point;
   it calls a new Agent connector API over the device token. No MCP server in v1.
3. **Vault-adopt, run local.** Agent stores creds; Design adopts on pairing and executes
   connectors locally, replacing Composio's cloud execution.
4. **Seed set:** M365 / Microsoft Graph (`app_creds`), galvo-memory (`internal`), and a
   **generic API-key / HTTP** connector (`api_key`).
5. **Credentials live only in the Agent vault**, per-engagement; Design adopts them locally
   (like BYOK). Design's local-CLI/ACP AI is untouched.

## 3. Architecture

```
 Design desktop (local plane)                    OmniVyx Agent (vault + authority)
 ┌────────────────────────────┐   device token   ┌──────────────────────────────┐
 │ local CLI/ACP AI (brain)   │  ───────────────► │ GET /api/design/connectors   │ catalog (no secrets)
 │ live-artifact / Ritual     │   on pairing      │ GET /api/design/connectors/  │ confirm-gated cred
 │   refreshers               │  ◄─────────────── │     {id}/credential          │
 │ daemon connector layer ────┼── runs LOCALLY ──►│ Connector registry + per-    │
 │  (adopted creds, .od/...)  │   with adopted    │   engagement credential vault│
 │ IntegrationsView (UI)      │   creds → Graph,  └──────────────────────────────┘
 └────────────────────────────┘   HTTP API, memory          (M365 app creds, API keys)
            │ local execution
            ▼
   Microsoft Graph / generic HTTP API / galvo-memory
```

The Agent **describes** connectors and **vaults** their credentials; the Design daemon
**adopts** and **executes** them. The Agent does not execute connectors in v1.

## 4. Components

### 4.1 Agent (Celerex) — `packages/backend/api`
- **`Connector` registry/trait** (new, under `src/connectors/` extending the existing
  Handler pattern). Each connector declares: `id`, `label`, `auth_kind`
  (`api_key` | `app_creds` | `internal`), and its **read** capability set — `tools`
  (name, description, input schema, all read-only in v1) and/or `data` (a named data-read
  shape, e.g. M365 "recent messages"). No write tools in v1.
- **Per-engagement connector credential store** — reuse `src/plugins/credentials.rs` +
  engagement scoping (same machinery as engagement provider keys). Secrets never appear in
  the catalog response.
- **Endpoints (under `DeviceAuth`, `design.device` scope):**
  - `GET /api/design/connectors` → `{ connectors: [{ id, label, auth_kind, configured,
    tools, data }] }`. No secrets. `configured` = whether the engagement has a stored cred.
  - `GET /api/design/connectors/{id}/credential` → confirm-gated raw credential
    (header `x-celerex-confirm-copy: true`, mirroring `/providers/{id}/key`) for the daemon
    to adopt. Returns `{ kind, value }` (e.g. an API key, or Graph app-creds bundle).
  - (No execute-on-server endpoint in v1 — execution is local.)
- **Agent-side config UI** (`packages/frontend/app`): a **"Connectors"** section in
  Settings (alongside `ProviderKeysSettings`) to enter/clear connector creds per engagement.

### 4.2 Seam — `@celerex/connect` (celerex-design `packages/celerex-connect`)
- `listConnectors(): ConnectorCatalog` — GET the catalog.
- `fetchConnectorCredential(id): { kind, value }` — confirm-gated, mirrors
  `fetchProviderKey`.
- Connector typedefs shared with the daemon.
- `buildSeedPlan` **extended** to include connector creds + catalog in the seed it writes to
  the daemon on connect (alongside the existing provider/agentCliEnv seed).

### 4.3 Design daemon — `celerex-design apps/daemon/src`
- **Reimplement** `connectors/routes.ts` + `tools/connectors.ts`
  (`listConnectorTools()` / `executeConnectorTool()`): read the catalog from the adopted
  seed; **execute connectors locally** with the adopted credential.
  - **Local executors** (new, `connectors/executors/`): `graph` (HTTP to Microsoft Graph
    with the adopted app-creds token), `httpApiKey` (configurable base URL + `Authorization`
    from the adopted key), `memory` (HTTP to galvo-memory).
- **Delete** `connectors/composio-config.ts`, `config.composio.apiKey`, and the Composio
  OAuth flow. `.od/connectors/composio-config.json` is replaced by an adopted-catalog file.
- `/api/connectors/*` routes keep their **external contract** (web UI unchanged in shape);
  `IntegrationsView.tsx`'s `ConnectorSection` now lists Agent connectors + their
  `configured` state, with a "configure in OmniVyx Agent" affordance for unconfigured ones.

## 5. Data flow

1. **Pair / connect** → daemon calls `listConnectors()` + (confirm-gated)
   `fetchConnectorCredential()` for each configured connector → `buildSeedPlan` writes the
   catalog + creds to `.od/connectors/` (local, like BYOK provider keys).
2. **Use** (Design AI tool-call, or a live-artifact / Ritual refresher needing data) →
   daemon `executeConnectorTool()` runs the matching local executor with the adopted cred →
   returns data to the caller.
3. **Re-sync** on reconnect (catalog + creds refreshed); offline = last adopted catalog.

## 6. Credential model

- Source of truth = **Agent vault**, per-engagement. Catalog responses carry **no secrets**;
  credential fetch is **confirm-gated** and only over `DeviceAuth`.
- Design stores adopted creds **locally** in the daemon (same trust model as today's BYOK
  provider keys). Revocation in v1 = re-pair / clear in the Agent; a dedicated revocation UI
  is deferred.

## 7. Error handling

- **Unconfigured connector** (no vault cred) → catalog marks `configured: false`; UI shows
  "configure in OmniVyx Agent"; tool/data calls return a typed `not_configured` error.
- **Invalid / expired adopted cred** → local executor surfaces the provider error; daemon
  marks the connector `needs_resync`; next pair/sync refreshes.
- **Agent unreachable** at pair/sync → keep the last adopted catalog (offline), surface a
  stale indicator; never hard-fail the Design app.
- **Read-only enforcement** → connector tool schemas expose read tools only in v1; any write
  tool name is rejected by the daemon with `write_not_supported`.

## 8. Testing

- **Agent** (`design_connectors.rs`): catalog shape (no secrets), confirm-gated credential
  endpoint (403 without confirm header), `DeviceAuth` enforcement (a `design.publish`-only
  token gets 403 on connector routes), per-engagement isolation.
- **Seam** (bun): `listConnectors` / `fetchConnectorCredential` against a wiremock Agent;
  `buildSeedPlan` includes connector creds.
- **Daemon** (vitest): `executeConnectorTool` runs the right local executor with the adopted
  cred against a mocked provider; `listConnectorTools` reflects the adopted catalog; an
  integration test asserting Composio is fully removed (no `composio` import / config path).

## 9. Out of scope (later sub-projects / connectors)

- **Server-side connector execution + the broader vault/profile/plugin portability and
  ephemeral-desktop vision → sub-project 2e (Agent backplane).** 2a is designed
  forward-compatible: the catalog/credential API and the `auth_kind`/capability model are
  the same primitives 2e will reuse to mark a connector `runs_on: server`.
- Interactive user-OAuth connectors (Gmail / Slack / GitHub).
- Connector **write** actions / tools.
- Connector revocation UI; multi-account per connector.
- The Orbit→Ritual rename (2b) and document-workspace migration (2c).

## 10. Open parameters

- Exact Graph scopes for the M365 read connector's seed data shape (recent messages /
  files) — pin at implementation.
- The generic HTTP connector's config surface (base URL, header name, optional path
  templating) — keep minimal in v1.
