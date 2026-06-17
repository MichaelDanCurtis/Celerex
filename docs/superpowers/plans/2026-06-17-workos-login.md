# WorkOS Login (sub-project B, slice 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user log in via WorkOS AuthKit; the Agent validates the WorkOS access token, provisions the user's personal workspace, and mints its own session JWT — so every Stage-A engagement check stays intact.

**Architecture:** Web (Next.js 16 `apps/web`) uses AuthKit-Next.js for the browser login; `withAuth()` yields a WorkOS access token (RS256 JWT). The web app sends it to a new public Agent endpoint `POST /api/auth/workos`, which verifies it against WorkOS JWKS, finds-or-provisions an engagement keyed to the WorkOS user id, and mints the existing HS256 session JWT (`auth/jwt.rs::mint`). The web app stores that session for Agent-direct calls. Agent built first, then web.

**Tech Stack:** Rust/Axum (`celerex-api`), `jsonwebtoken` 9.3.1 (RS256 + `jwk`), `reqwest`, sqlx/Postgres, `wiremock`+`rsa` (test); Next.js 16, `@workos-inc/authkit-nextjs`, vitest+jsdom.

---

## Conventions

- Agent tests run from `cd "packages/backend/api"`; integration tests use `tests/common/mod.rs::start_test_db()` (real Postgres via testcontainers, sets `CELEREX_CRED_KEY`+`CELEREX_JWT_SECRET`) + `axum_test::TestServer::new(celerex_api::build_app(db.pool.clone()))`.
- Keys for the web are already in `apps/web/.env.local` (gitignored). The Agent reads `WORKOS_CLIENT_ID` and an overridable `WORKOS_JWKS_URL` (default `https://api.workos.com`) so tests can point JWKS at a stub.
- Existing `LoginResponse` shape to mirror: `{ token: String, engagement: { id: Uuid, name: String } }`. Session mint: `jwt::mint(engagement_id, sub, SESSION_TTL_SECS)` (HS256, `SESSION_TTL_SECS = 7*24*3600`).

---

# Stage 1 — Agent

### Task 1: Migration 0018 + DB functions for WorkOS users

**Files:**
- Create: `packages/backend/api/src/db/migrations/0018_workos_users.sql`
- Modify: `packages/backend/api/src/db/mod.rs`

- [ ] **Step 1: Write the migration**

`0018_workos_users.sql`:
```sql
ALTER TABLE users ADD COLUMN workos_user_id TEXT;
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
CREATE UNIQUE INDEX users_workos_user_id_uq ON users (workos_user_id) WHERE workos_user_id IS NOT NULL;
```

- [ ] **Step 2: Add DB functions to `db/mod.rs`**

```rust
/// Engagement (id+name) owned by the user with this WorkOS id, if provisioned.
pub async fn find_engagement_by_workos_user(
    pool: &PgPool,
    workos_user_id: &str,
) -> sqlx::Result<Option<(Uuid, String)>> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT e.id, e.name
         FROM users u JOIN engagements e ON e.id = u.engagement_id
         WHERE u.workos_user_id = $1",
    )
    .bind(workos_user_id)
    .fetch_optional(pool)
    .await
}

/// Create a WorkOS-backed user (no local password) in a fresh engagement.
pub async fn create_workos_user(
    pool: &PgPool,
    email: &str,
    workos_user_id: &str,
    engagement_id: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, password_hash, workos_user_id, engagement_id)
         VALUES ($1, NULL, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(workos_user_id)
    .bind(engagement_id)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 3: Build (confirms SQL types resolve)**

Run: `cd "packages/backend/api" && cargo build --lib 2>&1 | tail -10`
Expected: clean (the two fns are unused until Task 3 — add `#[allow(dead_code)]` if the build denies warnings; remove in Task 3).

- [ ] **Step 4: Commit**
```bash
git add packages/backend/api/src/db/migrations/0018_workos_users.sql packages/backend/api/src/db/mod.rs
git commit -m "feat(agent): 0018 workos_user_id migration + provisioning db fns"
```

---

### Task 2: WorkOS token validation module (JWKS, RS256)

**Files:**
- Create: `packages/backend/api/src/auth/workos.rs`
- Modify: `packages/backend/api/src/auth/mod.rs` (add `pub mod workos;`)

- [ ] **Step 1: Write the module**

`auth/workos.rs`:
```rust
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// Subset of WorkOS access-token claims we rely on. `sub` = WorkOS user id (stable identity).
#[derive(Debug, Deserialize)]
pub struct WorkosClaims {
    pub sub: String,
    #[serde(default)]
    pub exp: usize,
}

fn jwks_base() -> String {
    std::env::var("WORKOS_JWKS_URL").unwrap_or_else(|_| "https://api.workos.com".to_string())
}

fn client_id() -> Result<String, String> {
    std::env::var("WORKOS_CLIENT_ID").map_err(|_| "WORKOS_CLIENT_ID not set".to_string())
}

/// Fetch the WorkOS JWKS for the configured client id.
pub async fn fetch_jwks(http: &reqwest::Client) -> Result<JwkSet, String> {
    let url = format!("{}/sso/jwks/{}", jwks_base().trim_end_matches('/'), client_id()?);
    let res = http.get(&url).send().await.map_err(|e| format!("jwks fetch failed: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("jwks fetch status {}", res.status()));
    }
    res.json::<JwkSet>().await.map_err(|e| format!("jwks parse failed: {e}"))
}

/// Verify a WorkOS RS256 access token against a JwkSet. Checks signature + expiry.
pub fn verify_with_jwks(token: &str, jwks: &JwkSet) -> Result<WorkosClaims, String> {
    let header = decode_header(token).map_err(|e| format!("bad token header: {e}"))?;
    let kid = header.kid.ok_or_else(|| "token missing kid".to_string())?;
    let jwk = jwks.find(&kid).ok_or_else(|| "no matching jwk for kid".to_string())?;
    let key = DecodingKey::from_jwk(jwk).map_err(|e| format!("bad jwk: {e}"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    // WorkOS audience/issuer validation can be tightened later; signature + exp is the gate for slice 1.
    validation.validate_aud = false;
    let data = decode::<WorkosClaims>(token, &key, &validation).map_err(|e| format!("token invalid: {e}"))?;
    Ok(data.claims)
}

/// Fetch JWKS then verify. (JWKS caching is a later optimization; per-call fetch is fine for slice 1.)
pub async fn verify_access_token(http: &reqwest::Client, token: &str) -> Result<WorkosClaims, String> {
    let jwks = fetch_jwks(http).await?;
    verify_with_jwks(token, &jwks)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Unit test of verify_with_jwks is covered by the integration test in Task 3
    // (which builds a real JwkSet from a generated RSA key). This module compiles standalone.
    #[test]
    fn jwks_base_defaults() {
        // SAFETY: single-process test.
        unsafe { std::env::remove_var("WORKOS_JWKS_URL"); }
        assert_eq!(jwks_base(), "https://api.workos.com");
    }
}
```

Add `pub mod workos;` to `auth/mod.rs`.

- [ ] **Step 2: Build + unit test**

Run: `cd "packages/backend/api" && cargo test --lib auth::workos 2>&1 | tail -15`
Expected: PASS (jwks_base_defaults).

- [ ] **Step 3: Commit**
```bash
git add packages/backend/api/src/auth/workos.rs packages/backend/api/src/auth/mod.rs
git commit -m "feat(agent): WorkOS access-token validation (JWKS, RS256)"
```

---

### Task 3: `POST /api/auth/workos` handler + route + integration test

**Files:**
- Modify: `packages/backend/api/src/auth/routes.rs`, `packages/backend/api/src/lib.rs`, `packages/backend/api/Cargo.toml` (dev-deps)
- Create: `packages/backend/api/tests/workos_login.rs`

- [ ] **Step 1: Add dev-deps for the test keypair**

In `Cargo.toml` `[dev-dependencies]` add (if absent):
```toml
rsa = "0.9"
rand = "0.8"
base64 = "0.22"
```

- [ ] **Step 2: Write the failing integration test**

`tests/workos_login.rs`:
```rust
mod common;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn b64u(bytes: &[u8]) -> String { URL_SAFE_NO_PAD.encode(bytes) }

/// Build (signing PEM, JWKS-json) for a fresh RSA key with the given kid.
fn keypair(kid: &str) -> (String, serde_json::Value) {
    let mut rng = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
    let pem = priv_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).unwrap().to_string();
    let pub_key = RsaPublicKey::from(&priv_key);
    let n = b64u(&pub_key.n().to_bytes_be());
    let e = b64u(&pub_key.e().to_bytes_be());
    let jwks = serde_json::json!({ "keys": [
        { "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": n, "e": e }
    ]});
    (pem, jwks)
}

fn sign(pem: &str, kid: &str, sub: &str) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let claims = serde_json::json!({ "sub": sub, "exp": (chrono::Utc::now().timestamp() + 3600) as usize });
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, &claims, &key).unwrap()
}

async fn setup_jwks(jwks: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/sso/jwks/client_test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jwks))
        .mount(&server).await;
    server
}

#[tokio::test]
async fn workos_login_provisions_and_mints_session_idempotently() {
    let (pem, jwks) = keypair("kid-1");
    let jwks_server = setup_jwks(jwks).await;
    // SAFETY: single-process test; set before build_app reads env.
    unsafe {
        std::env::set_var("WORKOS_CLIENT_ID", "client_test");
        std::env::set_var("WORKOS_JWKS_URL", jwks_server.uri());
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();

    let token = sign(&pem, "kid-1", "wos_user_123");
    // First login → 200, provisions an engagement, returns a session token.
    let r1 = server.post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "email": "alice@acme.test" })).await;
    r1.assert_status_ok();
    let body1: serde_json::Value = r1.json();
    let eng1 = body1["engagement"]["id"].as_str().unwrap().to_string();
    let session = body1["token"].as_str().unwrap().to_string();

    // The minted session is accepted by an existing protected route.
    let projects = server.get("/api/projects")
        .add_header("authorization", format!("Bearer {session}")).await;
    projects.assert_status_ok();

    // Second login (same WorkOS sub) → same engagement (idempotent provisioning).
    let token2 = sign(&pem, "kid-1", "wos_user_123");
    let r2 = server.post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {token2}"))
        .json(&serde_json::json!({ "email": "alice@acme.test" })).await;
    r2.assert_status_ok();
    assert_eq!(r2.json::<serde_json::Value>()["engagement"]["id"].as_str().unwrap(), eng1);
}

#[tokio::test]
async fn workos_login_rejects_invalid_token() {
    let (_pem, jwks) = keypair("kid-1");
    let (other_pem, _other_jwks) = keypair("kid-1"); // different key, same kid
    let jwks_server = setup_jwks(jwks).await;
    unsafe {
        std::env::set_var("WORKOS_CLIENT_ID", "client_test");
        std::env::set_var("WORKOS_JWKS_URL", jwks_server.uri());
    }
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    // Token signed by a key that does NOT match the served JWKS → 401.
    let bad = sign(&other_pem, "kid-1", "wos_user_x");
    let resp = server.post("/api/auth/workos")
        .add_header("authorization", format!("Bearer {bad}"))
        .json(&serde_json::json!({ "email": "x@y.test" })).await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 3: Run it (fails — route missing)**

Run: `cd "packages/backend/api" && cargo test --test workos_login 2>&1 | tail -25`
Expected: FAIL (404 / route not found).

- [ ] **Step 4: Implement the handler in `auth/routes.rs`**

```rust
#[derive(serde::Deserialize)]
pub struct WorkosLoginRequest {
    /// Display email from the WorkOS user object (identity is the validated token `sub`).
    pub email: String,
}

pub async fn workos_login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<WorkosLoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;

    let http = reqwest::Client::new();
    let claims = crate::auth::workos::verify_access_token(&http, token)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;

    // Find-or-provision the personal engagement keyed to the WorkOS user id.
    let (engagement_id, engagement_name) =
        match db::find_engagement_by_workos_user(&state.pool, &claims.sub).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            Some(found) => found,
            None => {
                let name = format!("{}'s workspace", body.email.trim());
                let eng = db::create_engagement(&state.pool, &name).await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                db::create_workos_user(&state.pool, body.email.trim(), &claims.sub, eng.id).await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                (eng.id, eng.name)
            }
        };

    let token = jwt::mint(engagement_id, &claims.sub, SESSION_TTL_SECS);
    Ok(Json(LoginResponse { token, engagement: LoginEngagement { id: engagement_id, name: engagement_name } }))
}
```
(Confirm the `State`/`Json`/`StatusCode` imports already present in `routes.rs`; add `db`/`jwt` paths as the file already uses them.)

- [ ] **Step 5: Mount the route + remove dead-code allows**

In `lib.rs` beside the other auth routes:
```rust
.route("/api/auth/workos", post(auth::routes::workos_login))
```
Remove any `#[allow(dead_code)]` added in Task 1 (the db fns are now used).

- [ ] **Step 6: Run tests + full auth suite**

Run: `cd "packages/backend/api" && cargo test --test workos_login 2>&1 | tail -20 && cargo test --test auth 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5`
Expected: workos_login 2/2 PASS; existing auth tests still PASS; clean build.

- [ ] **Step 7: Commit**
```bash
git add packages/backend/api/src/auth/routes.rs packages/backend/api/src/lib.rs packages/backend/api/src/db/mod.rs packages/backend/api/Cargo.toml packages/backend/api/tests/workos_login.rs
git commit -m "feat(agent): POST /api/auth/workos — validate WorkOS token, provision engagement, mint session"
```

---

# Stage 2 — Web (`apps/web`)

> Stage-2 prerequisite: the SDK README is the source of truth — the implementer should `WebFetch https://raw.githubusercontent.com/workos/authkit-nextjs/main/README.md` and follow it where it differs from the snippets below (export paths/API can shift between SDK versions).

### Task 4: Install AuthKit + proxy + callback + provider

**Files:**
- Modify: `apps/web/package.json` (dependency), `apps/web/app/layout.tsx`
- Create: `apps/web/proxy.ts`, `apps/web/app/callback/route.ts`

- [ ] **Step 1: Install the SDK**

Run: `cd "/Volumes/Main External/Development/celerex-design" && pnpm --filter web add @workos-inc/authkit-nextjs 2>&1 | tail -5` (use the real package/filter name — confirm the web package's name in `apps/web/package.json`).
Verify: the package appears in `apps/web/package.json` + `node_modules`.

- [ ] **Step 2: Create `apps/web/proxy.ts`** (Next 16 convention; NOT middleware.ts)
```ts
import { authkitProxy } from "@workos-inc/authkit-nextjs";

export default authkitProxy();

export const config = {
  // Run on app routes; exclude the callback, Next internals, and static assets.
  matcher: ["/((?!callback|_next/static|_next/image|favicon.ico).*)"],
};
```

- [ ] **Step 3: Create `apps/web/app/callback/route.ts`**
```ts
import { handleAuth } from "@workos-inc/authkit-nextjs";

export const GET = handleAuth();
```

- [ ] **Step 4: Wrap the root layout** — in `apps/web/app/layout.tsx`, import and wrap children:
```tsx
import { AuthKitProvider } from "@workos-inc/authkit-nextjs/components";
// inside the returned <body>...</body>, wrap the existing children:
//   <AuthKitProvider>{children}</AuthKitProvider>
```
(Preserve the existing layout content; only add the provider around `{children}`.)

- [ ] **Step 5: Typecheck + commit**

Run: `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm tsc -b --noEmit 2>&1 | tail -15`
Expected: clean (or no NEW errors from these files).
```bash
git add apps/web/package.json apps/web/proxy.ts apps/web/app/callback/route.ts apps/web/app/layout.tsx ../../pnpm-lock.yaml
git commit -m "feat(web): install AuthKit-Next.js (proxy, callback, provider)"
```

### Task 5: Sign-in/out + Agent session exchange + gate + tests

**Files:**
- Create: `apps/web/src/state/session.ts` (the Agent exchange helper), `apps/web/app/auth/actions.ts` (server actions), a sign-in/out UI entry, and `apps/web/tests/state/session.test.ts`.

- [ ] **Step 1: Write the failing state test**

`apps/web/tests/state/session.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest";
import { exchangeWorkosSession } from "../../src/state/session";

function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as unknown as typeof fetch;
}

describe("exchangeWorkosSession", () => {
  it("POSTs the WorkOS access token + email and returns the Agent session", async () => {
    const f = mockFetch({ token: "agent_sess", engagement: { id: "e1", name: "w" } });
    const res = await exchangeWorkosSession("https://agent.example", "wos_access", "a@b.test", f);
    expect(res.token).toBe("agent_sess");
    const [url, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(url).toBe("https://agent.example/api/auth/workos");
    expect(init.method).toBe("POST");
    expect(init.headers.Authorization).toBe("Bearer wos_access");
    expect(JSON.parse(init.body)).toEqual({ email: "a@b.test" });
  });
  it("throws a plain error on non-ok", async () => {
    const f = mockFetch({}, 401);
    await expect(exchangeWorkosSession("https://agent.example", "bad", "a@b.test", f)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Run it (fails — module missing)**

Run: `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm vitest run tests/state/session.test.ts 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implement `apps/web/src/state/session.ts`**
```ts
export interface AgentSession { token: string; engagement: { id: string; name: string }; }

/** Exchange a validated WorkOS access token for the Agent's own engagement-scoped session. */
export async function exchangeWorkosSession(
  agentBaseUrl: string,
  workosAccessToken: string,
  email: string,
  fetchFn: typeof fetch = fetch,
): Promise<AgentSession> {
  const res = await fetchFn(`${agentBaseUrl.replace(/\/+$/, "")}/api/auth/workos`, {
    method: "POST",
    headers: { Authorization: `Bearer ${workosAccessToken}`, "content-type": "application/json" },
    body: JSON.stringify({ email }),
  });
  if (!res.ok) throw new Error("Could not establish your session. Please try signing in again.");
  return res.json() as Promise<AgentSession>;
}
```

- [ ] **Step 4: Run it (passes)**

Run: `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm vitest run tests/state/session.test.ts 2>&1 | tail -12`
Expected: PASS (2/2).

- [ ] **Step 5: Sign-out server action + a sign-in/out UI entry**

`apps/web/app/auth/actions.ts`:
```ts
"use server";
import { signOut } from "@workos-inc/authkit-nextjs";
export async function signOutAction() {
  await signOut();
}
```
Add a client sign-in/out control (e.g. `apps/web/src/components/AuthControl.tsx`) using `useAuth()` from `@workos-inc/authkit-nextjs/components`: when `!user` render a "Sign in" button calling `refreshAuth({ ensureSignedIn: true })`; when `user`, render a sign-out `<form action={signOutAction}>`. Mount it in the app's existing header/nav. (Follow the README for the exact `useAuth`/`refreshAuth` import + signature for the installed SDK version.)

- [ ] **Step 6: Wire the exchange into the authenticated entry**

Where the app boots an authenticated session (a Server Component that calls `withAuth()` — e.g. the catch-all `app/[[...slug]]` page or a top provider), when `accessToken` + `user.email` are present, call `exchangeWorkosSession(<agent base url>, accessToken, user.email)` and make the resulting Agent session token available to the client app for Agent-direct calls (e.g. via a context provider or an httpOnly cookie set in a route handler). For slice 1, the minimal acceptance is: an unauthenticated visitor sees "Sign in"; an authenticated one triggers the exchange and the app renders with a known user. (The agent base url is `process.env.NEXT_PUBLIC_AGENT_URL` defaulting to `http://localhost:3001`.)

- [ ] **Step 7: Component test for the gate** (`apps/web/tests/components/AuthControl.test.tsx`, jsdom): mock `@workos-inc/authkit-nextjs/components` `useAuth` to return `{ user: null }` → asserts a "Sign in" control renders; `{ user: {...} }` → asserts the sign-out control renders. Run: `pnpm vitest run tests/components/AuthControl.test.tsx 2>&1 | tail -12` → PASS.

- [ ] **Step 8: Typecheck + full web suite + commit**

Run: `cd "/Volumes/Main External/Development/celerex-design/apps/web" && pnpm tsc -b --noEmit 2>&1 | tail -10 && pnpm vitest run 2>&1 | tail -20`
Expected: clean; suite green (minus the 3 known pre-existing `default-background` failures).
```bash
git add apps/web/src/state/session.ts apps/web/app/auth/actions.ts apps/web/src/components/AuthControl.tsx apps/web/tests/state/session.test.ts apps/web/tests/components/AuthControl.test.tsx <entry file>
git commit -m "feat(web): WorkOS sign-in/out + Agent session exchange + gate"
```

---

## Final verification

- [ ] Agent: `cd "packages/backend/api" && cargo test --test workos_login 2>&1 | tail -10 && cargo test --test auth 2>&1 | tail -5` → PASS.
- [ ] Web: `cd "apps/web" && pnpm vitest run 2>&1 | tail -15 && pnpm tsc -b --noEmit` → green/clean.
- [ ] Manual (user, when ready): register `http://localhost:3000/callback` in WorkOS → Redirects; `pnpm --filter web dev`; click Sign in → real WorkOS URL (not `/[object Object]`) → callback succeeds → app shows you logged in. Agent run with `WORKOS_CLIENT_ID` set + `CELEREX_HOUSE_MODE` as desired.
- [ ] Each stage shipped as branch + PR + merge per the standing rule.

## Self-review (spec coverage)
- AuthKit in Next.js → Tasks 4-5. ✓
- Agent mints own session from WorkOS token → Task 3 (mirrors LoginResponse, uses `jwt::mint`). ✓
- Validate WorkOS JWT via JWKS (RS256) → Task 2 + Task 3 test (stubbed JWKS via wiremock + generated RSA key). ✓
- Personal-workspace provisioning keyed to WorkOS user id, idempotent → Task 1 (migration + fns) + Task 3 (handler + idempotency test). ✓
- Minted session accepted by existing protected route → Task 3 test (`GET /api/projects`). ✓
- Env `WORKOS_CLIENT_ID` (+ overridable `WORKOS_JWKS_URL` for tests) → Task 2. ✓
- Deferred: per-user settings store (slice 2), all-API-Agent-direct migration, homegrown-auth removal, SSO. ✓
- Note: identity = validated token `sub`; `email` is a display label supplied by the web (acceptable for slice 1 — flagged for hardening if email is ever security-relevant).
