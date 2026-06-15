#!/usr/bin/env bash
# Forward the in-cluster Agent to localhost:30001. Use this for host access on
# multi-node local clusters (e.g. kind-style "desktop-control-plane"/"desktop-worker"),
# where a NodePort is NOT auto-mapped to localhost the way single-node Docker
# Desktop does. Leave this running while the Design daemon is paired to the Agent.
set -euo pipefail
echo "Forwarding http://localhost:30001 -> svc/celerex-agent:3001 (Ctrl-C to stop)"
exec kubectl -n celerex port-forward svc/celerex-agent 30001:3001
