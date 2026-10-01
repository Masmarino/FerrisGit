use sha2::{Digest, Sha256};

pub fn hash_token(plain: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plain.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_the_same_token_twice_is_deterministic() {
        assert_eq!(hash_token("fg_abc"), hash_token("fg_abc"));
    }

    #[test]
    fn hashing_different_tokens_produces_different_hashes() {
        assert_ne!(hash_token("fg_abc"), hash_token("fg_xyz"));
    }
}
