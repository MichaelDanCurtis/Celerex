use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::{self, AUTHORIZATION, HeaderName};
use axum::http::request::Parts;
use axum::response::Html;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::auth::extractors::AuthClaims;
use crate::runs::generation::artifact_store::{
    get_artifact, get_artifact_by_project, list_artifacts,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactQuery {
    pub project_id: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// The four non-`Content-Type` security headers emitted by the daemon's
/// `setLiveArtifactPreviewHeaders` (`celerex-design/apps/daemon/src/server.ts:3610-3630`),
/// applied verbatim to BOTH live-artifact routes. `Content-Type` is intentionally
/// omitted so each handler's body type owns it (`Html` → `text/html; charset=utf-8`,
/// matching `server.ts:3611`; `Json` → `application/json`).
pub type SecurityHeaders = [(HeaderName, &'static str); 4];

/// Directive-for-directive copy of the daemon CSP (`server.ts:3615-3630`), in the
/// same order. `script-src 'none'` + the script-less `sandbox` directive are the
/// load-bearing controls that make the `html_template_v1` guard's known denylist
/// gaps (`<svg/onload=...>`, SMIL `to="javascript:..."`, external resource refs)
/// inert on serve — see Global Constraints + infographic spec §5.2/§5.4.1.
const LIVE_ARTIFACT_CSP: &str = "default-src 'none'; base-uri 'none'; script-src 'none'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'unsafe-inline'; sandbox allow-same-origin";

/// Build the CSP-parity header prefix for a live-artifact response. Mirrors
/// `setLiveArtifactPreviewHeaders` (`server.ts:3612-3630`) minus `Content-Type`.
fn live_artifact_security_headers() -> SecurityHeaders {
    [
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CONTENT_SECURITY_POLICY, LIVE_ARTIFACT_CSP),
    ]
}

/// `GET /api/live-artifacts?projectId=...` — every artifact for one project,
/// most-recently-created first. This is the route the shipped web SPA
/// actually calls to populate the workspace tab: `ProjectView.tsx:3284-3289`
/// -> `refreshLiveArtifacts()` -> `fetchLiveArtifacts(project.id)` ->
/// `registry.ts:1454-1462`, which parses `{ artifacts: [...] }` (falling back
/// to a legacy `{ liveArtifacts: [...] }` key neither server actually sends).
/// `FileWorkspace.tsx:548-550` derives its ENTIRE live-artifact tab list from
/// this array — the `live_artifact` SSE payload alone never opens the tab.
/// Daemon parity: `celerex-design/apps/daemon/src/server.ts:9166`
/// (`res.json({ artifacts })`). Engagement-scoped and Bearer-gated like the
/// meta/detail route — this is fetched with `apiFetch` (Bearer attached), not
/// an iframe, so it needs none of `get_artifact_preview`'s anonymous-embed
/// carve-out below.
pub async fn list_live_artifacts(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Query(q): Query<ArtifactQuery>,
) -> Result<(SecurityHeaders, Json<serde_json::Value>), (StatusCode, String)> {
    let rows = list_artifacts(&state.pool, claims.engagement_id, &q.project_id)
        .await
        .map_err(|e| {
            // Don't leak sqlx/postgres internals (table/column/constraint
            // names) into a client-readable body — same convention as
            // `design_chat.rs:219-227` / `generation/mod.rs:133-160`: full
            // detail to `tracing::warn!`, a fixed generic message on the wire.
            tracing::warn!(
                engagement = %claims.engagement_id,
                project_id = %q.project_id,
                error = %e,
                "live-artifacts list: store error"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to list artifacts".to_string(),
            )
        })?;
    let artifacts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            let created = row.created_at.to_rfc3339();
            json!({
                "schemaVersion": 1,
                "id": row.artifact_id,
                "projectId": q.project_id,
                "title": row.title,
                "slug": row.slug,
                "status": "active",
                "pinned": false,
                "preview": { "type": "html", "entry": "index.html" },
                "refreshStatus": "idle",
                "createdAt": created,
                "updatedAt": created,
                "hasDocument": true,
            })
        })
        .collect();
    Ok((
        live_artifact_security_headers(),
        Json(json!({ "artifacts": artifacts })),
    ))
}

/// `GET /api/live-artifacts/{artifact_id}?projectId=...` — artifact metadata
/// the web FileViewer fetches on a `live_artifact` SSE event. Engagement-scoped.
/// Carries the CSP-parity headers (defense-in-depth; `Content-Type` stays
/// `application/json`).
pub async fn get_artifact_meta(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<ArtifactQuery>,
) -> Result<(SecurityHeaders, Json<serde_json::Value>), (StatusCode, String)> {
    let row = get_artifact(
        &state.pool,
        claims.engagement_id,
        &q.project_id,
        &artifact_id,
    )
    .await
    .map_err(|e| {
        // Don't leak sqlx/postgres internals (table/column/constraint names)
        // into a client-readable body — same convention as
        // `design_chat.rs:219-227` / `generation/mod.rs:133-160`: full detail
        // to `tracing::warn!`, a fixed generic message on the wire.
        tracing::warn!(
            engagement = %claims.engagement_id,
            project_id = %q.project_id,
            artifact_id = %artifact_id,
            error = %e,
            "live-artifact meta: store error"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to load artifact".to_string(),
        )
    })?
    .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    Ok((
        live_artifact_security_headers(),
        Json(json!({
            "artifact": {
                "id": row.artifact_id,
                "projectId": q.project_id,
                "title": row.title,
                "refreshStatus": "idle",
                "preview": { "type": "html", "entry": "index.html" },
            }
        })),
    ))
}

/// Resolves who is allowed to load `/preview` credential-lessly. See
/// `get_artifact_preview`'s doc comment for the complete rationale — this
/// extractor only decides WHICH of the two accepted callers (Bearer session,
/// or anonymous same-origin embed) the incoming request is; the anonymous
/// branch still isn't trusted until the handler body also checks Fetch
/// Metadata headers via `is_same_origin_iframe_embed`.
///
/// `pub`, not private/`pub(crate)`: both `lib.rs`'s route registration AND
/// the integration tests' OWN hand-built router
/// (`tests/common/mod.rs::build_app_for_generation_tests`, a separate crate)
/// call `get(...get_artifact_preview)` — axum's `Handler` bound resolution at
/// EACH of those call sites needs to be able to name every argument type in
/// the handler's signature, including this one.
pub enum PreviewAuth {
    /// A verified Bearer session — identical to what `AuthClaims` would
    /// produce. Carries just the one field the preview lookup needs.
    Session(Uuid),
    /// No `Authorization` header was present at all. NOT yet authorized — the
    /// handler still gates this on Fetch Metadata headers before doing
    /// anything with it.
    Anonymous,
}

impl<S: Send + Sync> FromRequestParts<S> for PreviewAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // A request presenting NO Authorization header at all is exactly the
        // shape a browser produces for `<iframe src>` (it cannot attach
        // custom headers to that navigation) — treat it as a candidate for
        // the anonymous path rather than rejecting outright.
        if parts.headers.get(AUTHORIZATION).is_none() {
            return Ok(PreviewAuth::Anonymous);
        }
        // A header WAS presented: hold it to the exact same bar as every
        // other Bearer-gated route (full `AuthClaims` verification, same
        // rejection). A malformed/expired token still 401s here — it does
        // NOT fall back to the anonymous path just because it failed.
        let AuthClaims(claims) = AuthClaims::from_request_parts(parts, state).await?;
        Ok(PreviewAuth::Session(claims.engagement_id))
    }
}

/// True only for a request whose Fetch Metadata headers affirmatively prove
/// it is a same-origin iframe subresource load — the Agent serves the SPA
/// itself (`lib.rs`'s `web_service`), so a genuine `<iframe src="/api/...">`
/// embed is always same-origin. Fails closed on anything else: missing
/// headers (browsers old enough to predate Fetch Metadata support), a
/// cross-site or same-site origin, or a destination other than `iframe` (a
/// bare `fetch()`/XHR with no Bearer attached does not qualify — only the
/// literal browser-navigation shape this route exists to unblock does).
fn is_same_origin_iframe_embed(headers: &HeaderMap) -> bool {
    let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    let dest = headers.get("sec-fetch-dest").and_then(|v| v.to_str().ok());
    site == Some("same-origin") && dest == Some("iframe")
}

/// `GET /api/live-artifacts/{artifact_id}/preview?projectId=...&variant=rendered`
/// — the rendered HTML the web iframes. `variant=template` returns the raw
/// author template; anything else returns the rendered preview. The response
/// replicates `setLiveArtifactPreviewHeaders` verbatim; this serve does NOT
/// re-run the Task 3 guard, so the CSP is its enforcement boundary against
/// any guard-gap payload that reached the row.
///
/// ## Auth: this route must be loadable from a bare `<iframe src>`
///
/// The shipped SPA renders the returned URL as
/// `<iframe sandbox="allow-scripts allow-popups allow-downloads"
/// src={previewUrl}>` (`FileViewer.tsx:1551-1557`) — a browser subresource
/// navigation, which by HTML spec CANNOT carry a custom `Authorization`
/// header (the same limitation `<img src>` has — see the `bearer-auth-img-
/// src-blob-url-workaround` family of bugs). Gating this route
/// unconditionally behind a Bearer session — as the sibling meta/list routes
/// correctly do — makes the preview 401 inside every frame. Daemon parity:
/// the daemon deliberately carves this exact path out of its own equivalent
/// gate (`celerex-design/apps/daemon/src/server.ts:5018-5021`: "Live artifact
/// previews have stricter local-daemon validation ... on the route itself").
///
/// Two distinct callers are accepted:
///   1. **Bearer-authenticated** (e.g. the code panel's
///      `fetchLiveArtifactCode`, a plain `apiFetch` JS call) — scoped by
///      `claims.engagement_id`, byte-for-byte the same as before this change.
///   2. **Anonymous same-origin iframe embed** — no `Authorization` header at
///      all, accepted ONLY when `is_same_origin_iframe_embed` confirms
///      `Sec-Fetch-Site: same-origin` + `Sec-Fetch-Dest: iframe`. Every other
///      shape (cross-site, same-site, missing metadata, non-iframe dest) is
///      rejected with 401 — this deliberately does NOT become an open
///      internet endpoint.
///
/// Branch 2 has no claims to scope by, so it looks the artifact up by
/// `(project_id, artifact_id)` alone (`get_artifact_by_project`), leaning on
/// `artifact_id` being unguessable (`la-` + 12 hex from `generate_artifact_id`
/// — 48 bits of random suffix) as the operative access control instead of
/// engagement membership.
///
/// **Security posture, stated plainly**: on this single-user product (the
/// product owner's explicit directive; broader multi-tenant hardening is
/// tracked separately), anyone able to issue a same-origin iframe-shaped
/// request carrying one specific artifact id can read that one artifact's
/// rendered HTML without a session — there is no engagement check and no
/// revocation on this path. The CSP this handler still applies
/// (`frame-ancestors 'self'`, `script-src 'none'`, `sandbox
/// allow-same-origin`, ...) constrains what the RETURNED document can do, not
/// who can request it. A missing/unknown artifact id still 404s on both
/// branches — this route is not a broader existence oracle than the
/// unguessable id already is.
pub async fn get_artifact_preview(
    auth: PreviewAuth,
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<ArtifactQuery>,
    headers: HeaderMap,
) -> Result<(SecurityHeaders, Html<String>), (StatusCode, String)> {
    let row = match auth {
        PreviewAuth::Session(engagement_id) => {
            get_artifact(&state.pool, engagement_id, &q.project_id, &artifact_id)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        engagement = %engagement_id,
                        project_id = %q.project_id,
                        artifact_id = %artifact_id,
                        error = %e,
                        "live-artifact preview (session): store error"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "failed to load artifact".to_string(),
                    )
                })?
        }
        PreviewAuth::Anonymous => {
            if !is_same_origin_iframe_embed(&headers) {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "missing authorization".to_string(),
                ));
            }
            get_artifact_by_project(&state.pool, &q.project_id, &artifact_id)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        project_id = %q.project_id,
                        artifact_id = %artifact_id,
                        error = %e,
                        "live-artifact preview (anonymous): store error"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "failed to load artifact".to_string(),
                    )
                })?
        }
    }
    .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    let html = match q.variant.as_deref() {
        Some("template") => row.template_html,
        _ => row.rendered_html,
    };
    Ok((live_artifact_security_headers(), Html(html)))
}
