use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderName};
use axum::response::Html;
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::auth::extractors::AuthClaims;
use crate::runs::generation::artifact_store::get_artifact;

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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
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

/// `GET /api/live-artifacts/{artifact_id}/preview?projectId=...&variant=rendered`
/// — the rendered HTML the web iframes. `variant=template` returns the raw
/// author template; anything else returns the rendered preview. Engagement-scoped.
/// The response replicates `setLiveArtifactPreviewHeaders` verbatim; this serve
/// does NOT re-run the Task 3 guard, so the CSP is its enforcement boundary
/// against any guard-gap payload that reached the row.
pub async fn get_artifact_preview(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(q): Query<ArtifactQuery>,
) -> Result<(SecurityHeaders, Html<String>), (StatusCode, String)> {
    let row = get_artifact(
        &state.pool,
        claims.engagement_id,
        &q.project_id,
        &artifact_id,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
    .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;
    let html = match q.variant.as_deref() {
        Some("template") => row.template_html,
        _ => row.rendered_html,
    };
    Ok((live_artifact_security_headers(), Html(html)))
}
