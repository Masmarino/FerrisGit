use crate::error::infra;
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::PasswordHasherPort;

pub struct Argon2PasswordHasher;

impl Argon2PasswordHasher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Argon2PasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordHasherPort for Argon2PasswordHasher {
    fn hash(&self, plain: &str) -> Result<String, DomainError> {
        Argon2::default()
            .hash_password(plain.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(infra)
    }

    fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError> {
        let parsed = PasswordHash::new(hash).map_err(infra)?;
        Ok(Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_then_verifying_the_same_password_succeeds() {
        let hasher = Argon2PasswordHasher::new();
        let hash = hasher.hash("correct horse battery staple").unwrap();
        assert!(
            hasher
                .verify("correct horse battery staple", &hash)
                .unwrap()
        );
    }

    #[test]
    fn verifying_the_wrong_password_fails() {
        let hasher = Argon2PasswordHasher::new();
        let hash = hasher.hash("correct horse battery staple").unwrap();
        assert!(!hasher.verify("wrong password", &hash).unwrap());
    }

    // Produced by argon2 0.5; passwords hashed before an upgrade must keep verifying.
    const HASH_FROM_ARGON2_0_5: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$ISO7kkvFzh19GM8qB7patN3C3Y9HHsjlVTfEZ9T600Y";

    #[test]
    fn a_hash_written_by_the_previous_argon2_version_still_verifies() {
        let hasher = Argon2PasswordHasher::new();
        assert!(
            hasher
                .verify("correct horse battery staple", HASH_FROM_ARGON2_0_5)
                .unwrap()
        );
        assert!(
            !hasher
                .verify("wrong password", HASH_FROM_ARGON2_0_5)
                .unwrap()
        );
    }
}
