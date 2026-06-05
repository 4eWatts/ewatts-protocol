#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // JSON parser memory safety fuzz
    // Catches panics from malformed JSON, excessive recursion, etc.
    let _ = serde_json::from_slice::<serde_json::Value>(data);
    
    // UTF-8 string handling
    if let Ok(s) = std::str::from_utf8(data) {
        let _: Vec<&str> = s.split(|c: char| c.is_whitespace()).collect();
    }
});
