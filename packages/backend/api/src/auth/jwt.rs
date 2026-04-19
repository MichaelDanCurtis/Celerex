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

#[doc(hidden)]
pub fn mint_for_tests(engagement_id: Uuid) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": "test-user",
        "engagement_id": engagement_id,
        "exp": (chrono::Utc::now().timestamp() + 3600)
    });
    let key = std::env::var("CELEREX_JWT_SECRET").unwrap_or_else(|_| "test-secret".into());
    encode(&Header::default(), &claims, &EncodingKey::from_secret(key.as_bytes())).unwrap()
}
