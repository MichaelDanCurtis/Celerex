//! Celerex Design LLM gateway: an Anthropic/OpenAI-compatible reverse proxy.
//! Authenticated by a `design.device` token; injects the engagement's decrypted
//! provider key server-side so keys never leave Celerex. Upstream targets are a
//! fixed allowlist (no caller-supplied target -> no SSRF).

use axum::{
    body::{Body, Bytes},
    extract::{Path, RawQuery, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::IntoResponse,
};

use std::sync::OnceLock;

use crate::{AppState, auth::extractors::DeviceAuth};

/// Process-wide reqwest client: reuses connection pools/TLS across proxied
/// requests rather than rebuilding them per call.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Map a provider id to (upstream base URL, protocol). Base is overridable via
/// `DESIGN_LLM_<PROVIDER>_BASE` for tests. Returns None for unknown providers.
fn upstream_for(provider: &str) -> Option<(String, &'static str)> {
    let (default_base, protocol) = match provider {
        "anthropic" => ("https://api.anthropic.com", "anthropic"),
        "openai" => ("https://api.openai.com", "openai"),
        _ => return None,
    };
    let env_key = format!("DESIGN_LLM_{}_BASE", provider.to_uppercase());
    let base = std::env::var(&env_key).unwrap_or_else(|_| default_base.to_string());
    Some((base.trim_end_matches('/').to_string(), protocol))
}

/// `ANY /api/design/llm/{provider}/{*rest}` — proxy to the provider, injecting the key.
pub async fn proxy(
    DeviceAuth(claims): DeviceAuth,
    State(state): State<AppState>,
    Path((provider, rest)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (base, protocol) = upstream_for(&provider)
        .ok_or((StatusCode::NOT_FOUND, format!("unknown provider '{provider}'")))?;
    let key = state.provider_keys.get(claims.engagement_id, &provider).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or((StatusCode::CONFLICT, format!("no key configured for '{provider}'")))?;

    let mut url = format!("{base}/{rest}");
    if let Some(q) = query { url.push('?'); url.push_str(&q); }

    let mut req = http_client().request(method, &url).body(body);
    if let Some(ct) = headers.get(header::CONTENT_TYPE) {
        req = req.header(header::CONTENT_TYPE, ct);
    }
    req = match protocol {
        "anthropic" => req.header("x-api-key", &key).header("anthropic-version", "2023-06-01"),
        "openai" => req.header(header::AUTHORIZATION, format!("Bearer {key}")),
        _ => unreachable!("protocol must match a variant returned by upstream_for"),
    };

    let upstream = req.send().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")))?;
    let status = StatusCode::from_u16(upstream.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let ct = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let stream = upstream.bytes_stream();
    let mut resp = (status, Body::from_stream(stream)).into_response();
    if let Some(ct) = ct { resp.headers_mut().insert(header::CONTENT_TYPE, ct); }
    Ok(resp)
}
