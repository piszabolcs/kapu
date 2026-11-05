//! The maker's error: a message for the user, with the source it came from.

use std::fmt;

/// A failure of one step; the run reports it and goes on with the others
/// where it can (KD2: nothing is skipped silently).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl Error {
    /// An error with this message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// The same error with `context` in front (`file.pdf: ...`).
    #[must_use]
    pub fn context(self, context: &str) -> Self {
        Self(format!("{context}: {}", self.0))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<kapu_core::Error> for Error {
    fn from(e: kapu_core::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

/// The maker's result.
pub type Result<T> = std::result::Result<T, Error>;
