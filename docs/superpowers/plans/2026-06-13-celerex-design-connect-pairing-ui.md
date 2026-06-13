# Connect to Celerex — Device-Pairing UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user connect Celerex Design to a Celerex workspace via device pairing — show a code, approve it in Celerex, auto-seed provider config.

**Architecture:** Daemon-orchestrates pairing (holds `device_code`/token); the web shows only the `user_code` + status. Reuses `@celerex/connect` (`startPairing`/`pollOnce`/`fetchProviderConfig`/`buildSeedPlan`) and Task 6's seed logic (extracted into a shared `seedFromToken`). A Celerex-side "Approve Device" UI calls the existing `/api/design/pair/approve`.

**Tech Stack:** TypeScript/Express (celerex-design daemon), React + Next.js (celerex-design web), React + jotai (Celerex frontend), bun/vitest tests.

**Two repos:**
- **CD** = `/Volumes/Main External/Development/celerex-design` (local git, branch `main`).
- **CX** = `/Volumes/Main External/Development/Celerex` (branch `feat/celerex-connect-ui`, based on the gateway HEAD so the pairing endpoints exist).

**Spec:** `Celerex/docs/superpowers/specs/2026-06-13-celerex-design-connect-pairing-ui-design.md`.

---

## File Structure

**CD daemon (`apps/daemon/src/`):**
- `server.ts` — extract `seedFromToken(...)` from the existing `/api/celerex/connect` handler; add `/api/celerex/pair/start` + `/api/celerex/pair/status` (modify).
- `tests/celerex-pair-route.test.ts` — integration test (create).

**CD web (`apps/web/src/`):**
- `state/config.ts` — add `celerex?` to `AppConfig` + `DEFAULT_CONFIG` (modify).
- `components/CelerexConnectPanel.tsx` — the panel (create).
- `components/SettingsDialog.tsx` — add a `'celerex'` nav section + render the panel (modify).
- `components/CelerexConnectPanel.test.tsx` — panel state-machine test (create, if web has a component-test harness).

**CX frontend (`packages/frontend/app/src/`):**
- `api/client.ts` — add `approveDevice(token, userCode)` (modify).
- `components/ApproveDeviceSettings.tsx` — the approve UI (create).
- wire into the Settings page (modify whatever renders settings sections — discover on read).
- a component test for the approve UI (create).

---

## Task 1 — Daemon pairing routes [CD]

Work in `/Volumes/Main External/Development/celerex-design`.

**Files:** modify `apps/daemon/src/server.ts`; create `apps/daemon/tests/celerex-pair-route.test.ts`.

**Context:** `server.ts` already has `POST /api/celerex/connect` (added in Task 6) that inlines: `fetchProviderConfig` → find provider → `buildSeedPlan` → `readAppConfig`/`writeAppConfig(RUNTIME_DATA_DIR, ...)` → returns `{ web, agentId }`. READ that handler first. It also has `isLocalSameOrigin(req, resolvedPort)` guards and `express.json()` body parsing. `@celerex/connect` exports `startPairing(baseUrl)`, `pollOnce(baseUrl, deviceCode)`, `fetchProviderConfig`, `fetchProviderKey`, `buildSeedPlan`.

- [ ] **Step 1: Write the failing integration test** `apps/daemon/tests/celerex-pair-route.test.ts`, mirroring `apps/daemon/tests/celerex-connect-route.test.ts` (read it for the harness: temp `OD_DATA_DIR`, `startServer({port:0,returnServer:true})`, a local `http` stub for Celerex, `readAppConfig`). The Celerex stub must serve: `POST /api/design/pair/start` → `{user_code:"WXYZ-1234", device_code:"dev-1", expires_at:"...", poll_interval:1}`; `POST /api/design/pair/poll` → first call `{status:"pending"}`, second `{status:"approved", token:"tok"}`; `GET /api/design/providers` → `{providers:[{id:"anthropic",label:"A",protocol:"anthropic",has_key:true,proxy_base_url:"/api/design/llm/anthropic",models:["claude-sonnet-4-6"],default_model:"claude-sonnet-4-6"}]}`. Assertions:
  - `POST /api/celerex/pair/start {celerexBaseUrl}` → 200, body has `user_code:"WXYZ-1234"` and `device_code:"dev-1"`.
  - `POST /api/celerex/pair/status {celerexBaseUrl, device_code:"dev-1"}` → first returns `{status:"pending"}`; second returns `{status:"connected"}` with `web.mode==="api"`, `web.baseUrl` ending `/api/design/llm/anthropic`, `web.apiKey==="tok"`.
  - After connected, `readAppConfig(dataDir)` has `agentId==="claude"` and `agentCliEnv.claude.ANTHROPIC_BASE_URL` set.

- [ ] **Step 2: Run → fail.** `pnpm --filter @open-design/daemon test -- celerex-pair-route` (use the repo's actual test command — discover from `package.json`/the existing test). Expect FAIL (routes missing).

- [ ] **Step 3: Extract `seedFromToken` and add the routes** in `server.ts`. Refactor the existing `/api/celerex/connect` body into a shared async helper (place it near the route), then have BOTH `/connect` and `/pair/status` call it:
```ts
async function seedFromToken(
  celerexBaseUrl: string, deviceToken: string,
  mode: 'proxy' | 'copy', providerId?: string, model?: string,
) {
  const cfg = await fetchProviderConfig(celerexBaseUrl, deviceToken);
  const provider =
    (providerId ? cfg.providers.find((p) => p.id === providerId) : undefined) ??
    cfg.providers.find((p) => p.has_key) ?? cfg.providers[0];
  if (!provider) throw new Error('no providers configured in Celerex');
  const copiedKey = mode === 'copy'
    ? await fetchProviderKey(celerexBaseUrl, deviceToken, provider.id) : undefined;
  const plan = buildSeedPlan({ mode, celerexBaseUrl, deviceToken, provider, model, copiedKey });
  const current = await readAppConfig(RUNTIME_DATA_DIR);
  await writeAppConfig(RUNTIME_DATA_DIR, {
    ...current,
    agentId: plan.daemon.agentId,
    agentModels: { ...(current.agentModels ?? {}), ...plan.daemon.agentModels },
    agentCliEnv: { ...(current.agentCliEnv ?? {}), ...plan.daemon.agentCliEnv },
  });
  return { web: plan.web, agentId: plan.daemon.agentId, provider: provider.id };
}
```
Update the existing `/api/celerex/connect` handler to `return res.json(await seedFromToken(celerexBaseUrl, deviceToken, mode === 'copy' ? 'copy' : 'proxy', providerId, model));` (preserving its existing validation + 400/404/502 handling). Then add:
```ts
app.post('/api/celerex/pair/start', async (req, res) => {
  if (!isLocalSameOrigin(req, resolvedPort)) return res.status(403).json({ error: 'cross-origin request rejected' });
  const { celerexBaseUrl } = req.body ?? {};
  if (!celerexBaseUrl) return res.status(400).json({ error: 'celerexBaseUrl required' });
  try {
    const p = await startPairing(celerexBaseUrl);
    return res.json({ user_code: p.userCode, device_code: p.deviceCode, expires_at: p.expiresAt, poll_interval: p.pollInterval });
  } catch (e) {
    return res.status(502).json({ error: `pair/start failed: ${(e as Error).message}` });
  }
});

app.post('/api/celerex/pair/status', async (req, res) => {
  if (!isLocalSameOrigin(req, resolvedPort)) return res.status(403).json({ error: 'cross-origin request rejected' });
  const { celerexBaseUrl, device_code, mode, providerId, model } = req.body ?? {};
  if (!celerexBaseUrl || !device_code) return res.status(400).json({ error: 'celerexBaseUrl, device_code required' });
  try {
    const poll = await pollOnce(celerexBaseUrl, device_code);
    if (poll.status === 'approved' && poll.token) {
      const seeded = await seedFromToken(celerexBaseUrl, poll.token, mode === 'copy' ? 'copy' : 'proxy', providerId, model);
      return res.json({ status: 'connected', ...seeded });
    }
    return res.json({ status: poll.status }); // 'pending' | 'expired'
  } catch (e) {
    return res.status(502).json({ error: `pair/status failed: ${(e as Error).message}` });
  }
});
```
(Confirm `startPairing` returns `{userCode, deviceCode, expiresAt, pollInterval}` — it does, per `@celerex/connect/src/pairing.ts`.)

- [ ] **Step 4: Run → pass.** Re-run the test command from Step 2 → 3/3 (or however many cases) pass. Also run the daemon typecheck (`pnpm --filter @open-design/daemon typecheck`) → clean.

- [ ] **Step 5: Commit (CD repo root):**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/daemon/src/server.ts apps/daemon/tests/celerex-pair-route.test.ts
git commit -m "feat(daemon): /api/celerex/pair/start + /pair/status (device pairing orchestration)"
```

---

## Task 2 — Web "Connect to Celerex" panel [CD]

Work in `/Volumes/Main External/Development/celerex-design`.

**Files:** modify `apps/web/src/state/config.ts`, `apps/web/src/components/SettingsDialog.tsx`; create `apps/web/src/components/CelerexConnectPanel.tsx` (+ a test if a harness exists).

**Context (READ FIRST):**
- `apps/web/src/state/config.ts` — `AppConfig` type + `DEFAULT_CONFIG` (~line 61); `loadConfig()`, `saveConfig(config)`, `syncConfigToDaemon(config)`. The panel updates config via the parent's `setCfg` then `saveConfig`/`syncConfigToDaemon` (the dialog already runs `syncConfigToDaemon(cfg)` in an effect — confirm).
- `apps/web/src/components/SettingsDialog.tsx` — sidebar sections keyed by `activeSection` (`useState<SettingsSection>` ~line 1143); nav buttons ~lines 3084-3261; section panels rendered conditionally (e.g. `activeSection === 'integrations' ? ... : null` ~line 3431). The Composio `ConnectorSection` (~line 4416) and the AMR login polling (`fetchVelaLoginStatus`, ~line 1223) are the patterns to mirror. `cfg`/`setCfg` are in scope (~line 1073).
- `apps/web/src/components/Toast.tsx` — `{ message, tone?: 'default'|'success'|'error'|'loading', ttlMs? }`.
- Daemon calls go through plain `fetch('/api/...')` (same-origin); see `apps/web/src/providers/daemon.ts`.

- [ ] **Step 1: Add `celerex` to config.** In `config.ts`, add to the `AppConfig` type and to `DEFAULT_CONFIG`:
```ts
// in AppConfig:
  celerex?: { connected: boolean; serverUrl: string; provider?: string };
// in DEFAULT_CONFIG:
  celerex: { connected: false, serverUrl: '' },
```

- [ ] **Step 2: Create `CelerexConnectPanel.tsx`.** A self-contained panel; props `{ cfg: AppConfig; setCfg: React.Dispatch<React.SetStateAction<AppConfig>> }`. State machine: `idle | starting | awaiting_approval | connected | error`. Skeleton (adapt imports/styling to the file's neighbors — match `ConnectorSection`'s markup classes):
```tsx
import { useEffect, useRef, useState } from 'react';
import type { AppConfig } from '../state/config';
import { saveConfig, syncConfigToDaemon } from '../state/config';

type Phase = 'idle' | 'starting' | 'awaiting' | 'connected' | 'error';

export function CelerexConnectPanel({ cfg, setCfg }: { cfg: AppConfig; setCfg: React.Dispatch<React.SetStateAction<AppConfig>> }) {
  const [serverUrl, setServerUrl] = useState(cfg.celerex?.serverUrl ?? '');
  const [phase, setPhase] = useState<Phase>(cfg.celerex?.connected ? 'connected' : 'idle');
  const [userCode, setUserCode] = useState('');
  const [error, setError] = useState('');
  const deviceCode = useRef<string | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => { if (pollTimer.current) clearTimeout(pollTimer.current); }, []);

  async function connect() {
    setError(''); setPhase('starting');
    try {
      const r = await fetch('/api/celerex/pair/start', {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ celerexBaseUrl: serverUrl }),
      });
      if (!r.ok) throw new Error((await r.json()).error ?? 'failed to start');
      const j = await r.json();
      deviceCode.current = j.device_code;
      setUserCode(j.user_code);
      setPhase('awaiting');
      poll(Math.max(1, j.poll_interval) * 1000, Date.parse(j.expires_at) || Date.now() + 600000);
    } catch (e) { setError((e as Error).message); setPhase('error'); }
  }

  function poll(intervalMs: number, deadline: number) {
    pollTimer.current = setTimeout(async () => {
      if (Date.now() > deadline) { setError('Pairing expired — try again.'); setPhase('error'); return; }
      try {
        const r = await fetch('/api/celerex/pair/status', {
          method: 'POST', headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ celerexBaseUrl: serverUrl, device_code: deviceCode.current }),
        });
        const j = await r.json();
        if (j.status === 'connected') {
          setCfg((c) => {
            const next: AppConfig = { ...c, ...j.web, celerex: { connected: true, serverUrl, provider: j.provider } };
            saveConfig(next); void syncConfigToDaemon(next);
            return next;
          });
          setPhase('connected'); return;
        }
        if (j.status === 'expired') { setError('Pairing expired — try again.'); setPhase('error'); return; }
        poll(intervalMs, deadline); // still pending
      } catch (e) { setError((e as Error).message); setPhase('error'); }
    }, intervalMs);
  }

  function disconnect() {
    setCfg((c) => {
      const next: AppConfig = { ...c, mode: 'daemon', celerex: { connected: false, serverUrl } };
      saveConfig(next); void syncConfigToDaemon(next);
      return next;
    });
    setPhase('idle'); setUserCode('');
  }

  // Render: idle/error → URL field + Connect; awaiting → big userCode + "enter in Celerex → Settings → Devices"; connected → serverUrl + provider + Disconnect. Show `error` when phase==='error'.
  return (/* JSX mirroring ConnectorSection markup; omitted here — match neighbor classes */ null);
}
```
Implement the JSX to match the file's existing section markup (labels, inputs, buttons, status badges). Keep it one focused component.

- [ ] **Step 3: Wire into SettingsDialog.** Add `'celerex'` to the `SettingsSection` union/type, a nav button (mirror the `integrations` button), and a conditional render `{activeSection === 'celerex' ? <CelerexConnectPanel cfg={cfg} setCfg={setCfg} /> : null}` next to the other section panels. Import the component.

- [ ] **Step 4: Verify.** Run the web typecheck (`pnpm --filter @open-design/web typecheck` or the repo's equivalent — discover) → clean. If the web app has a component-test harness (check `apps/web/tests/components/*`), add `CelerexConnectPanel.test.tsx`: mock `fetch` to return start→pending→connected, render the panel, click Connect, assert the `user_code` shows, then assert `setCfg`/`saveConfig` is called with `mode:'api'` + `celerex.connected:true`. If there's no harness for this, skip the test and note it.

- [ ] **Step 5: Commit (CD repo root):**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/config.ts apps/web/src/components/CelerexConnectPanel.tsx apps/web/src/components/SettingsDialog.tsx apps/web/src/components/CelerexConnectPanel.test.tsx 2>/dev/null
git commit -m "feat(web): Connect to Celerex settings panel (device pairing)"
```

---

## Task 3 — Celerex "Approve Device" UI [CX]

Work in `/Volumes/Main External/Development/Celerex` on branch `feat/celerex-connect-ui`.

**Files:** modify `packages/frontend/app/src/api/client.ts`; create `packages/frontend/app/src/components/ApproveDeviceSettings.tsx` + test; wire into the settings surface.

**Context (READ FIRST):**
- `packages/frontend/app/src/api/client.ts` — existing methods like `setProviderKey(token, providerId, key)` show the fetch+auth pattern (Bearer token, base URL helper). Mirror it.
- `packages/frontend/app/src/components/ProviderKeysSettings.tsx` — the closest existing "settings panel that calls an authed API" component; mirror its structure (loading/error/success states, how it gets the auth token).
- The backend route is `POST /api/design/pair/approve` (AuthClaims session), body `{ user_code }`, response `{ ok: true, label }` or 404 `"no pending pairing for that code..."`.
- Check the frontend test pattern (Vitest + Testing Library) used by existing component tests.

- [ ] **Step 1: Add the API client method** in `api/client.ts`, mirroring `setProviderKey`'s signature/auth:
```ts
export async function approveDevice(token: string, userCode: string): Promise<{ ok: boolean; label?: string }> {
  const res = await fetch(`${API_BASE}/api/design/pair/approve`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
    body: JSON.stringify({ user_code: userCode }),
  });
  if (!res.ok) throw new Error((await res.text()) || 'approve failed');
  return res.json();
}
```
(Use the file's real `API_BASE`/base-url helper + its existing fetch wrapper if it has one — match `setProviderKey`.)

- [ ] **Step 2: Write the failing component test** mirroring an existing frontend component test: render `ApproveDeviceSettings`, type a code into the input, click Approve, assert `approveDevice` is called with the uppercased code and a success message renders. (Mock the client method.)

- [ ] **Step 3: Create `ApproveDeviceSettings.tsx`**, mirroring `ProviderKeysSettings.tsx` (same auth-token source, loading/error pattern):
```tsx
import { useState } from 'react';
import { approveDevice } from '../api/client';
// get the session token the same way ProviderKeysSettings does (jotai atom / context)

export function ApproveDeviceSettings() {
  const token = /* same source as ProviderKeysSettings */ '';
  const [code, setCode] = useState('');
  const [status, setStatus] = useState<{ ok: boolean; msg: string } | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true); setStatus(null);
    try {
      const r = await approveDevice(token, code.trim().toUpperCase());
      setStatus({ ok: true, msg: `Approved${r.label ? ` ${r.label}` : ''}.` });
      setCode('');
    } catch (e) {
      setStatus({ ok: false, msg: 'Code not found or expired.' });
    } finally { setBusy(false); }
  }

  return (/* input(value=code, placeholder "XXXX-XXXX") + Approve button(disabled=busy||!code) + status line; mirror ProviderKeysSettings markup */ null);
}
```
Implement the JSX to match `ProviderKeysSettings` styling.

- [ ] **Step 4: Wire it into the settings surface** — find where `ProviderKeysSettings` is rendered (its parent settings page/section) and add `ApproveDeviceSettings` as a sibling section titled "Devices" / "Approve a device". Match how sections are added there.

- [ ] **Step 5: Verify.** Run the frontend test command (discover: `bun run test` / `vitest` in `packages/frontend`) for the new test → pass. Run the first-party typecheck if quick (`node scripts/typecheck.mjs` is the repo's gate — but it's heavy; at minimum ensure the new files typecheck via the test run).

- [ ] **Step 6: Commit (CX, branch feat/celerex-connect-ui):**
```bash
cd "/Volumes/Main External/Development/Celerex"
git add packages/frontend/app/src/api/client.ts packages/frontend/app/src/components/ApproveDeviceSettings.tsx packages/frontend/app/src/components/ApproveDeviceSettings.test.tsx
git commit -m "feat(frontend): Approve Device settings UI for design pairing"
```

---

## Notes / Gotchas
- **No rustfmt issue here** — these are TS/React. But still keep diffs minimal; don't run repo-wide formatters that reflow unrelated files.
- **CD commits land on `main`** (the celerex-design repo has no branch convention yet; that's how Tasks 5–6 were committed).
- **CX commits land on `feat/celerex-connect-ui`** (based on the gateway HEAD, so `/api/design/pair/approve` exists to build against).
- **Discover test/typecheck commands on read** — both repos have their own; the existing neighbor tests (`celerex-connect-route.test.ts`, `ProviderKeysSettings`) show the exact harness.

## Self-Review
- **Spec coverage:** §3.2 daemon routes + `seedFromToken` → Task 1; §3.3 web panel + §3.4 `AppConfig.celerex` → Task 2; §3.1 Celerex approve UI → Task 3; §5 error handling → Task 1 (502s) + Task 2 (expired/error phases) + Task 3 (404 message); §6 testing → each task. §8 out-of-scope excluded. No gaps.
- **Placeholder scan:** the React JSX bodies are intentionally `null`-stubbed with explicit "implement to match neighbor X" — this is adapt-on-read for foreign UI (same approach that worked for Task 6), with exact neighbor components named. The logic (state machine, fetch calls, config writes) is fully concrete.
- **Type consistency:** `seedFromToken(baseUrl, token, mode, providerId, model)` signature consistent Task 1↔ usage; `celerex: { connected, serverUrl, provider? }` consistent config (Task 2) ↔ daemon return `provider` (Task 1); `/api/celerex/pair/{start,status}` request/response shapes consistent daemon (Task 1) ↔ web (Task 2); `approveDevice(token, userCode)` consistent client (Task 3) ↔ component.
