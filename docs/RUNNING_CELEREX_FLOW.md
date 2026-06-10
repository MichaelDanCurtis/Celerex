# Running Celerex Flow locally

The visual flow editor is on `main` (merged from `feat/celerex-flow-editor`).
Backend = Rust/Axum on `:3001`, frontend = Vite on `:5173`.

## 1. Postgres (Docker)

```bash
docker run -d --name celerex-pg \
  -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=celerex \
  -p 5432:5432 postgres:16-alpine
```

Migrations run automatically on API startup (`sqlx::migrate!`).

## 2. Backend API

```bash
cd "/Volumes/Main External/Development/Celerex"
DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/celerex" \
CELEREX_CRED_KEY="celerex-dev-cred-key-0123456789abcdefghij" \
CELEREX_JWT_SECRET="celerex-dev-jwt-secret" \
cargo run -p celerex-api
# → celerex-api listening on http://0.0.0.0:3001
```

To make **LLM agent** nodes actually call a model, also export a provider key,
e.g. `ANTHROPIC_API_KEY=sk-...` (or OPENAI/GEMINI/KIMI). Without one, LLM nodes
fail at run time; every other node type (trigger, script, HTTP, doc, filter) runs
fully offline.

## 3. Frontend

```bash
cd "/Volumes/Main External/Development/Celerex/packages/frontend/app"
bun install      # first time only
bunx vite        # → http://localhost:5173
```

## 4. Sign in

The dev login swaps an **existing** engagement name for a JWT. Seed one once:

```bash
curl -s -X POST http://localhost:3001/api/engagements \
  -H 'content-type: application/json' -d '{"name":"Demo"}'
```

Then open http://localhost:5173, sign in as `Demo`, and click **⚡ Flows** in the
sidebar (under "Agents").

## 5. Try it

- **Flows landing** → pick a template (Jean's Newsletter, Bob the Researcher,
  Ideation → Docs, or the chained **Doc Builder**). LLM templates need a provider key.
- For a **fully-offline** demo, build: `Manual trigger → Run script (echo …) →
  Create document`, wire them left→right, hit **▶ Run**, watch the Monitor light
  the nodes, then check the new doc under **Docs**.
- Wire data between nodes with `{{steps.<node-id>.outputs.<path>}}` (the config
  panel's "Wire data from" chips copy the token for you).

## Verified end-to-end (2026-06-10)

A `trigger → script → doc.create` flow ran on a live server: status `succeeded`,
live SSE emitted `run_step_started / run_step_succeeded / run_completed`, and a
document was created from the templated script output. Full backend + frontend
test suites green; `vite build` clean.

## Notes / known
- `tsc -b` reports pre-existing type errors inside `node_modules/@blocksuite/*`
  (they ship raw TS); the app is bundled by Vite (no typecheck), so this does not
  affect running or building the app. All **first-party** `src/` code is type-clean.
- Per-engagement provider keys are stored encrypted but not yet injected into the
  LLM handler at call time (pre-existing Phase-1 limitation) — LLM nodes use the
  env-configured house provider.
