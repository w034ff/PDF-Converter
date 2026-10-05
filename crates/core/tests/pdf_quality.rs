//! Quality and conformance tests for PDF writing (design §11.2).
//!
//! Because pdfium does not support multithreaded usage within the same process,
//! all tests that invoke pdfium run sequentially within a single `#[test]` function.

use std::path::PathBuf;

use image::Rgb;
use pdfconv_core::probe::probe_reader;
use pdfconv_core::{POINTS_PER_INCH, PageSize, PdfWriter, ProbeError, calculate_layout};
use pdfium_render::prelude::*;

/// Tolerance for pixel value differences when comparing rendered image with original (design §11.2).
const COLOR_TOLERANCE_IMAGE: u8 = 8;

/// Maximum allowable ratio of mismatched pixels (design §11.2).
const MAX_MISMATCH_RATIO_IMAGE: f64 = 0.001; // 0.1%

/// Tolerance for quadrant center colors in the A4 orientation test (design §11.2).
const COLOR_TOLERANCE_RENDER: u8 = 32;

/// DPI used for rendering A4 pages.
const RENDER_DPI_A4: f32 = 150.0;

fn library_dir() -> PathBuf {
    let os_dir = if cfg!(windows) {
        "windows/bin"
    } else {
        "linux/lib"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/pdfium")
        .join(os_dir)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Renders a pdfium page with the given configuration to an RGB8 image.
fn render_page_to_rgb(page: &PdfPage<'_>, config: &PdfRenderConfig) -> image::RgbImage {
    let bitmap = page
        .render_with_config(config)
        .expect("rendering page with config");
    let width = bitmap.width() as u32;
    let height = bitmap.height() as u32;
    let rgba_bytes = bitmap.as_rgba_bytes();
    let rgba = image::RgbaImage::from_raw(width, height, rgba_bytes)
        .expect("creating RgbaImage from bitmap rgba bytes");
    image::DynamicImage::ImageRgba8(rgba).to_rgb8()
}

/// Composites an RGBA image onto a solid white background.
fn composite_over_white(rgba: &image::RgbaImage) -> image::RgbImage {
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let a = a as u32;
        let r = ((r as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        let g = ((g as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        let b = ((b as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        rgb.put_pixel(x, y, Rgb([r, g, b]));
    }
    rgb
}

#[test]
fn pdf_quality_and_conformance_suite() {
    let lib_dir = library_dir();
    let lib_path = Pdfium::pdfium_platform_library_name_at_path(&lib_dir);
    let bindings = Pdfium::bind_to_library(&lib_path)
        .unwrap_or_else(|e| panic!("failed to bind to pdfium at {lib_path:?}: {e:?}"));
    let pdfium = Pdfium::new(bindings);
    let fix_dir = fixtures_dir();

    // -----------------------------------------------------------------------
    // 1. "Fit" quality test across all supported image fixtures (except CMYK)
    // -----------------------------------------------------------------------
    let fit_fixtures = [
        "logo_alpha.png",
        "logo_alpha.webp",
        "photo.jpg",
        "rotate0.jpg",
        "flip_h.jpg",
        "rotate180.jpg",
        "flip_v.jpg",
        "rotate90_flip_h.jpg",
        "rotate90.jpg",
        "rotate270_flip_h.jpg",
        "rotate270.jpg",
        "deep16.png",
        "opaque.bmp",
        "alpha32.bmp",
        "dpi300.png",
        "dpi300.jpg",
    ];

    println!("\n--- [Fit] Quality Test Results ---");
    for name in fit_fixtures {
        let path = fix_dir.join(name);
        let raw_bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {name}: {e:?}"));

        let info = probe_reader(std::io::Cursor::new(&raw_bytes))
            .unwrap_or_else(|e| panic!("failed to probe {name}: {e:?}"));

        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
        writer
            .add_page(&raw_bytes, PageSize::Fit)
            .unwrap_or_else(|e| panic!("failed to add page for {name}: {e:?}"));
        let pdf_bytes = writer
            .finish()
            .unwrap_or_else(|e| panic!("failed to finish PDF for {name}: {e:?}"));

        let doc = pdfium
            .load_pdf_from_byte_slice(&pdf_bytes, None)
            .unwrap_or_else(|e| panic!("failed to load generated PDF for {name}: {e:?}"));
        let page = doc
            .pages()
            .get(0)
            .unwrap_or_else(|e| panic!("failed to get page 0 for {name}: {e:?}"));

        // Render at native image DPI
        let scale = info.dpi as f32 / POINTS_PER_INCH;
        let render_config = PdfRenderConfig::new().scale_page_by_factor(scale);
        let rendered_image = render_page_to_rgb(&page, &render_config);

        // Build expected image: decode, apply EXIF orientation, and composite over white.
        let mut expected_dyn = image::load_from_memory(&raw_bytes)
            .unwrap_or_else(|e| panic!("failed to load expected image {name}: {e:?}"));
        expected_dyn.apply_orientation(info.orientation);
        let expected_rgb = composite_over_white(&expected_dyn.to_rgba8());

        assert_eq!(
            rendered_image.dimensions(),
            expected_rgb.dimensions(),
            "rendered dimensions mismatch for {name}: rendered={:?}, expected={:?}",
            rendered_image.dimensions(),
            expected_rgb.dimensions()
        );

        let mut mismatch_count = 0usize;
        let mut max_diff: u8 = 0;
        let total_pixels = (rendered_image.width() * rendered_image.height()) as usize;

        for (p_act, p_exp) in rendered_image.pixels().zip(expected_rgb.pixels()) {
            let dr = (p_act[0] as i16 - p_exp[0] as i16).unsigned_abs() as u8;
            let dg = (p_act[1] as i16 - p_exp[1] as i16).unsigned_abs() as u8;
            let db = (p_act[2] as i16 - p_exp[2] as i16).unsigned_abs() as u8;
            let diff = dr.max(dg).max(db);
            if diff > max_diff {
                max_diff = diff;
            }
            if diff > COLOR_TOLERANCE_IMAGE {
                mismatch_count += 1;
            }
        }

        let mismatch_ratio = mismatch_count as f64 / total_pixels as f64;
        let mismatch_percent = mismatch_ratio * 100.0;
        println!(
            "{name:<20}: mismatch = {mismatch_percent:>6.4}%, max_diff = {max_diff:>2} (tolerance = {COLOR_TOLERANCE_IMAGE})"
        );

        assert!(
            mismatch_ratio <= MAX_MISMATCH_RATIO_IMAGE,
            "{name}: mismatch ratio {mismatch_percent:.4}% exceeds limit {:.4}%",
            MAX_MISMATCH_RATIO_IMAGE * 100.0
        );
    }

    // -----------------------------------------------------------------------
    // 2. A4 placement and orientation test (11 white-pixel-free fixtures)
    // -----------------------------------------------------------------------
    let a4_fixtures = [
        "rotate0.jpg",
        "flip_h.jpg",
        "rotate180.jpg",
        "flip_v.jpg",
        "rotate90_flip_h.jpg",
        "rotate90.jpg",
        "rotate270_flip_h.jpg",
        "rotate270.jpg",
        "dpi300.png",
        "dpi300.jpg",
        "opaque.bmp",
    ];

    println!("\n--- [A4] Placement Test Results ---");
    let scale_a4 = RENDER_DPI_A4 / POINTS_PER_INCH;

    for name in a4_fixtures {
        let path = fix_dir.join(name);
        let raw_bytes = std::fs::read(&path).unwrap();
        let info = probe_reader(std::io::Cursor::new(&raw_bytes)).unwrap();

        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
        writer.add_page(&raw_bytes, PageSize::A4).unwrap();
        let pdf_bytes = writer.finish().unwrap();

        let doc = pdfium.load_pdf_from_byte_slice(&pdf_bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();

        let render_config = PdfRenderConfig::new().scale_page_by_factor(scale_a4);
        let rendered_image = render_page_to_rgb(&page, &render_config);

        let layout = calculate_layout(&info, PageSize::A4);

        // Expected bounding box on the rendered pixel grid:
        // Start is rounded down (floor), end is rounded up (ceil) (design §11.2).
        let exp_min_x = (layout.image_rect.x * scale_a4).floor() as u32;
        let exp_min_y = (layout.image_rect.y * scale_a4).floor() as u32;
        let exp_max_x = ((layout.image_rect.x + layout.image_rect.width) * scale_a4).ceil() as u32;
        let exp_max_y = ((layout.image_rect.y + layout.image_rect.height) * scale_a4).ceil() as u32;

        // Measure actual non-white bounding box (RGB any < 250)
        let mut act_min_x = u32::MAX;
        let mut act_min_y = u32::MAX;
        let mut act_max_x = 0u32;
        let mut act_max_y = 0u32;

        for (x, y, pixel) in rendered_image.enumerate_pixels() {
            let [r, g, b] = pixel.0;
            if r < 250 || g < 250 || b < 250 {
                act_min_x = act_min_x.min(x);
                act_min_y = act_min_y.min(y);
                act_max_x = act_max_x.max(x);
                act_max_y = act_max_y.max(y);
            }
        }

        // End pixel coordinate (exclusive) is max_x + 1
        let act_end_x = act_max_x + 1;
        let act_end_y = act_max_y + 1;

        println!(
            "{name:<20}: actual [{act_min_x}, {act_min_y}, {act_end_x}, {act_end_y}] vs expected [{exp_min_x}, {exp_min_y}, {exp_max_x}, {exp_max_y}]"
        );

        assert!(
            (act_min_x as i64 - exp_min_x as i64).abs() <= 1,
            "{name}: min_x diff too large: act={act_min_x}, exp={exp_min_x}"
        );
        assert!(
            (act_min_y as i64 - exp_min_y as i64).abs() <= 1,
            "{name}: min_y diff too large: act={act_min_y}, exp={exp_min_y}"
        );
        assert!(
            (act_end_x as i64 - exp_max_x as i64).abs() <= 1,
            "{name}: max_x diff too large: act={act_end_x}, exp={exp_max_x}"
        );
        assert!(
            (act_end_y as i64 - exp_max_y as i64).abs() <= 1,
            "{name}: max_y diff too large: act={act_end_y}, exp={exp_max_y}"
        );

        // For the 8 orientation fixtures, verify quadrant center colors
        if name.starts_with("rotate") || name.starts_with("flip") {
            // Displayed dimensions are 160 x 96.
            // Centers of the 4 quadrants:
            // Top-left: (40, 24) -> RED [220, 40, 40]
            // Top-right: (120, 24) -> GREEN [40, 170, 90]
            // Bottom-left: (40, 72) -> BLUE [40, 80, 200]
            // Bottom-right: (120, 72) -> YELLOW [250, 210, 0]
            let quadrants = [
                ((40.0, 24.0), [220u8, 40, 40], "top-left (red)"),
                ((120.0, 24.0), [40, 170, 90], "top-right (green)"),
                ((40.0, 72.0), [40, 80, 200], "bottom-left (blue)"),
                ((120.0, 72.0), [250, 210, 0], "bottom-right (yellow)"),
            ];

            for &((disp_cx, disp_cy), expected_color, label) in &quadrants {
                let pt_x = layout.image_rect.x + (disp_cx / 160.0) * layout.image_rect.width;
                let pt_y = layout.image_rect.y + (disp_cy / 96.0) * layout.image_rect.height;
                let px_x = (pt_x * scale_a4).round() as u32;
                let px_y = (pt_y * scale_a4).round() as u32;

                let actual_pixel = rendered_image.get_pixel(px_x, px_y).0;
                for c in 0..3 {
                    let diff =
                        (actual_pixel[c] as i16 - expected_color[c] as i16).unsigned_abs() as u8;
                    assert!(
                        diff <= COLOR_TOLERANCE_RENDER,
                        "{name} {label} channel {c} mismatch: actual={actual_pixel:?}, expected={expected_color:?}, diff={diff}"
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // 3. corrupt.png fails with DecodeFailed; writer can continue afterwards
    // -----------------------------------------------------------------------
    let corrupt_bytes = std::fs::read(fix_dir.join("corrupt.png")).unwrap();
    let mut corrupt_writer = PdfWriter::new("PDF Converter 0.1.0");

    let err = corrupt_writer
        .add_page(&corrupt_bytes, PageSize::Fit)
        .unwrap_err();
    assert_eq!(err, ProbeError::DecodeFailed);

    // Document is still clean and can accept subsequent pages
    let valid_bytes = std::fs::read(fix_dir.join("rotate0.jpg")).unwrap();
    corrupt_writer
        .add_page(&valid_bytes, PageSize::Fit)
        .expect("should add valid page after corrupt page failure");

    let recovered_pdf = corrupt_writer.finish().expect("should finish PDF");
    let recovered_doc = pdfium
        .load_pdf_from_byte_slice(&recovered_pdf, None)
        .expect("should load recovered PDF");
    assert_eq!(recovered_doc.pages().len(), 1);
}

#[test]
fn jpeg_bytes_are_embedded_unchanged_including_cmyk() {
    let fix_dir = fixtures_dir();
    let jpegs = ["photo.jpg", "photo_cmyk.jpg", "rotate0.jpg", "flip_h.jpg"];

    for name in jpegs {
        let raw_bytes = std::fs::read(fix_dir.join(name)).unwrap();
        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
        writer.add_page(&raw_bytes, PageSize::Fit).unwrap();
        let pdf_bytes = writer.finish().unwrap();

        // Verify the exact original JPEG bytes exist as a contiguous substring in the PDF
        let found = pdf_bytes
            .windows(raw_bytes.len())
            .any(|window| window == raw_bytes.as_slice());
        assert!(
            found,
            "original JPEG bytes of {name} were not found verbatim in the generated PDF"
        );
    }
}

#[test]
fn pdf_metadata_contains_only_creator_and_no_file_names() {
    let fix_dir = fixtures_dir();
    let creator = "PDF Converter 0.1.0";
    let mut writer = PdfWriter::new(creator);

    let raw_bytes = std::fs::read(fix_dir.join("photo.jpg")).unwrap();
    writer.add_page(&raw_bytes, PageSize::Fit).unwrap();
    let pdf_bytes = writer.finish().unwrap();
    let pdf_str = String::from_utf8_lossy(&pdf_bytes);

    // /Creator must be present with the exact creator string
    assert!(
        pdf_str.contains("/Creator"),
        "PDF dictionary should contain /Creator"
    );
    assert!(
        pdf_str.contains(creator),
        "PDF should contain creator tool string"
    );

    // Other standard metadata keys must not be present
    assert!(!pdf_str.contains("/Title"), "PDF should not contain /Title");
    assert!(
        !pdf_str.contains("/Author"),
        "PDF should not contain /Author"
    );
    assert!(
        !pdf_str.contains("/Subject"),
        "PDF should not contain /Subject"
    );
    assert!(
        !pdf_str.contains("/Keywords"),
        "PDF should not contain /Keywords"
    );

    // File name and arbitrary external path must not appear anywhere in the output
    assert!(
        !pdf_bytes
            .windows(b"photo.jpg".len())
            .any(|w| w == b"photo.jpg"),
        "PDF should not contain input filename photo.jpg"
    );
    assert!(
        !pdf_bytes
            .windows(b"secret_input_path".len())
            .any(|w| w == b"secret_input_path"),
        "PDF should not leak input paths"
    );
}
