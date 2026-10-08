#![no_main]
//! Any bytes, through detection and the chosen parser.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = skyla_bank::detect(data);
    let _ = skyla_bank::parse(data, Some(&skyla_bank::CsvProfile::fio()));
});
