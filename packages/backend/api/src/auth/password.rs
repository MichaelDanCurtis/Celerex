//! Argon2id password hashing + verification for real auth.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

/// Hash a plaintext password into a PHC string (`$argon2id$...`) suitable for
/// storage in `users.password_hash`.
pub fn hash_password(plaintext: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verify a plaintext password against a stored PHC hash. Returns false for any
/// mismatch or malformed hash (never panics, never distinguishes the failure
/// modes to a caller).
pub fn verify_password(plaintext: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_roundtrips() {
        let h = hash_password("correct horse battery staple").unwrap();
        assert!(h.starts_with("$argon2"));
        assert!(verify_password("correct horse battery staple", &h));
    }

    #[test]
    fn wrong_password_fails() {
        let h = hash_password("hunter2").unwrap();
        assert!(!verify_password("hunter3", &h));
    }

    #[test]
    fn two_hashes_of_same_password_differ() {
        // Distinct salts → distinct PHC strings.
        assert_ne!(hash_password("x").unwrap(), hash_password("x").unwrap());
    }

    #[test]
    fn malformed_hash_is_rejected_not_panicked() {
        assert!(!verify_password("anything", "not-a-phc-string"));
    }
}
