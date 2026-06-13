//! Celerex Design handoff — the Celerex-side ingest of design bundles.
//!
//! `POST /api/design/ingest` receives a bundle (v0.1: the unzipped CDB as JSON;
//! the multipart `.cdb` zip wrapper lands with the desktop app), validates +
//! sanitizes it, and stores it in the Design Library (`design_templates`).
//! `GET /api/design/templates` lists the library; `GET /api/design/assets/{id}`
//! serves a stored asset. See docs/plans/2026-06-12-design-handoff-contract.md.

pub mod sanitize;
pub mod scaffold;

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{
        extractors::{AuthClaims, DesignAuth},
        jwt::{mint_scoped, SCOPE_DESIGN_DEVICE},
    },
};

/// A device credential lives 90 days; the desktop re-pairs after that.
const DESIGN_TOKEN_TTL_SECS: i64 = 90 * 24 * 60 * 60;
/// A pairing handshake must be completed within 10 minutes.
const PAIRING_TTL_MINUTES: i64 = 10;

const ALLOWED_KINDS: &[&str] = &[
    "newsletter", "slides", "web_prototype", "image", "video", "document",
];
const ALLOWED_MODES: &[&str] = &["static", "live"];
const SUPPORTED_CDB_VERSIONS: &[&str] = &["0.1"];

#[derive(Deserialize)]
pub struct IngestRequest {
    /// The CDB manifest.json.
    pub manifest: Value,
    /// The renderable template (template/index.html or .mjml).
    pub template_html: String,
    #[serde(default)]
    pub tokens_css: String,
    /// The `slots` array from slots.schema.json (live mode).
    #[serde(default)]
    pub slots: Value,
    #[serde(default)]
    pub assets: Vec<IngestAsset>,
}

#[derive(Deserialize)]
pub struct IngestAsset {
    pub path: String,
    pub content_type: String,
    /// base64-encoded bytes.
    pub base64: String,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub template_id: Uuid,
    pub version: i32,
    pub mode: String,
    pub kind: String,
    /// True if the sanitizer altered the template HTML.
    pub sanitized: bool,
    pub asset_count: usize,
    /// For `live` bundles, the auto-scaffolded delivery flow (project_member).
    /// `None` for `static` bundles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_member_id: Option<Uuid>,
}

fn manifest_str<'a>(m: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = m;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str()
}

/// `POST /api/design/ingest` — store a design bundle in the caller's engagement.
///
/// Authenticated by `DesignAuth`: either a full session token (the Celerex UI)
/// or a `design.publish`-scoped device token (the Celerex Design desktop app).
pub async fn ingest(
    DesignAuth(claims): DesignAuth,
    State(state): State<AppState>,
    Json(body): Json<IngestRequest>,
) -> Result<(StatusCode, Json<IngestResponse>), (StatusCode, String)> {
    let m = &body.manifest;

    // Validate the manifest envelope.
    let cdb_version = manifest_str(m, &["cdb_version"]).unwrap_or("");
    if !SUPPORTED_CDB_VERSIONS.contains(&cdb_version) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unsupported cdb_version '{cdb_version}' (supported: {SUPPORTED_CDB_VERSIONS:?})"),
        ));
    }
    let kind = manifest_str(m, &["kind"]).unwrap_or("document");
    if !ALLOWED_KINDS.contains(&kind) {
        return Err((StatusCode::BAD_REQUEST, format!("unknown kind '{kind}'")));
    }
    let mode = manifest_str(m, &["mode"]).unwrap_or("static");
    if !ALLOWED_MODES.contains(&mode) {
        return Err((StatusCode::BAD_REQUEST, format!("unknown mode '{mode}'")));
    }
    if mode == "live" && !body.slots.is_array() {
        return Err((
            StatusCode::BAD_REQUEST,
            "live mode requires a `slots` array".into(),
        ));
    }
    let title = manifest_str(m, &["title"]).unwrap_or("Untitled design").to_string();
    let engine = manifest_str(m, &["render", "engine"]).unwrap_or("html").to_string();
    let project_ref = manifest_str(m, &["source", "project_ref"]).map(|s| s.to_string());

    // Sanitize the template before it is ever stored/rendered.
    let (clean_html, sanitized) = sanitize::sanitize_html(&body.template_html);

    // Upsert: re-publish from the same desktop project bumps the version.
    // `flow_member_id` rides along so we can refresh an existing delivery flow
    // rather than scaffolding a duplicate on every re-publish.
    let row: (Uuid, i32, Option<Uuid>) = sqlx::query_as(
        "INSERT INTO design_templates
            (engagement_id, title, kind, mode, render_engine, template_html, tokens_css, slots, manifest, source_project_ref)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (engagement_id, source_project_ref) WHERE source_project_ref IS NOT NULL
         DO UPDATE SET title = EXCLUDED.title, kind = EXCLUDED.kind, mode = EXCLUDED.mode,
            render_engine = EXCLUDED.render_engine, template_html = EXCLUDED.template_html,
            tokens_css = EXCLUDED.tokens_css, slots = EXCLUDED.slots, manifest = EXCLUDED.manifest,
            version = design_templates.version + 1, updated_at = NOW()
         RETURNING id, version, flow_member_id",
    )
    .bind(claims.engagement_id)
    .bind(&title)
    .bind(kind)
    .bind(mode)
    .bind(&engine)
    .bind(&clean_html)
    .bind(&body.tokens_css)
    .bind(if body.slots.is_null() { Value::Array(vec![]) } else { body.slots.clone() })
    .bind(m)
    .bind(&project_ref)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (template_id, version, existing_flow) = row;

    // On re-publish, replace assets; on first publish there are none.
    sqlx::query("DELETE FROM design_assets WHERE template_id = $1")
        .bind(template_id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut asset_count = 0usize;
    for a in &body.assets {
        let bytes = STANDARD
            .decode(a.base64.as_bytes())
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("asset '{}': bad base64: {e}", a.path)))?;
        sqlx::query(
            "INSERT INTO design_assets (template_id, path, content_type, bytes) VALUES ($1,$2,$3,$4)",
        )
        .bind(template_id)
        .bind(&a.path)
        .bind(&a.content_type)
        .bind(&bytes)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        asset_count += 1;
    }

    // Live bundles auto-scaffold a delivery flow: trigger → fill → render →
    // review → deliver. Re-publish refreshes the same flow's Render node so the
    // deployed pipeline tracks the latest design.
    let flow_member_id = if mode == "live" {
        Some(
            ensure_delivery_flow(
                &state,
                claims.engagement_id,
                template_id,
                existing_flow,
                &title,
                &engine,
                &clean_html,
                &body.slots,
            )
            .await?,
        )
    } else {
        None
    };

    Ok((
        StatusCode::CREATED,
        Json(IngestResponse {
            template_id,
            version,
            mode: mode.to_string(),
            kind: kind.to_string(),
            sanitized,
            asset_count,
            flow_member_id,
        }),
    ))
}

/// Resolve the engagement's "Design Deliveries" project, creating it on first
/// use. Auto-scaffolded delivery flows are grouped here so they don't clutter
/// the user's own projects.
async fn design_deliveries_project(
    state: &AppState,
    engagement_id: Uuid,
) -> Result<Uuid, (StatusCode, String)> {
    const NAME: &str = "Design Deliveries";
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects WHERE engagement_id = $1 AND name = $2 ORDER BY created_at LIMIT 1",
    )
    .bind(engagement_id)
    .bind(NAME)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        return Ok(id);
    }
    crate::db::create_project(&state.pool, engagement_id, NAME)
        .await
        .map(|p| p.id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Create (or, on re-publish, refresh) the live template's delivery flow.
#[allow(clippy::too_many_arguments)]
async fn ensure_delivery_flow(
    state: &AppState,
    engagement_id: Uuid,
    template_id: Uuid,
    existing_flow: Option<Uuid>,
    title: &str,
    engine: &str,
    template_html: &str,
    slots: &Value,
) -> Result<Uuid, (StatusCode, String)> {
    let graph = scaffold::scaffold_live_flow(title, engine, template_html, slots);
    let name = format!("{title} — delivery");
    let provider_config = Value::Object(Default::default());

    // Refresh the existing flow if it still exists (re-publish path).
    if let Some(member_id) = existing_flow {
        if state.members.get(member_id).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
            .is_some()
        {
            state
                .members
                .update(member_id, &name, "Auto-scaffolded by Celerex Design publish.", &graph, &provider_config, "writer")
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            return Ok(member_id);
        }
    }

    // First publish (or the flow was deleted): create a fresh one and record it.
    let project_id = design_deliveries_project(state, engagement_id).await?;
    let member = state
        .members
        .create(project_id, &name, "Auto-scaffolded by Celerex Design publish.", &graph, &provider_config, "writer")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    sqlx::query("UPDATE design_templates SET flow_member_id = $1 WHERE id = $2")
        .bind(member.id)
        .bind(template_id)
        .execute(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(member.id)
}

#[derive(sqlx::FromRow, Serialize)]
pub struct TemplateSummary {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub mode: String,
    pub render_engine: String,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `GET /api/design/templates` — the Design Library for the caller's engagement.
pub async fn list_templates(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
) -> Result<Json<Vec<TemplateSummary>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, TemplateSummary>(
        "SELECT id, title, kind, mode, render_engine, version, created_at, updated_at
         FROM design_templates WHERE engagement_id = $1 ORDER BY updated_at DESC",
    )
    .bind(claims.engagement_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

#[derive(sqlx::FromRow, Serialize)]
pub struct TemplateDetail {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub mode: String,
    pub render_engine: String,
    pub template_html: String,
    pub tokens_css: String,
    pub slots: Value,
    pub version: i32,
}

/// `GET /api/design/templates/{id}` — full template (for the library detail view
/// and for scaffolding a render flow).
pub async fn get_template(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TemplateDetail>, (StatusCode, String)> {
    let row = sqlx::query_as::<_, TemplateDetail>(
        "SELECT id, title, kind, mode, render_engine, template_html, tokens_css, slots, version
         FROM design_templates WHERE id = $1 AND engagement_id = $2",
    )
    .bind(id)
    .bind(claims.engagement_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "template not found".into()))?;
    Ok(Json(row))
}

/// `GET /api/design/assets/{id}` — serve a stored asset (engagement-scoped).
pub async fn get_asset(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT a.content_type, a.bytes
         FROM design_assets a
         JOIN design_templates t ON t.id = a.template_id
         WHERE a.id = $1 AND t.engagement_id = $2",
    )
    .bind(id)
    .bind(claims.engagement_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (content_type, bytes) = row.ok_or((StatusCode::NOT_FOUND, "asset not found".into()))?;
    Ok(([(header::CONTENT_TYPE, content_type)], bytes))
}

// ===========================================================================
// Device credentials: scoped token mint + device-pairing handshake (piece C).
// ===========================================================================

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub scope: String,
    pub expires_at: DateTime<Utc>,
}

/// `POST /api/design/tokens` — mint a `design.publish`-scoped device token for
/// the caller's engagement. The user pastes this into the Celerex Design
/// desktop app ("paste-scoped-token" path). Requires a full session token.
pub async fn mint_token(
    AuthClaims(claims): AuthClaims,
    State(_state): State<AppState>,
) -> Json<TokenResponse> {
    let token = mint_scoped(
        claims.engagement_id,
        &claims.sub,
        SCOPE_DESIGN_DEVICE,
        DESIGN_TOKEN_TTL_SECS,
    );
    Json(TokenResponse {
        token,
        scope: SCOPE_DESIGN_DEVICE.to_string(),
        expires_at: Utc::now() + Duration::seconds(DESIGN_TOKEN_TTL_SECS),
    })
}

/// Generate a secret `device_code` (32 hex) and a short, human-typable
/// `user_code` (e.g. `A1B2-C3D4`) from fresh UUIDs.
fn gen_pairing_codes() -> (String, String) {
    let device_code = Uuid::new_v4().simple().to_string();
    let b = Uuid::new_v4().into_bytes();
    let user_code = format!("{:02X}{:02X}-{:02X}{:02X}", b[0], b[1], b[2], b[3]);
    (device_code, user_code)
}

#[derive(Deserialize)]
pub struct PairStartRequest {
    /// Optional human label for the device (shown to the approver).
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Serialize)]
pub struct PairStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub expires_at: DateTime<Utc>,
    /// Suggested poll cadence (seconds) for `/pair/poll`.
    pub poll_interval: u32,
}

/// `POST /api/design/pair/start` — unauthenticated. The desktop app (no
/// credential yet) opens a pairing: it gets a secret `device_code` to poll with
/// and a short `user_code` the human enters in Celerex to approve.
pub async fn pair_start(
    State(state): State<AppState>,
    Json(body): Json<PairStartRequest>,
) -> Result<(StatusCode, Json<PairStartResponse>), (StatusCode, String)> {
    let (device_code, user_code) = gen_pairing_codes();
    let expires_at = Utc::now() + Duration::minutes(PAIRING_TTL_MINUTES);
    sqlx::query(
        "INSERT INTO design_pairings (device_code, user_code, label, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&device_code)
    .bind(&user_code)
    .bind(&body.label)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(PairStartResponse {
            device_code,
            user_code,
            expires_at,
            poll_interval: 3,
        }),
    ))
}

#[derive(Deserialize)]
pub struct PairApproveRequest {
    pub user_code: String,
}

#[derive(Serialize)]
pub struct PairApproveResponse {
    pub ok: bool,
    pub label: Option<String>,
}

/// `POST /api/design/pair/approve` — authenticated (full session). Binds a
/// pending `user_code` to the caller's engagement and mints the scoped token
/// the desktop will collect on its next poll.
pub async fn pair_approve(
    AuthClaims(claims): AuthClaims,
    State(state): State<AppState>,
    Json(body): Json<PairApproveRequest>,
) -> Result<Json<PairApproveResponse>, (StatusCode, String)> {
    let user_code = body.user_code.trim().to_uppercase();
    // Find a live, still-pending pairing for this code.
    let row: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, label FROM design_pairings
         WHERE user_code = $1 AND status = 'pending' AND expires_at > NOW()",
    )
    .bind(&user_code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (pairing_id, label) = row.ok_or((
        StatusCode::NOT_FOUND,
        "no pending pairing for that code (it may have expired)".to_string(),
    ))?;

    let token = mint_scoped(
        claims.engagement_id,
        &claims.sub,
        SCOPE_DESIGN_DEVICE,
        DESIGN_TOKEN_TTL_SECS,
    );
    sqlx::query(
        "UPDATE design_pairings
            SET status = 'approved', engagement_id = $1, token = $2, approved_at = NOW()
          WHERE id = $3",
    )
    .bind(claims.engagement_id)
    .bind(&token)
    .bind(pairing_id)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(PairApproveResponse { ok: true, label }))
}

#[derive(Deserialize)]
pub struct PairPollRequest {
    pub device_code: String,
}

#[derive(Serialize)]
pub struct PairPollResponse {
    pub status: String,
    /// The scoped token, returned exactly once when status flips to `approved`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// `POST /api/design/pair/poll` — unauthenticated. The desktop polls with its
/// secret `device_code`; once approved it receives the token (delivered once,
/// then cleared from the row).
pub async fn pair_poll(
    State(state): State<AppState>,
    Json(body): Json<PairPollRequest>,
) -> Result<Json<PairPollResponse>, (StatusCode, String)> {
    let row: Option<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT status, token, (expires_at < NOW()) AS expired
         FROM design_pairings WHERE device_code = $1",
    )
    .bind(&body.device_code)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (status, token, expired) =
        row.ok_or((StatusCode::NOT_FOUND, "unknown device_code".to_string()))?;

    if status == "approved" {
        // Hand the token over once, then clear it so it can't be re-read.
        if token.is_some() {
            sqlx::query("UPDATE design_pairings SET token = NULL WHERE device_code = $1")
                .bind(&body.device_code)
                .execute(&state.pool)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        return Ok(Json(PairPollResponse { status, token }));
    }
    if expired {
        return Ok(Json(PairPollResponse { status: "expired".into(), token: None }));
    }
    Ok(Json(PairPollResponse { status, token: None }))
}
