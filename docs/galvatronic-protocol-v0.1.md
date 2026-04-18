# Galvatronic A2A Protocol — v0.1

## Overview

Galvatronic is the agent-to-agent (A2A) wire format used by Celerex agents to
delegate work to peer products in the AiLuxi ecosystem — Galvo first, but any
peer that implements this endpoint is reachable through the same path.

Galvo is **not** special. It is reached through the standard plugin surface:
a plugin manifest declares an outbound capability with `execution_type: "agent"`,
and the orchestrator's `AgentHandler` invokes an `A2AClient` that speaks this
protocol. Swapping from a fixture to live Galvo is a config change (plugin
install record), not a code change.

## Transport

- **Protocol:** HTTP/1.1 or HTTP/2 over TLS.
- **Content type:** `application/json; charset=utf-8`.
- **Endpoint:** one per peer, chosen by the peer:
  `POST {peer_base_url}/a2a/invoke`.
- **Header:** `Galvatronic-Version: 0.1` — required on every request and
  response. Peers MUST reject requests missing or declaring an incompatible
  version with HTTP 400 and a structured error body.
- **Idempotency:** callers SHOULD send the same `request_id` on retries. Peers
  MAY use `request_id` for idempotency; behavior is peer-defined in v0.1.

## Request shape

```json
{
  "target": "<peer>.<capability>",
  "params": { "...": "..." },
  "request_id": "<uuid-v4>",
  "auth": {
    "scheme": "bearer",
    "token": "<jwt>"
  }
}
```

| Field        | Type   | Required | Notes                                                    |
|--------------|--------|----------|----------------------------------------------------------|
| `target`     | string | yes      | Dotted path: `<peer_id>.<capability>` (e.g. `galvo.research.weekly`). |
| `params`     | object | yes      | Free-form JSON — peers publish a JSON Schema per capability. |
| `request_id` | string | yes      | UUID v4. Returned verbatim on the response.              |
| `auth`       | object | yes      | Bearer token issued per peer pair. See **Auth** below.   |

## Response shape

```json
{
  "request_id": "<uuid>",
  "status": "success",
  "result": "...markdown or object..."
}
```

or, on error:

```json
{
  "request_id": "<uuid>",
  "status": "error",
  "error": {
    "code": "<string>",
    "message": "<string>"
  }
}
```

| Field        | Type          | When                                                     |
|--------------|---------------|----------------------------------------------------------|
| `request_id` | string        | always — echoes the request                              |
| `status`     | enum          | `"success"` or `"error"`                                 |
| `result`     | string\|object | present iff `status == "success"`. Markdown by default; peers may return structured JSON when documented per capability. |
| `error`      | object        | present iff `status == "error"`. See codes below.        |

### Error codes (v0.1)

| Code              | HTTP | Meaning                                            |
|-------------------|------|----------------------------------------------------|
| `unauthorized`    | 401  | Token missing, invalid, or expired.                |
| `forbidden`       | 403  | Token valid but not authorized for `target`.       |
| `unknown_target`  | 404  | Peer does not expose the requested capability.     |
| `invalid_params`  | 400  | `params` failed the capability's JSON Schema.      |
| `upstream_error`  | 502  | Peer's own downstream dependency failed.           |
| `internal`        | 500  | Unclassified peer error.                           |

Peers MUST include `code` and SHOULD include a human-readable `message`.
Peers MAY include additional fields (e.g. `trace_id`, `retry_after_ms`).

## Auth

v0.1 uses bearer tokens issued per peer pair. Each pair shares a rotation
schedule out of band; no discovery protocol is defined.

- Tokens are JWTs. Required claims: `iss` (issuer peer id), `aud` (audience
  peer id), `exp` (expiry, max 1 hour), `sub` (caller identity within issuer).
- Tokens are signed with Ed25519. Peers publish their public key at
  `GET {peer_base_url}/a2a/keys` (`{ "ed25519": "<base64>" }`).
- Rotation: issuers SHOULD rotate keys at least every 30 days. Consumers MUST
  accept both the previous and current key during a 24-hour overlap.
- Tokens MUST NOT be reused across peer pairs.

## Streaming

Out of scope for v0.1. A future `POST /a2a/stream` with Server-Sent Events
will be specified in v0.2. Until then, peers MUST complete the response
synchronously or return `upstream_error` if the work exceeds their timeout
(peer-defined, SHOULD be ≥ 30s).

## Versioning

- **Wire version:** bumped in `Galvatronic-Version` header. Breaking changes
  require a major bump.
- **Capability version:** encoded in the plugin manifest, not in the wire
  format. Callers pin a capability version in the manifest install config.
- **Compatibility:** v0.1 is the baseline. Peers SHOULD support at least the
  two most recent minor versions.

## Reference implementations

- **Fixture (local dev + tests):** `celerex-orchestrator::handlers::agent_stub::FixtureA2AClient`
  — feature-gated behind `test-support`. Serves canned responses keyed by
  `target`. Any peer plugin can use it for integration tests.
- **Live Galvo:** the Galvo peer's implementation lives in its own repository.
  Celerex has no Galvo-specific code; the `AgentHandler` + `A2AClient` pair
  is the only integration point.

## Example

Request (Celerex → Galvo):

```json
POST https://galvo.example.com/a2a/invoke
Galvatronic-Version: 0.1
Content-Type: application/json

{
  "target": "galvo.research.weekly",
  "params": {},
  "request_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "auth": { "scheme": "bearer", "token": "eyJ..." }
}
```

Response (Galvo → Celerex):

```json
200 OK
Galvatronic-Version: 0.1
Content-Type: application/json

{
  "request_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "status": "success",
  "result": "# Weekly Research Digest\n\n- Item one\n- Item two\n"
}
```
