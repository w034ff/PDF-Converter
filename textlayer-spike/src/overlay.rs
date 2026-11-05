//! Item 5: a PNG of the page as pdfium draws it, with pdfium's loose
//! character boxes (the selection highlight) filled in translucent blue and
//! the OCR character quads outlined in red, to judge the highlight by eye.

use std::path::Path;

use image::{Rgba, RgbaImage};
use pdfium_render::prelude::*;

use crate::check::pdfium_chars;
use crate::types::OcrPage;

pub fn draw(pdfium: &Pdfium, pdf: &Path, ocr: &OcrPage, out: &Path, scale: f32) {
    let doc = pdfium.load_pdf_from_file(pdf, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let bmp = page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(scale)).unwrap();
    let (w, h) = (bmp.width() as u32, bmp.height() as u32);
    let mut img = RgbaImage::from_raw(w, h, bmp.as_rgba_bytes()).unwrap();
    let s = scale as f64;
    let (_, chars) = pdfium_chars(&page);
    for c in &chars {
        let [x0, y0, x1, y1] = c.bbox.map(|v| v * s);
        for y in y0.max(0.0) as u32..(y1 as u32).min(h) {
            for x in x0.max(0.0) as u32..(x1 as u32).min(w) {
                let p = img.get_pixel_mut(x, y);
                // 35 % blue, like a viewer's highlight.
                p.0[0] = (p.0[0] as f32 * 0.65) as u8;
                p.0[1] = (p.0[1] as f32 * 0.65 + 0.35 * 120.0) as u8;
                p.0[2] = (p.0[2] as f32 * 0.65 + 0.35 * 255.0) as u8;
            }
        }
    }
    for c in ocr.chars() {
        let pts: Vec<imageproc::point::Point<f32>> = c.quad.iter().map(|p| imageproc::point::Point::new((p[0] * s) as f32, (p[1] * s) as f32)).collect();
        for k in 0..4 {
            let (a, b) = (pts[k], pts[(k + 1) % 4]);
            imageproc::drawing::draw_line_segment_mut(&mut img, (a.x, a.y), (b.x, b.y), Rgba([230, 0, 0, 255]));
        }
    }
    img.save(out).unwrap();
}
