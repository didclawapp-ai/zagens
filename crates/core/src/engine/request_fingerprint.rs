//! Request prefix fingerprints for KV-cache CI (kernel-v2 M5).
//!
//! Hashes byte sequences that define DeepSeek prefix-cache identity. Wire
//! assembly lives in the runtime sidecar; this module holds the shared types
//! and SHA-256 helpers.

use sha2::{Digest, Sha256};

/// Two-level fingerprint attached to diagnostics / turn metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFingerprint {
    /// System static layer (through compaction template) + tool catalog bytes.
    pub static_prefix_sha256: String,
    /// Full rendered prefix: complete system + tools + wire messages.
    pub full_prefix_sha256: String,
}

/// SHA-256 hex digest (lowercase, no prefix).
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[must_use]
pub fn compute_request_fingerprint(static_prefix: &[u8], full_prefix: &[u8]) -> RequestFingerprint {
    RequestFingerprint {
        static_prefix_sha256: sha256_hex(static_prefix),
        full_prefix_sha256: sha256_hex(full_prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_deterministic() {
        assert_eq!(sha256_hex(b"hello"), sha256_hex(b"hello"),);
        assert_ne!(sha256_hex(b"hello"), sha256_hex(b"world"));
    }

    #[test]
    fn compute_splits_static_and_full_layers() {
        let fp = compute_request_fingerprint(b"static", b"static+messages");
        assert_ne!(fp.static_prefix_sha256, fp.full_prefix_sha256);
        assert_eq!(
            fp.static_prefix_sha256,
            compute_request_fingerprint(b"static", b"static").static_prefix_sha256,
        );
    }
}
