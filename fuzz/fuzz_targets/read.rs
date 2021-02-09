#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let file = Cursor::new(data);
    let _ = pmd_sir0::Sir0::new(file);
});
