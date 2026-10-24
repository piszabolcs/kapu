//! KAPU core: reading, validating, building and searching `.kapu` files.
//!
//! The on-disk format is defined by `SPEC-KAPU.md` v1.0. The body of a file is the
//! single source of truth; every other section is derived from it by the builder
//! and must be reproduced byte for byte (NFR-02).
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod about;
pub mod build;
pub mod catalog;
pub mod docs;
pub mod edit;
pub mod embed;
pub mod error;
pub mod evaluate;
pub mod header;
pub mod leiden;
pub mod reader;
pub mod scan;
pub mod search;
pub mod sections;
pub mod terms;
pub mod text;
pub mod validate;
pub mod view;
pub mod yake;

pub use docs::{DocEntry, blake3_of, served_text, strip_comments, unescape};
pub use error::{Error, ErrorCode, Result};
pub use header::{Hash, Header, Span};
pub use reader::{
    Described, Neighbour, Opened, OpenedDoor, Part, describe_bytes, describe_in, frame,
    frame_with_boundary, new_boundary, open_bytes, open_checked, open_path, parts,
};
pub use sections::{BodyLength, CheckedFile, Limits, Sections};
pub use validate::{Level, Report, is_valid_id, validate, validate_with};

/// Format version written on the first header line (`%KAPU 1.0`, FR-03).
pub const FORMAT_VERSION: &str = "1.0";

/// Exact size of the file header in bytes (FR-03).
pub const HEADER_LEN: usize = 512;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_exactly_512_bytes() {
        assert_eq!(HEADER_LEN, 512);
    }

    #[test]
    fn format_version_matches_spec() {
        assert_eq!(FORMAT_VERSION, "1.0");
    }
}
