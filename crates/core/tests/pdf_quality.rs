//! Quality and conformance tests for PDF writing (design §11.2).
//!
//! Because pdfium does not support multithreaded usage within the same process,
//! all tests that invoke pdfium run sequentially within a single `#[test]` function.

use std::path::PathBuf;

use image::Rgb;
use pdfconv_core::probe::probe_reader;
use pdfconv_core::{
    POINTS_PER_INCH, PageLayout, PageOrientation, PageSize, PaperSize, PdfWriter, ProbeError,
    calculate_layout,
};
use pdfium_render::prelude::*;

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
    let mut writer = PdfWriter::new("PDF Converter 0.1.0");
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
        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
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
        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
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
        let mut writer = PdfWriter::new("PDF Converter 0.1.0");
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
