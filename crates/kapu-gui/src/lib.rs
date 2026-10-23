//! KAPU desktop window (M10): the command layer and the reader's Markdown
//! rendering. The window itself is `src/main.rs` (Tauri); the interface is in
//! `ui/`, plain HTML, CSS and JavaScript.
#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod api;
pub mod markdown;
