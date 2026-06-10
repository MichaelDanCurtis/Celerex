use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub engagement_id: Uuid,
    pub project_id: Option<Uuid>,
    pub exp: usize,
}

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

#[doc(hidden)]
pub fn mint_for_tests(engagement_id: Uuid) -> String {
    mint(engagement_id, "test-user", 3600)
}
