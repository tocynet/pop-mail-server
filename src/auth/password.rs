//! Password hashing with Argon2

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher as ArgonHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Error, Debug)]
pub enum PasswordError {
    #[error("Failed to hash password: {0}")]
    HashError(String),
    #[error("Failed to verify password: {0}")]
    VerifyError(String),
    #[error("Invalid hash format")]
    InvalidFormat,
}

/// Password hasher using Argon2id
pub struct PasswordHasher {
    argon2: Argon2<'static>,
}

impl PasswordHasher {
    /// Create a new password hasher with default settings
    pub fn new() -> Self {
        Self {
            argon2: Argon2::default(),
        }
    }

    /// Hash a password
    pub fn hash(&self, password: &str) -> Result<String, PasswordError> {
        let password = Zeroizing::new(password.as_bytes().to_vec());
        let salt = SaltString::generate(&mut OsRng);

        let hash = self
            .argon2
            .hash_password(&password, &salt)
            .map_err(|e| PasswordError::HashError(e.to_string()))?;

        Ok(hash.to_string())
    }

    /// Verify a password against a hash
    pub fn verify(&self, password: &str, hash: &str) -> Result<bool, PasswordError> {
        let password = Zeroizing::new(password.as_bytes().to_vec());
        let parsed_hash =
            PasswordHash::new(hash).map_err(|_| PasswordError::InvalidFormat)?;

        match self.argon2.verify_password(&password, &parsed_hash) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(PasswordError::VerifyError(e.to_string())),
        }
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let hasher = PasswordHasher::new();
        let password = "secret123";

        let hash = hasher.hash(password).unwrap();
        assert!(hash.starts_with("$argon2"));

        assert!(hasher.verify(password, &hash).unwrap());
        assert!(!hasher.verify("wrong", &hash).unwrap());
    }

    #[test]
    fn test_different_hashes() {
        let hasher = PasswordHasher::new();
        let password = "secret123";

        let hash1 = hasher.hash(password).unwrap();
        let hash2 = hasher.hash(password).unwrap();

        // Same password should produce different hashes (different salts)
        assert_ne!(hash1, hash2);

        // But both should verify correctly
        assert!(hasher.verify(password, &hash1).unwrap());
        assert!(hasher.verify(password, &hash2).unwrap());
    }

    #[test]
    fn test_invalid_hash() {
        let hasher = PasswordHasher::new();
        assert!(hasher.verify("password", "invalid").is_err());
    }
}
