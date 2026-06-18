# Living Documents v1 — the agent-fill primitive — design

**Date:** 2026-06-17
**Status:** Design — awaiting user review before writing-plans
**Repos:** Celerex (Agent — Rust/Axum `celerex-api`, `packages/backend/api`) + celerex-design (web)
**Builds on:** the existing slot/templatize + `scaffold_live_flow` infra; per-engagement providers (Anthropic/OpenAI/Copilot/Codex/BYOK); the WorkOS session + Agent-direct call pattern (settings/Copilot slices); A2 engagement-scoping discipline.

## 1. Strategic framing (user, 2026-06-17)

The product is embracing the **Open Design** model: celerex-design (a fork of open-design) is the **document/authoring surface**; Celerex is the **Agent backend framework**. The old Celerex `documents`/`doc_blocks` model is NOT the home for this feature. Recon confirmed Open Design is an agent-native **design studio** (artifacts generated via chat, rendered in an iframe) with an existing **slot/templatize** system (`PublishToCelerexDialog` → typed `{{slot:name}}` slots → the Agent's `design_templates` + `scaffold_live_flow`). So a "Living Document" is expressed as **a design whose slots are agent-fillable**, not a new block editor.

**Full vision (decomposed; v1 is the first slice):** author a templated design with AI-fill slots → an agent **fills** the slots (collecting fresh info) → render → **deliver/publish** (email/website/YouTube/…) → optionally on a **schedule** (a weekly Ritual). Almost all plumbing exists (slots, `scaffold_live_flow` trigger→fill→render→deliver, Rituals (2b), publish/connectors (2a/2h)). The ONE missing primitive: a slot has no **server-held instruction** ("AI spot") and there's no way to **run an agent fill**. v1 builds exactly that.

## 2. Scope

**v1 = the agent-fill primitive.** Per-slot **server-held instruction** + **on-demand Fill** (a freeform one-shot agent run that writes the slot value) + **server-side render** of the filled design. Demoable end-to-end: set an instruction on a slot → click Fill → see the slot populate → preview the filled design.

**Deferred (named, not built):**
- **v2** recurring fill via Ritual (compose 2b scheduling + the fill primitive).
- **v3** publish-to-destinations (compose `scaffold_live_flow` deliver + 2a/2h connectors).
- connector-backed "fill this table from <X>" — rides on the fill primitive once the fill-agent reaches connector tools (2a).
- typed/structured slot values (v1 treats each slot value as a string); in-place sub-block text regions.

## 3. Architecture & data model

A **Living Document v1 = a published Design template** (`design_templates` row, already engagement-scoped, with a slots JSONB definition). The NEW **AI-spot state** is held server-side, separate from the immutable template definition.

**Migration `0022_template_slot_fills.sql`:**
```sql
CREATE TABLE template_slot_fills (
    template_id UUID NOT NULL REFERENCES design_templates(id) ON DELETE CASCADE,
    slot_name TEXT NOT NULL,
    instruction TEXT NOT NULL,
    last_value TEXT,
    provider_id TEXT,
    model TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (template_id, slot_name)
);
```
Engagement scoping is via the template: every handler loads the template, checks `template.engagement_id == claims.engagement_id` (defense-in-depth per A2P2), and 403/404s otherwise. The instruction/value never leak cross-tenant.

**Fill = one-shot completion (NOT a flow run).** Reuse the assistant path: `resolve_engagement_provider(state, engagement_id, provider_id)` → `provider.complete(model, messages, tools)`, where `messages` is built from the slot's `instruction` (+ a slot-type hint, e.g. "produce a markdown table" for a `table` slot). Lighter, synchronous, demoable; automatically uses whatever the engagement has vaulted. v1 stores each slot value as a **string** substituted into `{{slot:name}}`; a `table`/`list` slot = the agent returns markdown/HTML for that slot.

## 4. Agent endpoints (`src/design/`, DeviceAuth, engagement-scoped)

Mount beside the other `/api/design/*` routes; use `DeviceAuth` (accepts full session OR `design.device` pairing — consistent with the design surface, supports the later daemon/Ritual path). Each verifies template ownership first.

- **`PUT /api/design/templates/{template_id}/slots/{slot}/instruction`** — body `{ instruction, provider_id?, model? }` → upsert the row → `{ ok: true }`. Server-held AI spot.
- **`POST /api/design/templates/{template_id}/slots/{slot}/fill`** — load instruction; run the engagement completion; store `last_value`; return `{ value }`. 409 if the slot has no instruction; 404 if the slot/template isn't the caller's.
- **`POST /api/design/templates/{template_id}/fill`** — fill ALL slots that have an instruction; return `{ filled: [{slot, value}], skipped: [slot…] }`.
- **`GET /api/design/templates/{template_id}/slots`** — list `{ slot_name, instruction, last_value, updated_at }` for the template (drives the UI + render). Slots without a row appear with `instruction: null`.
- **`GET /api/design/templates/{template_id}/render`** — return `{ html }` = the template HTML with current `last_value`s substituted into `{{slot:name}}` (reuse the existing scaffold render/slot-substitution logic). Unfilled slots render empty or a placeholder. Server-rendered "see the filled design."

DB fns in `src/db/mod.rs` (or a `design/slot_fills.rs` store): `upsert_slot_instruction`, `get_slot_fill`, `list_slot_fills`, `set_slot_value` — all taking `engagement_id` for the ownership check or operating on a template already verified.

## 5. Web surface (celerex-design `apps/web`)

- **`src/state/living-doc.ts`** — direct-Agent helpers (`setSlotInstruction`, `fillSlot`, `fillAll`, `getSlots`, `renderFilled`) using `agentBaseUrl()` + `readAgentSession()` session Bearer (the settings/Copilot pattern; logged-in user → Agent directly, no daemon). Injectable `fetchFn`.
- A **"Living slots"** panel for a published template (extends the templatize/publish surface): per slot → an instruction textarea, a **Fill** button, the last filled value, and a live **preview** (the `render` HTML in a sandboxed iframe). A **Fill all** button. Greyed with a "sign in" hint when there's no Agent session (mirror `CodexOAuthPanel`'s daemon-gate pattern, but gate on session presence).

## 6. Testing

- **Agent:** integration tests (`tests/common::start_test_db` + `axum_test::TestServer`, stub the provider via the engagement registry or a wiremock provider): set instruction → fill (asserts `last_value` stored + returned) → render (asserts `{{slot:name}}` substituted in the returned HTML); `fill` with no instruction → 409; **cross-engagement isolation** (engagement A cannot set/fill/read/render B's template slots → 403/404); unauth → 401. Mirror the A2 cross-tenant test style.
- **Web:** vitest for the state helpers (URL + Bearer header assertions) and the panel (set instruction → Fill → shows value; logged-out → hint), mocked `state/living-doc.ts`.

## 7. Build order

Subagent-driven, two-stage review each, branch+PR+merge per repo (mirrors the OAuth/A2 rhythm). **Stage 1 (Agent, Celerex):** migration 0022 + slot-fill store + the 5 endpoints (reusing the assistant completion path + the scaffold render/substitution) + integration tests; one Celerex branch. **Stage 2 (web, celerex-design):** `state/living-doc.ts` + the "Living slots" panel + vitest; one celerex-design branch. Honors `celerex_file_connectors_principle` (fill *into* the design's slots; no format conversion).

See memory `omnivyx-architecture` (2c), `celerex_file_connectors_principle`, the slot/`scaffold_live_flow` recon, and the prior provider/A2 slices this composes with.
