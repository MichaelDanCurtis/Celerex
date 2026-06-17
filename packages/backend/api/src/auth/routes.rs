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

#[derive(Deserialize)]
pub struct WorkosLoginRequest {
    /// Display email from the WorkOS user object (identity is the validated token `sub`).
    pub email: String,
}

/// `POST /api/auth/workos` — PUBLIC. Validate a WorkOS access token (Bearer),
/// find-or-provision the personal engagement keyed to the WorkOS user id, then
/// mint the EXISTING HS256 session JWT so existing protected routes accept it.
/// Identity is the validated token `sub`; the body `email` is a display label only.
pub async fn workos_login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<WorkosLoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".to_string()))?;

    let http = reqwest::Client::new();
    let claims = crate::auth::workos::verify_access_token(&http, token)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;

    let (engagement_id, engagement_name) =
        match db::find_engagement_by_workos_user(&state.pool, &claims.sub)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            Some(found) => found,
            None => {
                let name = format!("{}'s workspace", body.email.trim());
                let eng = db::create_engagement(&state.pool, &name)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                db::create_workos_user(&state.pool, body.email.trim(), &claims.sub, eng.id)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                (eng.id, eng.name)
            }
        };

    let token = jwt::mint(engagement_id, &claims.sub, SESSION_TTL_SECS);
    Ok(Json(LoginResponse {
        token,
        engagement: LoginEngagement { id: engagement_id, name: engagement_name },
    }))
}
