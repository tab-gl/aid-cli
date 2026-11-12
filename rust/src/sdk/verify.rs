//! Artifact integrity verification.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::Result;

/// Pinned SHA-256 of the Android SDK release artifact (`library-release.aar`).
///
/// Kept identical to v1 so the same trusted artifact is installed by both CLIs.
pub const ANDROID_SDK_SHA256: &str =
    "8fde79fd7690f6b09c1198fec08ff160f5ecaf54301f14b88c4f43de90ebdba2";

pub fn sha256_bytes(bytes: &[u8]) -> String {
    crate::util::hex(&Sha256::digest(bytes))
}

/// Stream a file through SHA-256 so large artifacts stay memory-light.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(crate::util::hex(&hasher.finalize()))
}

/// True when the file's digest matches the pinned Android SDK digest.
pub fn verify_android_artifact(path: &Path) -> Result<bool> {
    Ok(sha256_file(path)? == ANDROID_SDK_SHA256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_known_vectors() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn file_and_byte_digests_agree() {
        let path = std::env::temp_dir().join(format!("ace-sha-{}.bin", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(b"abc"));
        assert!(!verify_android_artifact(&path).unwrap());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn pinned_digest_is_a_sha256_hex_string() {
        assert_eq!(ANDROID_SDK_SHA256.len(), 64);
        assert!(ANDROID_SDK_SHA256.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
