#![no_main]
use libfuzzer_sys::fuzz_target;
use pdfconv_worker::protocol::{Request, Response, read_message};

fuzz_target!(|data: &[u8]| {
    let _ = read_message::<_, Request>(&mut &data[..]);
    let _ = read_message::<_, Response>(&mut &data[..]);
});
