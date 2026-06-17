//! Postgres-backed encrypted store for per-engagement provider API keys.
//!
//! Reuses the same AEAD (ChaCha20-Poly1305) construction as
//! [`crate::plugins::credentials::LocalAeadStore`] but speaks directly to a
//! dedicated `engagement_provider_keys` table so we can store one key per
//! `(engagement_id, provider_id)` pair with its own nonce + timestamps.
//!
//! Phase 1 surface:
//!   - `set` — encrypt and upsert
//!   - `get` — decrypt on read (only callable from inside the API process)
//!   - `list_for_engagement` — return safe metadata (no plaintext, masked preview)
//!   - `delete`
//!
//! The plaintext key is never returned via any HTTP endpoint and is never
//! logged.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Provider ids we currently support per-engagement keys for. Anything not in
/// this list is rejected at the route layer with 400.
pub const SUPPORTED_PROVIDERS: &[&str] =
    &["anthropic", "openai", "kimi", "gemini", "copilot", "codex"];

/// Safe-to-render metadata about a stored provider key. The plaintext key is
/// intentionally not part of this struct.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderKeyMeta {
    pub provider_id: String,
    /// Masked preview such as `"sk-a...xyz"` — first 4 + last 3 plaintext chars.
    /// Generated on write so we don't have to decrypt on list.
    pub key_preview: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[async_trait]
pub trait ProviderKeysStore: Send + Sync {
    async fn set(
        &self,
        engagement_id: Uuid,
        provider_id: &str,
        plaintext_key: &str,
    ) -> Result<ProviderKeyMeta, String>;

    async fn get(
        &self,
        engagement_id: Uuid,
        provider_id: &str,
    ) -> Result<Option<String>, String>;

    async fn list_for_engagement(
        &self,
        engagement_id: Uuid,
    ) -> Result<Vec<ProviderKeyMeta>, String>;

    async fn delete(&self, engagement_id: Uuid, provider_id: &str) -> Result<bool, String>;
}

pub struct PgProviderKeysStore {
    pool: PgPool,
    key: aead::LessSafeKey,
}

impl PgProviderKeysStore {
    /// `aead_key` must be at least 32 bytes; only the first 32 are used.
    pub fn new(pool: PgPool, aead_key: Vec<u8>) -> Result<Self, String> {
        if aead_key.len() < 32 {
            return Err("aead_key must be at least 32 bytes".into());
        }
        let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &aead_key[..32])
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pool,
            key: aead::LessSafeKey::new(unbound),
        })
    }

    /// Convenience constructor matching [`LocalAeadStore::from_env`].
    pub fn from_env(pool: PgPool) -> Result<Self, String> {
        let raw = std::env::var("CELEREX_CRED_KEY")
            .map_err(|_| "CELEREX_CRED_KEY missing".to_string())?;
        Self::new(pool, raw.into_bytes())
    }

    fn seal(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let mut nonce_bytes = [0u8; 12];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|e| e.to_string())?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut buf = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok((buf, nonce_bytes.to_vec()))
    }

    fn open(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
        let nonce_arr: [u8; 12] = nonce
            .try_into()
            .map_err(|_| "bad nonce length".to_string())?;
        let n = aead::Nonce::assume_unique_for_key(nonce_arr);
        let mut buf = ciphertext.to_vec();
        let pt = self
            .key
            .open_in_place(n, aead::Aad::empty(), &mut buf)
            .map_err(|e| e.to_string())?;
        Ok(pt.to_vec())
    }
}

/// Produce a masked preview of the form `"abcd...xyz"` (first 4 + last 3
/// plaintext characters joined by `...`). For very short keys we degrade
/// gracefully so callers can still tell at a glance whether *something* is
/// configured without leaking the whole secret.
pub fn key_preview(plaintext: &str) -> String {
    // Iterate by characters, not bytes — keys may include non-ASCII chars in
    // theory and we don't want to slice mid-grapheme.
    let chars: Vec<char> = plaintext.chars().collect();
    if chars.len() <= 7 {
        // Don't reveal a key that fits entirely in the window we'd otherwise
        // show. Show the length only.
        return format!("[{} chars]", chars.len());
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(3).collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}...{tail}")
}

#[async_trait]
impl ProviderKeysStore for PgProviderKeysStore {
    async fn set(
        &self,
        engagement_id: Uuid,
        provider_id: &str,
        plaintext_key: &str,
    ) -> Result<ProviderKeyMeta, String> {
        let (ciphertext, nonce) = self.seal(plaintext_key.as_bytes())?;
        let preview = key_preview(plaintext_key);
        // Upsert. `key_preview` is recomputed on every write so it always
        // matches the encrypted plaintext.
        let row: (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
            "INSERT INTO engagement_provider_keys
                (engagement_id, provider_id, encrypted_key, nonce, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())
             ON CONFLICT (engagement_id, provider_id) DO UPDATE SET
                encrypted_key = EXCLUDED.encrypted_key,
                nonce = EXCLUDED.nonce,
                updated_at = NOW()
             RETURNING created_at, updated_at",
        )
        .bind(engagement_id)
        .bind(provider_id)
        .bind(&ciphertext)
        .bind(&nonce)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(ProviderKeyMeta {
            provider_id: provider_id.to_string(),
            key_preview: preview,
            created_at: row.0,
            updated_at: row.1,
        })
    }

    async fn get(
        &self,
        engagement_id: Uuid,
        provider_id: &str,
    ) -> Result<Option<String>, String> {
        let row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT encrypted_key, nonce FROM engagement_provider_keys
             WHERE engagement_id = $1 AND provider_id = $2",
        )
        .bind(engagement_id)
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let Some((ct, nonce)) = row else {
            return Ok(None);
        };
        let pt = self.open(&ct, &nonce)?;
        let s = String::from_utf8(pt).map_err(|e| e.to_string())?;
        Ok(Some(s))
    }

    async fn list_for_engagement(
        &self,
        engagement_id: Uuid,
    ) -> Result<Vec<ProviderKeyMeta>, String> {
        let rows: Vec<(String, Vec<u8>, Vec<u8>, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT provider_id, encrypted_key, nonce, created_at, updated_at
             FROM engagement_provider_keys
             WHERE engagement_id = $1
             ORDER BY provider_id ASC",
        )
        .bind(engagement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        // We have to decrypt each row to recompute the preview. We *could*
        // store the preview as a column to avoid this, but decrypt-on-list
        // sidesteps the question of "does this row's preview match its
        // ciphertext?" and avoids a denormalised field.
        let mut out = Vec::with_capacity(rows.len());
        for (provider_id, ct, nonce, created_at, updated_at) in rows {
            let pt = self.open(&ct, &nonce)?;
            let s = String::from_utf8(pt).map_err(|e| e.to_string())?;
            out.push(ProviderKeyMeta {
                provider_id,
                key_preview: key_preview(&s),
                created_at,
                updated_at,
            });
        }
        Ok(out)
    }

    async fn delete(&self, engagement_id: Uuid, provider_id: &str) -> Result<bool, String> {
        let r = sqlx::query(
            "DELETE FROM engagement_provider_keys
             WHERE engagement_id = $1 AND provider_id = $2",
        )
        .bind(engagement_id)
        .bind(provider_id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(r.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_preview_long_key() {
        // 4 + 3 chars are visible, joined by "..."
        assert_eq!(key_preview("sk-1234567890abcdef"), "sk-1...def");
    }

    #[test]
    fn key_preview_short_key_masks_length_only() {
        assert_eq!(key_preview("short"), "[5 chars]");
    }

    #[test]
    fn key_preview_handles_unicode() {
        // 8 chars >= threshold; head=4, tail=3
        let p = key_preview("åäöabcdefgh");
        assert_eq!(p, "åäöa...fgh");
    }
}
