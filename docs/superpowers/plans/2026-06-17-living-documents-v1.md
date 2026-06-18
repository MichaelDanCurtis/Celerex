# Living Documents v1 (agent-fill primitive) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a published Design template's slots agent-fillable — each slot carries a server-held instruction (the "AI spot"), an on-demand "Fill" runs the engagement's agent to populate it, and the filled design renders server-side.

**Architecture:** New `template_slot_fills` table (per `(template_id, slot)`: instruction + last_value) on the Agent, scoped to the template's engagement. Fill = a one-shot completion via the existing assistant provider path (Anthropic/OpenAI/Copilot/Codex/BYOK). Render = substitute stored values into the template's `{{slot:name}}` markers reusing the existing scaffold render logic. Web calls the Agent directly with the WorkOS session Bearer (settings/Copilot pattern).

**Tech Stack:** Rust/Axum (`celerex-api`), sqlx/Postgres, `axum_test::TestServer` + `tests/common::start_test_db()`; Next.js web (`apps/web`), vitest+jsdom.

## Global Constraints

- Agent: run from `cd "packages/backend/api"`; web: `cd "apps/web"` in celerex-design. Format ONLY changed files: `rustfmt --edition 2024 <file>` (revert pre-existing-drift reformatting — the repo is not edition-2024-formatted).
- **After ANY Rust signature change, run the FULL `cargo test` (all binaries), not just the new test** — a store/handler signature change breaks every test binary that calls it directly (the A2P2 cascade lesson). Grep `tests/` for callers.
- Auth: Agent routes use `DeviceAuth` (accepts full session OR `design.device` scope) → `claims.engagement_id: Uuid`. **Every handler loads the template and verifies `template.engagement_id == claims.engagement_id`** (else 404 for reads / 403 for the ownership mismatch — match the existing `design/` convention found in recon) before touching slot-fill rows. The instruction/value must never be returned for a template the caller doesn't own.
- Tests under the default parallel harness (no `--test-threads=1`); mint tokens with the helper used in `tests/engagements.rs` (e.g. `mint_for_tests`).
- **OUT OF SCOPE (deferred, do not build):** recurring Ritual-triggered fill (v2), publish-to-destinations (v3), connector-backed "fill from <X>", typed/structured slot values (v1 stores each slot value as a STRING). Fill is a ONE-SHOT completion, NOT a flow run.
- Next migration number: **0022**.

---

# Stage 1 — Agent (branch `feat/living-docs-agent`)

### Task 1: migration 0022 + slot-fill store + ownership helper

**Files:** Create `src/db/migrations/0022_template_slot_fills.sql`, `src/design/slot_fills.rs` (store + ownership helper); Modify `src/design/mod.rs` (`pub mod slot_fills;`); Test: covered by T2/T3 integration tests.

**Interfaces:**
- Produces: `SlotFillStore` (or free fns in `slot_fills.rs`):
  - `async fn template_engagement(pool, template_id) -> Result<Option<Uuid>, String>` (the owning engagement, for the ownership check).
  - `async fn upsert_instruction(pool, template_id: Uuid, slot: &str, instruction: &str, provider_id: Option<&str>, model: Option<&str>) -> Result<(), String>`
  - `async fn get_slot_fill(pool, template_id: Uuid, slot: &str) -> Result<Option<SlotFill>, String>` where `SlotFill { slot_name, instruction, last_value: Option<String>, provider_id: Option<String>, model: Option<String> }`
  - `async fn list_slot_fills(pool, template_id: Uuid) -> Result<Vec<SlotFill>, String>`
  - `async fn set_value(pool, template_id: Uuid, slot: &str, value: &str) -> Result<(), String>`

- [ ] **Step 1: Recon.** Read the `design_templates` schema (find its migration — confirm columns: `id`, `engagement_id`, the **slots JSONB** column name, and the column holding the **template HTML** used for render — likely `template_html`/`html`; note exact names). Read `src/design/mod.rs` (how submodules are declared + how the ingest handler reads design_templates). Read `tests/common/mod.rs` + an existing `design/` integration test for the harness + seed pattern. Confirm `gen_random_uuid()` availability (used in prior migrations).

- [ ] **Step 2: Migration** `src/db/migrations/0022_template_slot_fills.sql`:
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

- [ ] **Step 3: Store** `src/design/slot_fills.rs` (match the repo's sqlx style; `SlotFill` derives `serde::Serialize`):
```rust
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlotFill {
    pub slot_name: String,
    pub instruction: String,
    pub last_value: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
}

/// The engagement that owns a template (None if the template doesn't exist).
pub async fn template_engagement(pool: &PgPool, template_id: Uuid) -> Result<Option<Uuid>, String> {
    sqlx::query_scalar::<_, Uuid>("SELECT engagement_id FROM design_templates WHERE id = $1")
        .bind(template_id).fetch_optional(pool).await.map_err(|e| e.to_string())
}

pub async fn upsert_instruction(pool: &PgPool, template_id: Uuid, slot: &str, instruction: &str, provider_id: Option<&str>, model: Option<&str>) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO template_slot_fills (template_id, slot_name, instruction, provider_id, model)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (template_id, slot_name) DO UPDATE SET instruction=$3, provider_id=$4, model=$5, updated_at=NOW()",
    ).bind(template_id).bind(slot).bind(instruction).bind(provider_id).bind(model)
     .execute(pool).await.map(|_| ()).map_err(|e| e.to_string())
}

pub async fn get_slot_fill(pool: &PgPool, template_id: Uuid, slot: &str) -> Result<Option<SlotFill>, String> {
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>)>(
        "SELECT slot_name, instruction, last_value, provider_id, model FROM template_slot_fills WHERE template_id=$1 AND slot_name=$2",
    ).bind(template_id).bind(slot).fetch_optional(pool).await
     .map(|o| o.map(|(slot_name, instruction, last_value, provider_id, model)| SlotFill { slot_name, instruction, last_value, provider_id, model }))
     .map_err(|e| e.to_string())
}

pub async fn list_slot_fills(pool: &PgPool, template_id: Uuid) -> Result<Vec<SlotFill>, String> {
    sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>)>(
        "SELECT slot_name, instruction, last_value, provider_id, model FROM template_slot_fills WHERE template_id=$1 ORDER BY slot_name",
    ).bind(template_id).fetch_all(pool).await
     .map(|rows| rows.into_iter().map(|(slot_name, instruction, last_value, provider_id, model)| SlotFill { slot_name, instruction, last_value, provider_id, model }).collect())
     .map_err(|e| e.to_string())
}

pub async fn set_value(pool: &PgPool, template_id: Uuid, slot: &str, value: &str) -> Result<(), String> {
    sqlx::query("UPDATE template_slot_fills SET last_value=$3, updated_at=NOW() WHERE template_id=$1 AND slot_name=$2")
        .bind(template_id).bind(slot).bind(value).execute(pool).await.map(|_| ()).map_err(|e| e.to_string())
}
```
Add `pub mod slot_fills;` to `src/design/mod.rs`.

- [ ] **Step 4: Build** `cargo build --lib 2>&1 | tail -5` → clean (the store is exercised by T2/T3 tests). `rustfmt --edition 2024` the new files.

- [ ] **Step 5: Commit**
```bash
git add packages/backend/api/src/db/migrations/0022_template_slot_fills.sql packages/backend/api/src/design/slot_fills.rs packages/backend/api/src/design/mod.rs
git commit -m "feat(agent): template_slot_fills table + slot-fill store + template-ownership helper"
```

### Task 2: instruction + slots-read endpoints

**Files:** Create `src/design/living.rs` (handlers) or add to `src/design/slot_fills.rs`; Modify `src/lib.rs` (routes), `src/design/mod.rs`; Test `tests/living_docs_slots.rs`

**Interfaces:**
- Consumes: `slot_fills::{template_engagement, upsert_instruction, list_slot_fills}`.
- Produces: routes `PUT /api/design/templates/{template_id}/slots/{slot}/instruction`, `GET /api/design/templates/{template_id}/slots`.

- [ ] **Step 1: Recon.** Read an existing `src/design/` handler (e.g. `design/deploy.rs` or `design/codex.rs`) for the `DeviceAuth` extractor usage, `State(state)`, error mapping, and how routes mount in `src/lib.rs`. Confirm the ownership-mismatch status the design surface uses (404 vs 403) and mirror it.

- [ ] **Step 2: Write failing test** `tests/living_docs_slots.rs`:
```rust
mod common;
// token() / mint_for_tests helper per Conventions

#[tokio::test]
async fn set_instruction_and_list_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let server = axum_test::TestServer::new(celerex_api::build_app(db.pool.clone())).unwrap();
    // Seed engagement A + a design_templates row owned by A with slots [{name:"headline",...}] (direct SQL insert mirroring the ingest handler's columns; capture template_id).
    // PUT /api/design/templates/{tid}/slots/headline/instruction (Bearer A) {instruction:"Write a punchy headline"} -> 200
    // GET /api/design/templates/{tid}/slots (Bearer A) -> 200, body contains headline with instruction set
    // Seed engagement B + token(B): GET .../{tid}/slots (Bearer B) -> 404 (B doesn't own the template)
    // PUT .../instruction (Bearer B) -> 404; unauth PUT -> 401
}
```

- [ ] **Step 3: Run red** `cargo test --test living_docs_slots 2>&1 | tail -20`.

- [ ] **Step 4: Handlers** (in `src/design/living.rs`; adapt extractor/State/error to recon):
```rust
use axum::extract::{State, Path};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::extractors::DeviceAuth;
use crate::AppState;
use crate::design::slot_fills;

async fn own_template(state: &AppState, claims_eng: Uuid, template_id: Uuid) -> Result<(), (StatusCode, String)> {
    match slot_fills::template_engagement(&state.pool, template_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))? {
        Some(eng) if eng == claims_eng => Ok(()),
        _ => Err((StatusCode::NOT_FOUND, "template not found".into())), // not-found for cross-tenant (no existence leak)
    }
}

#[derive(Deserialize)]
pub struct InstructionBody { pub instruction: String, pub provider_id: Option<String>, pub model: Option<String> }

pub async fn set_instruction(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path((template_id, slot)): Path<(Uuid, String)>, Json(b): Json<InstructionBody>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    if b.instruction.trim().is_empty() { return Err((StatusCode::BAD_REQUEST, "instruction required".into())); }
    slot_fills::upsert_instruction(&state.pool, template_id, &slot, &b.instruction, b.provider_id.as_deref(), b.model.as_deref()).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_slots(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path(template_id): Path<Uuid>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let fills = slot_fills::list_slot_fills(&state.pool, template_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "slots": fills })))
}
```
Add `pub mod living;` to `src/design/mod.rs`. Mount in `src/lib.rs` beside the other design routes:
```rust
.route("/api/design/templates/{template_id}/slots/{slot}/instruction", put(design::living::set_instruction))
.route("/api/design/templates/{template_id}/slots", get(design::living::list_slots))
```
(Use the axum path-param brace syntax the repo already uses — confirm `{name}` vs `:name` in recon.)

- [ ] **Step 5: Run** `cargo test --test living_docs_slots 2>&1 | tail -20 && cargo build --lib 2>&1 | tail -5` → PASS. `rustfmt` changed files.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/design/living.rs packages/backend/api/src/design/mod.rs packages/backend/api/src/lib.rs packages/backend/api/tests/living_docs_slots.rs
git commit -m "feat(agent): living-docs slot instruction + slots-read endpoints (engagement-scoped)"
```

### Task 3: fill + render endpoints

**Files:** Modify `src/design/living.rs`, `src/lib.rs` (routes); Test `tests/living_docs_fill.rs`

**Interfaces:**
- Consumes: `slot_fills::{template_engagement, get_slot_fill, list_slot_fills, set_value}`; the assistant one-shot completion path; the existing scaffold slot-substitution fn.
- Produces: routes `POST /api/design/templates/{template_id}/slots/{slot}/fill`, `POST /api/design/templates/{template_id}/fill`, `GET /api/design/templates/{template_id}/render`.

- [ ] **Step 1: Recon (critical).** Find (a) the assistant one-shot path: `resolve_engagement_provider(&state, engagement_id, &provider_id)` (or `build_engagement_provider`) + the provider `.complete(model, messages, tools)` signature + how `assistant::chat` builds `messages` (the chat-completions `[{role,content}]` shape) and reads the result text out of `completion.raw` (e.g. `raw["choices"][0]["message"]["content"]`). (b) The existing **slot substitution** used by the render step (grep `{{slot` / `scaffold` / the render node) — the fn that takes template HTML + slot name→value and produces filled HTML; reuse it. (c) The `design_templates` columns for the **template HTML** + the **slots definition** (to know slot names/types). (d) The default provider/model the assistant uses when none is set (mirror it).

- [ ] **Step 2: Write failing test** `tests/living_docs_fill.rs`. Stub the provider: set the engagement up so `resolve_engagement_provider` returns a working provider against a wiremock (mirror how `tests/assistant_chat.rs` / the provider tests stub completion), OR if simpler, seed a BYOK/registry provider pointing at a wiremock that returns a fixed completion. Test body:
```rust
// Seed engagement A + a design_templates row with template_html "<h1>{{slot:headline}}</h1>" and slots [headline].
// PUT instruction on headline (Bearer A).
// POST /api/design/templates/{tid}/slots/headline/fill (Bearer A) -> 200 { value: "<stubbed completion text>" }; assert template_slot_fills.last_value updated.
// GET /api/design/templates/{tid}/render (Bearer A) -> 200 { html } and html contains the stubbed value (substituted into {{slot:headline}}).
// POST fill on a slot with NO instruction -> 409.
// Cross-engagement: POST fill / GET render with Bearer B -> 404. Unauth -> 401.
```

- [ ] **Step 3: Run red** `cargo test --test living_docs_fill 2>&1 | tail -20`.

- [ ] **Step 4: Handlers** (append to `src/design/living.rs`; adapt to the recon'd assistant + substitution APIs):
```rust
// Build a one-shot prompt from the instruction + a slot-type hint, run the engagement provider, return the text.
async fn run_fill(state: &AppState, engagement_id: Uuid, instruction: &str, slot_type: &str, provider_id: Option<&str>, model: Option<&str>) -> Result<String, (StatusCode, String)> {
    let pid = provider_id.unwrap_or(/* recon: the assistant default provider id */ "anthropic").to_string();
    let provider = crate::assistant::resolve_engagement_provider(state, engagement_id, &pid).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))?; // adapt to the real return type/Result
    let sys = format!("You fill a document slot named for type '{slot_type}'. Return ONLY the slot content (for a table/list, return markdown). No preamble.");
    let messages = json!([{"role":"system","content": sys}, {"role":"user","content": instruction}]);
    let model = model.unwrap_or(/* recon: assistant default model */ "claude-3-5-sonnet").to_string();
    let completion = provider.complete(&model, messages, json!([])).await.map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    let text = completion.raw["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    Ok(text)
}

pub async fn fill_slot(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path((template_id, slot)): Path<(Uuid, String)>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let fill = slot_fills::get_slot_fill(&state.pool, template_id, &slot).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, "slot has no instruction".into()))?;
    let slot_type = /* recon: look up the slot's type from design_templates.slots; default "text" */ "text".to_string();
    let value = run_fill(&state, claims.engagement_id, &fill.instruction, &slot_type, fill.provider_id.as_deref(), fill.model.as_deref()).await?;
    slot_fills::set_value(&state.pool, template_id, &slot, &value).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "value": value })))
}

pub async fn fill_all(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path(template_id): Path<Uuid>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let fills = slot_fills::list_slot_fills(&state.pool, template_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let mut filled = Vec::new();
    for f in fills { // each has an instruction by construction (list returns only rows, which all have instruction NOT NULL)
        let value = run_fill(&state, claims.engagement_id, &f.instruction, "text", f.provider_id.as_deref(), f.model.as_deref()).await?;
        slot_fills::set_value(&state.pool, template_id, &f.slot_name, &value).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        filled.push(json!({ "slot": f.slot_name, "value": value }));
    }
    Ok(Json(json!({ "filled": filled })))
}

pub async fn render(DeviceAuth(claims): DeviceAuth, State(state): State<AppState>, Path(template_id): Path<Uuid>) -> Result<Json<Value>, (StatusCode, String)> {
    own_template(&state, claims.engagement_id, template_id).await?;
    let template_html = /* recon: SELECT template_html FROM design_templates WHERE id=$1 */ String::new();
    let fills = slot_fills::list_slot_fills(&state.pool, template_id).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    // reuse the existing scaffold substitution: for each {{slot:name}}, substitute last_value (or "")
    let html = crate::design::scaffold::/* recon: the substitution fn */ render_slots(&template_html, &fills);
    Ok(Json(json!({ "html": html })))
}
```
**Implementer:** replace the `/* recon */` markers with the real APIs found in Step 1 (the assistant resolver's exact name/signature/return, the default provider+model, the slots-type lookup, the template-HTML column, and the substitution fn — if no reusable substitution fn exists, write a small one: replace each `{{slot:NAME}}` with the matching `last_value` or empty). Mount routes in `src/lib.rs`:
```rust
.route("/api/design/templates/{template_id}/slots/{slot}/fill", post(design::living::fill_slot))
.route("/api/design/templates/{template_id}/fill", post(design::living::fill_all))
.route("/api/design/templates/{template_id}/render", get(design::living::render))
```

- [ ] **Step 5: Run** `cargo test --test living_docs_fill 2>&1 | tail -25 && cargo test --lib 2>&1 | tail -10 && cargo build --lib 2>&1 | tail -5` → PASS. Then the FULL suite `cargo test 2>&1 | tail -15` (catch any cascade). `rustfmt` changed files.

- [ ] **Step 6: Commit**
```bash
git add packages/backend/api/src/design/living.rs packages/backend/api/src/lib.rs packages/backend/api/tests/living_docs_fill.rs
git commit -m "feat(agent): living-docs slot fill (one-shot completion) + fill-all + render endpoints"
```

**Stage 1 ships as one Celerex branch `feat/living-docs-agent` → PR → squash-merge.**

---

# Stage 2 — Web (celerex-design `apps/web`, branch `feat/living-docs-web`)

### Task 4: `state/living-doc.ts` helpers

**Files:** Create `apps/web/src/state/living-doc.ts`, `apps/web/tests/state/living-doc.test.ts`

**Interfaces:**
- Consumes: `agentBaseUrl()`, `readAgentSession()` from `src/state/user-settings.ts`.
- Produces: `setSlotInstruction(base, token, templateId, slot, instruction, fetchFn?)`, `fillSlot(base, token, templateId, slot, fetchFn?) -> {value}`, `fillAll(base, token, templateId, fetchFn?)`, `getSlots(base, token, templateId, fetchFn?) -> {slots}`, `renderFilled(base, token, templateId, fetchFn?) -> {html}`.

- [ ] **Step 1: Recon.** Read `src/state/copilot.ts` + `src/state/user-settings.ts` for the direct-Agent helper conventions (Bearer header, base trim, throw-on-!ok, injectable fetchFn) and the vitest test pattern (`tests/state/*.test.ts`).

- [ ] **Step 2: Write failing test** `apps/web/tests/state/living-doc.test.ts`:
```ts
import { describe, it, expect, vi } from "vitest";
import { fillSlot, getSlots, setSlotInstruction } from "../../src/state/living-doc";
function mockFetch(body: unknown, status = 200): typeof fetch {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })) as unknown as typeof fetch;
}
describe("living-doc state", () => {
  it("setSlotInstruction PUTs with Bearer", async () => {
    const f = mockFetch({ ok: true });
    await setSlotInstruction("https://agent.example", "sess1", "t1", "headline", "Write a headline", f);
    const [url, init] = (f as any).mock.calls[0];
    expect(url).toBe("https://agent.example/api/design/templates/t1/slots/headline/instruction");
    expect((init as RequestInit).method).toBe("PUT");
    expect((init as any).headers.Authorization).toBe("Bearer sess1");
  });
  it("fillSlot POSTs and returns value", async () => {
    const f = mockFetch({ value: "Big News" });
    const r = await fillSlot("https://agent.example", "sess1", "t1", "headline", f);
    expect(r.value).toBe("Big News");
    expect((f as any).mock.calls[0][0]).toBe("https://agent.example/api/design/templates/t1/slots/headline/fill");
  });
  it("getSlots GETs", async () => {
    const f = mockFetch({ slots: [{ slot_name: "headline", instruction: "x", last_value: null }] });
    const r = await getSlots("https://agent.example", "sess1", "t1", f);
    expect(r.slots[0].slot_name).toBe("headline");
  });
});
```

- [ ] **Step 3: Run red** `cd "apps/web" && pnpm vitest run tests/state/living-doc.test.ts 2>&1 | tail -20`.

- [ ] **Step 4: Implement** `apps/web/src/state/living-doc.ts`:
```ts
export interface SlotFill { slot_name: string; instruction: string; last_value: string | null; provider_id?: string | null; model?: string | null; }
const base0 = (b: string) => b.replace(/\/+$/, "");
const authd = (token: string, init: RequestInit = {}): RequestInit => ({ ...init, headers: { ...(init.headers ?? {}), Authorization: `Bearer ${token}` } });

export async function setSlotInstruction(base: string, token: string, templateId: string, slot: string, instruction: string, fetchFn: typeof fetch = fetch): Promise<{ ok: boolean }> {
  const res = await fetchFn(`${base0(base)}/api/design/templates/${templateId}/slots/${encodeURIComponent(slot)}/instruction`, authd(token, { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ instruction }) }));
  if (!res.ok) throw new Error("Could not save the slot instruction.");
  return res.json() as Promise<{ ok: boolean }>;
}
export async function fillSlot(base: string, token: string, templateId: string, slot: string, fetchFn: typeof fetch = fetch): Promise<{ value: string }> {
  const res = await fetchFn(`${base0(base)}/api/design/templates/${templateId}/slots/${encodeURIComponent(slot)}/fill`, authd(token, { method: "POST" }));
  if (!res.ok) throw new Error("Fill failed.");
  return res.json() as Promise<{ value: string }>;
}
export async function fillAll(base: string, token: string, templateId: string, fetchFn: typeof fetch = fetch): Promise<{ filled: Array<{ slot: string; value: string }> }> {
  const res = await fetchFn(`${base0(base)}/api/design/templates/${templateId}/fill`, authd(token, { method: "POST" }));
  if (!res.ok) throw new Error("Fill all failed.");
  return res.json() as Promise<{ filled: Array<{ slot: string; value: string }> }>;
}
export async function getSlots(base: string, token: string, templateId: string, fetchFn: typeof fetch = fetch): Promise<{ slots: SlotFill[] }> {
  const res = await fetchFn(`${base0(base)}/api/design/templates/${templateId}/slots`, authd(token, { method: "GET" }));
  if (!res.ok) throw new Error("Could not load slots.");
  return res.json() as Promise<{ slots: SlotFill[] }>;
}
export async function renderFilled(base: string, token: string, templateId: string, fetchFn: typeof fetch = fetch): Promise<{ html: string }> {
  const res = await fetchFn(`${base0(base)}/api/design/templates/${templateId}/render`, authd(token, { method: "GET" }));
  if (!res.ok) throw new Error("Could not render the document.");
  return res.json() as Promise<{ html: string }>;
}
```
Align to copilot.ts conventions if they differ; keep the injectable `fetchFn`.

- [ ] **Step 5: Run green** (3/3) + `pnpm tsc -b --noEmit 2>&1 | tail -8`.

- [ ] **Step 6: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/state/living-doc.ts apps/web/tests/state/living-doc.test.ts
git commit -m "feat(web): living-doc state helpers (direct Agent calls)"
```

### Task 5: "Living slots" panel

**Files:** Create `apps/web/src/components/living/LivingSlotsPanel.tsx`, `apps/web/tests/components/LivingSlotsPanel.test.tsx`; Modify the publish/templatize surface to mount it (recon for the exact host).

**Interfaces:**
- Consumes: `state/living-doc.ts` helpers; `readAgentSession()` + `agentBaseUrl()`.

- [ ] **Step 1: Recon.** Read `src/components/byok/CodexOAuthPanel.tsx` for the mounted-ref + logged-in gating pattern, and `PublishToCelerexDialog.tsx` for the slot model + where a "Living slots" panel should hang (it needs a `templateId` — confirm where a published template's id is available in the web flow; if the publish flow returns `template_id`, pass it in). Read the vitest+jsdom component-test pattern (the Copilot/Codex panel tests).

- [ ] **Step 2: Write failing test** `apps/web/tests/components/LivingSlotsPanel.test.tsx`:
```tsx
// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
vi.mock("../../src/state/living-doc", () => ({
  getSlots: vi.fn(async () => ({ slots: [{ slot_name: "headline", instruction: "Write a headline", last_value: null }] })),
  setSlotInstruction: vi.fn(async () => ({ ok: true })),
  fillSlot: vi.fn(async () => ({ value: "Big News Today" })),
  fillAll: vi.fn(async () => ({ filled: [] })),
  renderFilled: vi.fn(async () => ({ html: "<h1>Big News Today</h1>" })),
}));
vi.mock("../../src/state/user-settings", () => ({
  readAgentSession: vi.fn(() => ({ token: "sess1", engagement: { id: "e", name: "w" } })),
  agentBaseUrl: vi.fn(() => "https://agent.example"),
}));
import { fillSlot } from "../../src/state/living-doc";
import { LivingSlotsPanel } from "../../src/components/living/LivingSlotsPanel";
describe("LivingSlotsPanel", () => {
  afterEach(() => { cleanup(); vi.clearAllMocks(); });
  it("fills a slot and shows the value", async () => {
    render(<LivingSlotsPanel templateId="t1" />);
    fireEvent.click(await screen.findByRole("button", { name: /fill/i }));
    await waitFor(() => expect(fillSlot).toHaveBeenCalled());
    expect(await screen.findByText(/Big News Today/)).toBeTruthy();
  });
  it("shows a signed-out hint with no session", async () => {
    const us = await import("../../src/state/user-settings");
    (us.readAgentSession as any).mockReturnValueOnce(null);
    render(<LivingSlotsPanel templateId="t1" />);
    expect(await screen.findByText(/sign in/i)).toBeTruthy();
  });
});
```

- [ ] **Step 3: Run red** `cd "apps/web" && pnpm vitest run tests/components/LivingSlotsPanel.test.tsx 2>&1 | tail -20`.

- [ ] **Step 4: Implement** `apps/web/src/components/living/LivingSlotsPanel.tsx` (mirror `CodexOAuthPanel` structure + the mounted-ref guard; load slots on mount, per-slot instruction textarea + Fill button + last value, a Fill-all button, and a sandboxed iframe showing `renderFilled().html`):
```tsx
import { useEffect, useRef, useState } from "react";
import { readAgentSession, agentBaseUrl } from "../../state/user-settings";
import { getSlots, setSlotInstruction, fillSlot, fillAll, renderFilled, type SlotFill } from "../../state/living-doc";

interface Props { templateId: string; }

export function LivingSlotsPanel({ templateId }: Props) {
  const session = readAgentSession();
  const [slots, setSlots] = useState<SlotFill[]>([]);
  const [html, setHtml] = useState<string>("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);
  useEffect(() => {
    if (!session) return;
    (async () => {
      try { const r = await getSlots(agentBaseUrl(), session.token, templateId); if (mounted.current) setSlots(r.slots); } catch { /* ignore initial load error */ }
    })();
  }, [templateId]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!session) return <p className="living-slots__hint">Sign in to use AI-fill slots.</p>;

  async function doFill(slot: string) {
    setBusy(slot); setError(null);
    try {
      await fillSlot(agentBaseUrl(), session!.token, templateId, slot);
      const [s, r] = await Promise.all([getSlots(agentBaseUrl(), session!.token, templateId), renderFilled(agentBaseUrl(), session!.token, templateId)]);
      if (mounted.current) { setSlots(s.slots); setHtml(r.html); }
    } catch (e) { if (mounted.current) setError((e as Error).message); }
    finally { if (mounted.current) setBusy(null); }
  }
  async function saveInstruction(slot: string, instruction: string) {
    try { await setSlotInstruction(agentBaseUrl(), session!.token, templateId, slot, instruction); } catch (e) { if (mounted.current) setError((e as Error).message); }
  }
  async function doFillAll() {
    setBusy("__all__"); setError(null);
    try { await fillAll(agentBaseUrl(), session!.token, templateId); const [s, r] = await Promise.all([getSlots(agentBaseUrl(), session!.token, templateId), renderFilled(agentBaseUrl(), session!.token, templateId)]); if (mounted.current) { setSlots(s.slots); setHtml(r.html); } }
    catch (e) { if (mounted.current) setError((e as Error).message); }
    finally { if (mounted.current) setBusy(null); }
  }

  return (
    <div className="living-slots">
      <button type="button" onClick={() => void doFillAll()} disabled={busy !== null}>Fill all</button>
      {slots.map((s) => (
        <div key={s.slot_name} className="living-slots__row">
          <strong>{s.slot_name}</strong>
          <textarea defaultValue={s.instruction ?? ""} onBlur={(e) => void saveInstruction(s.slot_name, e.target.value)} aria-label={`instruction for ${s.slot_name}`} />
          <button type="button" onClick={() => void doFill(s.slot_name)} disabled={busy !== null}>Fill</button>
          {s.last_value ? <p className="living-slots__value">{s.last_value}</p> : null}
        </div>
      ))}
      {html ? <iframe className="living-slots__preview" sandbox="" srcDoc={html} title="Filled preview" /> : null}
      {error ? <p className="living-slots__err">{error}</p> : null}
    </div>
  );
}
```
(After a Fill the row's `last_value` comes from the refreshed `getSlots`; the test asserts the value text appears.) Mount the panel where recon says a published template's `templateId` is available (e.g. after publish in `PublishToCelerexDialog`, or a new tab). If wiring into an existing surface is non-trivial, at minimum export + render it behind the published-template flow and note the host in the report.

- [ ] **Step 5: Run green** (2/2) + `pnpm tsc -b --noEmit 2>&1 | tail -10 && pnpm vitest run 2>&1 | tail -20` (suite green except known pre-existing `default-background` failures).

- [ ] **Step 6: Commit**
```bash
cd "/Volumes/Main External/Development/celerex-design"
git add apps/web/src/components/living/LivingSlotsPanel.tsx apps/web/tests/components/LivingSlotsPanel.test.tsx apps/web/src/components/PublishToCelerexDialog.tsx
git commit -m "feat(web): Living slots panel (set instruction, Fill, render preview)"
```

**Stage 2 ships as one celerex-design branch `feat/living-docs-web` → PR → squash-merge.**

---

## Final verification
- [ ] Agent: `cd "packages/backend/api" && cargo test --test living_docs_slots --test living_docs_fill 2>&1 | tail -15 && cargo test 2>&1 | tail -10` → PASS (full suite, cascade check).
- [ ] Web: `cd "apps/web" && pnpm vitest run tests/state/living-doc.test.ts tests/components/LivingSlotsPanel.test.tsx 2>&1 | tail -10 && pnpm tsc -b --noEmit` → green/clean.
- [ ] Two branches: Celerex `feat/living-docs-agent` PR; celerex-design `feat/living-docs-web` PR.

## Self-review (spec coverage)
- Migration 0022 template_slot_fills + store + ownership helper → Task 1. ✓
- Server-held per-slot instruction (PUT) + slots read (GET) → Task 2. ✓
- On-demand Fill = one-shot engagement completion, store last_value; fill-all; server render via {{slot:name}} substitution → Task 3. ✓
- Engagement scoping / no cross-tenant leak (own_template check) + 409 on no-instruction + unauth 401 → Tasks 2-3 tests. ✓
- Web direct-Agent helpers (session Bearer) → Task 4. ✓
- Living slots panel (instruction + Fill + value + preview + logged-out hint) → Task 5. ✓
- Fill = one-shot (not flow run); v2/v3/connector-fill/typed-values deferred → Global Constraints + spec §2. ✓
- Reuse assistant completion + scaffold substitution (not new infra) → Task 3 recon. ✓
