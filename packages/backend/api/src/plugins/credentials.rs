use async_trait::async_trait;
use ring::{aead, rand::{SystemRandom, SecureRandom}};

#[derive(Debug, Clone)]
pub struct SealedBlob {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String>;
    async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String>;
}

pub struct LocalAeadStore { key: aead::LessSafeKey }

impl LocalAeadStore {
    pub fn from_env() -> Result<Self, String> {
        let raw = std::env::var("CELEREX_CRED_KEY")
            .map_err(|_| "CELEREX_CRED_KEY missing".to_string())?;
        if raw.len() < 32 { return Err("CELEREX_CRED_KEY must be at least 32 bytes".into()); }
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &raw.as_bytes()[..32])
            .map_err(|e| e.to_string())?;
        Ok(Self { key: aead::LessSafeKey::new(unbound) })
    }
}

#[async_trait]
impl CredentialStore for LocalAeadStore {
    async fn seal(&self, plaintext: &[u8]) -> Result<SealedBlob, String> {
        let mut nonce_bytes = [0u8; 12];
        SystemRandom::new().fill(&mut nonce_bytes).map_err(|e| e.to_string())?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut buf = plaintext.to_vec();
        self.key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok(SealedBlob { ciphertext: buf, nonce: nonce_bytes.to_vec() })
    }

    async fn open(&self, blob: &SealedBlob) -> Result<Vec<u8>, String> {
        let nonce_arr: [u8; 12] = blob.nonce.as_slice().try_into().map_err(|_| "bad nonce".to_string())?;
        let n = aead::Nonce::assume_unique_for_key(nonce_arr);
        let mut buf = blob.ciphertext.clone();
        let pt = self.key.open_in_place(n, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok(pt.to_vec())
    }
}
