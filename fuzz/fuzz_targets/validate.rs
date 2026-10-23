//! Whole file through the validator at both levels.
#![no_main]
use kapu_core::{Level, validate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = validate(data, Level::Basic);
    let _ = validate(data, Level::Full);
});
