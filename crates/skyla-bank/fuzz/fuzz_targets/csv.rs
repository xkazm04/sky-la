#![no_main]
//! Text through the CSV reader with the built-in profile.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = skyla_bank::parse_csv(text, &skyla_bank::CsvProfile::fio());
    }
});
