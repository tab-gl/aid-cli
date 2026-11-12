//! PKCE (RFC 7636, S256) and OAuth state generation.
//!
//! Byte-for-byte compatible with `src/auth/pkce.js`: 32 random bytes for the
//! verifier and 16 for the state, both base64url encoded without padding.

use sha2::{Digest, Sha256};

use crate::error::{AidError, Result};
use crate::util;

const VERIFIER_BYTES: usize = 32;
const STATE_BYTES: usize = 16;

fn random_bytes(len: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    getrandom::getrandom(&mut buffer)
        .map_err(|err| AidError::new(format!("Secure random source unavailable: {err}")))?;
    Ok(buffer)
}

/// Generate a PKCE code verifier (43 characters, base64url).
pub fn generate_verifier() -> Result<String> {
    Ok(util::base64url(&random_bytes(VERIFIER_BYTES)?))
}

/// Derive the S256 code challenge for a verifier.
pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    util::base64url(&digest)
}

/// Generate an unguessable OAuth `state` value.
pub fn generate_state() -> Result<String> {
    Ok(util::base64url(&random_bytes(STATE_BYTES)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_base64url_with_expected_length() {
        let verifier = generate_verifier().unwrap();
        assert_eq!(verifier.len(), 43);
        assert!(verifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'));
    }

    #[test]
    fn state_is_22_characters() {
        let state = generate_state().unwrap();
        assert_eq!(state.len(), 22);
    }

    #[test]
    fn generated_values_are_unique() {
        let first = generate_verifier().unwrap();
        let second = generate_verifier().unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn challenge_matches_rfc7636_appendix_b() {
        // RFC 7636 Appendix B test vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge_for(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
