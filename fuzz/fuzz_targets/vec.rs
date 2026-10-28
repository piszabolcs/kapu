//! `#VEC` sections (FR-10/6-7, D143, D171) on arbitrary text, sign bits and 4-
//! or 8-bit integers: parsing and scoring never panic, whatever the door count.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&doors, rest)) = data.split_first() else {
        return;
    };
    if let Ok(text) = std::str::from_utf8(rest)
        && let Ok(vectors) = kapu_core::embed::FileVectors::parse(text, usize::from(doors))
    {
        let query = vec![0.5f32; vectors.dim().min(4096)];
        let _ = vectors.best_scores(&query);
    }
});
