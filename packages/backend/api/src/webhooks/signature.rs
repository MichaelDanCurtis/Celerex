use async_trait::async_trait;
use axum::http::HeaderMap;

#[async_trait]
pub trait SignatureVerifier: Send + Sync {
    async fn verify(&self, headers: &HeaderMap, body: &[u8]) -> Result<(), String>;
}

pub struct NoopVerifier;

#[async_trait]
impl SignatureVerifier for NoopVerifier {
    async fn verify(&self, _h: &HeaderMap, _b: &[u8]) -> Result<(), String> { Ok(()) }
}
