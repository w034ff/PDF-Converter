//! Everything the main process does with an image file: probe, pixel check (done in the worker
//! in the app, but it is the same code), PDF writing and the thumbnail.
#![no_main]
use std::io::Cursor;

use libfuzzer_sys::fuzz_target;
use pdfconv_core::{PageSize, PaperSize, PageOrientation, PdfConformance, PdfWriter, probe_reader, thumbnail_png, verify_image};

fuzz_target!(|data: &[u8]| {
    let Ok(info) = probe_reader(Cursor::new(data)) else { return };
    // Keep each run under the fuzzer's memory limit; the app allows up to MAX_IMAGE_PIXELS.
    if u64::from(info.width) * u64::from(info.height) > 4_000_000 {
        return;
    }
    if verify_image(Cursor::new(data), |_| {}).is_err() {
        return;
    }
    let _ = thumbnail_png(Cursor::new(data));
    for (conf, size) in [
        (PdfConformance::Standard, PageSize::Fit),
        (PdfConformance::PdfA2b, PageSize::Paper(PaperSize::A4, PageOrientation::Auto)),
    ] {
        let mut w = PdfWriter::new("fuzz", conf);
        if w.add_checked_image(data.to_vec(), size, |_, _| {}, || false).is_ok() {
            // The pixels passed verify_image, so the app reports a failure here as
            // WriteFailed, which design §4.3 says must not happen.
            if let Err(e) = w.finish() {
                panic!("finish failed after the pixel check passed ({conf:?}): {e:?}");
            }
        }
    }
});
