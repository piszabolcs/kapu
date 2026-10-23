//! Any bytes into the 512-byte header parser: must return Ok or Err, never panic.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = kapu_core::Header::parse(data);
});
