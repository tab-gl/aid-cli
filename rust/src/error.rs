//! Structured error type.
//!
//! v1 surfaced user-facing failures as plain `Error` messages. v2 keeps the
//! exact same user-visible text but models the failure as one type so commands
//! can return early without `panic!` or stringly-typed plumbing.

use std::fmt;

/// A user-facing CLI failure.
///
/// The message is printed verbatim (after the `✗` marker) and the process exits
/// with status 1, matching v1 behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AidError {
    message: String,
}

impl AidError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AidError {}

impl From<std::io::Error> for AidError {
    fn from(err: std::io::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl From<serde_json::Error> for AidError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AidError>;

/// Shorthand for building an `Err(AidError)` with a formatted message.
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::AidError::new(format!($($arg)*)))
    };
}

/// Shorthand for building an `AidError` value.
#[macro_export]
macro_rules! fail {
    ($($arg:tt)*) => {
        $crate::error::AidError::new(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_message() {
        let err = AidError::new("Unknown command: nope");
        assert_eq!(err.to_string(), "Unknown command: nope");
        assert_eq!(err.message(), "Unknown command: nope");
    }
}
