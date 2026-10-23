//! Error codes of the format (FR-20) and the single error type of the crate.
//!
//! Every failure in `kapu-core` is a [`Error`] that carries one of the spec's
//! error codes plus a human-readable detail. The CLI prints the code and exits
//! non-zero (spec chapter 8); the MCP server reports it per id (FR-15/4).

use std::fmt;

/// The validation error codes of SPEC-KAPU.md FR-20. The numeric part is
/// stable and part of the CLI's machine-readable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCode {
    /// Missing or wrong magic line (`%KAPU 1.0`).
    E001,
    /// The header is not 512 bytes, does not end with LF, or is malformed.
    E002,
    /// A CR byte somewhere in the file.
    E003,
    /// A forbidden character (FR-02), including invalid UTF-8.
    E004,
    /// A section overruns, overlaps or is missing.
    E005,
    /// `bodyhash` does not match the body.
    E006,
    /// A door's fingerprint does not match its bytes.
    E007,
    /// Duplicate or malformed identifier.
    E008,
    /// Reference to a door that does not exist.
    E009,
    /// A caption above its hard limit.
    E010,
    /// Unknown hash prefix (only `b3:` exists in v1).
    E011,
    /// Environment error, not a fault of the `.kapu` file: a file that cannot
    /// be read or written, a missing or broken vector model, a malformed
    /// question file (D109).
    E012,
}

impl ErrorCode {
    /// The code as written in output, e.g. `"E004"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E001 => "E001",
            Self::E002 => "E002",
            Self::E003 => "E003",
            Self::E004 => "E004",
            Self::E005 => "E005",
            Self::E006 => "E006",
            Self::E007 => "E007",
            Self::E008 => "E008",
            Self::E009 => "E009",
            Self::E010 => "E010",
            Self::E011 => "E011",
            Self::E012 => "E012",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The error type of `kapu-core`: a spec error code and a detail message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Which FR-20 rule was violated.
    pub code: ErrorCode,
    /// What exactly was wrong and where (byte offset, field name, id).
    pub detail: String,
}

impl Error {
    /// Creates an error with the given code and detail.
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for Error {}

/// Shorthand used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
