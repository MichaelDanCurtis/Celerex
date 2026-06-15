# Celerex Agent on Kubernetes (local)

Runs the Celerex **Agent** (`celerex-api`) + Postgres in the `celerex` namespace on
Docker Desktop's Kubernetes. The Design daemon stays local and pairs out to the
in-cluster Agent. Cloud-portable: the base manifests are kustomize-clean; cloud
overlays (AWS/Azure) swap the in-cluster Postgres for a managed DB via the
`DATABASE_URL` Secret and NodePort for an Ingress.

## Prerequisites
- Docker Desktop with **Kubernetes enabled** (`kubectl config get-contexts` shows `docker-desktop`).
- The sibling `lucidity/Galvo` repo checked out next to `Celerex` (the Agent path-depends on `galvo-federation`).

## Start
```bash
./deploy/k8s/up.sh
```
This generates the `celerex-secrets` Secret (random values — never committed),
builds the `celerex-agent:dev` image from a staged build context, applies the
manifests, and waits for both rollouts.

## Verify
```bash
kubectl -n celerex get pods                      # both 1/1 Running
curl -s http://localhost:30001/api/providers     # HTTP 200 JSON
```

## Pair the Design app
Point the Design app's Agent/Celerex base URL at **`http://localhost:30001`**
(via the connect flow / app-config `celerex.serverUrl`). On connect, the daemon's
best-effort migration creates Agent Rituals from any local routines + Orbit.

## Logs / troubleshooting
```bash
kubectl -n celerex logs deploy/celerex-agent      # agent logs
kubectl -n celerex logs deploy/celerex-postgres   # db logs
kubectl -n celerex describe pod -l app=celerex-agent
```
If the agent crash-loops: usually a missing Secret key (`CELEREX_CRED_KEY`) or
Postgres not yet Ready (it recovers on restart once the DB is up).

## Stop
```bash
./deploy/k8s/down.sh           # delete workloads (keeps the PVC + Secret)
./deploy/k8s/down.sh --purge   # also delete the PVC + Secret
```
