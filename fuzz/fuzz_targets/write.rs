#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: Vec<u32>| {
    let mut file = Cursor::new(vec![0; 1024]);
    let _ = pmd_sir0::write_sir0_footer(&mut file, &data);
});
