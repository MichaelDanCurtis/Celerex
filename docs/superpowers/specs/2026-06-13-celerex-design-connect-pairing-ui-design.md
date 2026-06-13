# Connect to Celerex — Device-Pairing UI (design)

**Status:** Draft for review · 2026-06-13
**Builds on:** handoff piece C (pairing endpoints `/api/design/pair/{start,approve,poll}`), the provider gateway (`/api/design/providers`, `/api/design/llm/*`, `design.device` scope), and `@celerex/connect` (`startPairing`, `pollOnce`, `fetchProviderConfig`, `buildSeedPlan`), plus the daemon `POST /api/celerex/connect` (Task 6).
**Specs nearby:** `2026-06-12-design-handoff-contract.md`, `2026-06-12-celerex-design-provider-gateway-design.md`.

## 1. Goal

Let a user connect the **Celerex Design** desktop app to a Celerex workspace via **device pairing** (the desktop never holds the user's full session token), then auto-seed its provider config. Device-flow shape: the desktop shows a short code; the user approves it in Celerex; the desktop receives a scoped `design.device` token and seeds.

## 2. Decisions (settled with user, 2026-06-13)

1. **Connect UX = device pairing** (not paste-token).
2. **Daemon-orchestrated:** the daemon runs `startPairing`/`pollOnce`; the `device_code` and the returned device token live server-side. The browser only sees the short `user_code` and a status. (Consistent with `/api/celerex/connect` already being on the daemon; most secure.)
3. **Scope = both surfaces of the pairing flow in one spec:** the Celerex "Approve Device" web UI **and** the design-app connect panel + daemon pairing routes. **Out:** "Publish to Celerex" (separate cycle), provider-catalog trim, re-brand.

## 3. Two surfaces

### 3.1 Celerex web — "Approve Device" UI  (repo: `Celerex/packages/frontend`)
The endpoint `POST /api/design/pair/approve` (body `{ user_code }`, `AuthClaims` session) already exists; it has no UI. Add a minimal approval surface in the Celerex frontend (React + jotai), mirroring the existing settings/section patterns:
- A "Devices" section (or a small card in Settings) with a `user_code` text input (auto-uppercase, hyphen-format hint `XXXX-XXXX`) and an **Approve** button.
- On submit: `POST /api/design/pair/approve` with the session token; show success (the returned `label`, e.g. the device name) or the error (`no pending pairing for that code` → "Code not found or expired").
- No new Celerex backend — endpoint exists.

### 3.2 Design-app daemon — pairing orchestration  (repo: `celerex-design/apps/daemon`)
Refactor Task 6's connect logic into a shared helper, then add two routes (mirroring the existing `/api/celerex/connect` route + its local-origin guard + JSON error style):

```ts
// shared (extracted from the existing /api/celerex/connect handler)
async function seedFromToken(celerexBaseUrl, deviceToken, mode, providerId, model) {
  const cfg = await fetchProviderConfig(celerexBaseUrl, deviceToken);
  const provider = cfg.providers.find(p => p.id === providerId)
    ?? cfg.providers.find(p => p.has_key) ?? cfg.providers[0];
  if (!provider) throw new Error("no providers configured in Celerex");
  const copiedKey = mode === "copy"
    ? await fetchProviderKey(celerexBaseUrl, deviceToken, provider.id) : undefined;
  const plan = buildSeedPlan({ mode: mode ?? "proxy", celerexBaseUrl, deviceToken, provider, model, copiedKey });
  const current = await readAppConfig(RUNTIME_DATA_DIR);
  await writeAppConfig(RUNTIME_DATA_DIR, {
    ...current, agentId: plan.daemon.agentId,
    agentModels: { ...(current.agentModels ?? {}), ...plan.daemon.agentModels },
    agentCliEnv: { ...(current.agentCliEnv ?? {}), ...plan.daemon.agentCliEnv },
  });
  return { web: plan.web, agentId: plan.daemon.agentId, provider: provider.id };
}
```
- `POST /api/celerex/pair/start  { celerexBaseUrl }` → `startPairing(celerexBaseUrl)` → returns `{ user_code, device_code, expires_at, poll_interval }`. (`device_code` is a short-lived poll handle the same-origin renderer holds during the pairing window to pass back on status polls; the scoped device token is never returned by `start` — only by `status` on success, as the proxy `apiKey`. If we later want `device_code` to stay daemon-side, swap it for an opaque pairing id; not needed for v0.1 localhost/same-origin.)
- `POST /api/celerex/pair/status  { celerexBaseUrl, device_code, mode?, providerId?, model? }` → `pollOnce(celerexBaseUrl, device_code)`:
  - `approved` + token → `await seedFromToken(celerexBaseUrl, token, mode, providerId, model)` → `{ status: "connected", ...seedResult }`.
  - `pending` → `{ status: "pending" }`; `expired` → `{ status: "expired" }`.
- The existing `POST /api/celerex/connect` keeps working (now delegates to `seedFromToken`).

### 3.3 Design-app web — "Connect to Celerex" panel  (repo: `celerex-design/apps/web`)
A new `'celerex'` section in `SettingsDialog.tsx` rendering `CelerexConnectPanel` (props `{ cfg, setCfg }`), mirroring the Composio/AMR connect panels:
- **Disconnected:** a Celerex **server URL** field + **Connect** button.
- **Connecting:** POST `/api/celerex/pair/start`; display the `user_code` prominently with *"Open Celerex → Settings → Devices and enter this code."* Then poll `/api/celerex/pair/status` every `poll_interval` seconds (mirror `fetchVelaLoginStatus`'s polling), passing `device_code`.
- **Connected:** on `status:"connected"`, persist `web` into `AppConfig` (set `mode/apiProtocol/baseUrl/apiKey/model` via `setCfg` + `saveConfig` + `syncConfigToDaemon`), store `celerex: { connected:true, serverUrl, provider }`, success toast. Show connected state (server URL, provider/model) + **Disconnect** (clears `celerex` + resets `mode` to `daemon`/prior).
- **Expired/error:** toast + reset to disconnected. "No providers configured in Celerex" → message guiding the user to add a provider key in Celerex first.
- **Provider/model:** v0.1 defaults to proxy mode + first `has_key` provider + its `default_model`. A small picker (options from the connected response / `fetchProviderConfig`) lets the user change provider/model and re-apply (re-runs status/connect with the chosen values). Mode toggle (proxy/copy) is a secondary control, proxy default.

### 3.4 AppConfig addition  (`celerex-design/apps/web/src/state/config.ts`)
```ts
celerex?: { connected: boolean; serverUrl: string; provider?: string };
```
(The device token itself rides in the existing `apiKey` field via the seed's web-half in proxy mode — a scoped `design.device` token, acceptable in localStorage; never the user's session token.)

## 4. Data flow

```
design web ──/pair/start──▶ daemon ──startPairing──▶ Celerex /pair/start
   ◀── user_code ───────────┘
user ──user_code──▶ Celerex "Approve Device" UI ──/pair/approve(session)──▶ binds + mints token
design web ──/pair/status (poll)──▶ daemon ──pollOnce──▶ Celerex /pair/poll
                                       └─ approved+token ─▶ seedFromToken (writes app-config CLI half)
   ◀── {status:connected, web} ──────┘  → web persists API-mode half to localStorage
```

## 5. Error handling
- Pairing expiry (10 min, piece C) → `status:"expired"` → panel resets, toast.
- Daemon can't reach Celerex (bad URL / offline) → `/pair/start` 502 → toast "Couldn't reach Celerex at <url>".
- No provider keys in the engagement → `seedFromToken` throws → `/pair/status` surfaces a clear message; panel shows guidance.
- Poll loop bounded (reuse `@celerex/connect` `waitForToken`'s max-polls notion, or cap on the web side) — stop after `expires_at`.

## 6. Testing
- **Celerex approve UI** (`Celerex/packages/frontend`): component test — render the section, enter a code, assert it calls `POST /api/design/pair/approve` with the code + renders success/error. Match existing frontend test patterns (Vitest + Testing Library, as used elsewhere in the frontend).
- **Daemon pair routes** (`celerex-design/apps/daemon`): integration tests mirroring `celerex-connect-route.test.ts` — stub a Celerex server exposing `/api/design/pair/start`, `/api/design/pair/poll` (pending→approved), `/api/design/providers`; assert (a) `/pair/start` returns a `user_code`; (b) `/pair/status` returns `pending` then `connected`, and on connected `readAppConfig` has the seeded `agentId`/`agentCliEnv`. Use a temp `OD_DATA_DIR`.
- **Design-app web panel**: component test for the panel's state machine (disconnected → show code → connected) with a mocked daemon fetch, if the web app has a component-test harness; otherwise assert the panel's reducer/hook logic in isolation.
- `@celerex/connect` pairing/seed logic: already covered.

## 7. Build order
1. `celerex-design` daemon: extract `seedFromToken`, add `/api/celerex/pair/start` + `/pair/status` (+ tests).
2. `celerex-design` web: `CelerexConnectPanel` + `'celerex'` settings section + `AppConfig.celerex` (+ test).
3. `Celerex` frontend: "Approve Device" UI (+ test).
(1 and 3 are independent; 2 depends on 1. 3 is required for an end-to-end pairing to actually complete.)

## 8. Out of scope (follow-ups)
"Publish to Celerex" toolbar action; provider-catalog trim; `@open-design/*`→`@celerex/*` rename + app re-brand; multi-workspace/device management beyond a single connection.
