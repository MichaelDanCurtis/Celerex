use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::auth::jwt;
use crate::auth::password;
use crate::{db, AppState};

/// 7-day session tokens for real logins.
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;

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

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    /// The workspace to create-or-join. Case-insensitive.
    pub engagement_name: String,
}

/// `POST /api/auth/signup` — create an account (and its engagement if new),
/// then issue a session token.
pub async fn signup(
    State(state): State<AppState>,
    Json(body): Json<SignupRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let email = body.email.trim();
    let name = body.engagement_name.trim();
    if email.is_empty() || !email.contains('@') {
        return Err((StatusCode::BAD_REQUEST, "a valid email is required".into()));
    }
    if body.password.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "password must be at least 8 characters".into()));
    }
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "engagement_name is required".into()));
    }

    let engagement = db::find_or_create_engagement(&state.pool, name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let hash = password::hash_password(&body.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Err(e) = db::create_user(&state.pool, email, &hash, engagement.id).await {
        // Unique-email violation → 409; everything else → 500.
        let msg = e.to_string();
        if msg.contains("users_email_lower_uq") || msg.contains("unique") || msg.contains("duplicate") {
            return Err((StatusCode::CONFLICT, "an account with that email already exists".into()));
        }
        return Err((StatusCode::INTERNAL_SERVER_ERROR, msg));
    }

    let token = jwt::mint(engagement.id, email, SESSION_TTL_SECS);
    Ok(Json(LoginResponse {
        token,
        engagement: LoginEngagement { id: engagement.id, name: engagement.name },
    }))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// `POST /api/auth/login` — verify email + password, issue a session token.
/// Returns 401 (not 404) for both unknown email and wrong password so the
/// endpoint isn't an account-enumeration oracle.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let email = body.email.trim();
    let user = db::find_user_for_login(&state.pool, email)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some(user) = user else {
        return Err((StatusCode::UNAUTHORIZED, "invalid email or password".into()));
    };
    if !password::verify_password(&body.password, &user.password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "invalid email or password".into()));
    }

    let token = jwt::mint(user.engagement_id, email, SESSION_TTL_SECS);
    Ok(Json(LoginResponse {
        token,
        engagement: LoginEngagement { id: user.engagement_id, name: user.engagement_name },
    }))
}
