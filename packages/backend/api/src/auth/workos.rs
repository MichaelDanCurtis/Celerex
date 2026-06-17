use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// Subset of WorkOS access-token claims we rely on. `sub` = WorkOS user id (stable identity).
#[derive(Debug, Deserialize)]
pub struct WorkosClaims {
    pub sub: String,
    #[serde(default)]
    pub exp: usize,
}

fn jwks_base() -> String {
    std::env::var("WORKOS_JWKS_URL").unwrap_or_else(|_| "https://api.workos.com".to_string())
}

fn client_id() -> Result<String, String> {
    std::env::var("WORKOS_CLIENT_ID").map_err(|_| "WORKOS_CLIENT_ID not set".to_string())
}

/// Fetch the WorkOS JWKS for the configured client id.
#[allow(dead_code)]
pub async fn fetch_jwks(http: &reqwest::Client) -> Result<JwkSet, String> {
    let url = format!("{}/sso/jwks/{}", jwks_base().trim_end_matches('/'), client_id()?);
    let res = http.get(&url).send().await.map_err(|e| format!("jwks fetch failed: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("jwks fetch status {}", res.status()));
    }
    res.json::<JwkSet>().await.map_err(|e| format!("jwks parse failed: {e}"))
}

/// Verify a WorkOS RS256 access token against a JwkSet. Checks signature + expiry.
#[allow(dead_code)]
pub fn verify_with_jwks(token: &str, jwks: &JwkSet) -> Result<WorkosClaims, String> {
    let header = decode_header(token).map_err(|e| format!("bad token header: {e}"))?;
    let kid = header.kid.ok_or_else(|| "token missing kid".to_string())?;
    let jwk = jwks.find(&kid).ok_or_else(|| "no matching jwk for kid".to_string())?;
    let key = DecodingKey::from_jwk(jwk).map_err(|e| format!("bad jwk: {e}"))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    validation.validate_aud = false; // audience/issuer tightening is a later hardening; signature+exp gates slice 1
    let data = decode::<WorkosClaims>(token, &key, &validation)
        .map_err(|e| format!("token invalid: {e}"))?;
    Ok(data.claims)
}

/// Fetch JWKS then verify. (Per-call fetch is fine for slice 1; caching is a later optimization.)
pub async fn verify_access_token(http: &reqwest::Client, token: &str) -> Result<WorkosClaims, String> {
    let jwks = fetch_jwks(http).await?;
    verify_with_jwks(token, &jwks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jwks_base_defaults_when_unset() {
        // SAFETY: single-process test.
        unsafe { std::env::remove_var("WORKOS_JWKS_URL"); }
        assert_eq!(jwks_base(), "https://api.workos.com");
    }
    #[test]
    fn verify_rejects_garbage_token() {
        let jwks: JwkSet = serde_json::from_value(serde_json::json!({"keys": []})).unwrap();
        assert!(verify_with_jwks("not.a.jwt", &jwks).is_err());
    }
}
