//! Spike: render every page of a PDF with pdfium or hayro and report timings.
//!
//! Usage: render-bench <pdfium|hayro> <pdf> <out-dir> [dpi]
//! Prints one line per page: "page N WxH ms" or "error ...".

use std::path::Path;
use std::time::Instant;

const POINTS_PER_INCH: f32 = 72.0;
const DEFAULT_DPI: f32 = 150.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let backend = &args[1];
    let pdf_path = Path::new(&args[2]);
    let out_dir = Path::new(&args[3]);
    let dpi: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_DPI);
    std::fs::create_dir_all(out_dir).unwrap();
    let bytes = std::fs::read(pdf_path).unwrap();
    let scale = dpi / POINTS_PER_INCH;
    let start = Instant::now();
    match backend.as_str() {
        "pdfium" => pdfium(&bytes, out_dir, scale),
        "hayro" => hayro(bytes, out_dir, scale),
        other => panic!("unknown backend {other}"),
    }
    println!("total {} ms", start.elapsed().as_millis());
}

fn pdfium(bytes: &[u8], out_dir: &Path, scale: f32) {
    use pdfium_render::prelude::*;
    let lib = std::env::var("PDFIUM_LIB_DIR").expect("PDFIUM_LIB_DIR");
    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib)).unwrap(),
    );
    let doc = match pdfium.load_pdf_from_byte_slice(bytes, None) {
        Ok(d) => d,
        Err(e) => {
            println!("error open: {e:?}");
            return;
        }
    };
    let config = PdfRenderConfig::new().scale_page_by_factor(scale);
    for (i, page) in doc.pages().iter().enumerate() {
        let t = Instant::now();
        match page.render_with_config(&config).and_then(|b| b.as_image()) {
            Ok(img) => {
                let ms = t.elapsed().as_millis();
                println!("page {} {}x{} {ms} ms", i + 1, img.width(), img.height());
                img.save(out_dir.join(format!("p{:03}.png", i + 1))).unwrap();
            }
            Err(e) => println!("error page {}: {e:?}", i + 1),
        }
    }
}

fn hayro(bytes: Vec<u8>, out_dir: &Path, scale: f32) {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{RenderCache, RenderSettings, render};
    let pdf = match Pdf::new(bytes) {
        Ok(p) => p,
        Err(e) => {
            println!("error open: {e:?}");
            return;
        }
    };
    let settings = RenderSettings { x_scale: scale, y_scale: scale, bg_color: WHITE, ..Default::default() };
    let interp = InterpreterSettings::default();
    let cache = RenderCache::new();
    for (i, page) in pdf.pages().iter().enumerate() {
        let t = Instant::now();
        let pixmap = render(page, &cache, &interp, &settings);
        let ms = t.elapsed().as_millis();
        println!("page {} {}x{} {ms} ms", i + 1, pixmap.width(), pixmap.height());
        std::fs::write(out_dir.join(format!("p{:03}.png", i + 1)), pixmap.into_png().unwrap()).unwrap();
    }
}
