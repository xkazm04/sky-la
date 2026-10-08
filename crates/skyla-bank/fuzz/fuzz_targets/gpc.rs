#![no_main]
//! Text straight into the gpc parser.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = skyla_bank::parse_gpc(text);
    }
});
