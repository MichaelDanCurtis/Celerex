use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode, header::AUTHORIZATION},
};
use crate::auth::jwt::{verify, Claims, SCOPE_DESIGN_PUBLISH, SCOPE_DESIGN_DEVICE};

/// Verify the `Authorization: Bearer <jwt>` header and return its claims. Shared
/// by both extractors below; scope policy is applied by the caller.
fn claims_from_parts(parts: &Parts) -> Result<Claims, (StatusCode, &'static str)> {
    let header = parts.headers.get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing authorization"))?;
    let token = header.strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer scheme"))?;
    let secret = std::env::var("CELEREX_JWT_SECRET")
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JWT secret unset"))?;
    verify(token, &secret).map_err(|_| (StatusCode::UNAUTHORIZED, "invalid token"))
}

/// A full session token. Rejects any capability-scoped credential, so a leaked
/// `design.publish` device token is inert on every general API route.
pub struct AuthClaims(pub Claims);

impl<S: Send + Sync> FromRequestParts<S> for AuthClaims {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let claims = claims_from_parts(parts)?;
        if claims.scope.is_some() {
            return Err((StatusCode::FORBIDDEN, "token scope not permitted here"));
        }
        Ok(AuthClaims(claims))
    }
}

/// Accepts either a full session token or a `design.publish`-scoped device
/// token. Used only by the design *ingest* route, so the desktop credential can
/// publish bundles but nothing else.
pub struct DesignAuth(pub Claims);

// DesignAuth (ingest + providers-config): session, publish (legacy), or device.
impl<S: Send + Sync> FromRequestParts<S> for DesignAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let claims = claims_from_parts(parts)?;
        match claims.scope.as_deref() {
            None | Some(SCOPE_DESIGN_PUBLISH) | Some(SCOPE_DESIGN_DEVICE) => Ok(DesignAuth(claims)),
            Some(_) => Err((StatusCode::FORBIDDEN, "token scope not permitted here")),
        }
    }
}

/// Device-only routes (LLM proxy, key copy): full session OR `design.device`.
/// A publish-only legacy token is rejected here — it cannot proxy LLM traffic.
pub struct DeviceAuth(pub Claims);

impl<S: Send + Sync> FromRequestParts<S> for DeviceAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let claims = claims_from_parts(parts)?;
        match claims.scope.as_deref() {
            None | Some(SCOPE_DESIGN_DEVICE) => Ok(DeviceAuth(claims)),
            Some(_) => Err((StatusCode::FORBIDDEN, "device scope required")),
        }
    }
}
