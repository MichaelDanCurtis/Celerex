# WorkOS Login (sub-project B, slice 1) — design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repos:** celerex-design (web — `apps/web`, Next.js 16 App Router) + Celerex (Agent — Rust/Axum `celerex-api`)
**Builds on:** sub-project A (multi-user hardening). Decision recorded in memory `celerex-multitenancy-audit`: identity moves to **WorkOS**; the Agent keeps minting its own engagement-scoped session JWT so every Stage-A ownership check is unchanged.

## 1. Purpose

Replace the (unused-in-the-browser) homegrown login with **WorkOS AuthKit** so a user can log into the site and the app knows who they are — the prerequisite for the public browser experience and for Copilot. **Slice 1 = login works end-to-end and the Agent issues a session that all of Stage A trusts.** The per-user settings store (theme follows you) is slice 2; SSO (paid, per-connection) is deferred and customer-funded.

## 2. Locked decisions (from brainstorming)

- **WorkOS AuthKit-Next.js** in `apps/web` (Next.js 16, root `app/`). Free AuthKit tier (email/password + social incl. Sign-in-with-Microsoft/Google).
- **The Agent mints its own session.** AuthKit authenticates the human in the browser; `withAuth()` exposes a WorkOS **access token (JWT)**. The web app sends that token to a new Agent endpoint, which validates it, finds-or-provisions the user's **personal workspace** (engagement), and mints the **existing** session JWT (`Claims { sub, engagement_id }`). → all of Stage A's `claims.engagement_id` checks keep working untouched.
- **Personal workspace per user**, keyed to the WorkOS user id (solves the old signup-collision footgun — no shared-by-name engagements).
- **Homegrown `/api/auth/login`+`signup` are deprecated** (kept for now so existing tests pass; full removal is a later cleanup once WorkOS is the sole front door).
- **Secrets:** `apps/web/.env.local` (gitignored) holds `WORKOS_API_KEY` (sk_test), `WORKOS_CLIENT_ID=client_01KV9VXPN6DK8PG87WW6C80190`, `WORKOS_COOKIE_PASSWORD`, `NEXT_PUBLIC_WORKOS_REDIRECT_URI=http://localhost:3000/callback`. The Agent gets `WORKOS_CLIENT_ID` (for the JWKS issuer) in its own local env. Redirect URIs registered in the WorkOS dashboard: `http://localhost:3000/callback` (dev), `https://designer.ailuxis.com/callback` (prod).
- **Scope guardrail:** slice 1 does NOT rewire every existing `/api/*` call (currently daemon-routed) to talk to the Agent directly — that broad migration rides with the public-web-UI work. Slice 1 establishes identity + the Agent session; the app gates on it and the session token is available for Agent-direct calls.

## 3. Key facts from recon / references

- `apps/web` = Next.js **16.2.6**, root `app/` (catch-all `app/[[...slug]]` + `app/layout.tsx`), no existing `proxy.ts`/`middleware.ts`. Dev server `next dev --turbopack` (port 3000).
- AuthKit-Next.js (from the WorkOS skill + the canonical README, source of truth: `https://raw.githubusercontent.com/workos/authkit-nextjs/main/README.md`):
  - Package `@workos-inc/authkit-nextjs`.
  - Next 16 → root **`proxy.ts`**: `export default authkitProxy(); export const config = { matcher: [...] };` (NOT `middleware.ts` — Next 16 errors E900 if both exist).
  - Callback route from the redirect path: `/callback` → **`app/callback/route.ts`** with `export const GET = handleAuth();`.
  - **`AuthKitProvider`** must wrap `app/layout.tsx` (from `@workos-inc/authkit-nextjs/components`).
  - Server: `const { user, accessToken, organizationId } = await withAuth();` — **`accessToken` is the WorkOS JWT** to forward as `Authorization: Bearer`.
  - Sign-in: `getSignInUrl()` in a Server Action / Route Handler, or client `refreshAuth({ ensureSignedIn: true })` — never raw `getAuthorizationUrl()` in render. Sign-out: **POST server action** `signOut()`, never a GET route.
- The WorkOS access token is RS256, verifiable via WorkOS JWKS at **`https://api.workos.com/sso/jwks/<WORKOS_CLIENT_ID>`**; claims include `sub` (WorkOS user id), `sid` (session), and org context. (Confirm exact issuer/claims at plan time by decoding a real token from `withAuth()`.)
- Agent: `auth/jwt.rs::mint(engagement_id, sub, ttl) -> String` mints the existing session JWT; `db::create_engagement`/`find_or_create_engagement`; `users` table = `{ id, email, password_hash, engagement_id, created_at }` (migration `0014`). `jsonwebtoken` is already a dep (supports RS256 + JWKS verification).

## 4. Components

### 4.1 Web (`apps/web`, Next.js)
1. Install `@workos-inc/authkit-nextjs`.
2. Root **`proxy.ts`** — `authkitProxy()` with a matcher protecting the app routes (and excluding `/callback`, static assets).
3. **`app/callback/route.ts`** — `export const GET = handleAuth();`.
4. Wrap **`app/layout.tsx`** in `<AuthKitProvider>`.
5. **Sign-in entry** — a client component using `refreshAuth({ ensureSignedIn: true })` (or a Server Action calling `getSignInUrl()`), and a **POST sign-out** server action.
6. **Agent session exchange** — once `withAuth()` yields an `accessToken`, call the Agent `POST /api/auth/workos` with `Authorization: Bearer <accessToken>`; store the returned Agent session JWT (the value used for Agent-direct calls). Gate the app: no WorkOS session → sign-in; session but no Agent session yet → perform the exchange.

### 4.2 Agent (`celerex-api`, Rust)
1. **`POST /api/auth/workos`** (no `AuthClaims` — it's the entry point; it consumes a WorkOS Bearer token):
   - Read the WorkOS access token from the `Authorization` header.
   - **Validate** it against WorkOS JWKS (`https://api.workos.com/sso/jwks/<WORKOS_CLIENT_ID>`), RS256, with JWKS fetch + cache. Reject invalid/expired → 401.
   - Extract `sub` (WorkOS user id) + email (from the token or a WorkOS userinfo lookup with the API key if not in the token).
   - **Find-or-provision** the personal engagement: look up a user by WorkOS id; if absent, create an engagement + a user row carrying the WorkOS id; return its `engagement_id`.
   - **Mint** the existing session JWT via `jwt::mint(engagement_id, sub, ttl)`; return `{ token, engagement: { id, name } }` (mirror the existing `LoginResponse` shape).
2. **Identity mapping (migration):** add `workos_user_id TEXT UNIQUE` to `users` (nullable; homegrown users have it null) — or a dedicated `workos_identities(workos_user_id PK, user_id, engagement_id)` table. Provisioning is idempotent (second login finds the same engagement).
3. **Env:** `WORKOS_CLIENT_ID` (JWKS issuer) read at startup; optional `WORKOS_API_KEY` only if a userinfo lookup is needed for email.
4. Mount the route in `lib.rs` (public, like `/api/auth/login`). CORS already allowlists the web origin (Stage A).

### 4.3 Deprecate homegrown auth (light)
Mark `/api/auth/login`+`signup` deprecated in code comments; leave functional for existing tests. Full removal + the engagement-create policy tightening (the Stage-A T8 note) come in a later cleanup once WorkOS is the sole path.

## 5. Testing

- **Agent:** integration test for `POST /api/auth/workos` using a **stubbed JWKS** (serve a test RSA public key via wiremock; sign a token with the matching private key) → asserts: invalid token → 401; valid token → 200 with a session JWT whose `engagement_id` resolves; **idempotent provisioning** (second call returns the same engagement); the minted session is accepted by an existing protected route (e.g. `GET /api/projects`).
- **Web:** component tests (vitest + jsdom) — unauthenticated renders the sign-in entry; mocked `withAuth()` session triggers the exchange call and renders the app. Mock the WorkOS SDK + the Agent exchange `fetch`.

## 6. Non-goals / later

- **Slice 2:** per-user server-side settings store (theme/prefs follow the user).
- Rewiring all existing `/api/*` (daemon-routed) calls to Agent-direct (public-web-UI migration).
- Removing homegrown `/api/auth/*`; tightening engagement-create policy.
- **SSO** (SAML, $125/connection) — deferred, customer-funded.
- Directory Sync / SCIM / Admin Portal — later, enterprise.

## 7. Build order

Subagent-driven, branch + PR + merge per stage, each task spec-and-quality reviewed. Suggested: **Agent first** (migration + `/api/auth/workos` + JWKS validation + tests) so the exchange endpoint exists, then **Web** (AuthKit install + proxy + callback + provider + sign-in/out + the exchange + gate). Then slice 2 (settings store), then Copilot.

See memory `celerex-multitenancy-audit` (WorkOS decision + keys), `omnivyx-architecture` (web-vs-desktop + 3 auth modes), and the WorkOS skill references (`workos-authkit-nextjs.md`, `workos-node.md`, `workos-api-authkit.md`).
