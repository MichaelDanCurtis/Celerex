# Per-User Settings Store (sub-project B, slice 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store user preferences (theme, etc.) server-side per account so they follow the user — and make it the first feature wired to the WorkOS-issued Agent session.

**Architecture:** Agent gets an `engagement_settings` JSONB table + `GET`/`PUT /api/settings` (scoped to the session's `engagement_id`). The web reads/writes via the stored Agent session (Bearer from `localStorage['celerex:agent-session']`), applying the server theme over the local default; localStorage stays as cache/fallback. Agent first, then web.

**Tech Stack:** Rust/Axum (`celerex-api`), sqlx/Postgres, `AuthClaims`; Next.js web (`apps/web`), vitest.

---

## Conventions
- Agent tests run from `cd "packages/backend/api"` via `tests/common/mod.rs::start_test_db()` + `axum_test::TestServer::new(celerex_api::build_app(db.pool.clone()))`. Mint a per-engagement session token with the local helper used in other test files:
```rust
#[derive(serde::Serialize)] struct Claims { sub: String, engagement_id: String, project_id: Option<String>, exp: usize }
fn token(eng: uuid::Uuid) -> String {
    unsafe { std::env::set_var("CELEREX_JWT_SECRET", "test-secret"); } // SAFETY: single-process test
    let c = Claims { sub:"u".into(), engagement_id: eng.to_string(), project_id: None, exp:(chrono::Utc::now().timestamp()+3600) as usize };
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &c, &jsonwebtoken::EncodingKey::from_secret(b"test-secret")).unwrap()
}
```
- `AuthClaims` extractor: `a: AuthClaims` → `a.0.engagement_id: Uuid`. Migrations are `0001`–`0019` (next = **0019**).

---

# Stage 1 — Agent

### Task 1: Migration 0019 + settings DB functions

**Files:** Create `packages/backend/api/src/db/migrations/0019_engagement_settings.sql`; Modify `packages/backend/api/src/db/mod.rs`

- [ ] **Step 1: Migration**
```sql
CREATE TABLE engagement_settings (
    engagement_id UUID PRIMARY KEY REFERENCES engagements(id) ON DELETE CASCADE,
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 2: DB fns in `db/mod.rs`**
```rust
/// The settings blob for an engagement (defaults to `{}` when no row exists).
pub async fn get_settings(pool: &PgPool, engagement_id: Uuid) -> sqlx::Result<serde_json::Value> {
    let row: Option<(serde_json::Value,)> =
        sqlx::query_as("SELECT settings FROM engagement_settings WHERE engagement_id = $1")
            .bind(engagement_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0).unwrap_or_else(|| serde_json::json!({})))
}

/// Upsert the settings blob for an engagement.
pub async fn upsert_settings(pool: &PgPool, engagement_id: Uuid, settings: &serde_json::Value) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO engagement_settings (engagement_id, settings) VALUES ($1, $2)
         ON CONFLICT (engagement_id) DO UPDATE SET settings = $2, updated_at = NOW()",
    )
    .bind(engagement_id)
    .bind(settings)
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: DB round-trip test** `tests/settings_db.rs`:
```rust
mod common;
use uuid::Uuid;
#[tokio::test]
async fn settings_default_and_upsert_roundtrip() {
    let db = common::start_test_db().await;
    let eng: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id").fetch_one(&db.pool).await.unwrap();
    assert_eq!(celerex_api::db::get_settings(&db.pool, eng).await.unwrap(), serde_json::json!({}));
    celerex_api::db::upsert_settings(&db.pool, eng, &serde_json::json!({"theme":"dark"})).await.unwrap();
    assert_eq!(celerex_api::db::get_settings(&db.pool, eng).await.unwrap(), serde_json::json!({"theme":"dark"}));
    // upsert again replaces
    celerex_api::db::upsert_settings(&db.pool, eng, &serde_json::json!({"theme":"light","accent":"red"})).await.unwrap();
    assert_eq!(celerex_api::db::get_settings(&db.pool, eng).await.unwrap(), serde_json::json!({"theme":"light","accent":"red"}));
}
```

- [ ] **Step 4: Run** `cd "packages/backend/api" && cargo test --test settings_db 2>&1 | tail -15 && cargo build --lib 2>&1 | tail -5` → PASS (confirms 0019 applies). (`#[allow(dead_code)]` on the two fns if the build denies unused warnings; removed in Task 2.)

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/db/migrations/0019_engagement_settings.sql packages/backend/api/src/db/mod.rs packages/backend/api/tests/settings_db.rs
git commit -m "feat(agent): 0019 engagement_settings table + get/upsert db fns"
```

### Task 2: `GET`/`PUT /api/settings` handlers + route + cross-tenant test

**Files:** Create `packages/backend/api/src/settings/mod.rs`; Modify `packages/backend/api/src/lib.rs`; Create `packages/backend/api/tests/settings_api.rs`

- [ ] **Step 1: Write the failing cross-tenant test** `tests/settings_api.rs` (paste the Conventions `token()` helper):
```rust
mod common;
use uuid::Uuid;
// (paste token() from Conventions)

#[tokio::test]
async fn settings_are_per_engagement_and_isolated() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    let a: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('A') RETURNING id").fetch_one(&db.pool).await.unwrap();
    let b: Uuid = sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('B') RETURNING id").fetch_one(&db.pool).await.unwrap();

    // default empty
    let g0 = server.get("/api/settings").add_header("authorization", format!("Bearer {}", token(a))).await;
    g0.assert_status_ok();
    assert_eq!(g0.json::<serde_json::Value>()["settings"], serde_json::json!({}));

    // A writes
    let p = server.put("/api/settings").add_header("authorization", format!("Bearer {}", token(a)))
        .json(&serde_json::json!({"settings":{"theme":"dark"}})).await;
    p.assert_status_ok();

    // A reads back its value
    let ga = server.get("/api/settings").add_header("authorization", format!("Bearer {}", token(a))).await;
    assert_eq!(ga.json::<serde_json::Value>()["settings"], serde_json::json!({"theme":"dark"}));

    // B sees ONLY its own (empty) — never A's
    let gb = server.get("/api/settings").add_header("authorization", format!("Bearer {}", token(b))).await;
    assert_eq!(gb.json::<serde_json::Value>()["settings"], serde_json::json!({}));

    // unauthenticated → not 200
    let anon = server.get("/api/settings").await;
    assert_ne!(anon.status_code(), axum::http::StatusCode::OK);
}
```

- [ ] **Step 2: Run it (fails — route missing)** `cd "packages/backend/api" && cargo test --test settings_api 2>&1 | tail -25`

- [ ] **Step 3: Implement `src/settings/mod.rs`**
```rust
use axum::{extract::State, Json};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use crate::auth::extractors::AuthClaims;
use crate::{db, AppState};

pub async fn get_settings(a: AuthClaims, State(s): State<AppState>) -> Result<Json<Value>, (StatusCode, String)> {
    let settings = db::get_settings(&s.pool, a.0.engagement_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "settings": settings })))
}

#[derive(Deserialize)]
pub struct PutSettings { pub settings: Value }

pub async fn put_settings(a: AuthClaims, State(s): State<AppState>, Json(body): Json<PutSettings>) -> Result<Json<Value>, (StatusCode, String)> {
    db::upsert_settings(&s.pool, a.0.engagement_id, &body.settings).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "settings": body.settings })))
}
```
Add `pub mod settings;` to `lib.rs` (with the other `pub mod`s). Remove any `#[allow(dead_code)]` from the Task-1 db fns (now used).

- [ ] **Step 4: Mount the route** in `lib.rs` (with the other `/api/*` routes; ensure `get`/`put` are imported from `axum::routing`):
```rust
.route("/api/settings", get(settings::get_settings).put(settings::put_settings))
```

- [ ] **Step 5: Run** `cd "packages/backend/api" && cargo test --test settings_api 2>&1 | tail -15 && cargo build --lib 2>&1 | tail -5` → PASS, clean.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/settings/mod.rs packages/backend/api/src/lib.rs packages/backend/api/src/db/mod.rs packages/backend/api/tests/settings_api.rs
git commit -m "feat(agent): GET/PUT /api/settings scoped to the session engagement"
```

---

# Stage 2 — Web (`apps/web`)

### Task 3: Settings state helpers + tests

**Files:** Create `apps/web/src/state/user-settings.ts`; Create `apps/web/tests/state/user-settings.test.ts`

- [ ] **Step 1: Write the failing test** `apps/web/tests/state/user-settings.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest";
import { fetchUserSettings, saveUserSettings } from "../../src/state/user-settings";

function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as unknown as typeof fetch;
}

describe("user-settings state", () => {
  it("fetchUserSettings GETs with Bearer and returns the blob", async () => {
    const f = mockFetch({ settings: { theme: "dark" } });
    const res = await fetchUserSettings("https://agent.example", "sess1", f);
    expect(res).toEqual({ theme: "dark" });
    const [url, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe("https://agent.example/api/settings");
    expect((init as RequestInit).method ?? "GET").toBe("GET");
    expect((init as RequestInit & { headers: Record<string,string> }).headers.Authorization).toBe("Bearer sess1");
  });
  it("saveUserSettings PUTs the blob with Bearer", async () => {
    const f = mockFetch({ settings: { theme: "light" } });
    await saveUserSettings("https://agent.example", "sess1", { theme: "light" }, f);
    const [url, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(url).toBe("https://agent.example/api/settings");
    expect((init as RequestInit).method).toBe("PUT");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ settings: { theme: "light" } });
  });
  it("fetchUserSettings throws on non-ok", async () => {
    await expect(fetchUserSettings("https://agent.example", "bad", mockFetch({}, 401))).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Run (fails — module missing)** `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm vitest run tests/state/user-settings.test.ts 2>&1 | tail -20`

- [ ] **Step 3: Implement `apps/web/src/state/user-settings.ts`**
```ts
export type UserSettings = Record<string, unknown>;

interface StoredAgentSession { token: string; engagement: { id: string; name: string }; }

/** Read the Agent session stored at login (slice 1). Returns null when logged out. */
export function readAgentSession(): StoredAgentSession | null {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem("celerex:agent-session") : null;
    return raw ? (JSON.parse(raw) as StoredAgentSession) : null;
  } catch {
    return null;
  }
}

export function agentBaseUrl(): string {
  return (process.env.NEXT_PUBLIC_AGENT_URL ?? "http://localhost:3001").replace(/\/+$/, "");
}

export async function fetchUserSettings(base: string, sessionToken: string, fetchFn: typeof fetch = fetch): Promise<UserSettings> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/settings`, {
    method: "GET",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
  if (!res.ok) throw new Error("Could not load your settings.");
  const body = (await res.json()) as { settings: UserSettings };
  return body.settings ?? {};
}

export async function saveUserSettings(base: string, sessionToken: string, settings: UserSettings, fetchFn: typeof fetch = fetch): Promise<void> {
  const res = await fetchFn(`${base.replace(/\/+$/, "")}/api/settings`, {
    method: "PUT",
    headers: { Authorization: `Bearer ${sessionToken}`, "content-type": "application/json" },
    body: JSON.stringify({ settings }),
  });
  if (!res.ok) throw new Error("Could not save your settings.");
}
```

- [ ] **Step 4: Run (passes)** `pnpm vitest run tests/state/user-settings.test.ts 2>&1 | tail -12` → 3/3.

- [ ] **Step 5: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/user-settings.ts apps/web/tests/state/user-settings.test.ts
git commit -m "feat(web): user-settings state helpers (server-backed theme)"
```

### Task 4: Apply server theme on authenticated load (additive)

**Files:** Create `apps/web/src/components/SettingsBridge.tsx`; Modify the place where the slice-1 `WorkosSessionBridge` is mounted (mount `SettingsBridge` beside it); Create `apps/web/tests/components/SettingsBridge.test.tsx`

- [ ] **Step 1: Write the failing test** `apps/web/tests/components/SettingsBridge.test.tsx` (jsdom):
```tsx
// @vitest-environment jsdom
import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../src/state/user-settings", () => ({
  readAgentSession: vi.fn(() => ({ token: "sess1", engagement: { id: "e", name: "w" } })),
  agentBaseUrl: vi.fn(() => "https://agent.example"),
  fetchUserSettings: vi.fn(async () => ({ theme: "dark" })),
  saveUserSettings: vi.fn(),
}));
import { fetchUserSettings } from "../../src/state/user-settings";
import { SettingsBridge } from "../../src/components/SettingsBridge";

describe("SettingsBridge", () => {
  afterEach(() => { vi.clearAllMocks(); document.documentElement.removeAttribute("data-theme"); });
  it("applies the server theme on authenticated load", async () => {
    const applied: string[] = [];
    render(<SettingsBridge applyTheme={(t) => applied.push(t)} />);
    await waitFor(() => expect(fetchUserSettings).toHaveBeenCalled());
    await waitFor(() => expect(applied).toContain("dark"));
  });
});
```

- [ ] **Step 2: Run (fails — component missing)** `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm vitest run tests/components/SettingsBridge.test.tsx 2>&1 | tail -20`

- [ ] **Step 3: Implement `apps/web/src/components/SettingsBridge.tsx`**
```tsx
"use client";
import { useEffect, useRef } from "react";
import { readAgentSession, agentBaseUrl, fetchUserSettings, type UserSettings } from "../state/user-settings";

interface Props {
  /** Apply a theme value to the app (defaults to setting <html data-theme>). Injectable for tests. */
  applyTheme?: (theme: string) => void;
}

export function SettingsBridge({ applyTheme }: Props) {
  const done = useRef(false);
  useEffect(() => {
    if (done.current) return;
    const session = readAgentSession();
    if (!session) return; // logged out → keep existing localStorage behavior
    done.current = true;
    let cancelled = false;
    (async () => {
      try {
        const settings: UserSettings = await fetchUserSettings(agentBaseUrl(), session.token);
        if (cancelled) return;
        const theme = typeof settings.theme === "string" ? settings.theme : undefined;
        if (theme) {
          if (applyTheme) applyTheme(theme);
          else document.documentElement.setAttribute("data-theme", theme);
        }
      } catch {
        // server settings unavailable → fall back to the local default already applied
        done.current = false;
      }
    })();
    return () => { cancelled = true; };
  }, [applyTheme]);
  return null;
}
```

- [ ] **Step 4: Run (passes)** `pnpm vitest run tests/components/SettingsBridge.test.tsx 2>&1 | tail -12` → PASS.

- [ ] **Step 5: Mount it** beside the slice-1 `WorkosSessionBridge` (find where that's mounted — e.g. `EntryShell.tsx`) so it runs on authenticated load. Use the app's real theme-apply function for the default if one exists (pass it as `applyTheme`), otherwise the default `<html data-theme>` is fine. KEEP ADDITIVE — do not remove the existing `config.ts`/localStorage theme logic; the server value layers on top when a session exists.

- [ ] **Step 6: Typecheck + full web suite** `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm tsc -b --noEmit 2>&1 | tail -10 && pnpm vitest run 2>&1 | tail -20` → clean; suite green minus the known pre-existing `default-background` (3) failures.

- [ ] **Step 7: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/components/SettingsBridge.tsx apps/web/tests/components/SettingsBridge.test.tsx <the mount file>
git commit -m "feat(web): apply server-backed theme on authenticated load"
```

---

## Final verification
- [ ] Agent: `cd "packages/backend/api" && cargo test --test settings_db --test settings_api 2>&1 | tail -12` → PASS.
- [ ] Web: `cd "apps/web" && pnpm vitest run tests/state/user-settings.test.ts tests/components/SettingsBridge.test.tsx 2>&1 | tail -10 && pnpm tsc -b --noEmit` → green/clean.
- [ ] Stages shipped as branch + PR + merge per repo (Agent = Celerex; web = celerex-design).

## Self-review (spec coverage)
- engagement_settings JSONB table → Task 1. ✓
- get/upsert db fns → Task 1. ✓
- GET/PUT /api/settings scoped to claims.engagement_id → Task 2 (+ cross-tenant isolation + unauth-≠-200 test). ✓
- Web state helpers (Bearer from stored session, fetchFn-injectable) → Task 3. ✓
- Apply server theme on authenticated load, additive over localStorage → Task 4. ✓
- First real consumer of the Agent session → Tasks 3-4 (reads the slice-1 session). ✓
- Deferred (spec §6): full feature migration to Agent-direct; removing local/daemon copies; per-user-within-team; typed schema. ✓
