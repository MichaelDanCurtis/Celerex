# Celerex Execution Agent System Design

**Date:** 2026-03-06
**Status:** Approved

## Overview

A distributed job execution system for Celerex consisting of three components: a central orchestrator service, lightweight agent binaries deployed on execution hosts, and API integration for the frontend. Agents connect via WebSocket, advertise capabilities (Docker, KVM, Kubernetes), and receive dispatched jobs. The orchestrator handles scheduling, fault tolerance, and mTLS-based security.

## Architecture

### Components

| Crate | Role | Port |
|---|---|---|
| `celerex-api` (existing) | REST API, frontend-facing, proxies to orchestrator | 3001 |
| `celerex-orchestrator` | Job dispatch, agent management, WebSocket hub | 3002 |
| `celerex-agent` (new) | Lightweight binary on execution hosts | None (outbound only) |
| `celerex-common` (new) | Shared types and message protocol | N/A (library) |

### Communication Flow

```
Frontend --HTTP--> celerex-api --HTTP--> celerex-orchestrator
                                              |
                                    WebSocket connections
                                     /        |        \
                                Agent A    Agent B    Agent C
                              (Docker)   (KVM+Docker) (K8s)
```

- Frontend to API: Existing REST endpoints plus new execution/agent routes.
- API to Orchestrator: Internal HTTP. Localhost in dev, K8s service DNS in production (`celerex-orchestrator.celerex.svc.cluster.local:3002`).
- Orchestrator to Agents: Persistent bidirectional WebSocket connections.

### Kubernetes Deployment

- `celerex-api`: Stateless Deployment, HPA on HTTP request load.
- `celerex-orchestrator`: StatefulSet for sticky WebSocket connections, HPA on active agent/job count.
- `celerex-agent`: DaemonSet or standalone Deployment on execution nodes.
- Network policy restricts orchestrator internal API to API pods only.

## Security Model

### Three channels, three security levels

| Channel | Auth Method | Rationale |
|---|---|---|
| Frontend to API | Bearer token / session | Standard user auth |
| API to Orchestrator | Internal network + `X-Internal-Token` header | Co-deployed, K8s network policy restricts access |
| Orchestrator to Agents | mTLS after approval | Agents run on remote/untrusted hosts, need mutual identity |

### Agent Registration and mTLS Flow

1. **Bootstrap**: Agent starts with a one-time registration token (generated via UI or CLI). Connects to orchestrator over server-verified TLS, presents the token.
2. **Pending state**: Orchestrator records the agent (hostname, labels, capabilities), marks it "pending" in the UI. No jobs dispatched.
3. **Admin approval**: Admin approves in the Celerex UI. Orchestrator generates a client certificate signed by a Celerex-internal CA, sends it to the agent over the existing connection.
4. **Reconnect with mTLS**: Agent stores the certificate on disk, reconnects using mTLS. Orchestrator only accepts connections from agents with valid client certs going forward.
5. **Certificate rotation**: Certs have a 90-day TTL. Orchestrator issues a new cert before expiry over the existing mTLS connection. Expired certs force the agent back to "pending" for re-approval.
6. **Revocation**: Admin revokes from UI. Orchestrator adds cert serial to deny list, drops the WebSocket.

## Agent Binary

### Configuration

```toml
# agent.toml
orchestrator_url = "wss://orchestrator.celerex.io:3002/ws/agent"
registration_token = "abc123-one-time-use"

[labels]
region = "us-west-2"
gpu = "true"
gpu_type = "a100"
environment = "production"

[capabilities]
docker = true
kvm = false
kubernetes = false
```

Capabilities are also auto-detected on startup (Docker socket, libvirt, kubeconfig).

### Heartbeat Protocol

Every 10 seconds, agent sends:
- Active job count
- CPU / memory utilization (percentage)
- Available disk space
- Runtime health (Docker daemon responsive, etc.)

Three missed heartbeats = agent marked offline, active jobs reassigned.

## Job Lifecycle

### States

```
QUEUED -> DISPATCHING -> RUNNING -> STREAMING -> COMPLETED
                                              -> FAILED -> RETRYING (up to 5x)
                                              -> NO_AGENT (alert user)
```

### Dispatch Algorithm

1. Job arrives with execution config and required labels.
2. **Filter**: Find all online, approved agents whose labels match requirements.
3. **No match**: Set `NO_AGENT`, alert user immediately.
4. **Rank**: Among matching agents, select lowest load score: `active_jobs * 0.4 + cpu_pct * 0.3 + mem_pct * 0.3`.
5. **Dispatch**: Send job payload over WebSocket.
6. **Timeout**: If no ACK within 10 seconds, try next-best agent.

### Output Streaming (Dual Mode)

- **Short jobs** (< 30s or config flag): Agent buffers output, sends final result as single message.
- **Long jobs** (> 30s or streaming requested): Agent streams stdout/stderr chunks over WebSocket in real-time. Orchestrator forwards to API via SSE.

### Retry on Disconnect

1. Agent disconnects mid-job: orchestrator waits 5 seconds for reconnect.
2. No reconnect: mark as failed attempt, increment retry counter.
3. Retries < 5: re-dispatch to another matching agent.
4. Retries = 5: mark `FAILED`, alert user.
5. No other matching agent: mark `FAILED`, alert with "no available agents".

### Job Result

```rust
struct JobResult {
    job_id: Uuid,
    status: JobStatus,       // Completed | Failed
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    duration_ms: u64,
    agent_id: Uuid,
    attempt: u8,
}
```

## Crate Structure

```
packages/backend/
  api/                    # celerex-api (existing, port 3001)
  orchestrator/           # celerex-orchestrator (port 3002)
  agent/                  # celerex-agent (new, no port)
  common/                 # celerex-common (shared types, library)
```

### Shared Types (celerex-common)

- Job types: `JobRequest`, `JobResult`, `JobStatus`
- Agent types: `AgentInfo`, `AgentCapabilities`, `AgentStatus`, `Heartbeat`
- Execution config types (mirrors frontend): MCP, HTTP, Sandbox targets, etc.
- WebSocket protocol:
  - `AgentMessage`: `Register`, `Heartbeat`, `JobAck`, `JobOutput`, `JobComplete`, `JobFailed`
  - `OrchestratorMessage`: `Approved`, `CertIssued`, `DispatchJob`, `CancelJob`, `CertRotate`

### Key Dependencies

| Crate | Key deps |
|---|---|
| `common` | serde, uuid, chrono |
| `api` | axum, tokio, reqwest, common |
| `orchestrator` | axum (WebSocket), tokio, rustls, rcgen (cert generation), common |
| `agent` | tokio-tungstenite, bollard (Docker API), sysinfo (metrics), rustls, common |

## API Surface

### celerex-api (public, port 3001)

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/agents` | List all registered agents |
| `GET` | `/api/agents/{id}` | Agent detail |
| `POST` | `/api/agents/{id}/approve` | Approve pending agent |
| `POST` | `/api/agents/{id}/revoke` | Revoke agent |
| `DELETE` | `/api/agents/{id}` | Remove agent |
| `POST` | `/api/execute` | Submit a job |
| `GET` | `/api/jobs` | List jobs (filterable) |
| `GET` | `/api/jobs/{id}` | Job detail |
| `GET` | `/api/jobs/{id}/stream` | SSE stream of job output |
| `POST` | `/api/jobs/{id}/cancel` | Cancel a running job |
| `POST` | `/api/agents/register-token` | Generate registration token |

### celerex-orchestrator (internal, port 3002)

| Method | Path | Description |
|---|---|---|
| `GET` | `/agents` | Agent list |
| `POST` | `/agents/{id}/approve` | Approve + issue cert |
| `POST` | `/agents/{id}/revoke` | Revoke agent |
| `POST` | `/jobs` | Submit job |
| `GET` | `/jobs/{id}` | Job status |
| `GET` | `/jobs/{id}/stream` | SSE forward from agent |
| `POST` | `/jobs/{id}/cancel` | Cancel job |
| `POST` | `/register-token` | Generate registration token |
| `GET` | `/ws/agent` | WebSocket for agent connections |

The API is a thin authenticated proxy: validates user auth, forwards to orchestrator with internal token.
