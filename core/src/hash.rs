use sha2::{Digest, Sha256};

/// Hex-encoded sha256 of a lesson's text, used to detect stale cached
/// audio/questions after an edit.
pub fn hash_text(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_hash() {
        assert_eq!(hash_text("hello"), hash_text("hello"));
    }

    #[test]
    fn different_text_different_hash() {
        assert_ne!(hash_text("hello"), hash_text("world"));
    }
}
