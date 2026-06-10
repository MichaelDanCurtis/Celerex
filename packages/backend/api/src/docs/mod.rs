pub mod handler;

use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use uuid::Uuid;
use crate::{AppState, auth::AuthClaims, db};

#[derive(Deserialize)]
pub struct CreateBody {
    pub title: String,
}

pub async fn create(
    auth: AuthClaims,
    State(s): State<AppState>,
    Json(body): Json<CreateBody>,
) -> impl IntoResponse {
    match db::create_document(&s.pool, auth.0.engagement_id, auth.0.project_id, &body.title).await {
        Ok(d) => (StatusCode::CREATED, Json(d)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn append_block(
    _auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match db::append_block(&s.pool, doc_id, &payload).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn replace_block(
    _auth: AuthClaims,
    State(s): State<AppState>,
    Path((_doc, block)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    match db::replace_block(&s.pool, block, &payload).await {
        Ok(0) => (StatusCode::NOT_FOUND, "block not found").into_response(),
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_for_engagement(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path(engagement_id): Path<Uuid>,
) -> impl IntoResponse {
    if auth.0.engagement_id != engagement_id {
        return (StatusCode::FORBIDDEN, "engagement mismatch").into_response();
    }
    match db::list_documents_for_engagement(&s.pool, engagement_id).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
) -> impl IntoResponse {
    match db::get_document(&s.pool, doc_id).await {
        Ok(Some(d)) => {
            if d.engagement_id != auth.0.engagement_id {
                return (StatusCode::FORBIDDEN, "engagement mismatch").into_response();
            }
            (StatusCode::OK, Json(d)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "document not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_blocks(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
) -> impl IntoResponse {
    match db::get_document(&s.pool, doc_id).await {
        Ok(Some(d)) => {
            if d.engagement_id != auth.0.engagement_id {
                return (StatusCode::FORBIDDEN, "engagement mismatch").into_response();
            }
            match db::list_blocks(&s.pool, doc_id).await {
                Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "document not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete(
    auth: AuthClaims,
    State(s): State<AppState>,
    Path(doc_id): Path<Uuid>,
) -> impl IntoResponse {
    match db::get_document(&s.pool, doc_id).await {
        Ok(Some(d)) => {
            if d.engagement_id != auth.0.engagement_id {
                return (StatusCode::FORBIDDEN, "engagement mismatch").into_response();
            }
            match db::delete_document(&s.pool, doc_id).await {
                Ok(_) => StatusCode::NO_CONTENT.into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "document not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
