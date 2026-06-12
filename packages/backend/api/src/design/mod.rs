//! Celerex Design handoff — the Celerex-side ingest of design bundles.
//!
//! `POST /api/design/ingest` receives a bundle (v0.1: the unzipped CDB as JSON;
//! the multipart `.cdb` zip wrapper lands with the desktop app), validates +
//! sanitizes it, and stores it in the Design Library (`design_templates`).
//! `GET /api/design/templates` lists the library; `GET /api/design/assets/{id}`
//! serves a stored asset. See docs/plans/2026-06-12-design-handoff-contract.md.

pub mod sanitize;

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{AppState, auth::extractors::AuthClaims};

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
}

fn manifest_str<'a>(m: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = m;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str()
}

/// `POST /api/design/ingest` — store a design bundle in the caller's engagement.
pub async fn ingest(
    AuthClaims(claims): AuthClaims,
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
    let row: (Uuid, i32) = sqlx::query_as(
        "INSERT INTO design_templates
            (engagement_id, title, kind, mode, render_engine, template_html, tokens_css, slots, manifest, source_project_ref)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (engagement_id, source_project_ref) WHERE source_project_ref IS NOT NULL
         DO UPDATE SET title = EXCLUDED.title, kind = EXCLUDED.kind, mode = EXCLUDED.mode,
            render_engine = EXCLUDED.render_engine, template_html = EXCLUDED.template_html,
            tokens_css = EXCLUDED.tokens_css, slots = EXCLUDED.slots, manifest = EXCLUDED.manifest,
            version = design_templates.version + 1, updated_at = NOW()
         RETURNING id, version",
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
    let (template_id, version) = row;

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

    Ok((
        StatusCode::CREATED,
        Json(IngestResponse {
            template_id,
            version,
            mode: mode.to_string(),
            kind: kind.to_string(),
            sanitized,
            asset_count,
        }),
    ))
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
