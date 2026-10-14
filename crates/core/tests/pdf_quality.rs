//! Quality and conformance tests for PDF writing (design §11.2).
//!
//! Because pdfium does not support multithreaded usage within the same process,
//! all tests that invoke pdfium run sequentially within a single `#[test]` function.

use std::path::{Path, PathBuf};

use image::Rgb;
use pdfconv_core::probe::probe_reader;
use pdfconv_core::{
    IMAGE_EXTENSIONS, POINTS_PER_INCH, PageLayout, PageOrientation, PageSize, PaperSize,
    PdfConformance, PdfWriter, ProbeError, calculate_layout,
};
use pdfium_render::prelude::*;

#[allow(dead_code)]
#[path = "../examples/gen_fixtures.rs"]
mod gen_fixtures;

/// Tolerance for pixel value differences when comparing rendered image with original (design §11.2).
const COLOR_TOLERANCE_IMAGE: u8 = 8;

/// Maximum allowable ratio of mismatched pixels (design §11.2).
const MAX_MISMATCH_RATIO_IMAGE: f64 = 0.001; // 0.1%

/// Tolerance for quadrant center colors in the A4 orientation test (design §11.2).
const COLOR_TOLERANCE_RENDER: u8 = 32;

/// Threshold below which an RGB channel is considered non-white (design §11.2).
const NON_WHITE_THRESHOLD: u8 = 250;

/// Maximum allowable pixel coordinate discrepancy for image bounding boxes (design §11.2).
const PLACEMENT_TOLERANCE_PX: i64 = 1;

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

/// Compares two images with a tolerance per pixel and a maximum mismatch ratio.
fn assert_images_match(
    context: &str,
    actual: &image::RgbImage,
    expected: &image::RgbImage,
    tolerance: u8,
    max_mismatch_ratio: f64,
) {
    assert_eq!(
        actual.dimensions(),
        expected.dimensions(),
        "{context}: dimensions mismatch: actual={:?}, expected={:?}",
        actual.dimensions(),
        expected.dimensions()
    );
    let mut mismatch_count = 0usize;
    let mut max_diff = 0u8;
    let total_pixels = (actual.width() * actual.height()) as usize;
    for (p_act, p_exp) in actual.pixels().zip(expected.pixels()) {
        let dr = (p_act[0] as i16 - p_exp[0] as i16).unsigned_abs() as u8;
        let dg = (p_act[1] as i16 - p_exp[1] as i16).unsigned_abs() as u8;
        let db = (p_act[2] as i16 - p_exp[2] as i16).unsigned_abs() as u8;
        let diff = dr.max(dg).max(db);
        if diff > max_diff {
            max_diff = diff;
        }
        if diff > tolerance {
            mismatch_count += 1;
        }
    }
    let mismatch_ratio = mismatch_count as f64 / total_pixels as f64;
    assert!(
        mismatch_ratio <= max_mismatch_ratio,
        "{context}: mismatch ratio {:.4}% exceeds limit {:.4}%, max_diff={max_diff}",
        mismatch_ratio * 100.0,
        max_mismatch_ratio * 100.0
    );
}

fn extract_flate_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();
    let mut pos = 0;
    while let Some(idx) = pdf[pos..].windows(6).position(|w| w == b"stream") {
        let stream_start = pos + idx + 6;
        let mut data_start = stream_start;
        if data_start < pdf.len() && pdf[data_start] == b'\r' {
            data_start += 1;
        }
        if data_start < pdf.len() && pdf[data_start] == b'\n' {
            data_start += 1;
        }
        if let Some(end_idx) = pdf[data_start..].windows(9).position(|w| w == b"endstream") {
            let mut data_end = data_start + end_idx;
            if data_end > data_start && pdf[data_end - 1] == b'\n' {
                data_end -= 1;
            }
            if data_end > data_start && pdf[data_end - 1] == b'\r' {
                data_end -= 1;
            }
            let raw = &pdf[data_start..data_end];
            let mut decoder = flate2::read::ZlibDecoder::new(raw);
            let mut decompressed = Vec::new();
            if std::io::Read::read_to_end(&mut decoder, &mut decompressed).is_ok()
                && !decompressed.is_empty()
            {
                streams.push(decompressed);
            }
            pos = data_start + end_idx + 9;
        } else {
            break;
        }
    }
    streams
}

/// Writes a single-page PDF with `page_size`, renders it at `scale`, and verifies that
/// the non-white pixel bounding box matches `calculate_layout` within [`PLACEMENT_TOLERANCE_PX`].
fn assert_paper_placement(
    pdfium: &Pdfium,
    raw_bytes: &[u8],
    page_size: PageSize,
    scale: f32,
    context: &str,
) -> (image::RgbImage, PageLayout) {
    let info = probe_reader(std::io::Cursor::new(raw_bytes)).expect("probing fixture image");
    let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
    writer
        .add_page(raw_bytes, page_size)
        .expect("adding page to pdf");
    let pdf_bytes = writer.finish().expect("finishing pdf");

    let doc = pdfium
        .load_pdf_from_byte_slice(&pdf_bytes, None)
        .expect("loading written pdf");
    let page = doc.pages().get(0).expect("getting first page of pdf");

    let render_config = PdfRenderConfig::new().scale_page_by_factor(scale);
    let rendered_image = render_page_to_rgb(&page, &render_config);

    let layout = calculate_layout(&info, page_size);

    // Expected bounding box on the rendered pixel grid:
    // Start is rounded down (floor), end is rounded up (ceil) (design §11.2).
    let exp_min_x = (layout.image_rect.x * scale).floor() as u32;
    let exp_min_y = (layout.image_rect.y * scale).floor() as u32;
    let exp_max_x = ((layout.image_rect.x + layout.image_rect.width) * scale).ceil() as u32;
    let exp_max_y = ((layout.image_rect.y + layout.image_rect.height) * scale).ceil() as u32;

    // Measure actual non-white bounding box (RGB any < NON_WHITE_THRESHOLD)
    let mut act_min_x = u32::MAX;
    let mut act_min_y = u32::MAX;
    let mut act_max_x = 0u32;
    let mut act_max_y = 0u32;

    for (x, y, pixel) in rendered_image.enumerate_pixels() {
        let [r, g, b] = pixel.0;
        if r < NON_WHITE_THRESHOLD || g < NON_WHITE_THRESHOLD || b < NON_WHITE_THRESHOLD {
            act_min_x = act_min_x.min(x);
            act_min_y = act_min_y.min(y);
            act_max_x = act_max_x.max(x);
            act_max_y = act_max_y.max(y);
        }
    }

    let act_end_x = act_max_x + 1;
    let act_end_y = act_max_y + 1;

    println!(
        "{context:<25}: actual [{act_min_x}, {act_min_y}, {act_end_x}, {act_end_y}] vs expected [{exp_min_x}, {exp_min_y}, {exp_max_x}, {exp_max_y}]"
    );

    assert!(
        (act_min_x as i64 - exp_min_x as i64).abs() <= PLACEMENT_TOLERANCE_PX,
        "{context}: min_x diff too large: act={act_min_x}, exp={exp_min_x}"
    );
    assert!(
        (act_min_y as i64 - exp_min_y as i64).abs() <= PLACEMENT_TOLERANCE_PX,
        "{context}: min_y diff too large: act={act_min_y}, exp={exp_min_y}"
    );
    assert!(
        (act_end_x as i64 - exp_max_x as i64).abs() <= PLACEMENT_TOLERANCE_PX,
        "{context}: max_x diff too large: act={act_end_x}, exp={exp_max_x}"
    );
    assert!(
        (act_end_y as i64 - exp_max_y as i64).abs() <= PLACEMENT_TOLERANCE_PX,
        "{context}: max_y diff too large: act={act_end_y}, exp={exp_max_y}"
    );

    (rendered_image, layout)
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
        "exif_gps.jpg",
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

    // Writes `name` as a one-page "fit" PDF, renders it at the image's own dpi, and verifies
    // that the pixels match the source image with its EXIF orientation applied over white.
    let assert_fit_matches_source = |name: &str, conformance: PdfConformance| {
        let path = fix_dir.join(name);
        let raw_bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {name}: {e:?}"));

        let info = probe_reader(std::io::Cursor::new(&raw_bytes))
            .unwrap_or_else(|e| panic!("failed to probe {name}: {e:?}"));

        let mut writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
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
        let scale = info.dpi_x as f32 / POINTS_PER_INCH;
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
            "rendered dimensions mismatch for {name} ({conformance:?}): rendered={:?}, expected={:?}",
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
            "{name} ({conformance:?}): mismatch ratio {mismatch_percent:.4}% exceeds limit {:.4}%",
            MAX_MISMATCH_RATIO_IMAGE * 100.0
        );
    };

    for conformance in [PdfConformance::Standard, PdfConformance::PdfA2b] {
        println!("\n--- [Fit, {conformance:?}] Quality Test Results ---");
        for name in fit_fixtures {
            assert_fit_matches_source(name, conformance);
        }
    }

    // -----------------------------------------------------------------------
    // 1b. TIFF and anisotropic image quality tests (design §11.2)
    // -----------------------------------------------------------------------
    for conformance in [PdfConformance::Standard, PdfConformance::PdfA2b] {
        println!("\n--- [TIFF Quality, {conformance:?}] ---");

        // scan_g4.tif: 3 pages, sequentially verified against expected
        let scan_g4_bytes = std::fs::read(fix_dir.join("scan_g4.tif")).unwrap();
        let mut g4_writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
        g4_writer.add_page(&scan_g4_bytes, PageSize::Fit).unwrap();
        let g4_pdf = g4_writer.finish().unwrap();
        let g4_doc = pdfium.load_pdf_from_byte_slice(&g4_pdf, None).unwrap();
        assert_eq!(g4_doc.pages().len(), 3, "scan_g4 must have 3 pages");

        for (p_idx, exp_img) in [
            (0, gen_fixtures::scan_g4_expected(1)),
            (1, gen_fixtures::scan_g4_expected(2)),
            (2, gen_fixtures::scan_g4_expected(3)),
        ] {
            let p = g4_doc.pages().get(p_idx).unwrap();
            let rendered = render_page_to_rgb(
                &p,
                &PdfRenderConfig::new().scale_page_by_factor(300.0 / POINTS_PER_INCH),
            );
            assert_images_match(
                &format!("scan_g4 page {p_idx} ({conformance:?})"),
                &rendered,
                &exp_img,
                COLOR_TOLERANCE_IMAGE,
                MAX_MISMATCH_RATIO_IMAGE,
            );
        }

        // fax_standard.tif: 1 page, 204x98 dpi, ~610x840 pt (design §11.2)
        let fax_bytes = std::fs::read(fix_dir.join("fax_standard.tif")).unwrap();
        let mut fax_writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
        fax_writer.add_page(&fax_bytes, PageSize::Fit).unwrap();
        let fax_pdf = fax_writer.finish().unwrap();
        let fax_doc = pdfium.load_pdf_from_byte_slice(&fax_pdf, None).unwrap();
        let fax_page = fax_doc.pages().get(0).unwrap();
        assert!(
            (fax_page.width().value - 610.0).abs() < 1.0,
            "fax width pt ~ 610"
        );
        assert!(
            (fax_page.height().value - 839.8).abs() < 1.0,
            "fax height pt ~ 840"
        );
        let fax_rendered = render_page_to_rgb(
            &fax_page,
            &PdfRenderConfig::new().set_target_size(1728, 1143),
        );
        assert_images_match(
            &format!("fax_standard ({conformance:?})"),
            &fax_rendered,
            &gen_fixtures::fax_standard_expected(),
            COLOR_TOLERANCE_IMAGE,
            MAX_MISMATCH_RATIO_IMAGE,
        );

        // dpi_aniso.png: 300x150 px at 300x150 dpi -> 72x72 pt
        let aniso_bytes = std::fs::read(fix_dir.join("dpi_aniso.png")).unwrap();
        let mut aniso_writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
        aniso_writer.add_page(&aniso_bytes, PageSize::Fit).unwrap();
        let aniso_pdf = aniso_writer.finish().unwrap();
        let aniso_doc = pdfium.load_pdf_from_byte_slice(&aniso_pdf, None).unwrap();
        let aniso_page = aniso_doc.pages().get(0).unwrap();
        assert!(
            (aniso_page.width().value - 72.0).abs() < 0.1,
            "aniso width pt 72"
        );
        assert!(
            (aniso_page.height().value - 72.0).abs() < 0.1,
            "aniso height pt 72"
        );
        let aniso_rendered = render_page_to_rgb(
            &aniso_page,
            &PdfRenderConfig::new().set_target_size(300, 150),
        );
        assert_images_match(
            &format!("dpi_aniso ({conformance:?})"),
            &aniso_rendered,
            &gen_fixtures::dpi_aniso_expected(),
            COLOR_TOLERANCE_IMAGE,
            MAX_MISMATCH_RATIO_IMAGE,
        );

        // Single-page TIFFs tested against expected images
        let single_page_tiffs = [
            ("gray_lzw.tif", gen_fixtures::gray_lzw_expected()),
            (
                "gray16_deflate.tif",
                gen_fixtures::gray16_deflate_expected(),
            ),
            (
                "gray4_packbits.tif",
                gen_fixtures::gray4_packbits_expected(),
            ),
            ("rgb_lzw.tif", gen_fixtures::rgb_lzw_expected()),
            ("rgb16.tif", gen_fixtures::rgb16_expected()),
            ("rgba_unassoc.tif", gen_fixtures::rgba_unassoc_expected()),
            ("rgba_assoc.tif", gen_fixtures::rgba_assoc_expected()),
            ("gray_alpha.tif", gen_fixtures::gray_alpha_expected()),
        ];
        for (name, expected) in single_page_tiffs {
            let bytes = std::fs::read(fix_dir.join(name)).unwrap();
            let mut writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
            writer.add_page(&bytes, PageSize::Fit).unwrap();
            let pdf = writer.finish().unwrap();
            let doc = pdfium.load_pdf_from_byte_slice(&pdf, None).unwrap();
            let page = doc.pages().get(0).unwrap();
            let rendered = render_page_to_rgb(
                &page,
                &PdfRenderConfig::new().scale_page_by_factor(150.0 / POINTS_PER_INCH),
            );
            assert_images_match(
                &format!("{name} ({conformance:?})"),
                &rendered,
                &expected,
                COLOR_TOLERANCE_IMAGE,
                MAX_MISMATCH_RATIO_IMAGE,
            );
        }

        // photo_jpeg.tif: verified against image crate decode of embedded JPEG strip (design §11.2)
        let pj_bytes = std::fs::read(fix_dir.join("photo_jpeg.tif")).unwrap();
        let mut pj_writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
        pj_writer.add_page(&pj_bytes, PageSize::Fit).unwrap();
        let pj_pdf = pj_writer.finish().unwrap();
        let pj_doc = pdfium.load_pdf_from_byte_slice(&pj_pdf, None).unwrap();
        let pj_page = pj_doc.pages().get(0).unwrap();
        let pj_rendered = render_page_to_rgb(
            &pj_page,
            &PdfRenderConfig::new().scale_page_by_factor(72.0 / POINTS_PER_INCH),
        );
        let soi = pj_bytes.windows(2).position(|w| w == [0xFF, 0xD8]).unwrap();
        let eoi = pj_bytes
            .windows(2)
            .rposition(|w| w == [0xFF, 0xD9])
            .unwrap();
        let pj_expected = image::load_from_memory(&pj_bytes[soi..=eoi + 1])
            .unwrap()
            .to_rgb8();
        assert_images_match(
            &format!("photo_jpeg.tif ({conformance:?})"),
            &pj_rendered,
            &pj_expected,
            COLOR_TOLERANCE_IMAGE,
            MAX_MISMATCH_RATIO_IMAGE,
        );

        // rotate90.tif: orientation 6 applied, displays as 160x96 with quadrant colors
        let rot_bytes = std::fs::read(fix_dir.join("rotate90.tif")).unwrap();
        let mut rot_writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
        rot_writer.add_page(&rot_bytes, PageSize::Fit).unwrap();
        let rot_pdf = rot_writer.finish().unwrap();
        let rot_doc = pdfium.load_pdf_from_byte_slice(&rot_pdf, None).unwrap();
        let rot_page = rot_doc.pages().get(0).unwrap();
        let rot_rendered = render_page_to_rgb(
            &rot_page,
            &PdfRenderConfig::new().scale_page_by_factor(150.0 / POINTS_PER_INCH),
        );
        assert_eq!(rot_rendered.dimensions(), (160, 96), "rotate90 dimensions");
        let quadrants = [
            ((40, 24), [220u8, 40, 40], "top-left (red)"),
            ((120, 24), [40, 170, 90], "top-right (green)"),
            ((40, 72), [40, 80, 200], "bottom-left (blue)"),
            ((120, 72), [250, 210, 0], "bottom-right (yellow)"),
        ];
        for ((px, py), exp_col, label) in quadrants {
            let act = rot_rendered.get_pixel(px, py).0;
            for c in 0..3 {
                let diff = (act[c] as i16 - exp_col[c] as i16).unsigned_abs() as u8;
                assert!(
                    diff <= COLOR_TOLERANCE_RENDER,
                    "rotate90.tif {label} channel {c}: act={act:?}, exp={exp_col:?}, diff={diff}"
                );
            }
        }
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
        "rotate90.tif",
        "dpi300.png",
        "dpi300.jpg",
        "opaque.bmp",
    ];

    println!("\n--- [A4] Placement Test Results ---");
    let scale_a4 = RENDER_DPI_A4 / POINTS_PER_INCH;
    let a4_orientations = [
        PageOrientation::Auto,
        PageOrientation::Portrait,
        PageOrientation::Landscape,
    ];

    for orientation in a4_orientations {
        for name in a4_fixtures {
            let path = fix_dir.join(name);
            let raw_bytes = std::fs::read(&path).unwrap();
            let context = format!("{name} ({orientation:?})");
            let (rendered_image, layout) = assert_paper_placement(
                &pdfium,
                &raw_bytes,
                PageSize::Paper(PaperSize::A4, orientation),
                scale_a4,
                &context,
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
                        let diff = (actual_pixel[c] as i16 - expected_color[c] as i16)
                            .unsigned_abs() as u8;
                        assert!(
                            diff <= COLOR_TOLERANCE_RENDER,
                            "{name} ({orientation:?}) {label} channel {c} mismatch: actual={actual_pixel:?}, expected={expected_color:?}, diff={diff}"
                        );
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // 2b. Letter and JIS B5 placement and dimension tests (design §11.2)
    // -----------------------------------------------------------------------
    println!("\n--- [Letter & JIS B5] Placement and Dimension Tests ---");
    let other_papers = [(PaperSize::Letter, "Letter"), (PaperSize::JisB5, "JIS B5")];
    // One wide and one tall image, both free of white pixels. Every fixture
    // without white is wide once its EXIF orientation is applied, so the tall
    // one is made here.
    let wide_bytes = std::fs::read(fix_dir.join("rotate0.jpg")).unwrap();
    let mut tall_bytes = Vec::new();
    image::RgbImage::from_pixel(96, 160, Rgb([40, 90, 160]))
        .write_to(
            &mut std::io::Cursor::new(&mut tall_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let nonwhite_images = [
        ("rotate0.jpg (160x96)", &wide_bytes),
        ("generated tall PNG (96x160)", &tall_bytes),
    ];

    for (paper, paper_name) in other_papers {
        let (paper_w_pt, paper_h_pt) = paper.portrait_dimensions_pt();
        for orientation in [PageOrientation::Portrait, PageOrientation::Landscape] {
            let (exp_w_pt, exp_h_pt) = match orientation {
                PageOrientation::Portrait => (paper_w_pt, paper_h_pt),
                PageOrientation::Landscape => (paper_h_pt, paper_w_pt),
                PageOrientation::Auto => unreachable!(),
            };
            let exp_rendered_w = (exp_w_pt * scale_a4).round() as u32;
            let exp_rendered_h = (exp_h_pt * scale_a4).round() as u32;

            for (image_name, raw_bytes) in nonwhite_images {
                let context = format!("{paper_name} ({orientation:?}) {image_name}");
                let (rendered_image, _) = assert_paper_placement(
                    &pdfium,
                    raw_bytes,
                    PageSize::Paper(paper, orientation),
                    scale_a4,
                    &context,
                );

                // Verify rendered dimensions match paper dimensions and orientation (within 1 px for rounding)
                assert!(
                    (rendered_image.width() as i64 - exp_rendered_w as i64).abs()
                        <= PLACEMENT_TOLERANCE_PX,
                    "{context}: width mismatch: got {}, exp {}",
                    rendered_image.width(),
                    exp_rendered_w
                );
                assert!(
                    (rendered_image.height() as i64 - exp_rendered_h as i64).abs()
                        <= PLACEMENT_TOLERANCE_PX,
                    "{context}: height mismatch: got {}, exp {}",
                    rendered_image.height(),
                    exp_rendered_h
                );
            }
        }
    }

    // Verify rendered dimensions match page orientation for landscape (photo.jpg) and portrait (shapes_150dpi_p1.png) fixtures
    let landscape_bytes = std::fs::read(fix_dir.join("photo.jpg")).unwrap();
    let portrait_bytes = std::fs::read(fix_dir.join("shapes_150dpi_p1.png")).unwrap();

    for (orient, expect_portrait) in [
        (PageOrientation::Portrait, true),
        (PageOrientation::Landscape, false),
        (PageOrientation::Auto, false), // photo.jpg is landscape -> landscape in Auto
    ] {
        let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
        writer
            .add_page(&landscape_bytes, PageSize::Paper(PaperSize::A4, orient))
            .unwrap();
        let pdf_bytes = writer.finish().unwrap();
        let doc = pdfium.load_pdf_from_byte_slice(&pdf_bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let render_config = PdfRenderConfig::new().scale_page_by_factor(scale_a4);
        let rendered_image = render_page_to_rgb(&page, &render_config);
        if expect_portrait {
            assert!(
                rendered_image.height() > rendered_image.width(),
                "photo.jpg with {orient:?} should render portrait, got {}x{}",
                rendered_image.width(),
                rendered_image.height()
            );
        } else {
            assert!(
                rendered_image.width() > rendered_image.height(),
                "photo.jpg with {orient:?} should render landscape, got {}x{}",
                rendered_image.width(),
                rendered_image.height()
            );
        }
    }

    for (orient, expect_portrait) in [
        (PageOrientation::Portrait, true),
        (PageOrientation::Landscape, false),
        (PageOrientation::Auto, true), // shapes_150dpi_p1.png is portrait -> portrait in Auto
    ] {
        let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
        writer
            .add_page(&portrait_bytes, PageSize::Paper(PaperSize::A4, orient))
            .unwrap();
        let pdf_bytes = writer.finish().unwrap();
        let doc = pdfium.load_pdf_from_byte_slice(&pdf_bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let render_config = PdfRenderConfig::new().scale_page_by_factor(scale_a4);
        let rendered_image = render_page_to_rgb(&page, &render_config);
        if expect_portrait {
            assert!(
                rendered_image.height() > rendered_image.width(),
                "shapes_150dpi_p1.png with {orient:?} should render portrait, got {}x{}",
                rendered_image.width(),
                rendered_image.height()
            );
        } else {
            assert!(
                rendered_image.width() > rendered_image.height(),
                "shapes_150dpi_p1.png with {orient:?} should render landscape, got {}x{}",
                rendered_image.width(),
                rendered_image.height()
            );
        }
    }

    // -----------------------------------------------------------------------
    // 3. corrupt.png fails with DecodeFailed; writer can continue afterwards
    // -----------------------------------------------------------------------
    let corrupt_bytes = std::fs::read(fix_dir.join("corrupt.png")).unwrap();
    let mut corrupt_writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);

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

    // -----------------------------------------------------------------------
    // 3b. corrupt_page2.tif fails with DecodeFailed; writer can continue afterwards (design §11.2)
    // -----------------------------------------------------------------------
    let corrupt_t_bytes = std::fs::read(fix_dir.join("corrupt_page2.tif")).unwrap();
    let mut corrupt_t_writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);

    let err = corrupt_t_writer
        .add_page(&corrupt_t_bytes, PageSize::Fit)
        .unwrap_err();
    assert_eq!(err, ProbeError::DecodeFailed);

    corrupt_t_writer
        .add_page(&valid_bytes, PageSize::Fit)
        .expect("should add valid page after corrupt TIFF page failure");

    let recovered_t_pdf = corrupt_t_writer.finish().expect("should finish PDF");
    let recovered_t_doc = pdfium
        .load_pdf_from_byte_slice(&recovered_t_pdf, None)
        .expect("should load recovered PDF");
    assert_eq!(recovered_t_doc.pages().len(), 1);
}

#[test]
fn tiff_metadata_tags_do_not_appear_in_generated_pdf() {
    let fix_dir = fixtures_dir();
    let raw_bytes = std::fs::read(fix_dir.join("metadata.tif")).unwrap();
    let pdf = write_one_page(&raw_bytes, PageSize::Fit, PdfConformance::Standard);

    let secrets = [
        gen_fixtures::TIFF_DESC_SECRET.as_bytes(),
        gen_fixtures::TIFF_ARTIST_SECRET.as_bytes(),
        gen_fixtures::TIFF_SOFTWARE_SECRET.as_bytes(),
        gen_fixtures::TIFF_XMP_SECRET.as_bytes(),
    ];

    let flate_streams = extract_flate_streams(&pdf);

    for secret in secrets {
        assert!(
            !pdf.windows(secret.len()).any(|w| w == secret),
            "secret {:?} found in raw PDF",
            String::from_utf8_lossy(secret)
        );
        for stream in &flate_streams {
            assert!(
                !stream.windows(secret.len()).any(|w| w == secret),
                "secret {:?} found in decompressed stream",
                String::from_utf8_lossy(secret)
            );
        }
    }
}

#[test]
fn jpeg_image_data_is_embedded_without_metadata_including_cmyk() {
    let fix_dir = fixtures_dir();
    let jpegs = [
        "photo.jpg",
        "photo_cmyk.jpg",
        "rotate0.jpg",
        "flip_h.jpg",
        "rotate180.jpg",
        "flip_v.jpg",
        "rotate90_flip_h.jpg",
        "rotate90.jpg",
        "rotate270_flip_h.jpg",
        "rotate270.jpg",
        "exif_gps.jpg",
    ];

    for name in jpegs {
        let raw_bytes = std::fs::read(fix_dir.join(name)).unwrap();
        let stripped_bytes = pdfconv_core::strip_jpeg_metadata(&raw_bytes).unwrap();
        let mut writer = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
        writer.add_page(&raw_bytes, PageSize::Fit).unwrap();
        let pdf_bytes = writer.finish().unwrap();

        // Verify the stripped JPEG bytes exist as a contiguous substring in the generated PDF (design §11.2)
        let found = pdf_bytes
            .windows(stripped_bytes.len())
            .any(|window| window == stripped_bytes.as_slice());
        assert!(
            found,
            "stripped JPEG bytes of {name} were not found verbatim in the generated PDF"
        );

        // Fixtures with metadata (orientation EXIF or exif_gps.jpg) must NOT contain original bytes
        let has_metadata =
            name.starts_with("rotate") || name.starts_with("flip") || name == "exif_gps.jpg";
        if has_metadata {
            assert_ne!(
                raw_bytes, stripped_bytes,
                "metadata-bearing fixture {name} must have bytes stripped"
            );
            let original_found = pdf_bytes
                .windows(raw_bytes.len())
                .any(|window| window == raw_bytes.as_slice());
            assert!(
                !original_found,
                "original unstripped bytes of {name} must not be present in the generated PDF"
            );
        } else {
            // photo.jpg and photo_cmyk.jpg have no metadata to strip, so stripped == raw
            assert_eq!(
                raw_bytes, stripped_bytes,
                "clean fixture {name} should have unchanged bytes"
            );
        }
    }
}

#[test]
fn pdf_metadata_contains_only_creator_and_no_file_names() {
    let fix_dir = fixtures_dir();
    let creator = "PDF Converter 0.1.0";
    let mut writer = PdfWriter::new(creator, PdfConformance::Standard);

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

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Writes `raw_bytes` as one page and returns the PDF.
fn write_one_page(raw_bytes: &[u8], page_size: PageSize, conformance: PdfConformance) -> Vec<u8> {
    let mut writer = PdfWriter::new("PDF Converter 0.1.0", conformance);
    writer
        .add_page(raw_bytes, page_size)
        .expect("adding the page");
    writer.finish().expect("finishing the PDF")
}

/// The images of the fixtures folder, by file name.
fn image_fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("reading the fixtures folder")
        .map(|entry| {
            entry
                .expect("reading a fixtures entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext))
        })
        .collect();
    names.sort();
    names
}

#[test]
fn pdfa_finishes_for_every_fixture_image_except_cmyk_and_corrupt() {
    const A4_AUTO: PageSize = PageSize::Paper(PaperSize::A4, PageOrientation::Auto);
    let mut merged = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::PdfA2b);
    let mut written = 0;
    let mut refused = Vec::new();

    for name in image_fixture_names() {
        let raw_bytes = std::fs::read(fixtures_dir().join(&name)).unwrap();
        let mut as_fit = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::PdfA2b);
        match as_fit.add_page(&raw_bytes, PageSize::Fit) {
            Ok(()) => {}
            Err(
                err @ (ProbeError::DecodeFailed
                | ProbeError::CmykNotAllowedInPdfA
                | ProbeError::UnsupportedFormat
                | ProbeError::TooLarge
                | ProbeError::TooManyPages),
            ) => {
                refused.push((name, err));
                continue;
            }
            Err(err) => panic!("{name}: unexpected error {err:?}"),
        }
        as_fit
            .finish()
            .unwrap_or_else(|e| panic!("{name}: PDF/A at image size failed to finish: {e}"));

        let mut on_a4 = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::PdfA2b);
        on_a4.add_page(&raw_bytes, A4_AUTO).unwrap();
        on_a4
            .finish()
            .unwrap_or_else(|e| panic!("{name}: PDF/A on A4 failed to finish: {e}"));

        merged.add_page(&raw_bytes, A4_AUTO).unwrap();
        written += 1;
    }
    merged
        .finish()
        .unwrap_or_else(|e| panic!("merged PDF/A failed to finish: {e}"));

    assert!(written > 0);
    assert_eq!(
        refused,
        vec![
            ("cmyk.tif".to_owned(), ProbeError::UnsupportedFormat),
            ("corrupt.png".to_owned(), ProbeError::DecodeFailed),
            ("corrupt_page2.tif".to_owned(), ProbeError::DecodeFailed),
            ("fax_g3.tif".to_owned(), ProbeError::UnsupportedFormat),
            ("palette.tif".to_owned(), ProbeError::UnsupportedFormat),
            (
                "photo_cmyk.jpg".to_owned(),
                ProbeError::CmykNotAllowedInPdfA
            ),
            ("planar.tif".to_owned(), ProbeError::UnsupportedFormat),
            ("too_large_page2.tif".to_owned(), ProbeError::TooLarge),
            ("too_many_pages.tif".to_owned(), ProbeError::TooManyPages),
        ]
    );
}

#[test]
fn only_a_standard_pdf_marks_images_as_interpolated() {
    // One image of each format the writer hands to krilla with an `interpolate` flag.
    for name in [
        "photo.jpg",
        "logo_alpha.png",
        "logo_alpha.webp",
        "rgb_lzw.tif",
    ] {
        let raw_bytes = std::fs::read(fixtures_dir().join(name)).unwrap();
        let standard = write_one_page(&raw_bytes, PageSize::Fit, PdfConformance::Standard);
        let pdfa = write_one_page(&raw_bytes, PageSize::Fit, PdfConformance::PdfA2b);
        assert!(
            contains(&standard, "/Interpolate true"),
            "{name}: standard PDF must interpolate"
        );
        assert!(
            !contains(&pdfa, "/Interpolate"),
            "{name}: PDF/A must not have /Interpolate"
        );
    }
}

#[test]
fn pdfa_identification_and_utc_creation_date_are_in_the_xmp_of_pdfa_only() {
    let raw_bytes = std::fs::read(fixtures_dir().join("photo.jpg")).unwrap();
    let standard = write_one_page(&raw_bytes, PageSize::Fit, PdfConformance::Standard);
    let pdfa = String::from_utf8_lossy(&write_one_page(
        &raw_bytes,
        PageSize::Fit,
        PdfConformance::PdfA2b,
    ))
    .into_owned();

    assert!(pdfa.contains("<pdfaid:part>2</pdfaid:part>"));
    assert!(pdfa.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
    assert!(!contains(&standard, "pdfaid:"));

    // The creation date is the time of writing in UTC, to the second.
    let start = pdfa
        .find("<xmp:CreateDate>")
        .expect("PDF/A XMP must have a creation date")
        + "<xmp:CreateDate>".len();
    let date = &pdfa[start..pdfa[start..].find('<').unwrap() + start];
    let (stamp, offset) = date.split_at(date.len() - "+00:00".len());
    assert_eq!(offset, "+00:00", "creation date {date} must be UTC");
    let fields: Vec<u32> = stamp
        .split(['-', 'T', ':'])
        .map(|field| field.parse().unwrap())
        .collect();
    assert_eq!(
        fields.len(),
        6,
        "creation date {date} must go to the second"
    );
    assert!(
        fields[0] >= 2026,
        "creation date {date} must be the present"
    );

    // A standard PDF keeps only the creator, as before (design §4.3).
    for key in ["CreateDate", "ModifyDate", "CreationDate", "ModDate"] {
        assert!(!contains(&standard, key), "standard PDF has {key}");
    }
}

#[test]
fn cmyk_jpeg_is_refused_in_pdfa_without_adding_a_page_and_accepted_otherwise() {
    let cmyk = std::fs::read(fixtures_dir().join("photo_cmyk.jpg")).unwrap();
    let rgb = std::fs::read(fixtures_dir().join("photo.jpg")).unwrap();

    let mut pdfa = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::PdfA2b);
    assert_eq!(
        pdfa.add_page(&cmyk, PageSize::Fit),
        Err(ProbeError::CmykNotAllowedInPdfA)
    );
    // The writer keeps going and the refused image left no page behind.
    pdfa.add_page(&rgb, PageSize::Fit).unwrap();
    let pdf = pdfa.finish().unwrap();
    assert!(contains(&pdf, "/Count 1"), "only the RGB page must exist");
    let cmyk_scan = pdfconv_core::strip_jpeg_metadata(&cmyk).unwrap();
    assert!(
        !pdf.windows(cmyk_scan.len()).any(|w| w == cmyk_scan),
        "the refused CMYK image must not be in the document"
    );

    let mut standard = PdfWriter::new("PDF Converter 0.1.0", PdfConformance::Standard);
    standard.add_page(&cmyk, PageSize::Fit).unwrap();
    standard.add_page(&rgb, PageSize::Fit).unwrap();
    assert!(contains(&standard.finish().unwrap(), "/Count 2"));
}
