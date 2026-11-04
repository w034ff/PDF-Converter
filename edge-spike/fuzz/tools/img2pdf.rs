//! img2pdf IN OUT [pdfa]: runs the main-process image → PDF path on one file, timing each step.
use std::time::Instant;

use pdfconv_core::{PageSize, PdfConformance, PdfWriter};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let conf = if args.get(3).is_some_and(|a| a == "pdfa") { PdfConformance::PdfA2b } else { PdfConformance::Standard };
    let t = Instant::now();
    println!("probe (file): {:?} in {:?}", pdfconv_core::probe(std::path::Path::new(&args[1])), t.elapsed());
    let data = std::fs::read(&args[1]).expect("read");
    let t = Instant::now();
    println!("verify: {:?} in {:?}", pdfconv_core::verify_image(std::io::Cursor::new(&data), |_| {}), t.elapsed());
    let t = Instant::now();
    let mut w = PdfWriter::new("t", conf);
    println!("add: {:?} in {:?}", w.add_checked_image(data, PageSize::Fit, |_, _| {}, || false), t.elapsed());
    let t = Instant::now();
    match w.finish() {
        Ok(pdf) => { std::fs::write(&args[2], &pdf).expect("write"); println!("finish: {} bytes in {:?}", pdf.len(), t.elapsed()); }
        Err(e) => println!("finish: {e:?}"),
    }
}
