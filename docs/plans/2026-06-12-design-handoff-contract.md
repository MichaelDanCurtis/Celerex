# Celerex ↔ Celerex Design — Handoff Contract (v0.1 spec)

**Status:** LOCKED 2026-06-12. Specced *before* cloning open-design so the clone is
built to this seam from day one.

**Locked decisions (§10):** (1) auth = paste-a-scoped-token first, device-pairing
next; (2) assets = Postgres blob table v0.1, object store later; (3) live-render =
support both MJML + HTML via `render.engine`; (4) live ingest auto-scaffolds the
`schedule → fill → render → review → deliver` flow from `suggested_flow`, editable
after; (5) desktop-first distribution, hosted web (open-design `apps/web`) later.

**Build order:** (A) `Render` ExecutionType — keystone, server-side slot substitution
[DONE — render.rs]; (B) `design_templates`/`design_assets` + `/api/design/ingest`,
`/templates`, `/templates/{id}`, `/assets/{id}` [DONE — design/, migration 0015,
design_ingest.rs]; (C) scoped `design.publish` token + device-pairing + live-mode flow
auto-scaffold [DONE — `Claims.scope`/`DesignAuth`, `POST /api/design/tokens`,
`/pair/{start,approve,poll}`, migration 0016 `design_pairings`, `design/scaffold.rs`
(`trigger→fill→render→review→deliver`), design_pairing.rs]; (D) clone open-design →
strip/re-brand → `.cdb` export + "Connect to Celerex".

**v0.1 transport note:** `/api/design/ingest` accepts the bundle as **JSON** (the
unzipped CDB: `{manifest, template_html, tokens_css, slots, assets:[{path,content_type,base64}]}`).
The multipart `.cdb` **zip** wrapper (+ sha256 integrity) is added alongside the desktop
app in (D) — there is no real `.cdb` producer until then. HTML is sanitized on ingest
(strip script/`on*`/`javascript:`; design CSS preserved). Live-mode flow auto-scaffold
(decision §10.4) deferred to (C): v0.1 stores into the Library; the Render node inlines
the template's HTML+slots when a flow is wired.

## 0. The product shape (settled)

Two products, one brand, one account:

- **Celerex** — server-backed document *orchestrator*. Auto-populates data-rich
  documents, runs flows, human review/approve, delivery. (Rust/Axum + Postgres +
  React/Vite, what exists today.)
- **Celerex Design** — local **Electron** *design establishment* (a clean clone of
  open-design, re-skinned). Generates beautiful web/mobile/slide/newsletter designs
  on the user's machine; BYOK; render/export local.

They are **not** merged. The only coupling is this contract: an **export bundle**
the Design app produces + an **ingest API** Celerex exposes + a **shared identity**
so a publish lands in the right workspace. No shared code, no shared runtime.

```
Celerex (web)                         Celerex Design (desktop/Electron)
  "Start Design Mode" ──launch/deeplink──▶  design locally
                                              │  "Publish to Celerex"
  ingest API  ◀──── CDB bundle (HTTPS) ───────┘
   │
   ├─ static mode → store artifact, ship it
   └─ live mode  → store TEMPLATE w/ slots → flow binds data → review → deliver  ⟲ weekly
```

## 1. Goals / non-goals

**Goals**
- A portable, versioned **bundle** the Design app emits and Celerex ingests.
- Two modes: **static artifact** (ship as-is) and **live template** (Celerex
  re-renders with fresh data on a schedule — the differentiator).
- A pairing flow so the desktop app authenticates to a specific Celerex engagement.
- Land directly on the pipeline we already built: document → flow → review/approve → deliver.

**Non-goals (v0.1)**
- No shared component code, no shared editor (Design uses Lexical; Celerex uses BlockSuite — fine).
- No Celerex-side rich render of PDF/PPTX/MP4 (those stay one-shot, produced in the
  desktop app). Celerex only needs to re-render the **HTML/MJML live template**.
- No real-time sync. Handoff is a discrete publish event (+ re-publish).

## 2. The two artifact modes (core idea)

| | Static artifact | Live template |
|---|---|---|
| What it is | A finished HTML/PDF/PPTX/MP4 the Design app rendered | An HTML/MJML template with **named data slots** + a slot schema |
| Celerex does | Stores it, ships it once | Binds slots to flow data, **re-renders server-side every run** |
| Use case | "Here's the final deck, send it" | "Design the newsletter shell once → fill + send weekly" |
| Renders on | Desktop app | **Celerex server** (string/DOM slot substitution — no Electron needed) |

The live template is the magic: Design makes the *shell with slots*, Celerex *fills
and ships it forever*. **Hard requirement:** a live template must be renderable by
Celerex's backend with plain slot substitution (no headless browser, no Electron) —
so the template is HTML/MJML + tokens CSS + a slot map, nothing that needs the
design engine to re-run.

## 3. The Celerex Design Bundle (CDB) — format

A single `.cdb` file = a **zip** with a fixed layout:

```
bundle.cdb (zip)
├── manifest.json          # the contract (schema below) — REQUIRED
├── template/
│   ├── index.html         # the renderable template (slots as {{slot:name}})
│   ├── index.mjml         # OPTIONAL: MJML source for email rendering
│   ├── tokens.css         # design tokens (from the open-design design-system)
│   └── styles.css         # compiled styles (Tailwind v4 output, inlined-safe)
├── assets/                # images/fonts referenced by the template
│   └── ...
├── slots.schema.json      # the data-slot schema (live mode) — REQUIRED if mode=live
└── preview/
    └── rendered.html      # a fully-rendered preview (sample data filled in)
    └── thumbnail.png      # for the Celerex inbox/library card
```

For **static** mode, `template/` is the final artifact (`index.html` is the finished
piece, or a `final.pdf`/`final.pptx`/`final.mp4` lives in the bundle root) and
`slots.schema.json` is omitted.

### manifest.json (schema)

```jsonc
{
  "cdb_version": "0.1",
  "kind": "newsletter | slides | web_prototype | image | video | document",
  "mode": "static | live",
  "title": "Q3 Investor Newsletter",
  "source": {
    "app": "celerex-design",
    "app_version": "0.10.0",
    "design_system": "atelier-zero",        // which of the 152 systems was used
    "created_at": "2026-06-12T18:00:00Z",
    "project_ref": "local-uuid"             // desktop app's own id, for re-publish
  },
  "render": {
    "engine": "html | mjml",                // how Celerex re-renders (live mode)
    "entry": "template/index.html",
    "output_formats": ["html", "email"],    // what Celerex can produce per run
    "slot_syntax": "mustache"               // {{slot:name}} — fixed for v0.1
  },
  "publish": {
    "intent": "newsletter",                 // hint → which Celerex flow template
    "suggested_flow": "weekly-newsletter",  // optional, maps to a flow blueprint
    "delivery": { "channel": "email", "audience_ref": null }  // optional hint
  },
  "integrity": { "bundle_sha256": "…", "asset_count": 12, "bytes": 184320 }
}
```

### slots.schema.json (live mode)

Declares each slot, its type, and an optional **binding hint** to flow output.

```jsonc
{
  "slots": [
    { "name": "headline",   "type": "text",     "required": true,
      "bind_hint": "{{steps.research.outputs.title}}" },
    { "name": "metrics",    "type": "table",    "columns": ["metric","value","wow"] },
    { "name": "hero_image", "type": "image",    "bind_hint": null },
    { "name": "body",       "type": "richtext", "format": "markdown" },
    { "name": "cta_url",    "type": "url" }
  ]
}
```

Slot types (v0.1): `text | richtext | number | url | image | table | list`. Celerex
substitutes each into `{{slot:name}}` (text/url), HTML-escapes appropriately, renders
markdown for `richtext`, builds `<table>`/`<ul>` for `table`/`list`, and rewrites
`image` slots to ingested asset URLs.

## 4. Identity & pairing (auth)

The desktop app must publish into a *specific Celerex engagement* as the *signed-in
user*. Two-phase, recommend **device-pairing** (clean UX); v0.1 fallback is paste-a-token.

**Recommended — device pairing (OAuth device-flow style):**
1. Desktop app: "Connect to Celerex" → `POST /api/design/pair/start` → returns
   `{ pair_code, verification_url, device_code, expires_in }`.
2. User opens `verification_url` in their Celerex web session, sees the code, approves
   + picks the target **engagement/project**.
3. Desktop app polls `POST /api/design/pair/poll {device_code}` → on approval returns a
   **scoped token** (JWT, `scope: design.publish`, bound to engagement_id, long-ish TTL
   + refresh). Stored in the OS keychain (the clone already uses `bun-secrets`-style native creds; open-design uses keytar-equivalent).
4. Deep link the other way: Celerex "Start Design Mode" button →
   `celerex-design://connect?workspace=<eng>&hint=<token-handle>` to pre-pair.

**v0.1 fallback:** user copies a `design.publish`-scoped token from Celerex
Settings → pastes into the desktop app. Ship this first; add device-pairing next.

Scope matters: the publish token can ONLY hit the design ingest/pair endpoints, not
the full API. Add `scope` claim + middleware check.

## 5. Celerex ingest API (new)

All auth-scoped to the token's engagement_id (existing `AuthClaims` pattern).

| Method | Route | Purpose |
|---|---|---|
| `POST` | `/api/design/pair/start` | begin device pairing (web-session auth) |
| `POST` | `/api/design/pair/poll` | desktop polls for the scoped token |
| `POST` | `/api/design/ingest` | **the handoff** — multipart upload of `bundle.cdb` |
| `GET`  | `/api/design/templates` | list ingested design templates (the Design Library) |
| `POST` | `/api/design/templates/{id}/republish` | upload a new version of an existing template |

**`POST /api/design/ingest`** (multipart: `bundle` = the `.cdb` zip):
1. Verify `scope: design.publish` + engagement.
2. Validate manifest against the CDB schema; verify `integrity.bundle_sha256`; enforce
   size limits; **sanitize** all HTML/assets (§7).
3. Persist:
   - `documents` row for the rendered artifact (reuse the existing docs table) +
     `doc_blocks` (one HTML/markdown block) so it shows up in the doc list immediately.
   - **`design_templates`** (new table): `id, engagement_id, kind, mode, manifest_json,
     template_html, tokens_css, slots_json, version, source_project_ref, created_at`.
   - assets → object storage (or `doc_assets` table) with rewritten URLs.
4. If `mode=live` and `publish.suggested_flow` is set → scaffold (or attach to) a flow
   `project_member` whose graph binds the slots and ends in a `Review` + delivery node.
5. Return `{ document_id, template_id, version, flow_member_id? }`.

The response lets the desktop app deep-link the user *back* into Celerex:
`celerex://open?document=<id>` (or "View in Celerex" opens the browser).

## 6. Lifecycle

**One-shot (static):** Design → "Publish to Celerex" → ingest (mode=static) →
document created → user reviews/sends via Celerex (optionally through the
review/approve + delivery pipeline). Done.

**Recurring (live template) — the differentiator:**
1. Design the newsletter *shell with slots* → publish (mode=live).
2. Celerex stores the template + scaffolds a `weekly-newsletter` flow:
   `trigger(schedule) → agent/data steps fill slots → render(template+slots) →
   Review & Approve → deliver(email)`. (`render` = a new `ExecutionType::Render`
   that does server-side slot substitution → HTML.)
3. Every week the flow runs, fills the template with fresh data, a human approves
   the rendered result (our existing Review node — draft = the rendered HTML), and
   it ships. **Design once, fill + send forever.**

## 7. Security

- **Untrusted HTML/CSS/JS:** the bundle's template is user-generated and rendered into
  emails/docs. Sanitize on ingest: strip `<script>`, event handlers, external
  `javascript:`/`data:` URLs except whitelisted; CSP for any in-app preview; render
  email through an MJML/inliner that drops scripts. Treat assets as opaque blobs,
  re-host them (no hotlinking).
- **Size/asset caps** on ingest; reject bundles over N MB / M assets.
- **Scoped token** (publish-only) so a leaked desktop token can't touch the rest of the API.
- **SHA-256 integrity** check; reject mismatches.
- Slot substitution **escapes by context** (HTML-escape text, attribute-escape urls).

## 8. Versioning & re-publish

- `cdb_version` gates the format. Ingest rejects unknown majors.
- `source.project_ref` ties a re-publish to the same `design_templates` row →
  `republish` bumps `version`, preserving existing flow bindings (slots matched by
  `name`; new/removed slots surfaced as a diff for the user to re-map).

## 9. Bidirectional (future, not v0.1)

"Open in Designer" from a Celerex doc: Celerex emits a CDB-shaped *seed* (content +
tokens) the desktop app imports. Same bundle format, reverse direction. Designing the
CDB now with this in mind keeps it symmetric.

## 10. Open decisions (need your call)

1. **Auth v0.1:** ship paste-a-token first, device-pairing next? (rec: yes)
2. **Asset storage:** new object store (S3/R2) vs. a Postgres `doc_assets` blob table
   for v0.1? (rec: Postgres blob table v0.1, swap to object store later)
3. **Live-render engine:** standardize templates on **MJML** (great for email, easy
   server render) or generic HTML + a slot/inliner? Newsletters → MJML is the sweet
   spot; web prototypes → HTML. (rec: support both via `render.engine`)
4. **Who scaffolds the flow:** auto-create a flow on live ingest, or land the template
   in a "Design Library" and let the user wire a flow manually? (rec: auto-scaffold
   from `suggested_flow`, editable after)
5. **Distribution:** is the desktop app a required download, or also a hosted web
   version (open-design's `apps/web` is Next.js — embeddable)? Affects the "Start
   Design Mode" CTA. (rec: desktop-first; hosted web later)
```
