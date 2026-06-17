# Per-User Settings Store (sub-project B, slice 2) — design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`) + celerex-design (web — `apps/web`)
**Builds on:** B slice 1 (WorkOS login). This is the **first real consumer of the Agent session**: the web reads/writes the logged-in user's settings from the Agent, proving the login does something end-to-end.

## 1. Purpose

Make user preferences (theme, accent, etc.) **live server-side, scoped to the account, so they follow the user across browsers/devices** — the state-location rule from memory (`celerex-multitenancy-audit`, `omnivyx-architecture`): the desktop is a thin shell; the server holds state. Today theme/prefs live only in browser localStorage + the local daemon config, which is the wrong place for the thin-shell model.

## 2. Locked decisions

- **Server is the source of truth for user settings.** A per-engagement settings record on the Agent; the web reads it on authenticated load and writes on change.
- **Scoped by `engagement_id`** (the account/workspace), consistent with all other Agent state — for personal workspaces this is 1:1 with the user. Read/write authorized via the existing session JWT (`AuthClaims` → `claims.engagement_id`).
- **One JSONB blob** (`settings JSONB`) per engagement — flexible, no schema churn as preferences evolve. `GET /api/settings` returns the blob (or `{}`), `PUT /api/settings` upserts it.
- **First session-consumer:** the web calls the Agent **directly** with the stored Agent session token (from slice 1's `localStorage['celerex:agent-session']`). This is the first feature wired to the WorkOS-issued session — it does NOT migrate every other feature (that stays the deferred public-web-UI work).
- **localStorage stays as a cache/fallback** for the unauthenticated/offline case and to avoid a theme flash; the server value wins once loaded.
- **Scope guardrail:** ship **theme** (+ the handful of existing UI prefs as the blob) end-to-end as the proof. Migrating all features to Agent-direct, and removing the local/daemon copies, are later.

## 3. Key facts from recon

- **Agent:** the audit confirmed **no settings/preferences table exists** (checked all migrations). Next migration = **0019**. Auth: `AuthClaims` extractor → `claims.engagement_id: Uuid`; routes mounted in `lib.rs`; existing CRUD handlers in their modules return `(StatusCode, String)` / `Json`. `engagements` table is the parent (ON DELETE CASCADE pattern used elsewhere).
- **Web:** prefs live in `apps/web/src/state/config.ts` (`AppConfig` in `types.ts`, fields incl. `theme`, `accentColor`, `model`, etc.), persisted to `localStorage` (`loadConfig()`/`saveConfig()`). Theme is applied via a `<head>` init script + the config. Slice-1 stored the Agent session at `localStorage['celerex:agent-session']` (`{ token, engagement }`); state modules are bare async fns with `fetchFn: typeof fetch = fetch`. Agent base URL for the browser: `process.env.NEXT_PUBLIC_AGENT_URL ?? "http://localhost:3001"`.

## 4. Components

### 4.1 Agent (`celerex-api`)
1. **Migration 0019** — `CREATE TABLE engagement_settings ( engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE, settings JSONB NOT NULL DEFAULT '{}'::jsonb, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW() );`
2. **DB fns** (`db/mod.rs`): `get_settings(pool, engagement_id) -> sqlx::Result<serde_json::Value>` (returns the blob or `{}` if no row); `upsert_settings(pool, engagement_id, &serde_json::Value) -> sqlx::Result<()>` (INSERT … ON CONFLICT (engagement_id) DO UPDATE SET settings=$2, updated_at=NOW()).
3. **Handlers + routes** (a small `settings` module or in an existing one):
   - `GET /api/settings` (`AuthClaims`) → `200 { settings: <blob> }` for `claims.engagement_id`.
   - `PUT /api/settings` (`AuthClaims`, body `{ settings: <object> }`) → upsert, `200 { settings: <blob> }`.
   Both strictly scoped to `claims.engagement_id` (no client-supplied id) — consistent with Stage A.
4. Mount in `lib.rs`.

### 4.2 Web (`apps/web`)
1. **State module** `src/state/user-settings.ts` (bare `fetchFn`-injectable helpers): `fetchUserSettings(agentBaseUrl, sessionToken, fetchFn?)` → GET `/api/settings` with `Authorization: Bearer <sessionToken>`; `saveUserSettings(agentBaseUrl, sessionToken, settings, fetchFn?)` → PUT. A small helper to read the stored Agent session from `localStorage['celerex:agent-session']`.
2. **Wire theme to the server:** on authenticated load (Agent session present), fetch settings and apply `theme`/prefs (merging over the local default); on a theme/pref change, write through to the Agent (and keep the localStorage cache in sync). If no session (logged out), fall back to the existing localStorage behavior unchanged.
3. Keep the change minimal and additive — do not rip out `config.ts`/localStorage; layer the server as the source of truth when authenticated.

## 5. Testing

- **Agent:** integration test (testcontainers) — two engagements; `GET /api/settings` defaults to `{}`; `PUT` then `GET` round-trips the blob for engagement A; engagement B sees its own (and never A's) — a cross-tenant isolation assertion (mirrors Stage A style, using a minted token per engagement).
- **Web:** vitest unit tests for `fetchUserSettings`/`saveUserSettings` (mock fetch — assert URL, Bearer header, body shape, return) + a component/integration test that an authenticated load applies the server theme over the local default (mock the session + the fetch).

## 6. Non-goals / later

- Migrating ALL features to Agent-direct (public-web-UI work); removing the daemon/localStorage copies.
- Per-*user* (vs per-engagement) settings within a shared Team — N/A until Teams exist.
- A typed settings schema / validation — JSONB blob is intentional for slice 2.

## 7. Build order

Subagent-driven, branch + PR + merge per repo, each task spec-and-quality reviewed. Agent first (migration + db fns + endpoints + cross-tenant test), then web (state helpers + wire theme + tests). Then Copilot OAuth.

See memory `celerex-multitenancy-audit` (B status + state-location rule), `omnivyx-architecture`.
