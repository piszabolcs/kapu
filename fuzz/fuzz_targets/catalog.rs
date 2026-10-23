//! `#MAP`, `#DESC` and `#GRAPH` line parsers on arbitrary text.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = kapu_core::catalog::parse_map(text);
        let _ = kapu_core::catalog::parse_desc(text);
        let _ = kapu_core::catalog::parse_graph(text);
    }
});
