# Agent on Kubernetes (sub-project 2g)

**Date:** 2026-06-14
**Status:** Design — awaiting user review before writing-plans
**Repo:** Celerex (the Agent — `packages/backend/api`, Rust/Axum `celerex-api`)
**Related:** satisfies the 2f migration verify gate by giving a live, real Agent to pair against; first step toward a K8s-managed, cloud-portable (AWS/Azure) Galvo suite.

## 1. Purpose

Run the Celerex **Agent** (`celerex-api`) + its **Postgres** in a real Kubernetes namespace on this machine's Docker Desktop cluster — a reproducible, cloud-portable deployment, not a one-off local run. The **Design daemon + web stay local** (the compute edge that drives local CLIs); they pair *out* to the in-cluster Agent. Standing this up also provides the live environment to prove the 2f routine→Ritual migration end-to-end.

## 2. Locked decisions (from brainstorming)

- **Cluster:** the already-running **Docker Desktop Kubernetes** (`docker-desktop` context). No kind/minikube.
- **Namespace:** `celerex` (siblings for the rest of the Galvo suite come later).
- **Postgres:** **in-cluster** (`postgres:17-alpine` Deployment + PVC + Service), creds from a Secret. `DATABASE_URL` is Secret-driven so a cloud overlay can later point at RDS / Azure DB with no manifest change.
- **Exposure:** Service type **NodePort** `30001 → 3001`, so the local daemon pairs at a stable `http://localhost:30001` with no long-running `port-forward`.
- **Migrations:** run on Agent boot (`sqlx::migrate!("./src/db/migrations")` in `db/mod.rs`) — no migration Job/initContainer.
- **Image:** built locally; Docker Desktop K8s reads local images directly (`imagePullPolicy: IfNotPresent`, no registry).

## 3. Key facts from recon

- The Agent **requires** at boot: `DATABASE_URL`; `CELEREX_CRED_KEY` (panics without it — the cred-vault encryption key, `build_provider_keys_store`); `CELEREX_JWT_SECRET` (defaults to `"test-secret"` but must be set for real pairing/auth). Optional: `OPENAI_API_KEY`/`ANTHROPIC_API_KEY`/`GEMINI_API_KEY`/`MOONSHOT_API_KEY` (server-side LLM), `GALVO_*` (federation/memory). None of the optional keys are needed for pairing + the Ritual API + migration verification.
- Binds `0.0.0.0:3001`.
- **No compile-time `sqlx` macros** anywhere (all `sqlx::query`/`query_as` runtime strings) → the Docker build needs **no database and no `.sqlx` offline data**.
- Workspace is **edition 2024, resolver 3** → the Rust builder image must be ≥ 1.85 (`rust:1-bookworm` / pinned recent).
- **Cross-repo path dependency (the main build wrinkle):** `celerex-api` depends on `galvo-federation = { path = "../../../../lucidity/Galvo/engine/galvo-federation" }` — a crate in the **sibling `lucidity/Galvo` repo**, outside Celerex. Plus in-repo `celerex-orchestrator = { path = "../agent-orchestrator" }`. (`galvo-hub` is dev-only — not needed for the release binary.) So the Docker build context must include the Celerex workspace **and** the referenced Galvo engine crate(s), preserving their sibling relative paths.

## 4. Components

### 4.1 Dockerfile (`deploy/docker/agent.Dockerfile`)
Multi-stage:
- **builder:** `rust:1-bookworm` (≥1.85). Copies the assembled build context (see §4.2), runs `cargo build -p celerex-api --release`. (No `DATABASE_URL` needed — no compile-time SQL.)
- **runtime:** `debian:bookworm-slim` + `ca-certificates`. Copies the `celerex-api` binary. `EXPOSE 3001`. `CMD ["celerex-api"]`. Migrations run on boot.

### 4.2 Build context assembly (`deploy/docker/build-image.sh`)
Because of the cross-repo path dep, the script assembles a clean staging dir that preserves the sibling layout the Cargo paths expect:
- Stage the Celerex workspace (manifests + `packages/backend/**` member crates the build needs) and the referenced `lucidity/Galvo/engine/galvo-federation` (+ its own transitive intra-Galvo path-dep crates — resolve by reading `galvo-federation`'s `Cargo.toml` recursively) into a temp dir as siblings (`<tmp>/Celerex/...`, `<tmp>/lucidity/Galvo/engine/...`).
- A strict `.dockerignore` (exclude `target/`, `node_modules/`, `_reference/`, `.git/`) keeps the context lean.
- `docker build -f deploy/docker/agent.Dockerfile -t celerex-agent:dev <tmp>`.
- **Fallback if staging proves fiddly:** `cargo vendor` from the Celerex workspace (vendors path deps too) into the context + build `--offline` with context = Celerex repo only. The plan picks whichever builds cleanly; the host already compiles `celerex-api`, which is the source of truth for required crates.

> First build is slow (full Rust compile). Layer-cache deps (copy manifests → `cargo fetch`/build deps → copy sources) to speed rebuilds; `cargo-chef` is optional polish, not required for v1.

### 4.3 Kubernetes manifests (`deploy/k8s/base/`)
- `namespace.yaml` — `celerex`.
- `postgres.yaml` — `postgres:17-alpine` Deployment (1 replica) + `PersistentVolumeClaim` + `Service` (`celerex-postgres:5432`). Env (`POSTGRES_USER/PASSWORD/DB=celerex`) from the Secret. Readiness probe via `pg_isready`.
- `agent.yaml` — `celerex-agent` Deployment (1 replica, image `celerex-agent:dev`, `imagePullPolicy: IfNotPresent`) + `Service` type **NodePort** (`30001→3001`). Env from the Secret (`DATABASE_URL`, `CELEREX_CRED_KEY`, `CELEREX_JWT_SECRET`). Readiness/liveness probe: HTTP GET on `:3001` (use an existing always-200 route — e.g. `GET /api/providers`; the plan confirms a cheap unauthenticated endpoint, else adds a `/healthz`).
- `secret.example.yaml` — a **template** (placeholder values) documenting the keys. The real Secret is generated locally by the apply script and **never committed** (`.gitignore` the generated file).
- `kustomization.yaml` — ties the base together.

### 4.4 Apply + lifecycle (`deploy/k8s/README.md` + `deploy/k8s/up.sh`/`down.sh`)
- `up.sh`: generate the real Secret (random `CELEREX_CRED_KEY`/`CELEREX_JWT_SECRET`, DB creds) if absent → `build-image.sh` → `kubectl apply -k deploy/k8s/base` → wait for rollout.
- `down.sh`: `kubectl delete -k` (optionally keep the PVC).
- README documents the daemon pairing target (`http://localhost:30001`) + how to set the Design app's Agent URL.

## 5. Data flow

```
local Design daemon ──(pair/connect, http://localhost:30001)──▶ Service(NodePort 30001)
   ▶ celerex-agent pod (:3001, migrates on boot) ──▶ celerex-postgres pod (:5432, PVC)
Agent scheduler fires Rituals in-pod; daemon reads /api/rituals back through the NodePort.
```

## 6. Portability to cloud K8s (later, out of scope here)
Base manifests are kustomize-clean. Future `overlays/aws` + `overlays/azure`: swap in-cluster Postgres for managed (RDS / Azure DB) by repointing the `DATABASE_URL` Secret; swap NodePort for Ingress/LoadBalancer; push the image to a registry (ECR/ACR) and flip `imagePullPolicy`. No app code change — the app is already env-driven.

## 7. Error handling / operational

- **Missing required env:** the pod crash-loops if `CELEREX_CRED_KEY` is absent — the Secret must carry it; `up.sh` generates it. Surfaced via pod logs + the readiness probe.
- **DB not ready:** the Agent connects on boot; rely on `restartPolicy: Always` + the Postgres readiness gate (Agent pod may restart a few times until Postgres is up — acceptable for v1; an initContainer `wait-for-postgres` is optional polish).
- **Image not found:** `imagePullPolicy: IfNotPresent` + a locally-built `celerex-agent:dev` tag; `up.sh` builds before apply.
- **Secret hygiene:** real Secret never committed; only `secret.example.yaml` template is in git.

## 8. Testing / verification

- `kubectl apply -k deploy/k8s/base --dry-run=client` (and `--dry-run=server`) validates manifests.
- After `up.sh`: assert both pods reach `Ready`; `curl http://localhost:30001/api/providers` returns 200.
- **2f gate:** point the local daemon at `http://localhost:30001`, pair, seed a local routine, run the connect→migration path, and confirm via `GET /api/rituals` that the routine + "Daily briefing" Rituals were created on the real in-cluster Agent. This is the live proof 2f PR2 was waiting on.

## 9. Out of scope

- Cloud overlays (AWS/Azure) — designed for, built later.
- Containerizing the Design daemon/web — stays local (compute edge).
- CI image publishing (GitHub Actions disabled), Helm (kustomize suffices), TLS/secret-manager hardening, HA/replicas, autoscaling.
- Bringing the rest of the Galvo suite into K8s — this establishes the pattern; siblings follow later.

## 10. Build order (for the plan)

1. Dockerfile + build-image.sh (+ .dockerignore) → produce `celerex-agent:dev` locally; verify the binary runs.
2. K8s base manifests (namespace, postgres, agent, secret template, kustomization) → `--dry-run` clean.
3. up.sh/down.sh + README → `up.sh` brings both pods to Ready; `/api/providers` 200.
4. Live 2f verification (pair daemon → migrate → `GET /api/rituals`).

Each step its own PR/commit where it makes sense (per the standing "PR everything" rule; Actions stays disabled).
