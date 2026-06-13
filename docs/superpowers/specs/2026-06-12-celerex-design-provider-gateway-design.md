# Celerex Design ⇄ Celerex — Provider Gateway & Auto-Seed (design)

**Status:** Draft for review · 2026-06-12
**Builds on:** handoff pieces A–C (`docs/plans/2026-06-12-design-handoff-contract.md`); the
`@celerex/connect` seam in the sibling `celerex-design` repo.
**Depends on:** PR #15 (piece C) being in `main` — provides `Claims.scope`, the `DesignAuth`
extractor, the device token, and the pairing handshake this design extends.

## 1. Goal

Let the **Celerex Design** desktop app (the open-design clone) get its AI provider
credentials and model config from the **paired Celerex workspace**, instead of being
configured separately. Celerex is the credential authority; the design app holds only its
pairing token. Both of open-design's execution modes (local coding-CLI **and** BYOK API)
keep working.

## 2. Decisions (settled with user, 2026-06-12)

1. **Hybrid + auto-seed.** Keep open-design's own local credential stores, but populate them
   from Celerex on connect.
2. **Credential flow = proxy by default, opt-in copy.** By default the device's credentials
   point at a **Celerex LLM gateway** (real keys never leave the server); the user may
   explicitly opt into copying raw keys for offline use.
3. **Keep both execution modes** (CLI + API). We only populate their config; we do not
   rebuild the call paths.
4. **Lean on ACP/CLI.** Reuse open-design's existing agent execution (per-agent
   `streamFormat`, incl. its ACP-family formats); we do not introduce a bespoke LLM client.
5. **Defer the "Connect to Celerex" settings UI** to a fast follow. This spec covers the
   seam contract, the Celerex endpoints, the `@celerex/connect` additions, and the daemon
   seeding — all testable headless.

## 3. Why proxy fits open-design exactly

open-design's daemon strips a CLI's real API key at spawn time **unless a `*_BASE_URL` is
set** (`apps/daemon/src/runtimes/env.ts`). Its intended pattern is therefore "point the CLI
at a gateway URL + a token." Proxy-through-Celerex is a glove fit: we set
`ANTHROPIC_BASE_URL=<celerex>/api/design/llm/anthropic` + `ANTHROPIC_AUTH_TOKEN=<device
token>` and the daemon does the rest.

## 4. The two credential homes on the device (seed targets)

open-design persists credentials in two places; **one connect action writes both**:

| Mode | Home | Fields we set |
|---|---|---|
| Local CLI | daemon `app-config.json → agentCliEnv[agentId]` (via `PUT /api/app-config`) | `{ <PROVIDER>_BASE_URL, <PROVIDER>_AUTH_TOKEN }` + `agentId`, `agentModels[agentId].model` |
| BYOK API | web `localStorage['open-design:config']` (`AppConfig`) | `mode:'api'`, `apiProtocol`, `baseUrl`, `apiKey`, `model` |

Provider → agent mapping for CLI seeding (v0.1): `anthropic → claude` (Claude Code),
`openai → codex`. The selected agent may override this.

## 5. Celerex side (new)

### 5.1 Token scope graduation
The paired device token graduates from `design.publish` to **`design.device`**.
- `pair_approve` and `POST /api/design/tokens` mint `design.device`.
- `DesignAuth` authorizes `design.device` for ingest + providers-config + llm-proxy.
- Legacy `design.publish` tokens remain accepted on **ingest only** (publish-only), rejected
  on proxy/providers — so an old token can't suddenly proxy LLM traffic.
- `AuthClaims` (full session) continues to reject all scoped tokens (unchanged).

### 5.2 `GET /api/design/providers`  (DesignAuth: device or session)
Returns the engagement's configured providers — **no secrets**:
```json
{
  "providers": [
    { "id": "anthropic", "label": "Anthropic (Claude)", "protocol": "anthropic",
      "has_key": true, "proxy_base_url": "/api/design/llm/anthropic",
      "models": ["claude-sonnet-4-5","claude-opus-4-5","claude-haiku-4-5"],
      "default_model": "claude-sonnet-4-5" }
  ]
}
```
`has_key` comes from `providers::store::list_for_engagement` (already returns masked meta).
Model lists are the curated set per provider (constant, mirrors Celerex's known models).

### 5.3 `ANY /api/design/llm/{provider}/{*rest}`  (DesignAuth: device, scope `design.device`)
An **Anthropic/OpenAI-compatible reverse proxy**:
1. Map `{provider}` → a **fixed allowlisted upstream base URL** (anthropic →
   `https://api.anthropic.com`, openai → `https://api.openai.com`). No caller-supplied
   target — prevents SSRF.
2. Look up the engagement's decrypted key (`providers::store::get`); 409 if none configured.
3. Forward method + `{*rest}` path + body to `upstream + /{rest}`, injecting the provider's
   auth header (Anthropic: `x-api-key` + `anthropic-version`; OpenAI: `Authorization:
   Bearer`). Strip the inbound Celerex `Authorization`.
4. Stream the upstream response (status + body) back, including SSE token streams.
Implemented with `reqwest` streaming; reuses the providers store for decryption. Only the
provider's own auth header is added — no Celerex headers leak upstream.

### 5.4 `GET /api/design/providers/{id}/key`  (DesignAuth: device + confirm)
The **opt-in copy** path. Requires header `x-celerex-confirm-copy: true`; returns
`{ provider_id, api_key, base_url }` (decrypted). Audit-logged (`tracing::warn!` with
engagement + provider, never the key). Without the confirm header → 400.

## 6. Device side — `@celerex/connect` additions (pure, unit-testable)

- `fetchProviderConfig(baseUrl, token, fetchFn?)` → the §5.2 payload.
- `fetchProviderKey(baseUrl, token, providerId, fetchFn?)` → §5.4 (copy mode only).
- `buildSeedPlan(input)` → the exact config to write, no I/O:
  ```ts
  buildSeedPlan({
    mode: 'proxy' | 'copy',
    celerexBaseUrl: string,
    deviceToken: string,
    provider: { id, protocol, proxy_base_url, default_model },
    model?: string,
    copiedKey?: { api_key, base_url },   // required when mode === 'copy'
  }): {
    daemon: { agentId, agentModels: Record<string,{model:string}>,
              agentCliEnv: Record<string, Record<string,string>> },
    web: { mode:'api', apiProtocol, baseUrl, apiKey, model },
  }
  ```
  - **proxy:** `baseUrl = celerexBaseUrl + provider.proxy_base_url`, `apiKey = deviceToken`,
    CLI env `{ <PROVIDER>_BASE_URL, <PROVIDER>_AUTH_TOKEN }` = same.
  - **copy:** `baseUrl = copiedKey.base_url`, `apiKey = copiedKey.api_key`.
- `CelerexConnection` gains `seedFrom({ mode, providerId?, model?, applyDaemon, applyWeb })`
  where `applyDaemon`/`applyWeb` are caller-supplied sinks (the daemon route / web store) —
  keeps `@celerex/connect` I/O-free and runtime-agnostic.

## 7. Device side — open-design daemon

New route `POST /api/celerex/connect` in `apps/daemon/src/server.ts`:
- Body: `{ celerexBaseUrl, deviceToken, mode, providerId, model? }`.
- Calls `fetchProviderConfig` (+ `fetchProviderKey` when `mode==='copy'`), runs
  `buildSeedPlan`, **writes the daemon half** via the existing `writeAppConfig` (agentId,
  agentModels, agentCliEnv), and **returns the web half** so the web client persists it to
  `localStorage` (the daemon never stores the web BYOK blob).
- Reuses `app-config.ts` allowlist (already permits `agentCliEnv`); no schema change there.

## 8. Security

- Default path keeps provider keys server-side; the device holds only its `design.device`
  token (90-day TTL, revocable via future pairing-revoke).
- Proxy targets are a fixed allowlist (no caller-controlled upstream → no SSRF).
- Raw-key copy is explicit, confirm-gated, and audit-logged.
- `design.device` ≠ `design.publish`: a publish-only token cannot proxy LLM traffic.
- Inbound Celerex auth header is stripped before forwarding upstream.

## 9. Testing

- **`@celerex/connect` (bun, zero-dep):** `buildSeedPlan` proxy-vs-copy × anthropic-vs-openai
  (asserts exact env + web config); `fetchProviderConfig`/`fetchProviderKey` with mock fetch.
- **Celerex e2e (`design_gateway.rs`):**
  - providers-config lists a configured provider with `has_key:true`, masks secrets.
  - llm-proxy: device token → mock upstream (via a `ProviderBases`-style override) → asserts
    the injected provider auth header + relayed body/stream; 409 when no key.
  - key-copy: returns decrypted key with confirm header; 400 without.
  - scope: `design.device` required for proxy; legacy `design.publish` rejected on proxy,
    accepted on ingest.
- **daemon connect route:** asserts both config homes get the right values for each mode
  (using a stub Celerex base URL).

## 10. Scope

**In:** token-scope graduation; the three Celerex endpoints; `@celerex/connect` seed logic;
the daemon connect route; tests for all of the above.
**Out (fast follow):** the open-design "Connect to Celerex" Settings UI panel; media-gen
provider seeding; pairing/token revocation UI; curating/trimming open-design's built-in
provider catalog and re-brand.

## 11. Build order

1. Celerex: `design.device` scope + `DesignAuth` update (+ keep publish-on-ingest).
2. Celerex: `GET /api/design/providers`.
3. Celerex: `ANY /api/design/llm/{provider}/{*rest}` proxy.
4. Celerex: `GET /api/design/providers/{id}/key` (copy, gated).
5. `@celerex/connect`: `fetchProviderConfig`/`fetchProviderKey`/`buildSeedPlan` (+ tests).
6. open-design daemon: `POST /api/celerex/connect` (+ test).
7. (Fast follow) Settings UI.
