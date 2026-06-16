#!/usr/bin/env bash
# Tear down the celerex deployment. Pass --purge to also delete the PVC + Secret.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
kubectl delete -k "$HERE/base" --ignore-not-found
if [[ "${1:-}" == "--purge" ]]; then
  kubectl -n celerex delete pvc celerex-postgres --ignore-not-found
  kubectl -n celerex delete secret celerex-secrets --ignore-not-found
fi
echo "Down."
