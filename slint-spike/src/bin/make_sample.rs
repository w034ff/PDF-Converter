//! Writes `samples/pages-240.pdf`: 240 numbered pages, every seventh one in
//! landscape, made with `pdfconv-core`'s `PdfWriter` so that the licence of
//! the test file is this repository's.
//!
//! Usage: cargo run --bin make_sample [-- <pages> <out.pdf>]

use std::io::Cursor;

use image::{ImageFormat, Rgb, RgbImage};
use pdfconv_core::layout::PageSize;
use pdfconv_core::{PdfConformance, PdfWriter};

/// 3x5 digits, one row per line, bit 2 is the left column.
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn page_image(number: u32, landscape: bool) -> RgbImage {
    let (w, h) = if landscape { (594, 420) } else { (420, 594) };
    let hue = (number * 47) % 360;
    let tint = hsl_tint(hue);
    let mut img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
    // A coloured header band and "text" lines so pages look like documents.
    for y in 0..60 {
        for x in 0..w {
            img.put_pixel(x, y, tint);
        }
    }
    let mut y = 220;
    while y + 6 < h - 30 {
        let len = w - 80 - ((y * 13 + number * 7) % 120);
        for yy in y..y + 6 {
            for x in 40..40 + len {
                img.put_pixel(x, yy, Rgb([200, 200, 200]));
            }
        }
        y += 18;
    }
    let text = number.to_string();
    let scale = 18;
    let total = text.len() as u32 * 4 * scale;
    let mut x0 = (w - total) / 2;
    for ch in text.chars() {
        let d = DIGITS[ch.to_digit(10).unwrap_or(0) as usize];
        for (row, bits) in d.iter().enumerate() {
            for col in 0..3u32 {
                if bits & (4 >> col) != 0 {
                    for yy in 0..scale {
                        for xx in 0..scale {
                            img.put_pixel(x0 + col * scale + xx, 80 + row as u32 * scale + yy, Rgb([30, 30, 30]));
                        }
                    }
                }
            }
        }
        x0 += 4 * scale;
    }
    img
}

fn hsl_tint(hue: u32) -> Rgb<u8> {
    let h = hue as f32 / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let f = |c: f32| (150.0 + c * 90.0) as u8;
    Rgb([f(r), f(g), f(b)])
}

fn main() {
    let mut args = std::env::args().skip(1);
    let pages: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(240);
    let out = args.next().unwrap_or_else(|| format!("samples/pages-{pages}.pdf"));
    let mut writer = PdfWriter::new("slint-spike sample", PdfConformance::Standard);
    for n in 1..=pages {
        let img = page_image(n, n % 7 == 0);
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        writer.add_page(&png, PageSize::Fit).unwrap();
    }
    let bytes = writer.finish().unwrap();
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&out, bytes).unwrap();
    println!("wrote {out} ({pages} pages)");
}
