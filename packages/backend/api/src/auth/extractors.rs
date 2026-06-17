use crate::auth::jwt::{Claims, SCOPE_DESIGN_DEVICE, SCOPE_DESIGN_PUBLISH, verify};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
};

/// Verify the `Authorization: Bearer <jwt>` header and return its claims. Shared
/// by both extractors below; scope policy is applied by the caller.
fn claims_from_parts(parts: &Parts) -> Result<Claims, (StatusCode, &'static str)> {
    let header = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing authorization"))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "expected Bearer scheme"))?;
    let secret = crate::auth::jwt::jwt_secret();
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

/// Accepts a full session token, a legacy `design.publish` token, or a
/// `design.device` token. Guards the design ingest + provider-config routes.
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
