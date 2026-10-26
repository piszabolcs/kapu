//! `#VEC` sections (FR-10/6, D143) on arbitrary text: parsing and scoring
//! never panic, whatever the door count.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&doors, rest)) = data.split_first() else {
        return;
    };
    if let Ok(text) = std::str::from_utf8(rest)
        && let Ok(signs) = kapu_core::embed::SignVectors::parse(text, usize::from(doors))
    {
        let query = vec![0.5f32; signs.dim().min(4096)];
        let _ = signs.best_scores(&query);
    }
});
