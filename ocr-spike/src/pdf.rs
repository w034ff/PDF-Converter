//! Item 7: a scanned-like PDF (one JPEG per page) rendered with pdfium, the
//! way the worker would feed OCR.

use std::path::Path;

use pdfium_render::prelude::*;

use crate::imgops::Bgr;

/// A one-page PDF whose page is `page_pt` points and shows `jpeg` (w × h px).
pub fn image_pdf(jpeg: &[u8], w: u32, h: u32, page_pt: (f64, f64)) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    let content = format!("q {} 0 0 {} 0 0 cm /Im0 Do Q", page_pt.0, page_pt.1);
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>",
            page_pt.0, page_pt.1
        )
        .into_bytes(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.into_bytes(), b"\nendstream".to_vec()].concat(),
        [
            format!(
                "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                jpeg.len()
            )
            .into_bytes(),
            jpeg.to_vec(),
            b"\nendstream".to_vec(),
        ]
        .concat(),
    ];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// Renders page 1 of `pdf` with its long side at `long_side` px; returns the
/// image and the page size in points.
pub fn render_first_page(pdfium_lib: &Path, pdf: &[u8], long_side: u32) -> (Bgr, (f64, f64)) {
    let pdfium = Pdfium::new(Pdfium::bind_to_library(pdfium_lib).expect("load pdfium"));
    let doc = pdfium.load_pdf_from_byte_slice(pdf, None).expect("open pdf");
    let page = doc.pages().get(0).expect("page 1");
    let (wpt, hpt) = (f64::from(page.width().value), f64::from(page.height().value));
    let s = f64::from(long_side) / wpt.max(hpt);
    let (w, h) = ((wpt * s).round() as i32, (hpt * s).round() as i32);
    let cfg = PdfRenderConfig::new().set_target_size(w, h).set_clear_color(PdfColor::WHITE);
    let bitmap = page.render_with_config(&cfg).expect("render");
    let rgba = bitmap.as_rgba_bytes();
    let mut img = Bgr::new(w as usize, h as usize);
    for (i, p) in rgba.chunks_exact(4).enumerate() {
        img.data[i * 3..i * 3 + 3].copy_from_slice(&[p[2], p[1], p[0]]);
    }
    (img, (wpt, hpt))
}
