#!/usr/bin/env bash
# Build the celerex-agent image from a staged context that includes the Celerex
# workspace + the sibling-repo galvo-federation crate (the only cross-repo path dep).
set -euo pipefail

IMAGE="${IMAGE:-celerex-agent:dev}"
# Repo roots (this script lives in Celerex/deploy/docker).
CELEREX_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV_ROOT="$(cd "$CELEREX_ROOT/.." && pwd)"          # .../Development
# celerex-api path-depends on galvo-federation, which is a MEMBER of the Galvo
# engine workspace (lucidity/Galvo/engine) and inherits version/edition + pulls
# sibling crates (galvo-core, galvo-a2a) via workspace deps. So the whole engine
# workspace must be staged for the path dep to resolve, not just the one crate.
GALVO_ENGINE="$DEV_ROOT/lucidity/Galvo/engine"

if [[ ! -d "$GALVO_ENGINE/galvo-federation" ]]; then
  echo "ERROR: galvo engine workspace not found at $GALVO_ENGINE" >&2
  echo "The Agent build needs the sibling lucidity/Galvo repo checked out." >&2
  exit 1
fi

STAGE="$(mktemp -d -t celerex-build-XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

# rsync excludes — never drag build artifacts/vcs into the staged context (the
# Galvo engine target/ alone is ~13G).
EXCL=(--exclude=target --exclude=.git --exclude=node_modules)

# Stage the Celerex workspace (manifests + lockfile + both member crates).
mkdir -p "$STAGE/Celerex/packages/backend"
cp "$CELEREX_ROOT/Cargo.toml" "$STAGE/Celerex/Cargo.toml"
cp "$CELEREX_ROOT/Cargo.lock" "$STAGE/Celerex/Cargo.lock"
rsync -a "${EXCL[@]}" "$CELEREX_ROOT/packages/backend/api/" "$STAGE/Celerex/packages/backend/api/"
rsync -a "${EXCL[@]}" "$CELEREX_ROOT/packages/backend/agent-orchestrator/" "$STAGE/Celerex/packages/backend/agent-orchestrator/"

# Stage the whole Galvo engine workspace (minus target), preserving its relative
# location so the galvo-federation path dep + its sibling members resolve.
mkdir -p "$STAGE/lucidity/Galvo/engine"
rsync -a "${EXCL[@]}" "$GALVO_ENGINE/" "$STAGE/lucidity/Galvo/engine/"

# Lean context.
cp "$CELEREX_ROOT/deploy/docker/.dockerignore" "$STAGE/.dockerignore"

# One service: stage the built OmniVyx web app so the Agent image serves the SPA
# itself (CELEREX_WEB_DIR). Built locally beforehand via `bun run build`.
WEB_DIST="$CELEREX_ROOT/packages/frontend/app/dist"
mkdir -p "$STAGE/web-dist"   # always present so the Dockerfile COPY never fails
if [[ -d "$WEB_DIST" ]]; then
  rsync -a "$WEB_DIST/" "$STAGE/web-dist/"
  echo "Staged web app from $WEB_DIST"
else
  echo "WARNING: $WEB_DIST not found — image will serve API only (run 'bun run build' in packages/frontend/app first)." >&2
fi

echo "Building $IMAGE from staged context $STAGE ..."
docker build -f "$CELEREX_ROOT/deploy/docker/agent.Dockerfile" -t "$IMAGE" "$STAGE"
echo "Built $IMAGE"
