#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(out) = pdfconv_core::strip_jpeg_metadata(data) {
        // Stripping is idempotent and never grows the data.
        assert!(out.len() <= data.len(), "strip grew the data");
        let again = pdfconv_core::strip_jpeg_metadata(&out).expect("stripped output must parse");
        assert_eq!(again, out, "strip is not idempotent");
    }
});
