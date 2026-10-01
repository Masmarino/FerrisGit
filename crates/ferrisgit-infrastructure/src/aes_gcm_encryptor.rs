use aes_gcm::aead::{Aead, Generate, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::secret_encryption::SecretEncryptorPort;

pub struct AesGcmSecretEncryptor {
    cipher: Aes256Gcm,
}

impl AesGcmSecretEncryptor {
    /// `key` must be exactly 32 bytes (AES-256). Any other length panics, since it's
    /// a configuration error caught at startup, not a runtime one.
    pub fn new(key: &[u8]) -> Self {
        Self {
            cipher: Aes256Gcm::new_from_slice(key)
                .expect("encryption key must be exactly 32 bytes"),
        }
    }
}

impl SecretEncryptorPort for AesGcmSecretEncryptor {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, DomainError> {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, DomainError> {
        if ciphertext.len() < 12 {
            return Err(DomainError::Infrastructure(
                "ciphertext too short to contain a nonce".to_string(),
            ));
        }
        let (nonce_bytes, encrypted) = ciphertext.split_at(12);
        let nonce = Nonce::<Aes256Gcm>::try_from(nonce_bytes).map_err(|_| {
            DomainError::Infrastructure("ciphertext too short to contain a nonce".to_string())
        })?;
        let plaintext_bytes = self
            .cipher
            .decrypt(&nonce, encrypted)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        String::from_utf8(plaintext_bytes).map_err(|e| DomainError::Infrastructure(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_encryptor() -> AesGcmSecretEncryptor {
        AesGcmSecretEncryptor::new(&[7u8; 32])
    }

    #[test]
    fn encrypting_then_decrypting_returns_the_original_plaintext() {
        let encryptor = test_encryptor();
        let ciphertext = encryptor.encrypt("super-secret-value").unwrap();
        assert_eq!(
            encryptor.decrypt(&ciphertext).unwrap(),
            "super-secret-value"
        );
    }

    #[test]
    fn the_same_plaintext_encrypted_twice_produces_different_ciphertext() {
        let encryptor = test_encryptor();
        let a = encryptor.encrypt("same-value").unwrap();
        let b = encryptor.encrypt("same-value").unwrap();
        assert_ne!(
            a, b,
            "a random nonce per encryption must prevent identical ciphertexts for identical plaintexts"
        );
    }

    #[test]
    fn decrypting_with_the_wrong_key_fails() {
        let encryptor_a = AesGcmSecretEncryptor::new(&[1u8; 32]);
        let encryptor_b = AesGcmSecretEncryptor::new(&[2u8; 32]);
        let ciphertext = encryptor_a.encrypt("secret").unwrap();
        assert!(encryptor_b.decrypt(&ciphertext).is_err());
    }

    // Produced by aes-gcm 0.10 with the key `[7; 32]`: nonce (12 bytes) followed by the ciphertext and tag.
    // Secrets stored before an upgrade must stay readable.
    const BLOB_FROM_AES_GCM_0_10: &str = "0102030405060708090a0b0cc80998fb18c79149864c676ab8d1c22ca180cd5cc90c6c6c9c8856ec3caebeae6ebc";

    #[test]
    fn a_value_encrypted_by_the_previous_aes_gcm_version_still_decrypts() {
        let blob = hex::decode(BLOB_FROM_AES_GCM_0_10).unwrap();
        assert_eq!(
            test_encryptor().decrypt(&blob).unwrap(),
            "super-secret-value"
        );
    }
}
