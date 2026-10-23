//! Whole file through the reader: every door listed in #DOCS plus an unknown
//! id, so the fast path, the stale rebuild and the per-id errors all run.
#![no_main]
use kapu_core::{BodyLength, Limits, Sections, describe_bytes, frame, open_bytes};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let ids: Vec<String> = Sections::parse_with(data, &Limits::default(), BodyLength::Tolerate)
        .ok()
        .and_then(|s| s.doc_entries().ok().map(|e| e.iter().map(|d| d.id.to_owned()).collect()))
        .unwrap_or_default();
    let mut refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    refs.push("nincs-ilyen");
    refs.push("a/b");
    if let Ok(opened) = open_bytes(data, &refs) {
        for door in opened.doors.iter().flatten() {
            let _ = frame("f.kapu", door);
        }
    }
    let _ = describe_bytes(data, &refs);
});
