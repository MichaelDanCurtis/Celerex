use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub exp: usize,
    /// Capability scope. `None` = a full session token (the default for
    /// browser logins). A `Some(_)` token is restricted: it only works on the
    /// routes whose extractor opts that scope in (see `DesignAuth`). This is
    /// what lets the Celerex Design desktop app hold a credential that can
    /// publish bundles but cannot drain the rest of the engagement's API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// The only capability a Celerex Design device credential is granted: publish
/// design bundles into the engagement's Design Library.
pub const SCOPE_DESIGN_PUBLISH: &str = "design.publish";

/// The full Celerex Design device capability: publish + read provider config +
/// proxy LLM traffic. Granted to a paired device (supersedes publish-only).
pub const SCOPE_DESIGN_DEVICE: &str = "design.device";

pub fn verify(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}

/// Mint a session JWT for a real authenticated user. `sub` is the user's email;
/// `engagement_id` is the tenant key the rest of the API authorizes against.
pub fn mint(engagement_id: Uuid, sub: &str, ttl_secs: i64) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": sub,
        "engagement_id": engagement_id,
        "exp": (chrono::Utc::now().timestamp() + ttl_secs),
    });
    let key = std::env::var("CELEREX_JWT_SECRET").unwrap_or_else(|_| "test-secret".into());
    encode(&Header::default(), &claims, &EncodingKey::from_secret(key.as_bytes())).unwrap()
}

/// Mint a capability-scoped token. Unlike `mint`, the resulting JWT carries a
/// `scope` claim, so it is rejected by every route guarded by the default
/// `AuthClaims` extractor and accepted only where that scope is opted in. Used
/// for the Celerex Design device credential (`SCOPE_DESIGN_PUBLISH`).
pub fn mint_scoped(engagement_id: Uuid, sub: &str, scope: &str, ttl_secs: i64) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": sub,
        "engagement_id": engagement_id,
        "scope": scope,
        "exp": (chrono::Utc::now().timestamp() + ttl_secs),
    });
    let key = std::env::var("CELEREX_JWT_SECRET").unwrap_or_else(|_| "test-secret".into());
    encode(&Header::default(), &claims, &EncodingKey::from_secret(key.as_bytes())).unwrap()
}

#[doc(hidden)]
pub fn mint_for_tests(engagement_id: Uuid) -> String {
    mint(engagement_id, "test-user", 3600)
}
