#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Basic memory safety fuzz: feed random bytes to JSON parser
    // If it deserializes as a block, we at least validate it doesn't crash
    if data.len() > 4 {
        let _ = serde_json::from_slice::<serde_json::Value>(data);
    }
});
