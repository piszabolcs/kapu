//! Whole file through the section reader, strict and tolerant, with tight limits
//! too, so the limit checks themselves are exercised.
#![no_main]
use kapu_core::{BodyLength, Limits, Sections};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = Sections::parse(data);
    let tight = Limits { max_section: 4096, max_line: 256 };
    if let Ok(s) = Sections::parse_with(data, &tight, BodyLength::Tolerate) {
        let _ = s.check_bodyhash();
        if let Ok(entries) = s.doc_entries() {
            for e in &entries {
                let _ = s.stored(e);
            }
        }
    }
});
