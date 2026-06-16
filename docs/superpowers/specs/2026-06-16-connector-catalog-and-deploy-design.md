# Connector Catalog & One-Click Deploy — sub-project 2h

**Date:** 2026-06-16
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`) + celerex-design (seam + daemon + web)
**Builds on:** the connectors-via-Agent engine (2a) — Agent vaults credentials, Design adopts + runs connectors. This sub-project formalizes the **catalog backend strategy** and adds the **one-click Deploy wizard** on top of those primitives.

## 1. Audience (the lens for every decision)

The user is a **non-technical creative** — "someone like my wife: not technically capable." The product exists to let creative people do creative work **without ever being exposed to technical complexity**. Every flow in this spec must assume **zero** technical knowledge: no jargon, no config files, no DNS records, no build commands. When a choice is unavoidable, it comes with an **"Ask AI"** button that decides for the user. This audience constraint overrides convenience for the implementer — if a step would make sense to a developer but not to this user, it's wrong.

## 2. Purpose

Two related things behind one seamless surface:

1. **A connector catalog** the user experiences as "the things I can connect to," with a clear backend strategy: **Composio is one hidden engine behind our seam for breadth; our own vault always owns the keys; our hand-built first-party connectors take precedence.**
2. **One-click Deploy** — the user builds something in Design, hits **Deploy**, answers a few simple questions (each with "Ask AI"), and the agent ships it for them.

## 3. Locked decisions (from brainstorming)

### Connector backend strategy
- **Reframe the 2a goal** from "replace Composio" to **"Composio is one backend behind our seam."** The user never sees "Composio" — only a seamless catalog.
- **Our per-user vault always owns the keys.** Split by who holds the credential:
  - **API-key services** → the user's key lives in **our vault**, injected at call time; Composio (or our executor) is just plumbing. Matches "we hold everything."
  - **OAuth apps** (Facebook/Google etc.) → Composio's managed login typically holds the token on **their** side; accepted only where it's the sane option, eyes open.
- **Composio is default-ON for the long tail** (instant large catalog).
- **Hand-built first-party connectors take precedence** over Composio for the same service.
- **Per-user own API keys, always** — every credential lives in our per-user vault.

### Deploy
- **One-click Deploy = a short guided wizard**, each step a simple choice with an **"Ask AI"** button:
  1. **Web host** — Vercel / Netlify (later: opinionate toward **Lucidity**).
  2. **Database** — Supabase / Convex / Neon.
  3. **Domain/hostname** — nudge toward **Cloudflare**.
  4. **Publish** — the agent executes the deploy.
- **A "deploy connector" = vaulted credentials + an agent-run that ships the artifact to that target.** Setup is minimal: the user **logs in / stores credentials once**; the agent does the rest.
- **Opinionated defaults** nudge toward our preferred stack (Lucidity host eventually, Cloudflare domain now).
- **"Ask AI" is a product-wide pattern**, not deploy-only: any choice a non-technical user faces should offer "Ask AI."

### First-party seed connectors (hand-built, up front), by role
- **Web host:** Netlify, Vercel (+ later Lucidity)
- **Database:** Supabase, Convex, Neon
- **Domain/hostname:** Cloudflare (the preferred nudge)
- **Hosting (general):** Hostinger
- **Flagship:** AiLuxi Lucidity Platform — shipped as a **"coming soon"** stub (visible, selectable-disabled)

These are **ours** (Composio won't meaningfully cover them), so they live on the agent-vault / executor side, not the Composio side.

## 4. Key facts from 2a (the primitives this builds on)

- **Agent catalog + credential API:** `GET /api/design/connectors`, confirm-gated `GET /api/design/connectors/{id}/credential`, session `PUT .../{id}/credential` — connector ids validated against a catalog. Credentials reuse the `engagement_provider_keys` store (per-user/per-engagement vault).
- **Seam:** `@celerex/connect` `fetchConnectors` / `fetchConnectorCredential` (+ types).
- **Daemon local executors:** `apps/daemon/src/connectors/agent-vault-executors.ts` (http-api / m365-graph / galvo-memory, injectable `fetchFn`) + `agent-vault-provider.ts`; service.ts has an agent-vault branch alongside Composio (additive — Composio is the fallthrough).
- **Agent-run firing primitive:** `run_job::enqueue_run(...)` (used by Rituals + Automations + manual runs) — the deploy executes as an agent-run.
- **Pattern to mirror:** every prior sub-project (Rituals 2b, Automations 2d) ships as **Agent API → `@celerex/connect` seam → daemon `/api/*` proxy → web surface.** Deploy follows the same spine.

## 5. Architecture

```
Design (web)                 daemon                  @celerex/connect        Agent (celerex-api)
─────────────                ──────                  ────────────────        ───────────────────
Deploy wizard UI    ──────►  /api/deploy/* proxy ──► deploy.ts seam   ──────► /api/design/deploy/*
  step: host                 (reads pairing)                                    - catalog (roles)
  step: database                                                                - Ask AI recommend
  step: domain                                                                  - deploy executor
  "Ask AI" button                                                               - per-target recipe
Connector setup     ──────►  /api/connectors/*   ──► connectors.ts   ──────►  /api/design/connectors/*
  (store creds once)         (existing 2a)           (existing 2a)              (existing 2a vault)
```

The deploy connectors are registered in the **same catalog** as 2a connectors, tagged with a **role** (`web-host | database | domain | hosting | platform`) and a **deploy capability**. The wizard reads the catalog filtered by role per step.

### 5.1 Backend selection (the "Composio is one backend" rule)

A single resolver decides, per service, which backend handles a call:

1. **First-party executor present?** → use it (our hand-built connector wins).
2. **Else Composio covers it?** → route through Composio (default-on for the long tail).
3. **Else** → not available (don't surface it).

Credentials are **always** read from our vault and injected at call time, regardless of backend. This is mostly a **framing + a small precedence check** over what 2a already built (agent-vault path + Composio fallthrough) — not a new engine.

## 6. The deploy connector model

A deploy connector adds two things to a normal 2a connector:

- **A credential** (an API token for the target — Vercel token, Supabase key, Neon key, Cloudflare token, Hostinger key). API-key flavor → clean, **our vault owns it**.
- **A deploy recipe** — a per-target executor that, given the user's token + the project's deployable bundle, performs the deployment and returns a result (a live URL or a clear status). Recipes extend the existing agent-vault executor pattern.

**The deployable bundle (boundary):** Design already produces artifacts (the `artifacts/` system). The deploy takes the project's built/exported web output as its input. This spec treats "produce the deployable bundle from the artifact" as a **defined handoff owned by Design's existing artifact/export path** — the deploy recipe consumes a bundle, it does not build it. (Exact bundle shape is resolved at planning time against the artifact code.)

## 7. The Deploy wizard (UX)

Triggered by a **Deploy** button on a project. A short modal wizard, one decision per step, plain language, **"Ask AI"** on every step:

- **Step 1 — "Where should it live?" (web host).** Cards: Vercel, Netlify. "Ask AI" → recommends one with a one-line reason ("Your project is a Next.js site, so Vercel is the easiest fit"). If the chosen host has no stored login yet, a **one-time** "log in / paste your key" step appears, stored in the vault.
- **Step 2 — "Where's the data?" (database).** Cards: Supabase, Convex, Neon, plus a "No database needed" option. "Ask AI" recommends. Same one-time credential step if needed. *(May be skipped if the project has no data needs — see v1.)*
- **Step 3 — "What's the address?" (domain).** Steer to Cloudflare; offer "Use a free address for now" as the zero-friction default. "Ask AI" available.
- **Step 4 — Publish.** A single "Publish" button → the agent runs the deploy recipe(s) → progress shown in plain language → ends with a **clickable live link**.

### 7.1 "Ask AI" endpoint
`POST /api/design/deploy/recommend` (Agent), proxied via the daemon. Input: a short project summary + the candidate options for that step. Output: `{ choice, reason }` (one option + one plain-language sentence). Reuses the existing LLM proxy. The same primitive backs the product-wide "Ask AI" pattern.

### 7.2 Error handling (non-technical-first)
Failures never surface stack traces, HTTP codes, or provider jargon. Each failure maps to a plain sentence + one action: e.g. *"We couldn't reach Vercel with that login. Want to re-enter it or Ask AI?"* Technical detail is logged server-side for us, never shown to the user.

## 8. v1 — the first slice (smallest end-to-end proof)

Mirrors how 2c/2d started (prove one path, then widen):

**Deploy a project to ONE web host (Vercel) through the wizard, end-to-end:**
1. Catalog gains the deploy connectors as entries with roles (all of them visible), but only **Vercel** has a working deploy recipe in v1; the rest render as "coming soon" cards.
2. Deploy button → wizard **Step 1 only** (host = Vercel), with **"Ask AI"** working.
3. One-time **store Vercel token** in the vault (reuses 2a credential PUT).
4. **Publish** → agent runs the Vercel deploy recipe against the project's bundle → returns a live URL.
5. Steps 2 (database) and 3 (domain) are **scaffolded but skippable** ("Skip for now").

This proves: catalog-by-role, the credential-once flow, "Ask AI," and a real agent-executed deploy returning a link — with one target, against a real Vercel account.

### Later phases (not v1)
- Add deploy recipes for Netlify, then Supabase/Convex/Neon (database provisioning), then Cloudflare (domain).
- Wire the database + domain steps for real.
- Lucidity host connector flips from "coming soon" to live; opinionation nudges toward it.
- Formalize the Composio backend-selection resolver across the data-source catalog.

## 9. Data model changes

Minimal — reuse 2a's catalog + vault. Additions:
- **Catalog entries** for the deploy connectors, each carrying `role` and a `deploy_capable` flag (catalog is code-defined today; these are new entries, not a schema change).
- **No new credential table** — deploy creds use `engagement_provider_keys`.
- A deploy run is a normal **agent-run** (`enqueue_run` with a `triggered_by` of `manual`); no new run type needed for v1. (If deploy history needs first-class status later, revisit — not in v1.)

## 10. Testing

- **Agent (Rust):** unit tests for the catalog-by-role filter + backend-selection resolver; the Vercel deploy recipe tested against a **stubbed Vercel API** (injectable fetch, like the http-api adapter / wiremock pattern in 2d); `recommend` endpoint tested with a stubbed LLM.
- **Seam:** bun tests for `deploy.ts` (request/response shapes), mirroring `automations.ts`.
- **Daemon:** proxy tests mirroring the ritual/automation proxy (pairing read, 403/409/502 paths).
- **Web:** component tests for the wizard (step navigation, "Ask AI" call, credential-once prompt, skip behavior, plain-language error rendering).
- **No live third-party calls in tests** — all provider APIs stubbed.

## 11. Non-goals (YAGNI)

- No multi-environment / staging / rollback management.
- No build pipeline ownership (Design's artifact/export path produces the bundle).
- No team/seat/billing concepts.
- No custom DNS record editing UI (Cloudflare nudge handles the common path; advanced DNS is out).
- No Composio data-source expansion in this spec beyond the backend-selection framing (that's its own follow-up).

## 12. Resolved at planning time (recon needed)

These are **decisions made**; the plan confirms the exact code seams:
- The deployable-bundle shape Design's artifact/export path emits (Step 6 boundary).
- The precise Vercel deploy mechanism (API vs CLI) given the bundle shape.
- Where catalog entries are defined in the Agent (extend the 2a catalog).
- The daemon proxy module placement (beside ritual/automation proxies).

## 13. Build order

1. **v1:** catalog-by-role + Vercel deploy recipe + wizard Step 1 + "Ask AI" + credential-once → live URL.
2. Netlify recipe; wire database step (Supabase first); wire domain step (Cloudflare).
3. Lucidity host live + opinionation; Composio backend-selection resolver across data sources.

Each ships as **Agent API → seam → daemon → web**, branch + PR + merge per the standing rule, every task spec-and-quality reviewed (the subagent-driven pattern from 2b/2d/2f).

See [[omnivyx-architecture]] (2a, 2h), [[celerex_connectors]], [[celerex_plugin_strategy]], [[celerex_file_connectors_principle]], [[celerex_user_stories]].
