use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{auth::jwt::mint_for_tests, db, AppState};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub engagement_name: String,
}

#[derive(Serialize)]
pub struct LoginEngagement {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub engagement: LoginEngagement,
}

/// Dev-mode signin. Looks up an engagement by name (case-insensitive) and
/// mints a JWT for it. No password / no user registration — this stands in
/// for the missing real auth flow.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let name = body.engagement_name.trim();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "engagement_name is required".into()));
    }

    let found = db::find_engagement_by_name(&state.pool, name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let engagement = match found {
        Some(e) => e,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("engagement '{name}' not found"),
            ))
        }
    };

    let token = mint_for_tests(engagement.id);
    Ok(Json(LoginResponse {
        token,
        engagement: LoginEngagement {
            id: engagement.id,
            name: engagement.name,
        },
    }))
}
