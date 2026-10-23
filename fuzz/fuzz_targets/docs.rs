//! `#DOCS` lines, escape and comment stripping on arbitrary text.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = kapu_core::docs::parse_docs(text);
        let _ = kapu_core::unescape(text);
        let _ = kapu_core::strip_comments(text);
    }
    let _ = kapu_core::served_text(data);
});
