use crate::error::DomainError;

/// Reversible encryption for values that must be read back in plaintext later (CI/CD variables), unlike
/// `PasswordHasherPort`, which is one-way. Implementations own the encryption key and callers never see it.
pub trait SecretEncryptorPort: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, DomainError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentityEncryptor;
    impl SecretEncryptorPort for IdentityEncryptor {
        fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, DomainError> {
            Ok(plaintext.as_bytes().to_vec())
        }
        fn decrypt(&self, ciphertext: &[u8]) -> Result<String, DomainError> {
            Ok(String::from_utf8(ciphertext.to_vec()).unwrap())
        }
    }

    #[test]
    fn a_port_implementation_round_trips_a_value() {
        let encryptor = IdentityEncryptor;
        let ciphertext = encryptor.encrypt("secret-value").unwrap();
        assert_eq!(encryptor.decrypt(&ciphertext).unwrap(), "secret-value");
    }
}
