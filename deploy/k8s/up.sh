#!/usr/bin/env bash
# Bring up the celerex Agent + Postgres on the local Docker Desktop cluster.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CELEREX_ROOT="$(cd "$HERE/../.." && pwd)"

kubectl config use-context docker-desktop >/dev/null
kubectl apply -f "$HERE/base/namespace.yaml"

# Create the Secret once (idempotent: skip if present). Real values never committed.
if ! kubectl -n celerex get secret celerex-secrets >/dev/null 2>&1; then
  PGPW="$(openssl rand -hex 16)"
  CREDKEY="$(openssl rand -hex 32)"
  JWT="$(openssl rand -hex 32)"
  kubectl -n celerex create secret generic celerex-secrets \
    --from-literal=POSTGRES_PASSWORD="$PGPW" \
    --from-literal=DATABASE_URL="postgres://celerex:${PGPW}@celerex-postgres:5432/celerex" \
    --from-literal=CELEREX_CRED_KEY="$CREDKEY" \
    --from-literal=CELEREX_JWT_SECRET="$JWT"
  echo "Created celerex-secrets."
else
  echo "celerex-secrets already exists; leaving as-is."
fi

# Build image into the Docker Desktop daemon (shared with the cluster).
"$CELEREX_ROOT/deploy/docker/build-image.sh"

kubectl apply -k "$HERE/base"
echo "Waiting for rollouts..."
kubectl -n celerex rollout status deploy/celerex-postgres --timeout=120s
kubectl -n celerex rollout status deploy/celerex-agent --timeout=180s
echo "Agent reachable at http://localhost:30001 (e.g. GET /api/providers)."
